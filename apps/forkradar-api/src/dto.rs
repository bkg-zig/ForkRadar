//! Conversion of the stored records into the API data transfer objects.

use forkradar_domain::api::{ScanRun, WatchedRepository};
use forkradar_store::{PgStore, ScanRunRecord, WatchedRepositoryRecord};

use crate::error::ApiError;

/// Builds the API view of a scan run.
pub async fn run_to_dto(store: &PgStore, run: &ScanRunRecord) -> Result<ScanRun, ApiError> {
    let root = store.get_repository(run.root_github_id).await?;
    Ok(ScanRun {
        id: run.id,
        root_github_id: run.root_github_id,
        root_full_name: root
            .map(|repository| repository.full_name)
            .unwrap_or_else(|| format!("github-id-{}", run.root_github_id)),
        status: run.status,
        max_depth: run.max_depth,
        discovery_mode: run.discovery_mode.clone(),
        config_version: run.config_version,
        discovered_count: run.discovered_count,
        processed_count: run.processed_count,
        pending_count: run.pending_count,
        failed_count: run.failed_count,
        max_reached_depth: run.max_reached_depth,
        pages_fetched: run.pages_fetched,
        last_success_at: run.last_success_at,
        last_error: run.last_error.clone(),
        created_at: run.created_at,
        started_at: run.started_at,
        updated_at: run.updated_at,
        finished_at: run.finished_at,
        rate_limit: match (run.rate_limit_limit, run.rate_limit_remaining) {
            (Some(limit), Some(remaining)) => Some(forkradar_domain::api::RateLimitState {
                limit,
                remaining,
                reset_at: run.rate_limit_reset_at,
                scope: "AUTHENTICATED".to_string(),
            }),
            _ => None,
        },
    })
}

/// Builds the API view of a watched repository.
pub fn watched_to_dto(record: &WatchedRepositoryRecord) -> WatchedRepository {
    WatchedRepository {
        id: record.id,
        github_id: record.github_id,
        full_name: record.full_name.clone(),
        html_url: record.html_url.clone(),
        description: record.description.clone(),
        language: record.language.clone(),
        is_fork: record.is_fork,
        created_at: record.created_at,
        last_scan_id: record.last_scan_id,
    }
}
