//! ForkRadar API server.
//!
//! The server owns the database connection, the GitHub client, the model
//! adapter and the background worker. The browser only receives JSON.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use forkradar_github::GitHubClient;
use forkradar_nim::NimClient;
use forkradar_store::PgStore;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod config;
mod crypto;
mod dto;
mod error;
mod routes;
mod state;
mod worker;

use crate::config::Config;
use crate::crypto::TokenCipher;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A missing .env file is not an error. The server reads the environment.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(Config::from_env()?);
    let store = PgStore::connect(&config.database_url, 8).await?;
    store.migrate().await?;

    let github = GitHubClient::new(&config.github_api_base_url, config.github_api_token.clone())?;
    let nim = NimClient::new(config.nim.clone())?;
    let cipher = config.token_encryption_key.map(TokenCipher::new);

    if cipher.is_none() {
        tracing::warn!(
            "FORKRADAR_TOKEN_ENCRYPTION_KEY is not set. The OAuth callback stores no token."
        );
    }
    if config.github_oauth.is_none() {
        tracing::warn!(
            "The GitHub OAuth app is not configured. The login endpoint reports GITHUB_OAUTH_NOT_CONFIGURED."
        );
    }

    // Jobs of a previous process were interrupted. They go back into the
    // queue and continue at their checkpoint.
    match store.recover_interrupted_jobs().await {
        Ok(count) if count > 0 => tracing::info!(count, "interrupted jobs returned to the queue"),
        Ok(_) => {}
        Err(error) => tracing::error!(error = %error, "interrupted jobs could not be recovered"),
    }

    let state = AppState::new(store.clone(), config.clone(), github, nim, cipher);
    let worker_handle = worker::spawn(state.clone());

    let app = build_router(state.clone(), config.web_dist.clone());

    let addr: SocketAddr = config.bind_addr;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "ForkRadar API listening");

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    worker_handle.abort();
    result?;
    Ok(())
}

/// Builds the router with the API routes and the static frontend.
fn build_router(state: AppState, web_dist: Option<PathBuf>) -> Router {
    let api = routes::api_router();
    let auth = routes::auth_router();

    let mut app = Router::new()
        .merge(api)
        .merge(auth)
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http());

    if let Some(origin) = state.config.cors_allowed_origin.clone() {
        match origin.parse::<axum::http::HeaderValue>() {
            Ok(value) => {
                let cors = CorsLayer::new()
                    .allow_origin(value)
                    .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                    .allow_credentials(true);
                app = app.layer(cors);
            }
            Err(error) => {
                tracing::error!(error = %error, "FORKRADAR_CORS_ALLOWED_ORIGIN is not a valid header value");
            }
        }
    }

    match web_dist {
        Some(dist) => {
            let index = dist.join("index.html");
            app =
                app.fallback_service(ServeDir::new(&dist).not_found_service(ServeFile::new(index)));
        }
        None => {
            app = app.fallback(missing_frontend);
        }
    }

    app.with_state(state)
}

/// Liveness probe of the server.
async fn health() -> Response {
    axum::Json(serde_json::json!({ "status": "ok" })).into_response()
}

/// Answer when no frontend bundle was configured.
async fn missing_frontend() -> Response {
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "code": "WEB_DIST_NOT_CONFIGURED",
            "message": "No frontend bundle is served. Set FORKRADAR_WEB_DIST to the built assets.",
        })),
    )
        .into_response()
}

/// Waits for the shutdown signal of the process.
async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("the interrupt signal could not be installed");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("the terminate signal could not be installed")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {}
        _ = terminate => {}
    }

    tracing::info!("shutdown signal received");
}
