use std::{collections::HashMap, sync::Arc, usize};

use axum::{
    Json,
    extract::{Query, State, WebSocketUpgrade, ws::WebSocket},
    http::{self, StatusCode},
    response::{IntoResponse, Response}, routing::any,
};
use mqtt::{
    self,
    topics::{PApp, PrixKwhActif},
};
use mqtt::QoS;
use serde::{Deserialize, Serialize};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::db;

mod jwt;

#[derive(Debug)]
pub struct AppState {
    db: db::Db,
    broker: mqtt::BrokerAddress,
    secret_key: String,
}

pub async fn start(config: &crate::Config) -> anyhow::Result<()> {
    use axum::routing::post;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "domotux=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db = db::Db::open(config.db_path.clone()).await?;
    if !db.is_initialized().await? {
        log::error!("Database not initialized, please run `domotux initialize` first");
    }

    let secret_key = {
        use base64::prelude::*;
        use rand::RngExt;

        let mut rng = rand::rng();
        let key: [u8; 16] = rng.random();
        BASE64_STANDARD.encode(key)
    };

    let state = Arc::new(AppState {
        db,
        broker: config.broker.clone(),
        secret_key,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers([http::header::CONTENT_TYPE, http::header::AUTHORIZATION]);

    // build our application with a single route
    let app = axum::Router::new()
        .route("/v1/auth", post(authenticate_user))
        .route("/v1/dashboard_ws", any(dashboard_ws))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state);

    log::info!("Starting Domotux service on {}", config.bind_addr);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: usize,
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    name: String,
    password: String,
}

fn internal_server_error() -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error".to_string(),
    )
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
            .as_secs() as usize
            + 1800, // 30 minutes expiration
    };

    jwt::generate_jwt(&claims, &state.secret_key)
        .map(|token| (StatusCode::OK, token))
        .unwrap_or_else(|_| internal_server_error())
}

mqtt::subscribe_msg! {
    enum DashboardMsg {
        PApp(PApp),
        PrixKwhActif(PrixKwhActif),
    }
}

#[axum::debug_handler]
async fn dashboard_ws(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    log::info!("Dashboard WebSocket connection attempt");

    let user = if let Some(token) = query.get("token") {
        match jwt::verify_jwt::<JwtClaims>(token, &state.secret_key) {
            Ok(claims) => claims.sub,
            Err(_) => {
                log::warn!("Invalid token provided for Dashboard WebSocket connection");
                return (StatusCode::UNAUTHORIZED, "Invalid token".to_string()).into_response();
            }
        }
    } else {
        log::warn!("Missing token for Dashboard WebSocket connection");
        return (StatusCode::BAD_REQUEST, "Missing token".to_string()).into_response();
    };

    log::info!("User '{}' authenticated successfully", user);

    ws.on_upgrade(move |socket| handle_dashboard_ws(socket, state, user))
}

async fn handle_dashboard_ws(mut socket: WebSocket, state: Arc<AppState>, _user: String) {
    let mut client =
        mqtt::Client::<DashboardMsg>::new("domotux_dashboard_ws", state.broker.clone());
    client.subscribe_all(QoS::AtMostOnce).await.unwrap();
    loop {
        match client.recv().await {
            Some(DashboardMsg::PApp(papp)) => {
                let msg = format!("papp={}", papp.0);
                if socket
                    .send(axum::extract::ws::Message::Text(msg.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Some(DashboardMsg::PrixKwhActif(prix_kwh)) => {
                let msg = format!("prixKwh={}", prix_kwh.0);
                if socket
                    .send(axum::extract::ws::Message::Text(msg.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            None => {}
        }
    }
}
