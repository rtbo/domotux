use base::mqtt::{self, topics::Topic};
use rumqttc::v5::{
    Event,
    mqttbytes::{QoS, v5::Packet},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: base::mqtt::BrokerAddress,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    PApp(mqtt::topics::PApp),
    Compteurs(mqtt::topics::Compteurs),
}

#[derive(Debug, Clone)]
pub struct Client {
    client: rumqttc::v5::AsyncClient,
    papp_topic: String,
    compteurs_topic: String,
}

impl Client {
    pub fn new(config: Config) -> (Self, rumqttc::v5::EventLoop) {
        let options = mqtt::make_options("wattflux", config.broker.clone());
        log::debug!("MQTT options: {:#?}", options);

        let (client, event_loop) = rumqttc::v5::AsyncClient::new(options, 10);
        let papp_topic = mqtt::topics::PApp::topic();
        let compteurs_topic = mqtt::topics::Compteurs::topic();
        (Self { client, papp_topic, compteurs_topic }, event_loop)
    }

    pub async fn subscribe(&mut self) -> anyhow::Result<()> {
        self.client
            .subscribe(&self.papp_topic, QoS::AtMostOnce)
            .await?;
        self.client
            .subscribe(&self.compteurs_topic, QoS::AtLeastOnce)
            .await?;
        Ok(())
    }

    pub async fn translate_event(&self, event: Event) -> anyhow::Result<Option<Msg>> {
        match event {
            Event::Incoming(Packet::Publish(msg)) => {
                let topic = std::str::from_utf8(&msg.topic)?;
                let payload = std::str::from_utf8(&msg.payload)?;

                if topic == self.papp_topic {
                    let papp = payload.trim().parse::<f32>().map(mqtt::topics::PApp)?;
                    Ok(Some(Msg::PApp(papp)))
                } else if topic == self.compteurs_topic {
                    let compteurs = serde_json::from_str(payload)?;
                    Ok(Some(Msg::Compteurs(compteurs)))
                } else {
                    log::warn!("Received message on unknown topic: {}", topic);
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}
