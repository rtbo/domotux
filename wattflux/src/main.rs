use std::collections::VecDeque;
use std::path::PathBuf;
use std::{process, str};

use clap::Parser;
use mqtt::topics::{Compteurs, PApp, WattFluxDrain};
use mqtt::{self, QoS};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    broker: mqtt::BrokerAddress,
    papp_buffer: Option<usize>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker: mqtt::BrokerAddress::default(),
            papp_buffer: Some(10),
        }
    }
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

    #[clap(long)]
    mqtt_broker: Option<String>,

    #[clap(long)]
    influx_host: Option<String>,

    #[clap(long)]
    papp_buffer: Option<usize>,
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
        WattFluxDrain(Option<WattFluxDrain>) <= "domotux/wattflux/drain",
    }
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let mut config: Config = base::cfg::load_config("wattflux", cli.config_file).await?;
    if let Some(mqtt_broker) = cli.mqtt_broker {
        config.mqtt.broker = mqtt_broker.parse()?;
    }
    if let Some(influx_host) = cli.influx_host {
        config.influx.host = influx_host;
    }
    if let Some(papp_buffer) = cli.papp_buffer {
        config.mqtt.papp_buffer = Some(papp_buffer);
    }
    log::info!("Starting with config: {:#?}", config);

    let Config {
        mqtt: mqtt_cfg,
        influx: influx_cfg,
    } = config;

    let influx = influx::Client::new(influx_cfg);

    let buf_size = mqtt_cfg.papp_buffer.unwrap_or(0);
    log::info!("Using a buffer size of {} for PApp messages", buf_size);
    let mut papp_buffer = VecDeque::<(PApp, std::time::SystemTime)>::with_capacity(buf_size);
    let mut drain_requested = false;

    loop {
        let mut mqtt_client = mqtt::Client::<Msg>::new("wattflux", mqtt_cfg.broker.clone());
        mqtt_client.subscribe::<PApp>(QoS::AtMostOnce).await?;
        mqtt_client.subscribe::<Compteurs>(QoS::AtLeastOnce).await?;
        mqtt_client
            .subscribe::<WattFluxDrain>(QoS::AtLeastOnce)
            .await?;

        loop {
            match mqtt_client.recv().await {
                Some(msg) => {
                    let res = match msg {
                        Msg::PApp(papp) => {
                            if papp_buffer.len() >= buf_size {
                                log::warn!(
                                    "PApp buffer overflow: buffer size is {}, but it has {} entries. Oldest entry will be dropped.",
                                    buf_size,
                                    papp_buffer.len()
                                );
                                papp_buffer.pop_front();
                            }
                            papp_buffer.push_back((papp, std::time::SystemTime::now()));
                            if drain_requested || papp_buffer.len() >= buf_size {
                                log::debug!(
                                    "Flushing PApp buffer to InfluxDB ({} entries)",
                                    papp_buffer.len()
                                );
                                match influx.write_lines(papp_buffer.iter()).await {
                                    Ok(_) => {
                                        papp_buffer.clear();
                                        Ok(())
                                    }
                                    Err(e) => Err(e),
                                }
                            } else {
                                Ok(())
                            }
                        }
                        Msg::Compteurs(meters) => {
                            log::debug!("Sending Compteurs to InfluxDB");
                            influx.write_lines(Some(meters)).await
                        }
                        Msg::WattFluxDrain(drain) => {
                            log::debug!("Received drain request: {:?}", drain);
                            drain_requested = drain.map(|d| d.0).unwrap_or(false);
                            if drain_requested && !papp_buffer.is_empty() {
                                log::debug!(
                                    "Flushing PApp buffer to InfluxDB ({} entries)",
                                    papp_buffer.len()
                                );
                                match influx.write_lines(papp_buffer.iter()).await {
                                    Ok(_) => {
                                        papp_buffer.clear();
                                        Ok(())
                                    }
                                    Err(e) => Err(e),
                                }
                            } else {
                                Ok(())
                            }
                        }
                    };
                    if let Err(e) = res {
                        log::error!("Failed to publish to InfluxDB: {}", e);
                    }
                }
                None => {
                    log::error!("MQTT receive channel closed. Reconnecting in 5s...");
                    break;
                }
            }
        }
        if !papp_buffer.is_empty() {
            log::debug!(
                "Flushing PApp buffer to InfluxDB ({} entries)",
                papp_buffer.len()
            );
            if !influx.write_lines(papp_buffer.iter()).await.is_err() {
                papp_buffer.clear();
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
