use base::mqtt;
use rumqttc::v5::{
    Event,
    mqttbytes::{QoS, v5::Packet},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: base::mqtt::BrokerAddress,
    power: String,
    meters: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: Default::default(),
            power: "domotux/papp".to_string(),
            meters: "domotux/compteurs".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Power(i32),
    Meters(mqtt::MetersPayload),
}

#[derive(Debug, Clone)]
pub struct Client {
    client: rumqttc::v5::AsyncClient,
    config: Config,
}

impl Client {
    pub fn new(config: Config) -> (Self, rumqttc::v5::EventLoop) {
        let options = mqtt::make_options("wattflux", config.broker.clone());
        log::debug!("MQTT options: {:#?}", options);

        let (client, event_loop) = rumqttc::v5::AsyncClient::new(options, 10);
        (Self { client, config }, event_loop)
    }

    pub async fn subscribe(&mut self) -> anyhow::Result<()> {
        self.client
            .subscribe(&self.config.power, QoS::AtMostOnce)
            .await?;
        self.client
            .subscribe(&self.config.meters, QoS::AtLeastOnce)
            .await?;
        Ok(())
    }

    pub async fn translate_event(&self, event: Event) -> anyhow::Result<Option<Msg>> {
        match event {
            Event::Incoming(Packet::Publish(msg)) => {
                let topic = std::str::from_utf8(&msg.topic)?;
                let payload = std::str::from_utf8(&msg.payload)?;

                if topic == self.config.power {
                    let power = payload.trim().parse()?;
                    Ok(Some(Msg::Power(power)))
                } else if topic == self.config.meters {
                    let meters_payload = serde_json::from_str(payload)?;
                    Ok(Some(Msg::Meters(meters_payload)))
                } else {
                    log::warn!("Received message on unknown topic: {}", topic);
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}
