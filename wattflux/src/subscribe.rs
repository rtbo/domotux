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
    Power(mqtt::topics::AppPower),
    Meters(mqtt::topics::Meters),
}

#[derive(Debug, Clone)]
pub struct Client {
    client: rumqttc::v5::AsyncClient,
    power_topic: String,
    meters_topic: String,
}

impl Client {
    pub fn new(config: Config) -> (Self, rumqttc::v5::EventLoop) {
        let options = mqtt::make_options("wattflux", config.broker.clone());
        log::debug!("MQTT options: {:#?}", options);

        let (client, event_loop) = rumqttc::v5::AsyncClient::new(options, 10);
        let power_topic = mqtt::topics::AppPower::topic();
        let meters_topic = mqtt::topics::Meters::topic();
        (Self { client, power_topic, meters_topic }, event_loop)
    }

    pub async fn subscribe(&mut self) -> anyhow::Result<()> {
        self.client
            .subscribe(&self.power_topic, QoS::AtMostOnce)
            .await?;
        self.client
            .subscribe(&self.meters_topic, QoS::AtLeastOnce)
            .await?;
        Ok(())
    }

    pub async fn translate_event(&self, event: Event) -> anyhow::Result<Option<Msg>> {
        match event {
            Event::Incoming(Packet::Publish(msg)) => {
                let topic = std::str::from_utf8(&msg.topic)?;
                let payload = std::str::from_utf8(&msg.payload)?;

                if topic == self.power_topic {
                    let power = payload.trim().parse::<f32>().map(mqtt::topics::AppPower)?;
                    Ok(Some(Msg::Power(power)))
                } else if topic == self.meters_topic {
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
