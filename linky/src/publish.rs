use std::{collections::HashMap, time::Duration};

use base::{
    mqtt::{self, topics::Topic},
    vecmap::VecMap,
};
use rumqttc::v5::mqttbytes::{QoS, v5::PublishProperties};
use serde::{Deserialize, Serialize};

use crate::tic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: base::mqtt::BrokerAddress,
    compteurs: CompteursConfig,
    contrat: ContratConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompteursConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContratConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    min_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: "domotux.lan".parse().unwrap(),
            compteurs: CompteursConfig {
                min_interval: Duration::from_secs(60),
            },
            contrat: ContratConfig {
                min_interval: Duration::from_hours(24),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    client: rumqttc::v5::AsyncClient,
    config: Config,
    meters_map: HashMap<&'static str, &'static str>,
    ptec_map: HashMap<&'static str, &'static str>,
    last_meters_pub: Option<std::time::Instant>,
    last_meter_len: Option<usize>,
    last_contract_pub: Option<std::time::Instant>,
}

impl Client {
    pub fn new(config: Config) -> (Self, rumqttc::v5::EventLoop) {
        let options = base::mqtt::make_options("linky", config.broker.clone());

        let (client, event_loop) = rumqttc::v5::AsyncClient::new(options, 10);

        let ptec_map = HashMap::from([
            ("TH..", "th"),
            ("HC..", "hc"),
            ("HP..", "hp"),
            ("HCJB", "bleuHc"),
            ("HPJB", "bleuHp"),
            ("HCJW", "blancHc"),
            ("HPJW", "blancHp"),
            ("HCJR", "rougeHc"),
            ("HPJR", "rougeHp"),
        ]);
        let meters_map = HashMap::from([
            ("BASE", "base"),
            ("HCHC", "hc"),
            ("HCHP", "hp"),
            ("BBRHCJB", "bleuHc"),
            ("BBRHPJB", "bleuHp"),
            ("BBRHCJW", "blancHc"),
            ("BBRHPJW", "blancHp"),
            ("BBRHCJR", "rougeHc"),
            ("BBRHPJR", "rougeHp"),
        ]);

        (
            Self {
                client,
                config,
                ptec_map,
                meters_map,
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
            .map(|last_pub| now.duration_since(last_pub) >= self.config.contrat.min_interval)
            .unwrap_or(true);
        if !publish_contract {
            log::debug!("Skipping contract publish because min_interval not reached");
        }

        let publish_meters = self
            .last_meters_pub
            .map(|last_pub| now.duration_since(last_pub) >= self.config.compteurs.min_interval)
            .unwrap_or(true);
        if !publish_meters {
            log::debug!("Skipping meters publish because min_interval not reached");
        }

        let power_value = tic_frame
            .iter()
            .find(|(field, _)| field == "PAPP")
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
        let topic = mqtt::topics::PApp::topic();
        log::debug!("Publishing power to MQTT: {} = {}", topic, value);
        let msg = mqtt::topics::PApp(
            value
                .as_f32()
                .ok_or_else(|| anyhow::anyhow!("Power value is not a number, got {:?}", value))?,
        );
        let payload = serde_json::to_vec(&msg)?;

        let properties = PublishProperties {
            content_type: Some("text/plain".to_string()),
            message_expiry_interval: Some(5),
            ..Default::default()
        };

        self.client
            .publish_with_properties(topic, QoS::AtMostOnce, true, payload, properties)
            .await?;
        Ok(())
    }

    async fn publish_contract(&self, tic_frame: &[(String, tic::Value)]) -> anyhow::Result<bool> {
        let isousc = tic_frame
            .iter()
            .find(|(field, _)| field == "ISOUSC")
            .map(|(_, value)| value);

        let optarif = tic_frame
            .iter()
            .find(|(field, _)| field == "OPTARIF")
            .map(|(_, value)| value);

        let Some(isousc) = isousc.and_then(|v| v.as_u32()) else {
            log::warn!("ISOUSC field not found in TIC frame, skipping contract publish");
            return Ok(false);
        };

        let Some(optarif) = optarif.map(|v| v.to_string()) else {
            log::warn!("OPTARIF field not found in TIC frame, skipping contract publish");
            return Ok(false);
        };
        let optarif = optarif.as_str().trim();

        let option = if optarif == "BASE" {
            "base"
        } else if optarif.starts_with("HC") {
            "hchp"
        } else if optarif.starts_with("BBR") {
            "tempo"
        } else {
            log::warn!(
                "Unknown OPTARIF value '{}', skipping contract publish",
                optarif
            );
            return Ok(false);
        };

        let contract = mqtt::topics::Contrat {
            subsc_power: Some(isousc * 200 / 1000),
            option: Some(option.to_string()),
        };

        let topic = mqtt::topics::Contrat::topic();
        let payload = serde_json::to_vec(&contract)?;

        log::debug!(
            "Publishing contract field to MQTT: {} = {:?}",
            topic,
            contract
        );

        let properties = PublishProperties {
            content_type: Some("text/plain".to_string()),
            message_expiry_interval: Some(self.config.contrat.min_interval.as_secs() as u32 * 2),
            ..Default::default()
        };

        self.client
            .publish_with_properties(topic, QoS::AtLeastOnce, true, payload, properties)
            .await?;
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
            if tic_field == "PTEC" {
                let tic::Value::String(s) = value.clone() else {
                    log::warn!(
                        "PTEC {} is not a string, got {:?}",
                        tic_field,
                        value
                    );
                    continue;
                };
                let ptec_key = self
                    .ptec_map
                    .get(s.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Unknown PTEC value '{}'", s))?;
                active = Some(ptec_key.to_string());
                continue;
            }

            if let Some(meter_key) = self.meters_map.get(tic_field.as_str()) {
                let tic::Value::Integer(i) = value else {
                    log::warn!(
                        "Meter field {} is not an integer, got {:?}",
                        tic_field,
                        value
                    );
                    continue;
                };
                meters.push_no_check(meter_key.to_string(), *i as u32);
            }
        }

        if meters.is_empty() {
            log::warn!("No meter fields found in TIC frame, skipping MQTT publish");
            return Ok(None);
        }

        let meter_len = meters.len();

        let properties = PublishProperties {
            content_type: Some("text/plain".to_string()),
            message_expiry_interval: Some(self.config.compteurs.min_interval.as_secs() as u32 * 2),
            ..Default::default()
        };

        let msg = mqtt::topics::Compteurs { active, compteurs: meters };
        let topic = mqtt::topics::Compteurs::topic();
        log::debug!("Publishing meters to MQTT: {} = {:?}", topic, msg);

        let payload = serde_json::to_vec(&msg)?;

        self.client
            .publish_with_properties(topic, QoS::AtMostOnce, true, payload, properties)
            .await?;
        Ok(Some(meter_len))
    }
}
