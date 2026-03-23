use rumqttc::v5::{
    Event,
    mqttbytes::{QoS, v5::Packet},
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    /// Subscribed power in KVA
    /// Should be one of 6, 9, 12, 15, 18, 30, 36
    pub psousc: u32,
    /// Type of contract (ex: "base", "tempo", "hphc")
    pub optarif: String,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            psousc: 9,
            optarif: "tempo".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Config {
    Mqtt {
        broker: base::mqtt::BrokerAddress,
        topic: String,
    },
    Static(Contract),
}

impl Config {
    pub fn static_or_default(&self) -> Contract {
        match self {
            Config::Mqtt { .. } => {
                log::info!("Initializing to default 9kVA tempo contract. Will be updated if MQTT messages are received.");
                Contract::default()
            }
            Config::Static(contract) => contract.clone(),
        }
    }

    pub async fn subscribe_to_changes(
        &self,
        state: Arc<Mutex<crate::AppState>>,
    ) -> anyhow::Result<()> {
        if let Config::Mqtt {
            broker,
            topic,
        } = self
        {
            let options = base::mqtt::make_options("tarifs-cre", broker.clone());
            let (client, mut event_loop) = rumqttc::v5::AsyncClient::new(options, 10);
            client.subscribe(topic.to_string(), QoS::AtMostOnce).await?;

            tokio::spawn(async move {
                loop {
                    match event_loop.poll().await {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let topic = String::from_utf8_lossy(&publish.topic);
                            log::info!("Received MQTT message on topic '{}'", topic);
                            if topic.ends_with("ISOUSC") {
                                let payload = String::from_utf8_lossy(&publish.payload);
                                if let Ok(isousc) = payload.parse::<u32>() {
                                    let mut state = state.lock().await;
                                    state.contract.psousc = isousc * 200 / 1000;
                                    log::info!("Updated subscribed power to {} kVA", state.contract.psousc);
                                } else {
                                    log::warn!("Invalid ISOUSC value: {}", payload);
                                }
                            } else if topic.ends_with("OPTARIF") {
                                let payload = String::from_utf8_lossy(&publish.payload);
                                let optarif = if payload == "BASE" {
                                    "base".to_string()
                                } else if payload.starts_with("HC") {
                                    "hphc".to_string()
                                } else if payload.starts_with("BBR") {
                                    "tempo".to_string()
                                } else {
                                    log::warn!("Unknown OPTARIF value: {}", payload);
                                    continue;
                                };
                                let mut state = state.lock().await;
                                state.contract.optarif = optarif.clone();
                                log::info!("Updated contract type to '{}'", optarif);
                            }
                        }
                        Ok(_) => {}
                        Err(e) => log::error!("MQTT event loop error: {}", e),
                    }
                }
            });
        }

        Ok(())
    }
}
