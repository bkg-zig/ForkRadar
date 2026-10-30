use chrono::{DateTime, Utc};
use forkradar_domain::api::DiscoveredRepository;
use forkradar_domain::model::{RelationshipKind, RelationshipVerification, RepositoryNode};
use sqlx::Row;
use uuid::Uuid;

use crate::{PgStore, Result};

/// A repository on the watchlist of a user.
#[derive(Debug, Clone)]
pub struct WatchedRepositoryRecord {
    pub id: Uuid,
    pub github_id: i64,
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub is_fork: bool,
    pub created_at: DateTime<Utc>,
    pub last_scan_id: Option<Uuid>,
}

const REPOSITORY_COLUMNS: &str = "github_id, owner, name, full_name, html_url, is_fork, \
     parent_full_name, source_full_name, default_branch, language, description, \
     stargazers_count, forks_count, open_issues_count, size_kb, archived, disabled, \
     visibility, created_at, updated_at, pushed_at";

impl PgStore {
    /// Writes repository metadata. Existing rows are updated with the fresh values.
    pub async fn upsert_repository(&self, node: &RepositoryNode) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO repositories (
                github_id, owner, name, full_name, html_url, is_fork, parent_full_name,
                source_full_name, default_branch, language, description, stargazers_count,
                forks_count, open_issues_count, size_kb, archived, disabled, visibility,
                created_at, updated_at, pushed_at, fetched_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, now())
            ON CONFLICT (github_id) DO UPDATE SET
                owner = EXCLUDED.owner,
                name = EXCLUDED.name,
                full_name = EXCLUDED.full_name,
                html_url = EXCLUDED.html_url,
                is_fork = EXCLUDED.is_fork,
                parent_full_name = EXCLUDED.parent_full_name,
                source_full_name = EXCLUDED.source_full_name,
                default_branch = EXCLUDED.default_branch,
                language = EXCLUDED.language,
                description = EXCLUDED.description,
                stargazers_count = EXCLUDED.stargazers_count,
                forks_count = EXCLUDED.forks_count,
                open_issues_count = EXCLUDED.open_issues_count,
                size_kb = EXCLUDED.size_kb,
                archived = EXCLUDED.archived,
                disabled = EXCLUDED.disabled,
                visibility = EXCLUDED.visibility,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at,
                pushed_at = EXCLUDED.pushed_at,
                fetched_at = now()
            "#,
        )
        .bind(node.id)
        .bind(&node.owner)
        .bind(&node.name)
        .bind(&node.full_name)
        .bind(&node.html_url)
        .bind(node.is_fork)
        .bind(node.parent.as_ref().map(|parent| parent.full_name()))
        .bind(node.source.as_ref().map(|source| source.full_name()))
        .bind(&node.default_branch)
        .bind(&node.language)
        .bind(&node.description)
        .bind(node.stargazers_count)
        .bind(node.forks_count)
        .bind(node.open_issues_count)
        .bind(node.size_kb)
        .bind(node.archived)
        .bind(node.disabled)
        .bind(&node.visibility)
        .bind(node.created_at)
        .bind(node.updated_at)
        .bind(node.pushed_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_repository(&self, github_id: i64) -> Result<Option<RepositoryNode>> {
        let sql = format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE github_id = $1");
        let row = sqlx::query(&sql)
            .bind(github_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(repository_from_row))
    }

    pub async fn find_repository_by_full_name(
        &self,
        full_name: &str,
    ) -> Result<Option<RepositoryNode>> {
        let sql = format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE full_name = $1");
        let row = sqlx::query(&sql)
            .bind(full_name)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(repository_from_row))
    }

    /// Adds a repository to the watchlist of a user. Repeated calls return the existing entry.
    pub async fn add_watched_repository(&self, user_id: Uuid, github_id: i64) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO watched_repositories (id, user_id, github_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, github_id) DO UPDATE SET github_id = EXCLUDED.github_id
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(github_id)
        .fetch_one(self.pool())
        .await?;
        Ok(row.get("id"))
    }

    /// Lists the watchlist of one user. Other users never see these rows.
    pub async fn list_watched_repositories(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<WatchedRepositoryRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT w.id, w.github_id, w.created_at, r.full_name, r.html_url, r.description,
                   r.language, r.is_fork, s.id AS last_scan_id
            FROM watched_repositories w
            JOIN repositories r ON r.github_id = w.github_id
            LEFT JOIN LATERAL (
                SELECT id FROM scan_runs
                WHERE user_id = w.user_id AND root_github_id = w.github_id
                ORDER BY created_at DESC LIMIT 1
            ) s ON true
            WHERE w.user_id = $1
            ORDER BY w.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| WatchedRepositoryRecord {
                id: row.get("id"),
                github_id: row.get("github_id"),
                full_name: row.get("full_name"),
                html_url: row.get("html_url"),
                description: row.get("description"),
                language: row.get("language"),
                is_fork: row.get("is_fork"),
                created_at: row.get("created_at"),
                last_scan_id: row.get("last_scan_id"),
            })
            .collect())
    }

    /// Removes a repository from the watchlist. The user id is part of the condition.
    pub async fn remove_watched_repository(&self, user_id: Uuid, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM watched_repositories WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Lists the repositories discovered by a run with their relation and depth.
    pub async fn list_scan_repositories(
        &self,
        run_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<DiscoveredRepository>> {
        let rows = sqlx::query(
            r#"
            SELECT r.github_id, r.owner, r.name, r.full_name, r.html_url, r.is_fork,
                   r.parent_full_name, r.source_full_name, r.default_branch, r.language,
                   r.stargazers_count, r.forks_count, r.pushed_at, r.archived, r.visibility,
                   s.depth, s.relation_kind, s.verification, s.discovered_at
            FROM scan_repositories s
            JOIN repositories r ON r.github_id = s.github_id
            WHERE s.run_id = $1
            ORDER BY s.depth, r.full_name
            OFFSET $2 LIMIT $3
            "#,
        )
        .bind(run_id)
        .bind(offset)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DiscoveredRepository {
                github_id: row.get("github_id"),
                owner: row.get("owner"),
                name: row.get("name"),
                full_name: row.get("full_name"),
                html_url: row.get("html_url"),
                is_fork: row.get("is_fork"),
                parent_full_name: row.get("parent_full_name"),
                source_full_name: row.get("source_full_name"),
                default_branch: row.get("default_branch"),
                language: row.get("language"),
                stargazers_count: row.get("stargazers_count"),
                forks_count: row.get("forks_count"),
                pushed_at: row.get("pushed_at"),
                archived: row.get("archived"),
                visibility: row.get("visibility"),
                depth: row.get("depth"),
                relation_kind: RelationshipKind::parse(row.get::<&str, _>("relation_kind"))
                    .unwrap_or(RelationshipKind::Unclear),
                verification: RelationshipVerification::parse(row.get::<&str, _>("verification"))
                    .unwrap_or(RelationshipVerification::Unverified),
                discovered_at: row.get("discovered_at"),
            })
            .collect())
    }

    /// Counts the repositories discovered by a run.
    pub async fn count_scan_repositories(&self, run_id: Uuid) -> Result<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM scan_repositories WHERE run_id = $1")
                .bind(run_id)
                .fetch_one(self.pool())
                .await?;
        Ok(count)
    }

    /// Checks whether the repository is known to the user.
    ///
    /// The repository counts as known when it sits on the watchlist of the
    /// user or when one of the runs of the user discovered it.
    pub async fn user_knows_repository(&self, user_id: Uuid, github_id: i64) -> Result<bool> {
        let known: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM watched_repositories
                WHERE user_id = $1 AND github_id = $2
            ) OR EXISTS(
                SELECT 1 FROM scan_repositories s
                JOIN scan_runs r ON r.id = s.run_id
                WHERE r.user_id = $1 AND s.github_id = $2
            )
            "#,
        )
        .bind(user_id)
        .bind(github_id)
        .fetch_one(self.pool())
        .await?;
        Ok(known)
    }
}

fn repository_from_row(row: sqlx::postgres::PgRow) -> RepositoryNode {
    RepositoryNode {
        id: row.get("github_id"),
        owner: row.get("owner"),
        name: row.get("name"),
        full_name: row.get("full_name"),
        html_url: row.get("html_url"),
        is_fork: row.get("is_fork"),
        parent: row
            .get::<Option<String>, _>("parent_full_name")
            .and_then(|full_name| split_full_name(&full_name)),
        source: row
            .get::<Option<String>, _>("source_full_name")
            .and_then(|full_name| split_full_name(&full_name)),
        default_branch: row.get("default_branch"),
        language: row.get("language"),
        description: row.get("description"),
        stargazers_count: row.get("stargazers_count"),
        forks_count: row.get("forks_count"),
        open_issues_count: row.get("open_issues_count"),
        size_kb: row.get("size_kb"),
        archived: row.get("archived"),
        disabled: row.get("disabled"),
        visibility: row.get("visibility"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        pushed_at: row.get("pushed_at"),
    }
}

fn split_full_name(full_name: &str) -> Option<forkradar_domain::model::RepositoryRef> {
    let (owner, name) = full_name.split_once('/')?;
    Some(forkradar_domain::model::RepositoryRef::new(owner, name))
}
