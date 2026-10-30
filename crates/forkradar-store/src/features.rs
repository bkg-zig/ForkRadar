//! Persistence of feature suggestions and their votes.
//!
//! Every query carries the owner id. A suggestion of another user is not
//! reachable through this module, not even by guessing its id.

use chrono::{DateTime, Utc};
use forkradar_domain::api::{FeatureAuditDay, FeatureSuggestion};
use forkradar_domain::features::{
    ranking_score, ranking_score_total, roi, FeatureSort, FeatureStatus,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{PgStore, Result, StoreError};

/// A stored feature suggestion of one user.
#[derive(Debug, Clone)]
pub struct FeatureSuggestionRecord {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub area: String,
    pub impact: i32,
    pub effort: i32,
    pub status: String,
    pub votes: i64,
    pub voted_by_me: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FeatureSuggestionRecord {
    /// Builds the API view of the suggestion.
    ///
    /// The score comes from the shared domain function. The browser never
    /// calculates it, so every client shows the same number.
    pub fn to_suggestion(&self) -> FeatureSuggestion {
        let status = FeatureStatus::parse(&self.status).unwrap_or(FeatureStatus::Proposed);
        FeatureSuggestion {
            id: self.id,
            title: self.title.clone(),
            description: self.description.clone(),
            area: self.area.clone(),
            impact: self.impact,
            effort: self.effort,
            status: status.code().to_string(),
            status_label: status.label().to_string(),
            votes: self.votes,
            voted_by_me: self.voted_by_me,
            roi: roi(self.impact, self.effort),
            score: ranking_score_total(self.impact, self.effort, self.votes, status),
            breakdown: ranking_score(self.impact, self.effort, self.votes, status),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Filter of the feature list.
#[derive(Debug, Clone, Default)]
pub struct FeatureFilter {
    /// Free text over title, description and area.
    pub text: Option<String>,
    /// Exact area.
    pub area: Option<String>,
    /// Exact lifecycle code.
    pub status: Option<String>,
    /// Lowest accepted impact.
    pub min_impact: Option<i32>,
    /// Highest accepted effort.
    pub max_effort: Option<i32>,
    /// Lowest accepted number of votes.
    pub min_votes: Option<i64>,
}

/// Number of days of the audit timeline.
pub const AUDIT_WINDOW_DAYS: i32 = 30;

const SUGGESTION_COLUMNS: &str = "s.id, s.title, s.description, s.area, s.impact, s.effort, \
     s.status, s.created_at, s.updated_at, \
     (SELECT count(*) FROM feature_votes v WHERE v.suggestion_id = s.id)::BIGINT AS votes, \
     EXISTS (SELECT 1 FROM feature_votes v WHERE v.suggestion_id = s.id AND v.user_id = $1) \
         AS voted_by_me";

impl PgStore {
    /// Lists the suggestions of one user with the filters applied.
    ///
    /// The query uses one fixed set of parameters. A filter that is not set
    /// is bound as `NULL` and its condition stays inactive.
    pub async fn list_feature_suggestions(
        &self,
        user_id: Uuid,
        filter: &FeatureFilter,
        sort: FeatureSort,
    ) -> Result<Vec<FeatureSuggestion>> {
        let text = filter
            .text
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", escape_like(value)));
        let area = filter
            .area
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        let status = filter
            .status
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());

        let rows = sqlx::query(
            r#"
            SELECT id, title, description, area, impact, effort, status, created_at, updated_at,
                   votes, voted_by_me
            FROM (
                SELECT s.id, s.title, s.description, s.area, s.impact, s.effort, s.status,
                       s.created_at, s.updated_at,
                       (SELECT count(*) FROM feature_votes v WHERE v.suggestion_id = s.id)::BIGINT
                           AS votes,
                       EXISTS (
                           SELECT 1 FROM feature_votes v
                           WHERE v.suggestion_id = s.id AND v.user_id = $1
                       ) AS voted_by_me
                FROM feature_suggestions s
                WHERE s.user_id = $1
                  AND ($2::TEXT IS NULL
                       OR s.title ILIKE $2 ESCAPE '\'
                       OR s.description ILIKE $2 ESCAPE '\'
                       OR s.area ILIKE $2 ESCAPE '\')
                  AND ($3::TEXT IS NULL OR s.area = $3)
                  AND ($4::TEXT IS NULL OR s.status = $4)
                  AND ($5::INT IS NULL OR s.impact >= $5)
                  AND ($6::INT IS NULL OR s.effort <= $6)
                  AND ($7::BIGINT IS NULL
                       OR (SELECT count(*) FROM feature_votes v WHERE v.suggestion_id = s.id) >= $7)
            ) AS listed
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .bind(text)
        .bind(area)
        .bind(status)
        .bind(filter.min_impact)
        .bind(filter.max_effort)
        .bind(filter.min_votes)
        .fetch_all(self.pool())
        .await?;

        let mut records: Vec<FeatureSuggestionRecord> = rows
            .into_iter()
            .map(|row| FeatureSuggestionRecord {
                id: row.get("id"),
                title: row.get("title"),
                description: row.get("description"),
                area: row.get("area"),
                impact: row.get("impact"),
                effort: row.get("effort"),
                status: row.get("status"),
                votes: row.get("votes"),
                voted_by_me: row.get("voted_by_me"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        sort_records(&mut records, sort);
        Ok(records
            .iter()
            .map(FeatureSuggestionRecord::to_suggestion)
            .collect())
    }

    /// Reads one suggestion of the user. `None` means the row does not exist
    /// or belongs to another user.
    pub async fn get_feature_suggestion(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<Option<FeatureSuggestion>> {
        let sql = format!(
            "SELECT {SUGGESTION_COLUMNS} FROM feature_suggestions s WHERE s.user_id = $1 AND s.id = $2"
        );
        let row = sqlx::query(&sql)
            .bind(user_id)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(|row| feature_from_row(row).to_suggestion()))
    }

    /// Stores a new suggestion for the user.
    pub async fn create_feature_suggestion(
        &self,
        user_id: Uuid,
        title: &str,
        description: &str,
        area: &str,
        impact: i32,
        effort: i32,
    ) -> Result<Uuid> {
        if !(1..=10).contains(&impact) {
            return Err(StoreError::Invalid(
                "impact must be between 1 and 10".into(),
            ));
        }
        if !(1..=10).contains(&effort) {
            return Err(StoreError::Invalid(
                "effort must be between 1 and 10".into(),
            ));
        }
        let title = title.trim();
        if title.is_empty() {
            return Err(StoreError::Invalid("title must not be empty".into()));
        }

        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO feature_suggestions (id, user_id, title, description, area, impact, effort)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(description.trim())
        .bind(area.trim())
        .bind(impact)
        .bind(effort)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Adds the vote of the user. A second vote on the same suggestion does
    /// not create a second row.
    ///
    /// `false` means the suggestion does not exist for this user.
    pub async fn add_feature_vote(&self, user_id: Uuid, suggestion_id: Uuid) -> Result<bool> {
        let owned = self.get_feature_suggestion(user_id, suggestion_id).await?;
        if owned.is_none() {
            return Ok(false);
        }
        let result = sqlx::query(
            r#"
            INSERT INTO feature_votes (user_id, suggestion_id)
            VALUES ($1, $2)
            ON CONFLICT (user_id, suggestion_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(suggestion_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Removes the vote of the user.
    pub async fn remove_feature_vote(&self, user_id: Uuid, suggestion_id: Uuid) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM feature_votes WHERE user_id = $1 AND suggestion_id = $2")
                .bind(user_id)
                .bind(suggestion_id)
                .execute(self.pool())
                .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Changes the lifecycle state of a suggestion of the user.
    pub async fn set_feature_status(
        &self,
        user_id: Uuid,
        suggestion_id: Uuid,
        status: FeatureStatus,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE feature_suggestions SET status = $3, updated_at = now() \
             WHERE id = $2 AND user_id = $1",
        )
        .bind(user_id)
        .bind(suggestion_id)
        .bind(status.code())
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Counts the feature events of the user per day over the last days.
    ///
    /// The result contains one entry per day, oldest first. Days without an
    /// event are present with a zero count so that the chart has no gaps.
    pub async fn feature_audit_timeline(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<Vec<FeatureAuditDay>> {
        let rows = sqlx::query(
            r#"
            WITH days AS (
                SELECT generate_series(
                    (now() AT TIME ZONE 'UTC')::date - ($2::INT - 1),
                    (now() AT TIME ZONE 'UTC')::date,
                    interval '1 day'
                )::date AS day
            )
            SELECT to_char(d.day, 'YYYY-MM-DD') AS day,
                   count(a.id) FILTER (WHERE a.event_type = 'feature.created')::BIGINT AS created,
                   count(a.id) FILTER (WHERE a.event_type = 'feature.voted')::BIGINT AS votes,
                   count(a.id) FILTER (WHERE a.event_type = 'feature.status_changed')::BIGINT
                       AS status_changes
            FROM days d
            LEFT JOIN audit_events a
                ON a.user_id = $1
                AND (a.created_at AT TIME ZONE 'UTC')::date = d.day
                AND a.event_type IN ('feature.created', 'feature.voted', 'feature.status_changed')
            GROUP BY d.day
            ORDER BY d.day
            "#,
        )
        .bind(user_id)
        .bind(days)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FeatureAuditDay {
                date: row.get("day"),
                created: row.get("created"),
                votes: row.get("votes"),
                status_changes: row.get("status_changes"),
            })
            .collect())
    }
}

/// Sorts the records by the requested mode.
///
/// The ranking uses the score of the shared domain function. The comparison
/// is stable, so suggestions with the same key keep the order of the query.
fn sort_records(records: &mut [FeatureSuggestionRecord], sort: FeatureSort) {
    match sort {
        FeatureSort::RankingScore => records.sort_by(|a, b| {
            let left = ranking_score_total(a.impact, a.effort, a.votes, status_of(a));
            let right = ranking_score_total(b.impact, b.effort, b.votes, status_of(b));
            right
                .partial_cmp(&left)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.created_at.cmp(&a.created_at))
        }),
        FeatureSort::Votes => records.sort_by(|a, b| {
            b.votes
                .cmp(&a.votes)
                .then_with(|| b.created_at.cmp(&a.created_at))
        }),
        FeatureSort::Impact => records.sort_by(|a, b| {
            b.impact
                .cmp(&a.impact)
                .then_with(|| b.created_at.cmp(&a.created_at))
        }),
        FeatureSort::Roi => records.sort_by(|a, b| {
            let left = roi(a.impact, a.effort);
            let right = roi(b.impact, b.effort);
            right
                .partial_cmp(&left)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.created_at.cmp(&a.created_at))
        }),
        FeatureSort::Newest => records.sort_by_key(|a| std::cmp::Reverse(a.created_at)),
    }
}

/// Reads the lifecycle state of a record. Unknown states count as proposed.
fn status_of(record: &FeatureSuggestionRecord) -> FeatureStatus {
    FeatureStatus::parse(&record.status).unwrap_or(FeatureStatus::Proposed)
}

/// Reads one row of the suggestion query.
fn feature_from_row(row: sqlx::postgres::PgRow) -> FeatureSuggestionRecord {
    FeatureSuggestionRecord {
        id: row.get("id"),
        title: row.get("title"),
        description: row.get("description"),
        area: row.get("area"),
        impact: row.get("impact"),
        effort: row.get("effort"),
        status: row.get("status"),
        votes: row.get("votes"),
        voted_by_me: row.get("voted_by_me"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// Escapes the wildcards of a `LIKE` pattern.
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
