use std::{path::PathBuf, process, time::Duration};

use base::mqtt::topics::Contrat;
use base::mqtt::topics::PrixKwh;
use base::mqtt::topics::Topic;
use clap::Parser;
use rumqttc::v5::Event;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::mqttbytes::v5::PublishProperties;
use serde::{Deserialize, Serialize};
use tokio::sync;
use tokio::task;

mod cre;
mod tabular;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreDbConfig {
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    validity: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ContractConfig {
    Mqtt,
    Static(Contrat),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    broker: base::mqtt::BrokerAddress,
    contract: ContractConfig,
    cre_db: CreDbConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: "localhost".parse().unwrap(),
            contract: ContractConfig::Mqtt,
            cre_db: CreDbConfig {
                validity: Duration::from_secs(24 * 3600), // 24h
            },
        }
    }
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    default_config: bool,

    #[clap(short, long)]
    config_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> process::ExitCode {
    env_logger::init();

    let cli = Cli::parse();

    match run(cli).await {
        Ok(_) => process::ExitCode::SUCCESS,
        Err(e) => {
            log::error!("Error: {}", e);
            process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let config: Config = base::cfg::load_config("tarifs-cre", cli.config_file).await?;
    log::info!("Starting with config: {:#?}", config);

    let initial_contract = match &config.contract {
        ContractConfig::Mqtt => {
            log::debug!(
                "Initializing to default 9kVA tempo contract. Will be updated if MQTT messages are received."
            );
            Contrat {
                subsc_power: Some(9),
                option: Some("tempo".to_string()),
            }
        }
        ContractConfig::Static(contract) => contract.clone(),
    };

    let (tx, mut rx) = sync::watch::channel(initial_contract);

    let (client, mut ev_loop) = {
        let options = base::mqtt::make_options("tarifs-cre", config.broker.clone());
        rumqttc::v5::AsyncClient::new(options, 10)
    };

    // Contract update loop via MQTT
    let mut contract = match &config.contract {
        ContractConfig::Mqtt => {
            log::info!("Subscribing to MQTT for contract updates");
            let contract_topic = Contrat::topic();
            client
                .subscribe(contract_topic.clone(), QoS::AtMostOnce)
                .await?;
            task::spawn(async move {
                if let Err(e) = contract_poll_loop(ev_loop, tx).await {
                    log::error!("Error polling MQTT: {}", e);
                }
            });
            rx.changed().await?;
            rx.borrow_and_update().clone()
        }
        ContractConfig::Static(contract) => {
            task::spawn(async move {
                loop {
                    match ev_loop.poll().await {
                        Err(e) => {
                            log::error!("MQTT event loop error: {}", e);
                            break;
                        }
                        Ok(_) => {}
                    }
                }
            });
            contract.clone()
        }
    };

    let validity = config.cre_db.validity;

    loop {
        fetch_and_publish_kwh_price(&client, &contract, 2 * validity).await?;
        let last_pub = tokio::time::Instant::now();

        let changed = rx.changed();
        let elapsed = last_pub.elapsed();
        let validity_fut = tokio::time::sleep(validity - elapsed);
        tokio::select! {
            _ = changed => {
                contract = rx.borrow().clone();
                log::info!("Contract updated: {:#?}", contract);
            }
            _ = validity_fut => {}
        }
    }
}

async fn contract_poll_loop(
    mut ev_loop: rumqttc::v5::EventLoop,
    tx: sync::watch::Sender<Contrat>,
) -> anyhow::Result<()> {
    let contract_topic = Contrat::topic();
    loop {
        match ev_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let topic = String::from_utf8_lossy(&publish.topic);
                log::info!("Received MQTT message on topic '{}'", topic);
                if topic == contract_topic {
                    log::info!("Received full contract update via MQTT");
                    if let Ok(contract) = serde_json::from_slice::<Contrat>(&publish.payload) {
                        if let Err(e) = tx.send(contract.clone()) {
                            log::error!("Failed to send contract update: {}", e);
                            break;
                        } else {
                            log::info!("Updated contract to: {:#?}", contract);
                        }
                    } else {
                        log::warn!("Failed to parse contract from MQTT payload");
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                log::error!("MQTT event loop error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

async fn fetch_and_publish_kwh_price(
    client: &rumqttc::v5::AsyncClient,
    contract: &Contrat,
    validity: Duration,
) -> anyhow::Result<()> {
    match cre::fetch_kwh_price(contract, None).await? {
        Some(kwh_price) => {
            log::info!("Fetched kWh price: {:#?}", kwh_price);
            publish_kwh_price(client, kwh_price, validity).await?;
        }
        None => {
            log::warn!("Failed to fetch kWh price for contract {:#?}", contract);
        }
    }
    Ok(())
}

async fn publish_kwh_price(
    client: &rumqttc::v5::AsyncClient,
    msg: PrixKwh,
    validity: Duration,
) -> anyhow::Result<()> {
    let topic = PrixKwh::topic();
    let payload = serde_json::to_vec(&msg)?;
    let props = PublishProperties {
        message_expiry_interval: Some(validity.as_secs() as u32),
        ..Default::default()
    };
    client
        .publish_with_properties(topic, QoS::AtMostOnce, false, payload, props)
        .await?;
    Ok(())
}
