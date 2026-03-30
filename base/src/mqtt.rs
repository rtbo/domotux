use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};
use tokio::{sync, task};

pub mod topics;

/// Make MQTT options from a base client ID and a broker address
pub fn make_options(dev_base: &str, broker: BrokerAddress) -> rumqttc::v5::MqttOptions {
    let client_id = unique_dev_id(dev_base);
    rumqttc::v5::MqttOptions::new(client_id, broker.host.clone(), broker.port)
}

fn unique_dev_id(base: &str) -> String {
    use rand::RngExt;

    let random_suffix: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(4)
        .map(char::from)
        .collect();
    format!("{}-{}", base, random_suffix)
}

#[derive(Debug, Clone)]
pub struct BrokerAddress {
    pub host: String,
    pub port: u16,
}

impl fmt::Display for BrokerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.port == 1883 {
            write!(f, "{}", self.host)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

impl FromStr for BrokerAddress {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (host, port) = match s.rsplit_once(':') {
            Some((host, port)) => (host, port.parse::<u16>()),
            None => (s, Ok(1883)),
        };

        if host.is_empty() {
            return Err(anyhow::anyhow!("Broker host cannot be empty"));
        }

        let port = port.map_err(|_| anyhow::anyhow!("Broker port must be a valid port number"))?;

        Ok(Self {
            host: host.to_string(),
            port,
        })
    }
}

impl Default for BrokerAddress {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 1883,
        }
    }
}

impl Serialize for BrokerAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BrokerAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

pub fn spawn_event_loop(
    mut event_loop: rumqttc::v5::EventLoop,
    tx: sync::mpsc::Sender<rumqttc::v5::Event>,
) -> task::JoinHandle<()> {
    task::spawn(async move {
        loop {
            match event_loop.poll().await {
                Ok(event) => {
                    if let Err(e) = tx.send(event).await {
                        log::error!("Failed to send MQTT event: {}", e);
                        break; // Exit the loop if the receiver has been dropped
                    }
                }
                Err(e) => {
                    log::error!("MQTT event loop error: {}", e);
                    // Sleep a bit before retrying to avoid busy loop on persistent errors
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    })
}
