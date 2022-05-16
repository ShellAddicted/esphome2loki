use std::collections::HashMap;
use std::path::Path;

use figment::{providers::Format, Figment};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Config {
    pub system: ConfigSystem,
    pub device: Vec<ConfigDevice>,
    pub loki: ConfigLoki,
    pub mqtt: ConfigMqtt,
    #[serde(skip_deserializing)]
    pub topic2device: HashMap<String, ConfigDevice>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ConfigSystem {
    pub log_level: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ConfigLoki {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub batch_size: usize,
    pub batch_timeout_seconds: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ConfigMqtt {
    pub address: String,
    pub port: u16,
    pub use_tls: bool,
    pub username: String,
    pub password: String,
    pub client_id: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct ConfigDevice {
    pub label: String,
    pub topic: String,
}

pub fn load_config_from_path(path: impl AsRef<Path>) -> Result<Config, String> {
    load_config(figment::providers::Toml::file(path))
}

pub fn load_config(data: impl figment::Provider) -> Result<Config, String> {
    let mut cfg: Config = Figment::new()
        .merge(figment::providers::Env::prefixed("ESPHOME2LOKI_"))
        .merge(data)
        .extract()
        .map_err(|e| e.to_string())?;

    if cfg.loki.batch_size == 0 {
        return Err("batch_size cannot be 0.".to_string());
    } else if cfg.loki.batch_timeout_seconds == 0 {
        return Err("batch_timeout_seconds cannot be 0.".to_string());
    }

    for dev in &cfg.device {
        cfg.topic2device.insert(dev.topic.clone(), dev.clone());
    }
    Ok(cfg)
}

#[cfg(test)]
mod test {
    use figment::providers::Format;

    #[test]
    fn test_sample_config_valid() {
        const SAMPLE_CONFIG: &str = include_str!("../sample_config.toml");
        insta::assert_yaml_snapshot!(super::load_config(figment::providers::Toml::string(
            SAMPLE_CONFIG
        )));
    }
}
