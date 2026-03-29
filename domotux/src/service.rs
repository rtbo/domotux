use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{self, Method, StatusCode},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use crate::db;

mod jwt;

#[derive(Debug)]
pub struct AppState {
    db: db::Db,
    secret_key: String,
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

pub async fn start(config: &crate::Config) -> anyhow::Result<()> {
    use axum::routing::post;

    let db = db::Db::open(config.db_path.clone()).await?;
    let state = Arc::new(AppState {
        db,
        secret_key: config.secret_key.clone().unwrap_or_default(),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers([http::header::CONTENT_TYPE, http::header::AUTHORIZATION]);

    // build our application with a single route
    let app = axum::Router::new()
        .route("/v1/auth", post(authenticate_user))
        .layer(cors)
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
