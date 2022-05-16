use rumqttc::{AsyncClient, EventLoop, Incoming, MqttOptions, QoS, Transport};
use rustls::ClientConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::{signal, sync, time};
use tracing::{debug, error, info, trace};

use crate::loki::LokiAPI;

mod config;
mod loki;

#[derive(Debug, StructOpt)]
#[structopt(name = "esphome2loki", about = "ESPHome MQTT logs to Loki.")]
struct CliArgs {
    #[structopt(
        parse(from_os_str),
        short = "c",
        long = "config",
        default_value = "config.toml",
        help = "Path to configuration file. See sample_config.toml for format.",
        env = "ESPHOME2LOKI_CONFIG"
    )]
    config: PathBuf,
}

#[derive(Debug, Clone)]
struct MQTTMessage {
    pub timestamp: i64,
    pub message: String,
    pub device: config::ConfigDevice,
}

fn mqtt_init(cfg: config::ConfigMqtt) -> (AsyncClient, EventLoop) {
    let mut mqttoptions = MqttOptions::new(cfg.client_id, cfg.address, cfg.port);

    if cfg.use_tls {
        // Use rustls-native-certs to load root certificates from the operating system.
        let mut root_cert_store = rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().expect("could not load platform certs")
        {
            root_cert_store.add(&rustls::Certificate(cert.0)).unwrap();
        }

        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        mqttoptions.set_transport(Transport::tls_with_config(client_config.into()));
    }

    mqttoptions
        .set_keep_alive(std::time::Duration::from_secs(5))
        .set_pending_throttle(std::time::Duration::from_millis(10))
        .set_credentials(cfg.username, cfg.password)
        .set_clean_session(false);

    AsyncClient::new(mqttoptions, 10)
}

async fn push_batch(api: &LokiAPI, batch: &HashMap<String, Vec<loki::LokiValue>>) {
    for (k, v) in batch {
        for i in 1..=3 {
            debug!("Loki push attempt {}", i);
            let res = api.push(k.clone(), v.clone()).await;
            match res {
                Ok(_) => break,
                Err(_) => {
                    error!("Loki push failed");
                    time::sleep(time::Duration::from_secs(2)).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args = CliArgs::from_args();
    let cfg = match config::load_config_from_path(args.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Invalid config: {}", e);
            return;
        }
    };
    let version: &str = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");

    let filter = tracing_subscriber::EnvFilter::new(&cfg.system.log_level);
    let stdout_subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(stdout_subscriber).unwrap();

    info!("esphome2loki v{}", version);
    trace!("Config: {:?}", cfg);

    let lokiapi = LokiAPI::new(cfg.loki.base_url);
    let (client, mut eventloop) = mqtt_init(cfg.mqtt);
    let (tx, mut rx) = sync::mpsc::channel::<MQTTMessage>(512 * cfg.device.len());
    let (tx_shutdown, mut rx1_shutdown) = sync::broadcast::channel::<bool>(1);
    let mut rx2_shutdown = tx_shutdown.subscribe();

    let mut fail = true;
    for topic in cfg.topic2device.keys() {
        debug!("Subscribing to topic {}", topic);
        match client.subscribe(topic, QoS::AtMostOnce).await {
            Ok(_) => {
                fail = false;
                debug!("Subscription to {} succeed.", topic);
            }
            Err(err) => error!("Subscription to {} failed, due to {}", topic, err),
        }
    }
    if fail {
        error!("Couldn't subscribe to any topic. Quitting...");
        return;
    }

    let eventloop_handle = tokio::task::spawn(async move {
        loop {
            tokio::select! {
                _ = rx1_shutdown.recv() => break,
                evt = eventloop.poll() => {
                    match evt {
                        Ok(event) => match event {
                            rumqttc::Event::Incoming(Incoming::Publish(p)) => {
                                debug!("Received = {:?}", p);
                                let device = cfg.topic2device[&p.topic].clone();

                                let mqtt_message = MQTTMessage {
                                    timestamp: LokiAPI::get_timestamp(),
                                    device: device.clone(),
                                    message: String::from_utf8(p.payload.to_vec()).unwrap(),
                                };
                                _ = tx.send(mqtt_message).await;
                            }
                            rumqttc::Event::Incoming(Incoming::ConnAck(_)) => {
                                info!("Connected to MQTT broker.");
                            }
                            _ => {}
                        },
                        Err(e) => {
                            error!("MQTT Connection error encountered: {}", e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    });

    let worker_handle = tokio::task::spawn(async move {
        let mut batch = HashMap::<String, Vec<loki::LokiValue>>::new();
        let interval = time::interval(time::Duration::from_secs(cfg.loki.batch_timeout_seconds));
        let mut count: usize = 0;
        tokio::pin!(interval);

        loop {
            let should_push = tokio::select! {
                _ = interval.tick() => {
                    debug!("Batch timeout");
                    true
                },
                Some(msg) = rx.recv() => {
                    batch.entry(msg.device.label.clone()).or_default().push([msg.timestamp.to_string(), msg.message.clone()]);
                    count += 1;
                    let full = count >= cfg.loki.batch_size;
                    if full {
                        debug!("Batch size");
                    }
                    full
                }
                _ = rx2_shutdown.recv() => break
            };

            if should_push && count > 0 {
                debug!("Push Size={}", count);
                push_batch(&lokiapi, &batch).await;
                batch.clear();
                count = 0;
            }
        }
    });

    let mut main_future = futures::future::join(eventloop_handle, worker_handle);

    loop {
        tokio::select! {
            _ = &mut main_future => {
                break;
            },
            _ = signal::ctrl_c() => {
                info!("Shutting down. Waiting for all jobs to finish...");
                tx_shutdown.send(true).unwrap();
            },
        }
    }

    info!("Goodbye!");
}
