use std::process;

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync;
use tokio::time::{Duration, sleep};

mod mqtt;
mod tic;

/// Configuration for the Linky application
#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    tic: tic::Config,
    mqtt: mqtt::Config,
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    default_config: bool,

    #[clap(short, long)]
    config_file: Option<String>,
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

fn read_config<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T, anyhow::Error> {
    let config_contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path, e))?;
    let config: T = serde_yml::from_str(&config_contents)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path, e))?;
    Ok(config)
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        let example_config = Config::default();
        let yaml = serde_yml::to_string(&example_config)
            .map_err(|e| anyhow::anyhow!("YAML error: {}", e))?;
        println!("{}", yaml);
        return Ok(());
    }

    let config_file = cli
        .config_file
        .as_deref()
        .unwrap_or("/etc/linky.yml");

    let config: Config = read_config(config_file).unwrap_or_else(|_| Config::default());
    let Config {
        tic: tic_cfg,
        mqtt: mqtt_cfg,
    } = config;

    let mqtt_options = mqtt::make_options(&mqtt_cfg);
    log::debug!("MQTT options: {:?}", mqtt_options);
    let (mqtt_client, mut mqtt_evloop) = rumqttc::v5::AsyncClient::new(mqtt_options, 10);

    let (mpsc_tx, mut mpsc_rx) = sync::mpsc::channel(100);

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

    let mut tic_handle = tokio::spawn(async move { tic::read_frames(tic_cfg, mpsc_tx).await });

    loop {
        tokio::select! {
            Some(frame) = mpsc_rx.recv() => {
                log::debug!("Publishing frame: {:?}", frame);
                mqtt::publish_frame(&mqtt_client, &mqtt_cfg, &frame).await?;
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
