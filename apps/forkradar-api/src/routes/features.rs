//! Routes of the feature area.
//!
//! The routes work on the suggestions of the authenticated user. The score
//! is calculated on the server, the client only renders it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use forkradar_domain::api::{
    CreateFeatureRequest, FeatureAuditSummary, FeatureListQuery, FeatureSuggestion,
    UpdateFeatureStatusRequest,
};
use forkradar_domain::features::{FeatureSort, FeatureStatus};
use forkradar_store::features::{FeatureFilter, AUDIT_WINDOW_DAYS};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// Body of the answer after a vote.
#[derive(Debug, Clone, Serialize)]
pub struct VoteResponse {
    pub votes: i64,
    pub voted_by_me: bool,
}

/// Lists the suggestions of the user with the filters and the sort mode.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<FeatureListQuery>,
) -> Result<Json<Vec<FeatureSuggestion>>, ApiError> {
    let sort = match query.sort.as_deref() {
        None | Some("") => FeatureSort::RankingScore,
        Some(value) => FeatureSort::parse(value).ok_or_else(|| {
            ApiError::bad_request(
                "FEATURE_SORT_INVALID",
                format!(
                    "Unbekannte Sortierung '{value}'. Erlaubt sind: ranking_score, votes, impact, roi, newest."
                ),
            )
        })?,
    };

    let filter = FeatureFilter {
        text: query.q.clone(),
        area: query.area.clone(),
        status: query
            .status
            .as_deref()
            .and_then(FeatureStatus::parse)
            .map(|status| status.code().to_string()),
        min_impact: query.min_impact,
        max_effort: query.max_effort,
        min_votes: query.min_votes,
    };

    let suggestions = state
        .store
        .list_feature_suggestions(user.session.user_id, &filter, sort)
        .await?;
    Ok(Json(suggestions))
}

/// Reads one suggestion of the user.
pub async fn detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<FeatureSuggestion>, ApiError> {
    let suggestion = state
        .store
        .get_feature_suggestion(user.session.user_id, id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("FEATURE_NOT_FOUND", "Der Vorschlag wurde nicht gefunden.")
        })?;
    Ok(Json(suggestion))
}

/// Creates a suggestion for the user.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CreateFeatureRequest>,
) -> Result<(StatusCode, Json<FeatureSuggestion>), ApiError> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request(
            "FEATURE_TITLE_REQUIRED",
            "Der Titel darf nicht leer sein.",
        ));
    }
    if title.chars().count() > 200 {
        return Err(ApiError::bad_request(
            "FEATURE_TITLE_TOO_LONG",
            "Der Titel darf höchstens 200 Zeichen haben.",
        ));
    }
    if request.description.chars().count() > 4000 {
        return Err(ApiError::bad_request(
            "FEATURE_DESCRIPTION_TOO_LONG",
            "Die Beschreibung darf höchstens 4000 Zeichen haben.",
        ));
    }
    if !(1..=10).contains(&request.impact) {
        return Err(ApiError::bad_request(
            "FEATURE_IMPACT_INVALID",
            "Der Nutzen muss zwischen 1 und 10 liegen.",
        ));
    }
    if !(1..=10).contains(&request.effort) {
        return Err(ApiError::bad_request(
            "FEATURE_EFFORT_INVALID",
            "Der Aufwand muss zwischen 1 und 10 liegen.",
        ));
    }

    let id = state
        .store
        .create_feature_suggestion(
            user.session.user_id,
            title,
            &request.description,
            &request.area,
            request.impact,
            request.effort,
        )
        .await?;

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "feature.created",
            Some(&id.to_string()),
            Some(serde_json::json!({
                "title": title,
                "impact": request.impact,
                "effort": request.effort,
            })),
        )
        .await?;

    let suggestion = state
        .store
        .get_feature_suggestion(user.session.user_id, id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("FEATURE_NOT_FOUND", "Der Vorschlag wurde nicht gefunden.")
        })?;

    Ok((StatusCode::CREATED, Json(suggestion)))
}

/// Adds the vote of the user for a suggestion.
pub async fn vote(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<VoteResponse>, ApiError> {
    let changed = state
        .store
        .add_feature_vote(user.session.user_id, id)
        .await?;
    if !changed {
        return Err(ApiError::not_found(
            "FEATURE_NOT_FOUND",
            "Der Vorschlag wurde nicht gefunden.",
        ));
    }

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "feature.voted",
            Some(&id.to_string()),
            None,
        )
        .await?;

    let suggestion = state
        .store
        .get_feature_suggestion(user.session.user_id, id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("FEATURE_NOT_FOUND", "Der Vorschlag wurde nicht gefunden.")
        })?;

    Ok(Json(VoteResponse {
        votes: suggestion.votes,
        voted_by_me: suggestion.voted_by_me,
    }))
}

/// Withdraws the vote of the user.
pub async fn withdraw_vote(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<VoteResponse>, ApiError> {
    let changed = state
        .store
        .remove_feature_vote(user.session.user_id, id)
        .await?;
    if !changed {
        return Err(ApiError::not_found(
            "FEATURE_VOTE_NOT_FOUND",
            "Für diesen Vorschlag liegt keine Stimme des Benutzers vor.",
        ));
    }

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "feature.vote_withdrawn",
            Some(&id.to_string()),
            None,
        )
        .await?;

    let suggestion = state
        .store
        .get_feature_suggestion(user.session.user_id, id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("FEATURE_NOT_FOUND", "Der Vorschlag wurde nicht gefunden.")
        })?;

    Ok(Json(VoteResponse {
        votes: suggestion.votes,
        voted_by_me: suggestion.voted_by_me,
    }))
}

/// Changes the lifecycle state of a suggestion.
pub async fn update_status(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateFeatureStatusRequest>,
) -> Result<Json<FeatureSuggestion>, ApiError> {
    let status = FeatureStatus::parse(&request.status).ok_or_else(|| {
        ApiError::bad_request(
            "FEATURE_STATUS_INVALID",
            "Unbekannter Status. Erlaubt sind: PROPOSED, PLANNED, IN_DEVELOPMENT, SHIPPED.",
        )
    })?;

    let changed = state
        .store
        .set_feature_status(user.session.user_id, id, status)
        .await?;
    if !changed {
        return Err(ApiError::not_found(
            "FEATURE_NOT_FOUND",
            "Der Vorschlag wurde nicht gefunden.",
        ));
    }

    state
        .store
        .write_audit_event(
            Some(user.session.user_id),
            "feature.status_changed",
            Some(&id.to_string()),
            Some(serde_json::json!({ "status": status.code() })),
        )
        .await?;

    let suggestion = state
        .store
        .get_feature_suggestion(user.session.user_id, id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("FEATURE_NOT_FOUND", "Der Vorschlag wurde nicht gefunden.")
        })?;
    Ok(Json(suggestion))
}

/// Returns the audit summary of the last thirty days.
pub async fn audit(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<FeatureAuditSummary>, ApiError> {
    let timeline = state
        .store
        .feature_audit_timeline(user.session.user_id, AUDIT_WINDOW_DAYS)
        .await?;

    let total_created = timeline.iter().map(|day| day.created).sum();
    let total_votes = timeline.iter().map(|day| day.votes).sum();
    let total_status_changes = timeline.iter().map(|day| day.status_changes).sum();

    Ok(Json(FeatureAuditSummary {
        days: AUDIT_WINDOW_DAYS,
        total_created,
        total_votes,
        total_status_changes,
        timeline,
        generated_at: chrono::Utc::now(),
    }))
}
