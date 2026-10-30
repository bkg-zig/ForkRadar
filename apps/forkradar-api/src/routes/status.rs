//! `GET /api/status`.
//!
//! The endpoint reports the state of every optional integration. It never
//! contains a secret and never claims that an integration works when it was
//! not proven.

use axum::extract::State;
use axum::Json;
use forkradar_domain::api::{FeatureStatus, NimStatus, StatusResponse};

use crate::state::AppState;

/// Reports the state of the server and of its integrations.
pub async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let github_oauth = match state.config.github_oauth.as_ref() {
        Some(_) => FeatureStatus::ready(
            "The GitHub OAuth app is configured. Sign in with GitHub to use a personal token.",
        ),
        None => FeatureStatus::not_configured(
            "GITHUB_OAUTH_CLIENT_ID, GITHUB_OAUTH_CLIENT_SECRET and GITHUB_OAUTH_CALLBACK_URL are not set.",
        ),
    };

    let token_encryption = if state.cipher.is_some() {
        FeatureStatus::ready("GitHub tokens are encrypted with AES-256-GCM before they are stored.")
    } else {
        FeatureStatus::not_configured(
            "FORKRADAR_TOKEN_ENCRYPTION_KEY is not set. No GitHub token can be stored.",
        )
    };

    let github_api = if state.service_token().is_some() {
        FeatureStatus::ready("The server uses a configured GitHub token for API requests.")
    } else {
        FeatureStatus::new(
            "READY",
            "The server reads public GitHub data without a token. The anonymous limit is 60 requests per hour.",
        )
    };

    let database = FeatureStatus::ready("The PostgreSQL connection is open.");

    let nim = nim_status(&state).await;

    let github_rate_limit =
        state
            .rate_limit
            .get()
            .map(|observed| forkradar_domain::api::RateLimitState {
                limit: observed.limit,
                remaining: observed.remaining,
                reset_at: observed.reset_at,
                scope: observed.scope.clone(),
            });

    Json(StatusResponse {
        application: "ForkRadar".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        github_oauth: FeatureStatus {
            state: format!("{}/{}", github_oauth.state, token_encryption.state),
            message: format!("{} {}", github_oauth.message, token_encryption.message),
        },
        github_api,
        nim,
        database,
        worker: state.worker.describe(),
        github_rate_limit,
    })
}

/// Reads the state of the model backend. The result is cached for 30 seconds.
async fn nim_status(state: &AppState) -> NimStatus {
    if let Some(cached) = state.nim_status.get() {
        return cached;
    }

    let diagnosis = state.nim.diagnose().await;
    let status = NimStatus {
        state: diagnosis.state.as_str().to_string(),
        base_url: diagnosis.base_url,
        model: diagnosis.model,
        available_models: diagnosis.available_models,
        message: diagnosis.message,
    };
    state.nim_status.put(status.clone());
    status
}
