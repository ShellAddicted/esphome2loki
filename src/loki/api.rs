use serde::{Deserialize, Serialize};
use tracing::debug;

pub type LokiValue = [String; 2];

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct LokiStream {
    pub label: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct LokiStreams {
    pub stream: LokiStream,
    pub values: Vec<LokiValue>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct LokiPush {
    pub streams: Vec<LokiStreams>,
}

#[derive(Debug, Clone)]
pub struct LokiAPI {
    base_url: String,
    client: reqwest::Client,
}

impl LokiAPI {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    fn get_url(&self, method: &str) -> String {
        return format!("{}{}", self.base_url, method);
    }

    pub fn get_timestamp() -> i64 {
        chrono::offset::Utc::now().timestamp() * 10i64.pow(9)
    }

    #[allow(dead_code)]
    pub async fn is_ready(&self) -> Result<bool, reqwest::Error> {
        let url = self.get_url("/ready");
        let res = self.client.get(url).send().await?;
        debug!("Loki ready response {}", res.status());

        Ok(res.status().is_success())
    }

    pub async fn push(&self, label: String, values: Vec<LokiValue>) -> Result<(), reqwest::Error> {
        let req = LokiPush {
            streams: [LokiStreams {
                stream: LokiStream { label },
                values,
            }]
            .to_vec(),
        };
        let url = self.get_url("/loki/api/v1/push");
        let res = self.client.post(url).json(&req).send().await?;
        debug!("Loki push response {}", res.status());

        res.error_for_status()?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::loki::LokiAPI;

    fn get_loki() -> LokiAPI {
        LokiAPI::new("http://127.0.0.1:3100".to_string())
    }

    #[tokio::test]
    async fn test_loki_ready() {
        let loki = get_loki();
        assert_eq!(loki.is_ready().await.unwrap(), true);
    }

    #[tokio::test]
    async fn test_loki_push() {
        let loki = get_loki();
        assert_eq!(
            loki.push(
                "esphome2loki".to_string(),
                [[
                    LokiAPI::get_timestamp().to_string(),
                    "UNIT TEST".to_string()
                ]]
                .to_vec()
            )
            .await
            .unwrap(),
            ()
        );
    }
}
