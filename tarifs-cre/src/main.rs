use std::{path::PathBuf, process, sync::Arc, time::Duration};

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task;

mod contract;
mod cre;
mod tabular;

use crate::contract::Contract;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreDbConfig {
    #[serde(serialize_with = "base::cfg::serialize_seconds", deserialize_with = "base::cfg::deserialize_seconds")]
    validity: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerConfig {
    bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    contract: contract::Config,
    cre_db: CreDbConfig,
    server: ServerConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            contract: contract::Config::Mqtt {
                broker: Default::default(),
                topic: "domotux/contract/#".to_string(),
            },
            cre_db: CreDbConfig {
                validity: Duration::from_secs(24 * 3600), // 24h
            },
            server: ServerConfig {
                bind_address: "0.0.0.0:8080".to_string(),
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

#[derive(Debug, Clone)]
struct AppState {
    contract: Contract,
    db: cre::Db,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TarifQuery {
    date: Option<String>,
    psousc: Option<u32>,
    option: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "option", rename_all = "lowercase")]
pub enum OptionPrice {
    Base {
        base: f32,
    },
    Hphc {
        hp: f32,
        hc: f32,
    },
    Tempo {
        #[serde(rename = "hpBleu")]
        hp_bleu: f32,
        #[serde(rename = "hcBleu")]
        hc_bleu: f32,
        #[serde(rename = "hpBlanc")]
        hp_blanc: f32,
        #[serde(rename = "hcBlanc")]
        hc_blanc: f32,
        #[serde(rename = "hpRouge")]
        hp_rouge: f32,
        #[serde(rename = "hcRouge")]
        hc_rouge: f32,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct TarifResponse {
    date: String,
    psousc: u32,
    #[serde(rename = "prixKwh")]
    prix_kwh: OptionPrice,
}

#[axum::debug_handler]
async fn get_tarif(
    State(state): State<Arc<Mutex<AppState>>>,
    Query(query): Query<TarifQuery>,
) -> Result<Json<TarifResponse>, StatusCode> {
    let state = state.lock().await;

    let paris_tz: FixedOffset = FixedOffset::east_opt(1 * 3600).unwrap();
    let date = query
        .date
        .and_then(|d| {
            NaiveDateTime::parse_from_str(&d, "%Y-%m-%d")
                .map(|ndt| paris_tz.from_local_datetime(&ndt).unwrap())
                .ok()
        })
        .unwrap_or_else(|| chrono::Utc::now().with_timezone(&paris_tz));
    let psousc = query.psousc.unwrap_or(state.contract.psousc);
    let option = query
        .option
        .unwrap_or_else(|| state.contract.optarif.clone());

    let resp_opt = state.db.search(&option, psousc, date).map_err(|e| {
        log::error!("Error searching for tariff: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let resp = TarifResponse {
        date: date.format("%Y-%m-%d").to_string(),
        psousc,
        prix_kwh: resp_opt.ok_or(StatusCode::NOT_FOUND)?,
    };
    Ok(Json(resp))
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    if cli.default_config {
        return base::cfg::print_default_config::<Config>();
    }

    let config: Config = base::cfg::load_config("tarifs-cre", cli.config_file).await?;
    log::info!("Starting with config: {:#?}", config);

    let contract = config.contract.static_or_default();
    let db = cre::Db::load().await?;

    let state = Arc::new(Mutex::new(AppState { contract, db }));

    // Contract update loop via MQTT
    if matches!(config.contract, contract::Config::Mqtt { .. }) {
        log::info!("Subscribing to MQTT for contract updates");
        let state = state.clone();
        let config = config.contract.clone();
        task::spawn(async move {
            if let Err(e) = config.subscribe_to_changes(state).await {
                log::error!("Error subscribing to MQTT: {}", e);
            }
        });
    }

    // CRE DB update loop
    task::spawn({
        let state = state.clone();
        let config = config.cre_db.clone();
        async move {
            loop {
                tokio::time::sleep(config.validity).await;
                let db = match cre::Db::load().await {
                    Ok(db) => db,
                    Err(e) => {
                        log::error!("Error loading CRE DB: {}", e);
                        continue;
                    }
                };
                state.lock().await.db = db;
            }
        }
    });

    // build our application with a single route
    let app = Router::new()
        .route("/tarif", get(get_tarif))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind(&config.server.bind_address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}
