//! HTTP routes of the ForkRadar API.
//!
//! Every handler that touches user data checks the session on the server.
//! The frontend never decides which data a user may see.

pub mod analysis;
pub mod features;
pub mod overview;
pub mod repositories;
pub mod scans;
pub mod status;

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::auth;
use crate::state::AppState;

/// Builds the API router.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(status::status))
        .route("/api/analysis/tools", get(analysis::tools))
        .route("/api/analysis/score/{github_id}", get(analysis::score))
        .route("/api/overview", get(overview::overview))
        .route(
            "/api/repositories",
            get(repositories::list).post(repositories::add),
        )
        .route("/api/repositories/{id}", delete(repositories::remove))
        .route("/api/scans", get(scans::list).post(scans::create))
        .route("/api/scans/{id}", get(scans::detail))
        .route("/api/scans/{id}/repositories", get(scans::repositories))
        .route("/api/scans/{id}/graph", get(scans::graph))
        .route("/api/scans/{id}/resume", post(scans::resume))
        .route("/api/scans/{id}/cancel", post(scans::cancel))
}

/// Builds the routes of the GitHub login.
///
/// The paths stay outside `/api` because the browser follows them as a
/// redirect. The callback writes the session cookie and redirects to the
/// application root.
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/auth/github", get(auth::start))
        .route("/auth/github/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .route("/api/auth/session", get(auth::session))
        .route("/api/auth/state", get(auth::state))
}
