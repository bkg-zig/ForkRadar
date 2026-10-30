//! `GET /api/overview`.
//!
//! The dashboard numbers come from the stored rows of the user. Areas that
//! are not collected yet are not part of the payload.

use axum::extract::State;
use axum::Json;
use forkradar_domain::api::OverviewSummary;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// Returns the dashboard summary of the current user.
pub async fn overview(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<OverviewSummary>, ApiError> {
    Ok(Json(
        state.store.overview_summary(user.session.user_id).await?,
    ))
}
