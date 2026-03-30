use std::{path::PathBuf, process, time::Duration};

use base::mqtt::topics::CompteurActif;
use base::mqtt::topics::Contrat;
use base::mqtt::topics::PrixKwhActif;
use base::mqtt::topics::Topic;
use clap::Parser;
use rumqttc::v5::Event;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::mqttbytes::v5::Packet;
use serde::{Deserialize, Serialize};
use tokio::sync;
use tokio::task;

use crate::cre::fetch_kwh_price;

mod cre;
mod tabular;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    broker: base::mqtt::BrokerAddress,
    #[serde(
        serialize_with = "base::cfg::serialize_seconds",
        deserialize_with = "base::cfg::deserialize_seconds"
    )]
    expiration: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: "localhost:1883".parse().unwrap(),
            expiration: Duration::from_secs(24 * 3600), // 24h
        }
    }
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    default_config: bool,

    #[clap(short, long)]
    config_file: Option<PathBuf>,

    #[clap(short, long)]
    broker: Option<String>,

    #[clap(short, long)]
    expiration: Option<u64>,
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

    let (tx, mut rx) = sync::mpsc::channel(10);

    let broker = cli
        .broker
        .map(|b| b.parse())
        .transpose()?
        .unwrap_or(config.broker);

    let expiration = cli
        .expiration
        .map(Duration::from_secs)
        .unwrap_or(config.expiration);

    let (client, ev_loop) = {
        let options = base::mqtt::make_options("tarifs-cre", broker.clone());
        rumqttc::v5::AsyncClient::new(options, 10)
    };

    client.subscribe(Contrat::topic(), QoS::AtMostOnce).await?;
    client
        .subscribe(CompteurActif::topic(), QoS::AtMostOnce)
        .await?;

    task::spawn(async move {
        if let Err(e) = msg_poll_loop(ev_loop, tx).await {
            log::error!("Error polling MQTT: {}", e);
        }
    });

    let mut last_pub: Option<tokio::time::Instant> = None;
    let mut prix_kwh = None;

    loop {
        let validity = expiration
            .checked_sub(last_pub.map(|lp| lp.elapsed()).unwrap_or_default())
            .unwrap_or(expiration);
        let sleep_fut = tokio::time::sleep(validity);
        let msg_fut = rx.recv();

        tokio::select! {
            msg = msg_fut => {
                match msg {
                    Some(Msg::Contrat(c)) => {
                        prix_kwh = fetch_kwh_price(&c, None).await?;
                        if let Some(prix_kwh) = &prix_kwh {
                            let now = tokio::time::Instant::now();
                            publish_topic(&client, prix_kwh).await?;
                            last_pub = Some(now);
                        }
                    }
                    Some(Msg::CompteurActif(ca)) => {
                        if let Some(prix_kwh) = &prix_kwh {
                            if let Some(val) = prix_kwh.0.get(&ca.0) {
                                publish_topic(&client, &PrixKwhActif(*val)).await?;
                            }
                        }
                    }
                    None => {
                        anyhow::bail!("Channel closed!");
                    }
                }
            }
            _ = sleep_fut => {}
        }
    }
}

#[derive(Debug, Clone)]
enum Msg {
    Contrat(Contrat),
    CompteurActif(CompteurActif),
}

async fn msg_poll_loop(
    mut ev_loop: rumqttc::v5::EventLoop,
    tx: sync::mpsc::Sender<Msg>,
) -> anyhow::Result<()> {
    let contrat_topic = Contrat::topic();
    let compteur_actif_topic = CompteurActif::topic();
    loop {
        match ev_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let topic = String::from_utf8_lossy(&publish.topic);
                log::debug!("Received MQTT message on topic '{}'", topic);
                if topic == contrat_topic {
                    log::info!("Received contrat update via MQTT");
                    if let Ok(contrat) = serde_json::from_slice::<Contrat>(&publish.payload) {
                        if let Err(e) = tx.send(Msg::Contrat(contrat.clone())).await {
                            log::error!("Failed to send contrat update: {}", e);
                            break;
                        }
                    } else {
                        log::error!("Failed to parse contrat from MQTT payload");
                    }
                } else if topic == compteur_actif_topic {
                    log::info!("Received compteur actif update via MQTT");
                    if let Ok(compteur_actif) =
                        serde_json::from_slice::<CompteurActif>(&publish.payload)
                    {
                        if let Err(e) = tx.send(Msg::CompteurActif(compteur_actif.clone())).await {
                            log::error!("Failed to send compteur actif update: {}", e);
                            break;
                        }
                    } else {
                        log::error!("Failed to parse compteur actif from MQTT payload");
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

async fn publish_topic<T: Topic + Serialize>(
    client: &rumqttc::v5::AsyncClient,
    msg: &T,
) -> anyhow::Result<()> {
    let topic = T::topic();
    let payload = serde_json::to_vec(msg)?;
    client
        .publish(topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}
