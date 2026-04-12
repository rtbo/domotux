use std::path::PathBuf;
use std::process;
use std::time::Duration;

use clap::Parser;
use mqtt;
use mqtt::QoS;
use mqtt::topics::{CompteurActif, Contrat, PrixKwhActif};
use serde::{Deserialize, Serialize};

use tarifs_cre::fetch_kwh_price;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    broker: mqtt::BrokerAddress,
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

mqtt::subscribe_msg! {
    enum Msg {
        Contrat(Contrat),
        CompteurActif(CompteurActif),
    }
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let config: Config = base::cfg::load_config("tarifs-cre", cli.config_file).await?;
    log::info!("Starting with config: {:#?}", config);

    let broker = cli
        .broker
        .map(|b| b.parse())
        .transpose()?
        .unwrap_or(config.broker);

    let expiration = cli
        .expiration
        .map(Duration::from_secs)
        .unwrap_or(config.expiration);

    let mut client = mqtt::Client::<Msg>::new("tarifs-cre", broker.clone());

    client.subscribe_all(QoS::AtMostOnce).await?;

    let mut last_pub: Option<tokio::time::Instant> = None;
    let mut prix_kwh = None;
    let mut compteur_actif = None;
    let mut prix_kwh_actif: Option<PrixKwhActif> = None;

    loop {
        let validity = expiration
            .checked_sub(last_pub.map(|lp| lp.elapsed()).unwrap_or_default())
            .unwrap_or(expiration);
        let sleep_fut = tokio::time::sleep(validity);
        let msg_fut = client.recv();

        tokio::select! {
            msg = msg_fut => {
                match msg {
                    Some(Msg::Contrat(c)) => {
                        prix_kwh = fetch_kwh_price(&c, None).await?;
                    }
                    Some(Msg::CompteurActif(ca)) => {
                        compteur_actif = Some(ca);
                    }
                    None => {
                        anyhow::bail!("Channel closed!");
                    }
                }
            }
            _ = sleep_fut => {}
        }

        let now = tokio::time::Instant::now();

        if let Some(prix_kwh) = &prix_kwh {
            client.publish(prix_kwh, QoS::AtLeastOnce, true).await?;
            if let Some(ca) = &compteur_actif {
                if let Some(val) = prix_kwh.0.get(&ca.0) {
                    if let Some(prix_kwh_actif) = &prix_kwh_actif {
                        if prix_kwh_actif.0 == *val {
                            continue;
                        }
                    }
                    client
                        .publish(&PrixKwhActif(*val), QoS::AtLeastOnce, true)
                        .await?;
                    prix_kwh_actif = Some(PrixKwhActif(*val));
                }
            }
        }

        last_pub = Some(now);
    }
}
