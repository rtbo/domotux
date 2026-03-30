use std::path::PathBuf;
use std::process;

use base::mqtt;
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync;
use tokio::time::{Duration, sleep};

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

async fn tests() {
    let papp = mqtt::topics::AppPower(830.0);
    let payload = serde_json::to_string(&papp).unwrap();
    println!("Serialized AppPower: {}", payload);
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    tests().await;
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let config: Config = base::cfg::load_config("linky", cli.config_file).await?;
    let Config {
        tic: tic_cfg,
        publish: publish_cfg,
    } = config;

    let (mut mqtt_client, mut mqtt_evloop) = publish::Client::new(publish_cfg);

    let (mpsc_tx, mut mpsc_rx) = sync::mpsc::channel(100);
    let mut tic_handle = tokio::spawn(async move { tic::read_loop(tic_cfg, mpsc_tx).await });


    let mut mqtt_handle = tokio::spawn(async move {
        loop {
            match mqtt_evloop.poll().await {
                Ok(notification) => {
                    log::debug!("MQTT event: {:?}", notification);
                }
                Err(e) => {
                    log::warn!("MQTT poll error: {}", e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(tic_frame) = mpsc_rx.recv() => {
                log::debug!("Received TIC update: {:?}", tic_frame);
                if let Err(e) = mqtt_client.publish(tic_frame).await {
                    log::warn!("Failed to publish MQTT message: {}", e);
                }
            }
            result = &mut tic_handle => {
                match result {
                    Ok(Ok(())) => return Err(anyhow::anyhow!("TIC reader terminated unexpectedly")),
                    Ok(Err(e)) => return Err(anyhow::anyhow!("TIC reader error: {}", e)),
                    Err(e) => return Err(anyhow::anyhow!("Task panicked: {}", e)),
                }
            }
            result = &mut mqtt_handle => {
                match result {
                    Ok(_) => return Err(anyhow::anyhow!("MQTT event loop terminated unexpectedly")),
                    Err(e) => return Err(anyhow::anyhow!("MQTT task panicked: {}", e)),
                }
            }
        }
    }
}
