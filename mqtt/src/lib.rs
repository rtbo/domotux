use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

pub mod client;
pub mod topics;

pub use rumqttc::v5::mqttbytes::QoS;

pub trait Topic {
    fn topic() -> &'static str;
}

pub use client::Client;

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

/// A trait for messages that can be received from MQTT topics.
pub trait SubscribeMsg {
    /// The list of MQTT topics that this message type can be translated from.
    /// The client will subscribe to these topics and attempt to translate incoming messages
    /// into this type.
    fn topics() -> Vec<&'static str>;

    /// Translate an MQTT event (topic + payload) into a message of this type, if the topic matches
    /// Returns Ok(None) if the topic isn't recognized, Ok(Some(msg)) if it is,
    /// and Err(e) if there was an error parsing the payload
    fn translate(topic: &str, payload: &[u8]) -> anyhow::Result<Option<Self>>
    where
        Self: Sized;
}

/// Implement SubscribeMsg for the unit type, which effectively means "no messages"
/// Use this when you need a client for publishing only.
impl SubscribeMsg for () {
    fn topics() -> Vec<&'static str> {
        vec![]
    }

    fn translate(_topic: &str, _payload: &[u8]) -> anyhow::Result<Option<Self>> {
        Ok(None)
    }
}

#[macro_export]
macro_rules! subscribe_msg {
    (@topic [$ty:ty] <= $topic:expr) => {
        $topic
    };

    (@topic [$ty:ty]) => {
        <$ty as $crate::Topic>::topic()
    };

    (enum $name:ident { $($variant:ident($ty:ty) $(<= $topic:expr)?),* $(,)? }) => {
        #[derive(Debug, Clone)]
        pub enum $name {
            $($variant($ty)),*
        }

        impl $crate::SubscribeMsg for $name {
            fn topics() -> Vec<&'static str> {
                vec![$($crate::subscribe_msg!(@topic [$ty] $(<= $topic)?)),*]
            }

            fn translate(topic: &str, payload: &[u8]) -> anyhow::Result<Option<Self>> {
                $(if topic == $crate::subscribe_msg!(@topic [$ty] $(<= $topic)?) {
                        let msg = serde_json::from_slice::<$ty>(payload)
                            .map_err(|e| anyhow::anyhow!("Failed to parse MQTT message for topic '{}': {}", $crate::subscribe_msg!(@topic [$ty] $(<= $topic)?), e))?;
                        return Ok(Some(Self::$variant(msg)));
                    })*

                Ok(None)
            }
        }
    };
}
