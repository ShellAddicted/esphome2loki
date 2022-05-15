use rumqttc::{AsyncClient, EventLoop, Incoming, MqttOptions, QoS, Transport};
use rustls::ClientConfig;
use std::path::PathBuf;
use structopt::StructOpt;
use tracing::{debug, error, info, trace};

use std::time::Duration;

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
        .set_pending_throttle(Duration::from_millis(10))
        .set_credentials(cfg.username, cfg.password)
        .set_clean_session(false);

    AsyncClient::new(mqttoptions, 10)
}

#[tokio::main]
async fn main() {
    let args = CliArgs::from_args();
    let cfg = config::load_config_from_path(args.config).unwrap();
    let version: &str = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");

    let filter = tracing_subscriber::EnvFilter::new(&cfg.system.log_level);
    let stdout_subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(stdout_subscriber).unwrap();

    info!("esphome2loki v{}", version);
    trace!("Config: {:?}", cfg);

    let loki = LokiAPI::new(cfg.loki.base_url);
    let (client, mut eventloop) = mqtt_init(cfg.mqtt);

    let mut fail = true;
    for topic in cfg.topic2device.keys() {
        debug!("Subscribing to topic {}", topic);
        match client.subscribe(topic, QoS::AtMostOnce).await {
            Ok(_) => {
                fail = false;
                debug!("Subscription to {} succeed.", topic)
            }
            Err(err) => error!("Subscription to {} failed, due to {}", topic, err),
        }
    }
    if fail {
        error!("Couldn't subscribe to any topic :-( quitting...");
        return;
    }

    tokio::task::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(event) => match event {
                    rumqttc::Event::Incoming(Incoming::Publish(p)) => {
                        debug!("Received = {:?}", p);
                        let timestamp = LokiAPI::get_timestamp();
                        let dev = cfg.topic2device[&p.topic].clone();
                        let message = String::from_utf8(p.payload.to_vec()).unwrap();

                        for i in 1..=3 {
                            debug!("Loki push attempt {}", i);
                            let res = loki
                                .push(
                                    dev.label.clone(),
                                    [[timestamp.to_string(), message.clone()]].to_vec(),
                                )
                                .await;
                            match res {
                                Ok(_) => break,
                                Err(_) => error!("Loki push failed"),
                            }
                        }
                    }
                    rumqttc::Event::Incoming(Incoming::ConnAck(_)) => {
                        info!("Connected to MQTT broker.");
                    }
                    _ => {}
                },
                Err(e) => {
                    error!("MQTT Connection error encountered: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    let () = futures::future::pending().await;
}
