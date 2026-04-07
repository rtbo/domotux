use clap::Parser;
use std::{path::PathBuf, process};

use mqtt::topics::{CouleurTempo, CouleurTempoAujourdhui, CouleurTempoDemain};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    broker: mqtt::BrokerAddress,
    aujourdhui: bool,
    demain: bool,
    fetch_times: Vec<base::DayTime>,
    retries: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: mqtt::BrokerAddress::default(),
            aujourdhui: true,
            demain: true,
            fetch_times: vec![
                base::DayTime::new(11, 1, 0).unwrap(),
                base::DayTime::new(20, 1, 0).unwrap(),
            ],
            retries: 5,
        }
    }
}

#[derive(Parser)]
struct Cli {
    #[clap(long)]
    default_config: bool,

    #[clap(long)]
    config: Option<PathBuf>,

    #[clap(short, long)]
    broker: Option<String>,

    #[clap(short, long)]
    aujourdhui: bool,

    #[clap(short, long)]
    demain: bool,

    #[clap(short, long)]
    retries: Option<u32>,

    #[clap(short, long)]
    times: Vec<String>,
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

async fn run(cli: Cli) -> anyhow::Result<()> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let mut config: Config = base::cfg::load_config("couleur-tempo", cli.config).await?;
    if let Some(broker) = cli.broker {
        config.broker = broker.parse()?;
    }
    if cli.aujourdhui {
        config.aujourdhui = true;
    }
    if cli.demain {
        config.demain = true;
    }
    if let Some(retries) = cli.retries {
        config.retries = retries;
    }
    if !cli.times.is_empty() {
        config.fetch_times = cli
            .times
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()?;
    }

    if config.fetch_times.is_empty() {
        return Err(anyhow::anyhow!("At least one fetch time must be specified"));
    }
    if !config.aujourdhui && !config.demain {
        return Err(anyhow::anyhow!(
            "At least one of aujourdhui or demain must be true"
        ));
    }
    log::info!("Starting with config: {:#?}", config);

    let mqtt_client = mqtt::Client::<()>::new("couleur-tempo", config.broker.clone());

    loop {
        if config.aujourdhui {
            let ajd =
                fetch_couleur_tempo_with_retries(TempoDay::Aujourdhui, config.retries).await?;
            log::info!("Couleur tempo aujourd'hui: {:?}", ajd);
            mqtt_client
                .publish(
                    &CouleurTempoAujourdhui(ajd.clone()),
                    mqtt::QoS::AtLeastOnce,
                    true,
                )
                .await?;
        }
        if config.demain {
            let demain = fetch_couleur_tempo_with_retries(TempoDay::Demain, config.retries).await?;
            log::info!("Couleur tempo demain: {:?}", demain);
            mqtt_client
                .publish(
                    &CouleurTempoDemain(demain.clone()),
                    mqtt::QoS::AtLeastOnce,
                    true,
                )
                .await?;
        }

        let sleep_dur = config
            .fetch_times
            .iter()
            .map(base::DayTime::duration_until)
            .min()
            .unwrap();
        log::info!("Sleeping for {} seconds", sleep_dur.as_secs());
        tokio::time::sleep(sleep_dur).await;
    }
}

#[derive(Debug, Clone, Copy)]
enum TempoDay {
    Aujourdhui,
    Demain,
}

async fn fetch_couleur_tempo_with_retries(
    day: TempoDay,
    retries: u32,
) -> anyhow::Result<Option<CouleurTempo>> {
    let mut attempts = 0;
    let timeout = std::time::Duration::from_secs(5);
    loop {
        match fetch_couleur_tempo(day).await {
            Ok(Some(couleur)) => return Ok(Some(couleur)),
            Ok(None) => {
                attempts += 1;
                if attempts > retries {
                    return Ok(None);
                }
                log::warn!(
                    "Attempt {}/{} did not return data. Retrying...",
                    attempts,
                    retries
                );
            }
            Err(e) => {
                attempts += 1;
                if attempts > retries {
                    return Err(e);
                }
                log::warn!(
                    "Attempt {}/{} failed: {}. Retrying...",
                    attempts,
                    retries,
                    e
                );
            }
        }
        tokio::time::sleep(timeout).await;
    }
}

async fn fetch_couleur_tempo(day: TempoDay) -> anyhow::Result<Option<CouleurTempo>> {
    let day = match day {
        TempoDay::Aujourdhui => "today",
        TempoDay::Demain => "tomorrow",
    };

    let url = format!("https://www.api-couleur-tempo.fr/api/jourTempo/{}", day);
    log::info!("Fetching {}", url);

    let response = reqwest::get(url).await?;
    log::debug!("Fetched data for {}: HTTP {}", day, response.status());
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to fetch data: HTTP {}",
            response.status()
        ));
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Resp {
        date_jour: String,
        code_jour: u32,
        periode: String,
        lib_couleur: String,
    }

    let resp: Resp = response.json().await?;
    log::debug!("Converted from JSON: {:?}", resp);

    let couleur_tempo = match resp.code_jour {
        1 => Some(CouleurTempo::Bleu),
        2 => Some(CouleurTempo::Blanc),
        3 => Some(CouleurTempo::Rouge),
        _ => None,
    };

    Ok(couleur_tempo)
}
