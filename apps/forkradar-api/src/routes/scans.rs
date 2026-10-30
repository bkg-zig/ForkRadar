//! Routes for the scan runs of the user.
//!
//! A run of another user is reported as `404`. The ownership check happens on
//! the server against the database condition, so a manipulated run id cannot
//! open the data of a foreign user.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use forkradar_discovery::DiscoveryConfig;
use forkradar_domain::api::{
    CreateScanRequest, DiscoveredRepositoryPage, GraphPayload, ScanJobView, ScanRun,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dto::run_to_dto;
use crate::error::ApiError;
use crate::state::AppState;

/// A scan run with its jobs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanRunDetail {
    #[serde(flatten)]
    pub run: ScanRun,
    pub jobs: Vec<ScanJobView>,
}

/// Query parameters of the run list.
#[derive(Debug, serde::Deserialize)]
pub struct ListRunsQuery {
    /// Maximum number of runs. The server caps the value.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Query parameters of the discovered repository list.
#[derive(Debug, serde::Deserialize)]
pub struct ListRepositoriesQuery {
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Lists the scan runs of the current user.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<Vec<ScanRun>>, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let records = state.store.list_runs(user.session.user_id, limit).await?;
    let mut runs = Vec::with_capacity(records.len());
    for record in &records {
        runs.push(run_to_dto(&state.store, record).await?);
    }
    Ok(Json(runs))
}

/// Starts a scan for a repository of the watchlist.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CreateScanRequest>,
) -> Result<(StatusCode, Json<ScanRun>), ApiError> {
    let full_name = request.repository.trim();
    let (owner, name) = split_full_name(full_name)?;

    // The repository has to be on the watchlist of this user. Otherwise the
    // scan would read data that the user did not select.
    let watched = state
        .store
        .list_watched_repositories(user.session.user_id)
        .await?;
    let entry = watched
        .iter()
        .find(|entry| {
            entry
                .full_name
                .eq_ignore_ascii_case(&format!("{owner}/{name}"))
        })
        .ok_or_else(|| {
            ApiError::not_found(
                "REPOSITORY_NOT_WATCHED",
                format!("{owner}/{name} is not on the watchlist."),
            )
        })?;

    let root = state
        .store
        .get_repository(entry.github_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(
                "REPOSITORY_NOT_FOUND",
                format!("The metadata of {owner}/{name} is missing."),
            )
        })?;

    let max_depth = match request.max_depth {
        None => None,
        Some(depth) if depth >= 0 => Some(depth),
        Some(_) => {
            return Err(ApiError::bad_request(
                "MAX_DEPTH_INVALID",
                "The depth limit cannot be negative.",
            ))
        }
    };

    let engine = forkradar_discovery::DiscoveryEngine::new(
        state.store.clone(),
        state.config.github_api_base_url.clone(),
        DiscoveryConfig {
            max_pages_per_job: state.config.max_pages_per_job,
            ..DiscoveryConfig::default()
        },
    );
    let run_id = engine
        .start_run(user.session.user_id, &root, max_depth)
        .await?;

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "scan_started",
            Some(&run_id.to_string()),
            Some(serde_json::json!({
                "repository": root.full_name,
                "max_depth": max_depth,
            })),
        )
        .await?;

    let record =
        state.store.get_run(run_id).await?.ok_or_else(|| {
            ApiError::internal("SCAN_CREATE_FAILED", "The scan run was not stored.")
        })?;

    Ok((
        StatusCode::CREATED,
        Json(run_to_dto(&state.store, &record).await?),
    ))
}

/// Returns a run with its jobs.
pub async fn detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ScanRunDetail>, ApiError> {
    let record = owned_run(&state, user.session.user_id, id).await?;
    let jobs = state.store.list_job_views(id).await?;
    Ok(Json(ScanRunDetail {
        run: run_to_dto(&state.store, &record).await?,
        jobs,
    }))
}

/// Lists the repositories discovered by a run.
pub async fn repositories(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<ListRepositoriesQuery>,
) -> Result<Json<DiscoveredRepositoryPage>, ApiError> {
    owned_run(&state, user.session.user_id, id).await?;

    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let items = state
        .store
        .list_scan_repositories(id, offset, limit)
        .await?;
    let total = state.store.count_scan_repositories(id).await?;

    Ok(Json(DiscoveredRepositoryPage {
        run_id: id,
        items,
        total,
        offset,
        limit,
    }))
}

/// Returns the fork graph of a run.
pub async fn graph(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<GraphPayload>, ApiError> {
    owned_run(&state, user.session.user_id, id).await?;
    Ok(Json(state.store.graph_payload(id).await?))
}

/// Moves failed jobs of a run back into the queue.
pub async fn resume(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ScanRun>, ApiError> {
    let record = owned_run(&state, user.session.user_id, id).await?;
    if record.status.is_terminal() && record.status != forkradar_domain::model::JobStatus::Failed {
        return Err(ApiError::conflict(
            "SCAN_NOT_RESUMABLE",
            "Only a failed or partial scan can be resumed.",
        ));
    }

    state.store.requeue_failed_jobs(id).await?;
    let record = state.store.refresh_run(id).await?;
    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "scan_resumed",
            Some(&id.to_string()),
            None,
        )
        .await?;
    Ok(Json(run_to_dto(&state.store, &record).await?))
}

/// Cancels the open jobs of a run.
pub async fn cancel(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ScanRun>, ApiError> {
    let record = owned_run(&state, user.session.user_id, id).await?;
    if record.status.is_terminal() {
        return Err(ApiError::conflict(
            "SCAN_ALREADY_FINISHED",
            "The scan run is already in a final state.",
        ));
    }

    state.store.cancel_run(id).await?;
    let record = state.store.refresh_run(id).await?;
    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "scan_cancelled",
            Some(&id.to_string()),
            None,
        )
        .await?;
    Ok(Json(run_to_dto(&state.store, &record).await?))
}

/// Reads a run and rejects it when it belongs to another user.
async fn owned_run(
    state: &AppState,
    user_id: Uuid,
    run_id: Uuid,
) -> Result<forkradar_store::ScanRunRecord, ApiError> {
    state
        .store
        .get_run_for_user(run_id, user_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(
                "SCAN_NOT_FOUND",
                "The scan run does not exist or belongs to another user.",
            )
        })
}

/// Splits `owner/name` into its two parts.
fn split_full_name(full_name: &str) -> Result<(String, String), ApiError> {
    let trimmed = full_name.trim().trim_end_matches('/');
    let mut parts = trimmed.split('/').filter(|part| !part.is_empty());
    let owner = parts.next().unwrap_or_default().to_string();
    let name = parts.next().unwrap_or_default().to_string();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(ApiError::bad_request(
            "REPOSITORY_INPUT_INVALID",
            "Use the form owner/name.",
        ));
    }
    Ok((owner, name))
}

#[cfg(test)]
mod tests {
    use super::split_full_name;

    #[test]
    fn splits_owner_and_name() {
        assert_eq!(
            split_full_name("owner/name").unwrap(),
            ("owner".to_string(), "name".to_string())
        );
    }

    #[test]
    fn rejects_incomplete_input() {
        assert!(split_full_name("owner").is_err());
        assert!(split_full_name("owner/name/extra").is_err());
    }
}
