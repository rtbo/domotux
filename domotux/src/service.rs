use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::usize;

use axum::Json;
use axum::extract::ws::WebSocket;
use axum::extract::{FromRequestParts, Query, State, WebSocketUpgrade};
use axum::http::{self, StatusCode, request};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum_server::tls_rustls::RustlsConfig;
use base::vecmap::VecMap;
use mqtt::topics::{
    CompteurActif, Contrat, CouleurTempo, CouleurTempoAujourdhui, CouleurTempoDemain, PApp,
    PrixKwh, PrixKwhActif,
};
use mqtt::{self, QoS};
use serde::{Deserialize, Serialize};
use tokio::{sync, task, time};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::db;

mod jwt;

/// MQTT state shared across handlers
/// This include only the latest values received for "slow" topics.
/// "Fast" topics like PApp are directly sent to the WebSocket without going through this state.
#[derive(Debug, Default)]
struct MqttState {
    contrat: Option<Contrat>,
    compteur_actif: Option<CompteurActif>,
    prix_kwh_actif: Option<PrixKwhActif>,
    prix_kwh: Option<PrixKwh>,
    couleur_ajd: Option<CouleurTempoAujourdhui>,
    couleur_demain: Option<CouleurTempoDemain>,
}

#[derive(Debug)]
pub struct AppState {
    db: db::Db,
    broker: mqtt::BrokerAddress,
    secret_key: String,
    mqtt: sync::Mutex<MqttState>,
}

pub async fn start(config: &crate::Config) -> anyhow::Result<()> {
    use axum::routing::post;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "domotux=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db = super::find_and_open_db().await?;
    if !db.is_initialized().await? {
        log::error!("Database not initialized, please run `domotux initialize` first");
    }

    let secret_key = generate_secret_key();

    let state = Arc::new(AppState {
        db,
        broker: config.broker.clone(),
        secret_key,
        mqtt: sync::Mutex::new(MqttState::default()),
    });

    mqtt_loop(state.clone());

    let facade_dir = "/usr/local/share/domotux/facade";
    let serve_facade = tokio::fs::metadata(facade_dir).await.is_ok();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers([http::header::CONTENT_TYPE, http::header::AUTHORIZATION]);

    let app = axum::Router::new()
        .route("/v1/auth", post(authenticate_user))
        .route("/v1/check_auth", get(check_auth))
        .route("/v1/papp_ws", any(papp_ws))
        .route("/v1/info_contrat", get(get_info_contrat))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state);

    let app = if serve_facade {
        log::info!("Serving facade from {}", facade_dir);
        let facade_index = format!("{facade_dir}/index.html");
        app.fallback_service(
            ServeDir::new(facade_dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(facade_index)),
        )
    } else {
        app
    };

    let addr: SocketAddr = config.bind_addr.parse()?;
    log::info!("Starting Domotux service on {}", config.bind_addr);

    if let Some(tls) = &config.tls {
        let tls_config = RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path).await?;

        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    Ok(())
}

#[cfg(feature = "no-secret")]
fn generate_secret_key() -> String {
    #[cfg(debug_assertions)]
    {
        log::warn!("Running in debug mode with 'no-secret' feature, using fixed secret key");
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(
            false,
            "The 'no-secret' feature should only be used in debug mode for testing purposes"
        );
    }
    "not-so-secret".to_string()
}

#[cfg(not(feature = "no-secret"))]
fn generate_secret_key() -> String {
    use base64::prelude::*;
    use rand::RngExt;

    let mut rng = rand::rng();
    let key: [u8; 16] = rng.random();
    BASE64_STANDARD.encode(key)
}

mqtt::subscribe_msg! {
    enum MqttMsg {
        Contrat(Contrat),
        CompteurActif(CompteurActif),
        PrixKwhActif(PrixKwhActif),
        PrixKwh(PrixKwh),
        CouleurTempoAjd(CouleurTempoAujourdhui),
        CouleurTempoDemain(CouleurTempoDemain),
    }
}

fn mqtt_loop(state: Arc<AppState>) -> task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let mut client = mqtt::Client::<MqttMsg>::new("domotux_service", state.broker.clone());

            if let Err(e) = client.subscribe_all(QoS::AtLeastOnce).await {
                log::error!("Failed to subscribe to MQTT topics: {}", e);
                time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }

            loop {
                match client.recv().await {
                    Some(msg) => {
                        let mut mqtt_state = state.mqtt.lock().await;
                        match msg {
                            MqttMsg::Contrat(contrat) => mqtt_state.contrat = Some(contrat),
                            MqttMsg::CompteurActif(compteur_actif) => {
                                mqtt_state.compteur_actif = Some(compteur_actif)
                            }
                            MqttMsg::PrixKwhActif(prix_kwh_actif) => {
                                mqtt_state.prix_kwh_actif = Some(prix_kwh_actif)
                            }
                            MqttMsg::PrixKwh(prix_kwh) => mqtt_state.prix_kwh = Some(prix_kwh),
                            MqttMsg::CouleurTempoAjd(couleur_ajd) => {
                                mqtt_state.couleur_ajd = Some(couleur_ajd)
                            }
                            MqttMsg::CouleurTempoDemain(couleur_demain) => {
                                mqtt_state.couleur_demain = Some(couleur_demain)
                            }
                        }
                    }
                    None => {
                        log::error!(
                            "MQTT receive channel closed, reconnecting to broker {}",
                            state.broker
                        );
                        break;
                    }
                }
            }

            time::sleep(std::time::Duration::from_secs(5)).await;
        }
    })
}

fn internal_server_error() -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error".to_string(),
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: u64,
}

#[axum::debug_handler]
async fn authenticate_user(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AuthRequest>,
) -> (StatusCode, String) {
    match state.db.auth_user(&payload.name, &payload.password).await {
        Ok(true) => (),
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                "Invalid username or password".to_string(),
            );
        }
        Err(_) => {
            return internal_server_error();
        }
    };

    log::info!("User '{}' authenticated successfully", payload.name);

    let claims = JwtClaims {
        sub: payload.name,
        exp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 1800, // 30 minutes expiration
    };

    jwt::generate(&claims, &state.secret_key)
        .map(|token| (StatusCode::OK, token))
        .unwrap_or_else(|_| internal_server_error())
}

#[allow(dead_code)]
struct JwtAuth(JwtClaims);

impl FromRequestParts<Arc<AppState>> for JwtAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut request::Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = {
            let auth_header = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Missing Authorization header"))?;
            if !auth_header.starts_with("Bearer ") {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "Invalid Authorization header format",
                ));
            }
            &auth_header[7..]
        };

        let claims: JwtClaims = match jwt::verify(token, &state.secret_key) {
            Ok(claims) => claims,
            Err(_) => return Err((StatusCode::UNAUTHORIZED, "Invalid or expired token")),
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now > claims.exp {
            return Err((StatusCode::UNAUTHORIZED, "Token has expired"));
        }

        Ok(JwtAuth(claims))
    }
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    name: String,
    password: String,
}

async fn check_auth(_: JwtAuth) -> (StatusCode, &'static str) {
    (StatusCode::OK, "Authenticated")
}

mqtt::subscribe_msg! {
    enum PAppMsg {
        PApp(PApp),
    }
}

#[axum::debug_handler]
async fn papp_ws(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    log::info!("PApp WebSocket connection attempt");

    let claims = if let Some(token) = query.get("token") {
        match jwt::verify::<JwtClaims>(token, &state.secret_key) {
            Ok(claims) => claims,
            Err(_) => {
                log::warn!("Invalid token provided for PApp WebSocket connection");
                return (StatusCode::UNAUTHORIZED, "Invalid token".to_string()).into_response();
            }
        }
    } else {
        log::warn!("Missing token for PApp WebSocket connection");
        return (StatusCode::BAD_REQUEST, "Missing token".to_string()).into_response();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now > claims.exp {
        log::warn!("Expired token provided for PApp WebSocket connection");
        return (StatusCode::UNAUTHORIZED, "Token has expired".to_string()).into_response();
    }

    let user = claims.sub;

    log::info!("User '{}' authenticated successfully", user);

    ws.on_upgrade(move |socket| handle_papp_ws(socket, state, user))
}

async fn handle_papp_ws(mut socket: WebSocket, state: Arc<AppState>, _user: String) {
    log::info!("PApp WebSocket connection established for user '{}'", _user);
    log::info!("Subscribing to MQTT topics on broker {}", state.broker);
    let mut client = mqtt::Client::<PAppMsg>::new("domotux_dashboard_ws", state.broker.clone());
    client.subscribe_all(QoS::AtMostOnce).await.unwrap();
    loop {
        match client.recv().await {
            Some(PAppMsg::PApp(papp)) => {
                let msg = format!("papp={}", papp.0);
                if socket
                    .send(axum::extract::ws::Message::Text(msg.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            None => {
                log::warn!("PApp MQTT receive channel closed for user '{}'", _user);
                break;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InfoContrat {
    subsc_power: Option<u32>,
    option: Option<String>,
    compteur_actif: Option<String>,
    prix_kwh_actif: Option<f32>,
    prix_kwh: Option<VecMap<f32>>,
    couleur_ajd: Option<CouleurTempo>,
    couleur_demain: Option<CouleurTempo>,
}

async fn get_info_contrat(
    State(state): State<Arc<AppState>>,
    _: JwtAuth,
) -> Result<Json<InfoContrat>, (StatusCode, &'static str)> {
    let mqtt_state = state.mqtt.lock().await;
    let mut res = InfoContrat::default();
    if let Some(contrat) = mqtt_state.contrat.as_ref() {
        res.subsc_power = contrat.subsc_power;
        res.option = contrat.option.clone();
    }
    if let Some(compteur_actif) = mqtt_state.compteur_actif.as_ref() {
        res.compteur_actif = Some(compteur_actif.0.clone());
    }
    if let Some(prix_kwh_actif) = mqtt_state.prix_kwh_actif.as_ref() {
        res.prix_kwh_actif = Some(prix_kwh_actif.0);
    }
    if let Some(prix_kwh) = mqtt_state.prix_kwh.as_ref() {
        res.prix_kwh = Some(prix_kwh.0.clone());
    }
    if let Some(couleur_ajd) = mqtt_state.couleur_ajd.as_ref() {
        res.couleur_ajd = couleur_ajd.0;
    }
    if let Some(couleur_demain) = mqtt_state.couleur_demain.as_ref() {
        res.couleur_demain = couleur_demain.0;
    }
    Ok(Json(res))
}
