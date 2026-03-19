use rumqttc::v5::mqttbytes::{QoS, v5::PublishProperties};
use serde::{Deserialize, Serialize};

use crate::tic;


/// Configuration for the Linky reader
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Unique identifier for the device (e.g. "linky")
    device_id: String,
    /// Host MQTT broker
    host: String,
    /// Port of the MQTT broker
    port: u16,
    /// linky data topic (e.g. "domotux/linky")
    topic: String,
    /// QoS level for MQTT messages (0, 1, or 2)
    #[serde(serialize_with = "serde_qos", deserialize_with = "serde_qos::deserialize")]
    qos: QoS,
    /// Retain flag for MQTT messages
    retain: bool,
    /// Message expiry interval in seconds (optional)
    message_expiry: Option<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_id: "linky".to_string(),
            host: "domotux.lan".to_string(),
            port: 1883,
            topic: "domotux/linky".to_string(),
            qos: QoS::AtMostOnce,
            retain: true,
            message_expiry: Some(10),
        }
    }
}

fn serde_qos<S>(qos: &QoS, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let qos_value = match qos {
        QoS::AtMostOnce => 0,
        QoS::AtLeastOnce => 1,
        QoS::ExactlyOnce => 2,
    };
    serializer.serialize_u8(qos_value)
}

mod serde_qos {
    use super::*;
    pub fn deserialize<'de, D>(deserializer: D) -> Result<QoS, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let qos_value = u8::deserialize(deserializer)?;
        match qos_value {
            0 => Ok(QoS::AtMostOnce),
            1 => Ok(QoS::AtLeastOnce),
            2 => Ok(QoS::ExactlyOnce),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid QoS value: {}. Must be 0, 1, or 2.",
                qos_value
            ))),
        }
    }
}

pub fn make_options(cfg: &Config) -> rumqttc::v5::MqttOptions {
    let mut options = rumqttc::v5::MqttOptions::new(&cfg.device_id, &cfg.host, cfg.port);
    options.set_keep_alive(std::time::Duration::from_secs(10));
    options
}

pub async fn publish_frame(
    mqtt_client: &rumqttc::v5::AsyncClient,
    mqtt_cfg: &Config,
    frame: &tic::TicFrame,
) -> Result<(), anyhow::Error> {
    let payload = serde_json::to_string(frame)?;
    let props = PublishProperties {
        message_expiry_interval: mqtt_cfg.message_expiry,
        ..Default::default()
    };
    mqtt_client
        .publish_with_properties(&mqtt_cfg.topic, mqtt_cfg.qos, mqtt_cfg.retain, payload, props)
        .await?;
    Ok(())
}