use std::time::Duration;

use base::{mqtt, vecmap::VecMap};
use rumqttc::v5::mqttbytes::{QoS, v5::PublishProperties};
use serde::{Deserialize, Serialize};

use crate::tic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: base::mqtt::BrokerAddress,
    power: PowerConfig,
    meters: MetersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PowerConfig {
    topic: String,
    tic_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetersConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
    topic: String,
    active_meter: String,
    meters: VecMap<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: "domotux.lan".parse().unwrap(),
            power: PowerConfig {
                topic: "domotux/papp".to_string(),
                tic_field: "PAPP".to_string(),
            },
            meters: MetersConfig {
                min_interval: Duration::from_secs(60),
                topic: "domotux/compteurs".to_string(),
                active_meter: "PTEC".to_string(),
                // Les champs du TIC à publier dans MQTT, avec leur nom dans MQTT
                // Pour que la valeur active soit reconnue, faut que la valeur MQTT
                // correspondent à ce qui est publié par PTEC (les points sont enlevés)
                meters: VecMap::from(vec![
                    ("BASE".to_string(), "TH".to_string()),
                    ("HCHC".to_string(), "HC".to_string()),
                    ("HCHP".to_string(), "HP".to_string()),
                    ("EJPHN".to_string(), "HN".to_string()),
                    ("EJPHPM".to_string(), "PM".to_string()),
                    ("BBRHCJB".to_string(), "HCJB".to_string()),
                    ("BBRHPJB".to_string(), "HPJB".to_string()),
                    ("BBRHCJW".to_string(), "HCJW".to_string()),
                    ("BBRHPJW".to_string(), "HPJW".to_string()),
                    ("BBRHCJR".to_string(), "HCJR".to_string()),
                    ("BBRHPJR".to_string(), "HPJR".to_string()),
                ]),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    client: rumqttc::v5::AsyncClient,
    config: Config,
    last_meters_pub: Option<std::time::Instant>,
    last_meter_len: Option<usize>,
}

impl Client {
    pub fn new(config: Config) -> (Self, rumqttc::v5::EventLoop) {
        let options = base::mqtt::make_options("linky", config.broker.clone());

        let (client, event_loop) = rumqttc::v5::AsyncClient::new(options, 10);

        (
            Self {
                client,
                config,
                last_meters_pub: None,
                last_meter_len: None,
            },
            event_loop,
        )
    }

    pub async fn publish(&mut self, tic_frame: Vec<(String, tic::Value)>) -> anyhow::Result<()> {
        for (field, value) in &tic_frame {
            if field == &self.config.power.tic_field {
                self.publish_power(value.clone()).await?;
                break;
            }
        }
        self.publish_meters(tic_frame).await?;
        Ok(())
    }

    async fn publish_power(&mut self, value: tic::Value) -> anyhow::Result<()> {
        let topic = &self.config.power.topic;
        log::debug!("Publishing power to MQTT: {} = {}", topic, value);

        let properties = PublishProperties {
            content_type: Some("text/plain".to_string()),
            message_expiry_interval: Some(5),
            ..Default::default()
        };

        self.client
            .publish_with_properties(topic, QoS::AtMostOnce, true, value.to_string(), properties)
            .await?;
        Ok(())
    }

    async fn publish_meters(&mut self, tic_frame: Vec<(String, tic::Value)>) -> anyhow::Result<()> {
        let now = std::time::Instant::now();
        if let Some(last_pub) = self.last_meters_pub {
            if now.duration_since(last_pub) < self.config.meters.min_interval {
                log::debug!("Skipping meters publish because min_interval not reached");
                return Ok(());
            }
        }

        let mut active = None;
        let mut meters = if let Some(len) = self.last_meter_len {
            VecMap::with_capacity(len)
        } else {
            VecMap::new()
        };

        for (tic_field, value) in tic_frame {
            if &tic_field == &self.config.meters.active_meter {
                let tic::Value::String(mut s) = value else {
                    log::warn!(
                        "Active meter field {} is not a string, got {:?}",
                        tic_field,
                        value
                    );
                    continue;
                };
                // Les indexs publiés par le TIC finissent par des "." (sauf pour Tempo)
                while s.ends_with('.') {
                    s.pop();
                }
                active = Some(s);
                continue;
            }

            if let Some(meter_key) = self.config.meters.meters.get(&tic_field) {
                let tic::Value::Integer(i) = value else {
                    log::warn!(
                        "Meter field {} is not an integer, got {:?}",
                        tic_field,
                        value
                    );
                    continue;
                };
                meters.push_no_check(meter_key.clone(), i as u32);
            }
        }

        if meters.is_empty() {
            log::warn!("No meter fields found in TIC frame, skipping MQTT publish");
            return Ok(());
        }

        let properties = PublishProperties {
            content_type: Some("text/plain".to_string()),
            message_expiry_interval: Some(self.config.meters.min_interval.as_secs() as u32 * 2),
            ..Default::default()
        };

        let topic = &self.config.meters.topic;
        let payload = mqtt::MetersPayload { active, meters };
        log::debug!("Publishing meters to MQTT: {} = {:?}", topic, payload);

        let payload = serde_json::to_vec(&payload)?;

        self.client
            .publish_with_properties(topic, QoS::AtMostOnce, true, payload, properties)
            .await?;
        self.last_meters_pub = Some(std::time::Instant::now());
        Ok(())
    }
}
