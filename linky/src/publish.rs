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
    contract: ContractConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
    topic_prefix: String,
    fields: Vec<String>,
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
            contract: ContractConfig {
                min_interval: Duration::from_hours(24),
                topic_prefix: "domotux/contrat".to_string(),
                fields: vec![
                    "ADCO".to_string(),
                    "OPTARIF".to_string(),
                    "ISOUSC".to_string(),
                    "IMAX".to_string(),
                ],
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
    last_contract_pub: Option<std::time::Instant>,
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
                last_contract_pub: None,
            },
            event_loop,
        )
    }

    pub async fn publish(&mut self, tic_frame: Vec<(String, tic::Value)>) -> anyhow::Result<()> {
        let now = std::time::Instant::now();

        let publish_contract = self
            .last_contract_pub
            .map(|last_pub| now.duration_since(last_pub) >= self.config.contract.min_interval)
            .unwrap_or(true);
        if !publish_contract {
            log::debug!("Skipping contract publish because min_interval not reached");
        }

        let publish_meters = self
            .last_meters_pub
            .map(|last_pub| now.duration_since(last_pub) >= self.config.meters.min_interval)
            .unwrap_or(true);
        if !publish_meters {
            log::debug!("Skipping meters publish because min_interval not reached");
        }

        let power_value = tic_frame
            .iter()
            .find(|(field, _)| field == &self.config.power.tic_field)
            .map(|(_, value)| value);

        let power_fut = async {
            if let Some(value) = power_value {
                self.publish_power(value).await?;
            }
            Ok::<(), anyhow::Error>(())
        };

        let contract_fut = async {
            if publish_contract {
                self.publish_contract(&tic_frame).await
            } else {
                Ok(false)
            }
        };

        let meters_fut = async {
            if publish_meters {
                self.publish_meters(&tic_frame, self.last_meter_len).await
            } else {
                Ok(None)
            }
        };

        let (_, contract_published, meter_len) =
            tokio::try_join!(power_fut, contract_fut, meters_fut)?;

        if contract_published {
            self.last_contract_pub = Some(std::time::Instant::now());
        }

        if let Some(meter_len) = meter_len {
            self.last_meter_len = Some(meter_len);
            self.last_meters_pub = Some(std::time::Instant::now());
        }

        Ok(())
    }

    async fn publish_power(&self, value: &tic::Value) -> anyhow::Result<()> {
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

    async fn publish_contract(&self, tic_frame: &[(String, tic::Value)]) -> anyhow::Result<bool> {
        let mut handles = Vec::new();

        for (tic_field, tic_value) in tic_frame {
            if self.config.contract.fields.contains(tic_field) {

                let topic = format!("{}/{}", self.config.contract.topic_prefix, tic_field);
                let value = tic_value.to_string();
                log::debug!("Publishing contract field to MQTT: {} = {}", topic, value);

                let properties = PublishProperties {
                    content_type: Some("text/plain".to_string()),
                    message_expiry_interval: Some(
                        self.config.contract.min_interval.as_secs() as u32 * 2,
                    ),
                    ..Default::default()
                };

                handles.push(tokio::spawn({
                    let client = self.client.clone();
                    async move {
                        if let Err(e) = client
                            .publish_with_properties(topic, QoS::AtMostOnce, true, value, properties)
                            .await
                        {
                            log::error!("Failed to publish contract field to MQTT: {}", e);
                        }
                    }
                }));
            }
        }

        if handles.is_empty() {
            log::warn!("No contract fields found in TIC frame, skipping MQTT publish");
            return Ok(false);
        }
        for handle in handles {
            handle.await?;
        }

        Ok(true)
    }

    async fn publish_meters(
        &self,
        tic_frame: &[(String, tic::Value)],
        last_meter_len: Option<usize>,
    ) -> anyhow::Result<Option<usize>> {
        let mut active = None;
        let mut meters = if let Some(len) = last_meter_len {
            VecMap::with_capacity(len)
        } else {
            VecMap::new()
        };

        for (tic_field, value) in tic_frame {
            if tic_field == &self.config.meters.active_meter {
                let tic::Value::String(mut s) = value.clone() else {
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
                meters.push_no_check(meter_key.clone(), *i as u32);
            }
        }

        if meters.is_empty() {
            log::warn!("No meter fields found in TIC frame, skipping MQTT publish");
            return Ok(None);
        }

        let meter_len = meters.len();

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
        Ok(Some(meter_len))
    }
}
