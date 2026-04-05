use std::path::PathBuf;
use std::{process, str};

use clap::Parser;
use mqtt::topics::{Compteurs, PApp};
use mqtt::{self, QoS};
use serde::{Deserialize, Serialize};

mod influx;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MqttConfig {
    broker: mqtt::BrokerAddress,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    influx: influx::Config,
    mqtt: MqttConfig,
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

mqtt::subscribe_msg! {
    enum Msg {
        PApp(PApp),
        Compteurs(Compteurs),
    }
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let config: Config = base::cfg::load_config("wattflux", cli.config_file).await?;
    log::info!("Starting with config: {:#?}", config);

    let Config {
        mqtt: mqtt_cfg,
        influx: influx_cfg,
    } = config;

    let influx = influx::Client::new(influx_cfg);

    let mut mqtt_client = mqtt::Client::<Msg>::new("wattflux", mqtt_cfg.broker);
    mqtt_client.subscribe::<PApp>(QoS::AtMostOnce).await?;
    mqtt_client.subscribe::<Compteurs>(QoS::AtLeastOnce).await?;

    loop {
        if let Some(msg) = mqtt_client.recv().await {
            let res = match msg {
                Msg::PApp(power) => influx.write_papp_line(power).await,
                Msg::Compteurs(meters) => influx.write_compteurs_line(meters).await,
            };
            if let Err(e) = res {
                log::error!("Failed to publish to InfluxDB: {}", e);
            }
        }
    }
}
