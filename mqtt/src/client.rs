use rumqttc::v5::{
    Event,
    mqttbytes::{QoS, v5::Packet},
};
use serde::Serialize;
use tokio::{sync, task};

use crate::Topic;

use super::{BrokerAddress, SubscribeMsg};

#[derive(Debug)]
pub struct Client<S> {
    client: rumqttc::v5::AsyncClient,
    ev_loop_handle: task::JoinHandle<anyhow::Result<()>>,
    rx: sync::mpsc::Receiver<S>,
}

impl<S> Client<S>
where
    S: SubscribeMsg + Send + 'static,
{
    pub fn new(dev_base: &str, broker: BrokerAddress) -> Self {
        let options = make_options(dev_base, broker);
        log::debug!("MQTT options: {:#?}", options);

        let (client, ev_loop) = rumqttc::v5::AsyncClient::new(options, 10);
        let (tx, rx) = sync::mpsc::channel(10);

        let ev_loop_handle = task::spawn(msg_poll_loop(ev_loop, tx));

        Self {
            client,
            ev_loop_handle,
            rx,
        }
    }

    pub async fn subscribe<U>(&self, qos: QoS) -> anyhow::Result<()>
    where
        U: Topic + Serialize,
    {
        let topic = U::topic();
        debug_assert!(
            S::topics().contains(&topic),
            "Subscribing to topic '{}' which isn't in the list of topics for this client",
            topic
        );
        self.client.subscribe(topic, qos).await?;
        Ok(())
    }

    pub async fn subscribe_all(&self, qos: QoS) -> anyhow::Result<()> {
        for topic in S::topics() {
            self.client.subscribe(topic, qos).await?;
        }
        Ok(())
    }

    pub async fn recv(&mut self) -> Option<S> {
        self.rx.recv().await
    }

    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.client.disconnect().await?;
        self.ev_loop_handle.await??;
        Ok(())
    }
}

impl<S> Client<S> {
    /// Publish a message of type P (which must implement Topic) to the MQTT broker
    /// The topic is determined by P::topic() and the payload is the JSON serialization of the message
    /// P doesn't have to be one of the topics that this client is subscribed to.
    pub async fn publish<P>(&self, msg: &P, qos: QoS, retain: bool) -> anyhow::Result<()>
    where
        P: Topic + Serialize,
    {
        let topic = P::topic();
        let payload = serde_json::to_vec(msg)?;
        self.client.publish(topic, qos, retain, payload).await?;
        Ok(())
    }
}

async fn msg_poll_loop<S: SubscribeMsg>(
    mut ev_loop: rumqttc::v5::EventLoop,
    tx: sync::mpsc::Sender<S>,
) -> anyhow::Result<()> {
    loop {
        match ev_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let topic = String::from_utf8_lossy(&publish.topic);
                let payload = &publish.payload;
                log::debug!("Received MQTT message on topic '{}'", topic);
                match S::translate(&topic, payload) {
                    Ok(Some(msg)) => {
                        if let Err(e) = tx.send(msg).await {
                            log::error!("Failed to send MQTT message to channel: {}", e);
                            break Err(anyhow::anyhow!(
                                "Failed to send MQTT message to channel: {}",
                                e
                            ));
                        }
                    }
                    Ok(None) => {
                        log::warn!("Received MQTT message on unrecognized topic '{}'", topic);
                    }
                    Err(e) => {
                        log::error!("Failed to parse MQTT message: {}", e);
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                log::error!("MQTT event loop error: {}", e);
                return Err(anyhow::anyhow!(e));
            }
        }
    }
}

/// Make MQTT options from a base client ID and a broker address
fn make_options(dev_base: &str, broker: BrokerAddress) -> rumqttc::v5::MqttOptions {
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
