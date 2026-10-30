//! Routes for the analysis area.
//!
//! The endpoint reports the tools of the runtime. A tool that is not
//! installed is reported as missing and never produces a measurement.

use axum::extract::{Path, State};
use axum::Json;
use forkradar_analysis::{detect_tools, QualityScore, ToolInventory};

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// Returns the tools that the runtime offers.
pub async fn tools() -> Json<ToolInventory> {
    Json(detect_tools().await)
}

/// Returns the quality score of a repository discovered by the user.
///
/// The score stays `NOT_RUN` until a real tool measured the revision. The
/// endpoint never returns a value that no tool produced.
pub async fn score(
    State(state): State<AppState>,
    user: AuthUser,
    Path(github_id): Path<i64>,
) -> Result<Json<QualityScore>, ApiError> {
    let known = state
        .store
        .user_knows_repository(user.session.user_id, github_id)
        .await?;
    if !known {
        return Err(ApiError::not_found(
            "REPOSITORY_NOT_FOUND",
            "The repository is unknown for this user.",
        ));
    }

    let repository = state
        .store
        .get_repository(github_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(
                "REPOSITORY_NOT_FOUND",
                "The repository metadata is missing.",
            )
        })?;

    let inventory = detect_tools().await;
    let missing = inventory.missing();

    if missing.len() == inventory.tools.len() {
        return Ok(Json(QualityScore::not_run(format!(
            "No analysis tool of the runtime is installed. {} is not measured.",
            repository.full_name
        ))));
    }

    Ok(Json(QualityScore::not_run(format!(
        "No measurement of {} was stored yet. The available tools are {}.",
        repository.full_name,
        inventory
            .tools
            .iter()
            .filter(|tool| tool.installed)
            .map(|tool| tool.id.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    ))))
}
