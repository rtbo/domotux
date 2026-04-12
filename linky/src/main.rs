use std::path::PathBuf;
use std::process;

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync;

mod publish;
mod tic;

/// Configuration for the Linky application
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    tic: tic::Config,
    publish: publish::Config,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tic: tic::Config::default(),
            publish: publish::Config::default(),
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

    let config: Config = base::cfg::load_config("linky", cli.config_file).await?;
    let Config {
        tic: tic_cfg,
        publish: publish_cfg,
    } = config;

    let mut mqtt_client = publish::Client::new(publish_cfg.clone());

    let (tx, mut rx) = sync::mpsc::channel(100);
    let mut tic_handle = tokio::spawn(async move { tic::read_loop(tic_cfg, tx).await });

    loop {
        tokio::select! {
            Some(tic_frame) = rx.recv() => {
                log::debug!("Received TIC frame");
                if let Err(e) = mqtt_client.publish(&tic_frame).await {
                    log::warn!("Failed to publish MQTT message: {}", e);
                    log::warn!("Attempting to reconnect MQTT client...");
                    mqtt_client = publish::Client::new(publish_cfg.clone());
                    if let Err(e) = mqtt_client.publish(&tic_frame).await {
                        log::error!("Failed to publish MQTT message after reconnect: {}", e);
                    } else {
                        log::info!("Successfully published MQTT message after reconnect");
                    }
                }
            }
            result = &mut tic_handle => {
                match result {
                    Ok(Ok(())) => return Err(anyhow::anyhow!("TIC reader terminated unexpectedly")),
                    Ok(Err(e)) => return Err(anyhow::anyhow!("TIC reader error: {}", e)),
                    Err(e) => return Err(anyhow::anyhow!("Task panicked: {}", e)),
                }
            }
        }
    }
}
