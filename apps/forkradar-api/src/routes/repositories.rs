//! Routes for the watchlist of the user.
//!
//! A repository is added only after the server fetched its metadata from
//! GitHub. The server stores the stable GitHub id, not the name, because
//! owner/name pairs can change.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use forkradar_domain::api::{AddRepositoryRequest, WatchedRepository};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dto::watched_to_dto;
use crate::error::ApiError;
use crate::state::AppState;

/// Lists the watched repositories of the current user.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<WatchedRepository>>, ApiError> {
    let records = state
        .store
        .list_watched_repositories(user.session.user_id)
        .await?;
    Ok(Json(records.iter().map(watched_to_dto).collect()))
}

/// Adds a repository to the watchlist of the current user.
pub async fn add(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<AddRepositoryRequest>,
) -> Result<(StatusCode, Json<WatchedRepository>), ApiError> {
    let (owner, name) = parse_repository(&request.repository)?;

    let token = state
        .github_token_for(user.session.user_id)
        .await
        .or_else(|| state.service_token());
    let client = state.github.with_token(token.clone());

    let repository = client.repository(&owner, &name).await?;
    state.store.upsert_repository(&repository).await?;

    let existing = state
        .store
        .list_watched_repositories(user.session.user_id)
        .await?;
    if existing
        .iter()
        .any(|entry| entry.github_id == repository.id)
    {
        return Err(ApiError::conflict(
            "REPOSITORY_ALREADY_WATCHED",
            format!("{} is already on the watchlist.", repository.full_name),
        ));
    }

    state
        .store
        .add_watched_repository(user.session.user_id, repository.id)
        .await?;

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "watchlist_added",
            Some(&repository.full_name),
            Some(serde_json::json!({ "github_id": repository.id })),
        )
        .await?;

    let records = state
        .store
        .list_watched_repositories(user.session.user_id)
        .await?;
    let record = records
        .into_iter()
        .find(|entry| entry.github_id == repository.id)
        .ok_or_else(|| {
            ApiError::internal(
                "WATCHLIST_WRITE_FAILED",
                "The repository could not be added to the watchlist.",
            )
        })?;

    Ok((StatusCode::CREATED, Json(watched_to_dto(&record))))
}

/// Removes a repository from the watchlist of the current user.
pub async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let removed = state
        .store
        .remove_watched_repository(user.session.user_id, id)
        .await?;
    if !removed {
        return Err(ApiError::not_found(
            "WATCHLIST_ENTRY_NOT_FOUND",
            "The watchlist entry does not exist.",
        ));
    }
    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "watchlist_removed",
            Some(&id.to_string()),
            None,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Normalizes a repository input to an owner and a name.
///
/// Both `owner/name` and a full GitHub URL are accepted.
fn parse_repository(input: &str) -> Result<(String, String), ApiError> {
    let trimmed = input.trim();
    let without_scheme = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .unwrap_or(trimmed);
    let without_suffix = without_scheme
        .strip_suffix(".git")
        .unwrap_or(without_scheme);
    let without_trailing = without_suffix.trim_end_matches('/');

    let mut parts = without_trailing.split('/').filter(|part| !part.is_empty());
    let owner = parts.next().unwrap_or_default().to_string();
    let name = parts.next().unwrap_or_default().to_string();

    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(ApiError::bad_request(
            "REPOSITORY_INPUT_INVALID",
            "Use the form owner/name or the URL of a GitHub repository.",
        ));
    }

    Ok((owner, name))
}

#[cfg(test)]
mod tests {
    use super::parse_repository;

    #[test]
    fn accepts_owner_and_name() {
        assert_eq!(
            parse_repository("forkradar/forkradar").unwrap(),
            ("forkradar".to_string(), "forkradar".to_string())
        );
    }

    #[test]
    fn accepts_a_url() {
        assert_eq!(
            parse_repository("https://github.com/owner/name").unwrap(),
            ("owner".to_string(), "name".to_string())
        );
    }

    #[test]
    fn accepts_a_url_with_git_suffix() {
        assert_eq!(
            parse_repository("https://github.com/owner/name.git").unwrap(),
            ("owner".to_string(), "name".to_string())
        );
    }

    #[test]
    fn rejects_incomplete_input() {
        assert!(parse_repository("owner").is_err());
        assert!(parse_repository("").is_err());
        assert!(parse_repository("owner/name/extra").is_err());
    }
}
