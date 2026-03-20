use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process, str};
use tokio::sync;

mod influx;
mod subscribe;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    influx: influx::Config,
    subscribe: subscribe::Config,
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

    let config: Config = base::cfg::load_config("wattflux", cli.config_file).await?;
    log::info!("Starting with config: {:#?}", config);

    let Config {
        subscribe: mqtt_cfg,
        influx: influx_cfg,
    } = config;

    let influx = influx::Client::new(influx_cfg);

    let (mut mqtt_client, mqtt_evloop) = subscribe::Client::new(mqtt_cfg.clone());
    mqtt_client.subscribe().await?;

    let (tx, mut rx) = sync::mpsc::channel(100);

    base::mqtt::spawn_event_loop(mqtt_evloop, tx);

    loop {
        if let Some(event) = rx.recv().await {
            let msg = match mqtt_client.translate_event(event).await {
                Ok(Some(m)) => m,
                Ok(None) => continue,
                Err(e) => {
                    log::warn!("Failed to translate MQTT message: {}", e);
                    continue;
                }
            };
            log::debug!("Received MQTT message {msg:#?}");
            let res = match msg {
                subscribe::Msg::Power(power) => influx.write_power_line(power).await,
                subscribe::Msg::Meters(meters) => influx.write_meters_line(meters).await,
            };
            if let Err(e) = res {
                log::error!("Failed to publish to InfluxDB: {}", e);
            }
        }
    }
}
