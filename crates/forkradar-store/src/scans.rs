use chrono::{DateTime, Utc};
use forkradar_domain::api::{GraphEdge, GraphNode, GraphPayload, ScanJobView};
use forkradar_domain::model::{JobStatus, RelationshipKind, RelationshipVerification, RepositoryNode};
use sqlx::Row;
use uuid::Uuid;

use crate::{PgStore, Result};

/// A scan run with its counters.
#[derive(Debug, Clone)]
pub struct ScanRunRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub root_github_id: i64,
    pub status: JobStatus,
    pub max_depth: Option<i32>,
    pub discovery_mode: String,
    pub config_version: i32,
    pub discovered_count: i64,
    pub processed_count: i64,
    pub pending_count: i64,
    pub failed_count: i64,
    pub max_reached_depth: i32,
    pub pages_fetched: i64,
    pub rate_limit_limit: Option<i32>,
    pub rate_limit_remaining: Option<i32>,
    pub rate_limit_reset_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// One discovery job.
#[derive(Debug, Clone)]
pub struct ScanJob {
    pub id: Uuid,
    pub run_id: Uuid,
    pub repository_github_id: i64,
    pub job_type: String,
    pub depth: i32,
    pub status: JobStatus,
    pub attempt_count: i32,
    pub cursor_page: Option<i32>,
    pub pages_done: i32,
    pub forks_found: i32,
    pub last_success_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Result of storing one fetched page.
#[derive(Debug, Clone, Default)]
pub struct PageOutcome {
    pub repositories_inserted: i64,
    pub relationships_inserted: i64,
    pub jobs_enqueued: i64,
}

const RUN_COLUMNS: &str = "id, user_id, root_github_id, status, max_depth, discovery_mode, \
     config_version, discovered_count, processed_count, pending_count, failed_count, \
     max_reached_depth, pages_fetched, rate_limit_limit, rate_limit_remaining, \
     rate_limit_reset_at, last_success_at, last_error, created_at, started_at, \
     updated_at, finished_at";

const JOB_COLUMNS: &str = "id, run_id, repository_github_id, job_type, depth, status, \
     attempt_count, cursor_page, pages_done, forks_found, last_success_at, next_retry_at, \
     error_code, error_message, created_at, updated_at, finished_at";

impl PgStore {
    /// Creates a scan run in state `DISCOVERING`.
    pub async fn create_run(
        &self,
        user_id: Uuid,
        root_github_id: i64,
        max_depth: Option<i32>,
    ) -> Result<Uuid> {
        let run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO scan_runs (id, user_id, root_github_id, status, max_depth, discovery_mode)
            VALUES ($1, $2, $3, 'DISCOVERING', $4, 'exhaustive')
            "#,
        )
        .bind(run_id)
        .bind(user_id)
        .bind(root_github_id)
        .bind(max_depth)
        .execute(self.pool())
        .await?;
        Ok(run_id)
    }

    pub async fn get_run(&self, run_id: Uuid) -> Result<Option<ScanRunRecord>> {
        let sql = format!("SELECT {RUN_COLUMNS} FROM scan_runs WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(run_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(run_from_row))
    }

    pub async fn list_runs(&self, user_id: Uuid, limit: i64) -> Result<Vec<ScanRunRecord>> {
        let sql = format!(
            "SELECT {RUN_COLUMNS} FROM scan_runs WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(user_id)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// Lists runs that still have open work, for example after a server restart.
    pub async fn list_open_runs(&self) -> Result<Vec<ScanRunRecord>> {
        let sql = format!(
            "SELECT {RUN_COLUMNS} FROM scan_runs WHERE status IN ('DISCOVERING','QUEUED','RUNNING','PARTIAL') \
             ORDER BY created_at ASC"
        );
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// Adds the watched repository itself to the result set of a run.
    pub async fn insert_root_repository(&self, run_id: Uuid, github_id: i64) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO scan_repositories (run_id, github_id, depth, relation_kind, verification)
            VALUES ($1, $2, 0, $3, $4)
            ON CONFLICT (run_id, github_id) DO NOTHING
            "#,
        )
        .bind(run_id)
        .bind(github_id)
        .bind(RelationshipKind::MainSource.as_str())
        .bind(RelationshipVerification::ConfirmedByForkListing.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Adds a discovery job. Returns `false` when the job already exists.
    pub async fn enqueue_job(
        &self,
        run_id: Uuid,
        repository_github_id: i64,
        depth: i32,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO scan_jobs (id, run_id, repository_github_id, depth, status)
            VALUES ($1, $2, $3, $4, 'QUEUED')
            ON CONFLICT (run_id, repository_github_id) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(repository_github_id)
        .bind(depth)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Claims the next job of a run. Running jobs of other workers are skipped.
    pub async fn claim_next_job(&self, run_id: Uuid) -> Result<Option<ScanJob>> {
        let mut tx = self.pool().begin().await?;
        let sql = format!(
            "SELECT {JOB_COLUMNS} FROM scan_jobs \
             WHERE run_id = $1 AND status = 'QUEUED' \
               AND (next_retry_at IS NULL OR next_retry_at <= now()) \
             ORDER BY depth ASC, created_at ASC \
             FOR UPDATE SKIP LOCKED LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };

        let job_id: Uuid = row.get("id");
        sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = 'RUNNING', attempt_count = attempt_count + 1,
                started_at = COALESCE(started_at, now()), updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        // The row was read before the update. The returned job carries the
        // incremented attempt counter so the retry limit stays correct.
        let mut job = job_from_row(row);
        job.attempt_count += 1;
        job.status = JobStatus::Running;
        Ok(Some(job))
    }

    /// Stores one fetched fork page: repositories, relationships, child jobs and
    /// the checkpoint of the job.
    pub async fn record_page(
        &self,
        job: &ScanJob,
        parent_full_name: &str,
        forks: &[RepositoryNode],
        next_page: Option<u32>,
        root_github_id: i64,
        max_depth: Option<i32>,
    ) -> Result<PageOutcome> {
        let mut tx = self.pool().begin().await?;
        let mut outcome = PageOutcome::default();
        let child_depth = job.depth + 1;

        for fork in forks {
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
            .bind(fork.id)
            .bind(&fork.owner)
            .bind(&fork.name)
            .bind(&fork.full_name)
            .bind(&fork.html_url)
            .bind(fork.is_fork)
            .bind(fork.parent.as_ref().map(|parent| parent.full_name()))
            .bind(fork.source.as_ref().map(|source| source.full_name()))
            .bind(&fork.default_branch)
            .bind(&fork.language)
            .bind(&fork.description)
            .bind(fork.stargazers_count)
            .bind(fork.forks_count)
            .bind(fork.open_issues_count)
            .bind(fork.size_kb)
            .bind(fork.archived)
            .bind(fork.disabled)
            .bind(&fork.visibility)
            .bind(fork.created_at)
            .bind(fork.updated_at)
            .bind(fork.pushed_at)
            .execute(&mut *tx)
            .await?;

            let inserted = sqlx::query(
                r#"
                INSERT INTO scan_repositories (run_id, github_id, depth, relation_kind, verification)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (run_id, github_id) DO NOTHING
                "#,
            )
            .bind(job.run_id)
            .bind(fork.id)
            .bind(child_depth)
            .bind(RelationshipKind::VerifiedImmediateParent.as_str())
            .bind(RelationshipVerification::ConfirmedByForkListing.as_str())
            .execute(&mut *tx)
            .await?;
            if inserted.rows_affected() > 0 {
                outcome.repositories_inserted += 1;
            }

            let edge = sqlx::query(
                r#"
                INSERT INTO repository_relationships
                    (id, run_id, from_github_id, to_github_id, relation_kind, verification, depth, source_endpoint)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (run_id, from_github_id, to_github_id, relation_kind) DO NOTHING
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(job.run_id)
            .bind(job.repository_github_id)
            .bind(fork.id)
            .bind(RelationshipKind::VerifiedImmediateParent.as_str())
            .bind(RelationshipVerification::ConfirmedByForkListing.as_str())
            .bind(child_depth)
            .bind(format!("GET /repos/{parent_full_name}/forks"))
            .execute(&mut *tx)
            .await?;
            if edge.rows_affected() > 0 {
                outcome.relationships_inserted += 1;
            }

            // Relationships that GitHub itself reports. They are only stored when
            // the referenced repository is already known.
            if let Some(parent) = &fork.parent {
                let parent_id = lookup_repository_id(&mut tx, &parent.full_name()).await?;
                if let Some(parent_id) = parent_id {
                    outcome.relationships_inserted += insert_relationship(
                        &mut tx,
                        job.run_id,
                        fork.id,
                        parent_id,
                        RelationshipKind::GithubReportedParent,
                        RelationshipVerification::ReportedByGithub,
                        Some(child_depth),
                    )
                    .await?;
                }
            }
            if let Some(source) = &fork.source {
                let source_id = lookup_repository_id(&mut tx, &source.full_name()).await?;
                if let Some(source_id) = source_id {
                    outcome.relationships_inserted += insert_relationship(
                        &mut tx,
                        job.run_id,
                        fork.id,
                        source_id,
                        RelationshipKind::GithubReportedSource,
                        RelationshipVerification::ReportedByGithub,
                        Some(child_depth),
                    )
                    .await?;
                    if source_id == root_github_id {
                        outcome.relationships_inserted += insert_relationship(
                            &mut tx,
                            job.run_id,
                            root_github_id,
                            fork.id,
                            RelationshipKind::ForkNetworkMember,
                            RelationshipVerification::ReportedByGithub,
                            Some(child_depth),
                        )
                        .await?;
                    }
                }
            }

            // The fork becomes part of the discovery only when the depth limit allows it.
            let expand = match max_depth {
                None => true,
                Some(limit) => child_depth <= limit,
            };
            if expand {
                let enqueued = sqlx::query(
                    r#"
                    INSERT INTO scan_jobs (id, run_id, repository_github_id, depth, status)
                    VALUES ($1, $2, $3, $4, 'QUEUED')
                    ON CONFLICT (run_id, repository_github_id) DO NOTHING
                    "#,
                )
                .bind(Uuid::new_v4())
                .bind(job.run_id)
                .bind(fork.id)
                .bind(child_depth)
                .execute(&mut *tx)
                .await?;
                if enqueued.rows_affected() > 0 {
                    outcome.jobs_enqueued += 1;
                }
            }
        }

        sqlx::query(
            r#"
            UPDATE scan_jobs
            SET cursor_page = $2, pages_done = pages_done + 1,
                forks_found = forks_found + $3, last_success_at = now(), updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job.id)
        .bind(next_page.map(|page| page as i32))
        .bind(forks.len() as i32)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE scan_runs
            SET discovered_count = (SELECT count(*) FROM scan_repositories WHERE run_id = $1),
                max_reached_depth = GREATEST(
                    max_reached_depth,
                    COALESCE((SELECT max(depth) FROM scan_repositories WHERE run_id = $1), 0)),
                pages_fetched = pages_fetched + 1,
                last_success_at = now(),
                status = 'RUNNING',
                started_at = COALESCE(started_at, now()),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job.run_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(outcome)
    }

    /// Marks a job as completed.
    pub async fn complete_job(&self, job_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = 'COMPLETED', cursor_page = NULL, next_retry_at = NULL,
                error_code = NULL, error_message = NULL, finished_at = now(), updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Pauses a job without counting it as an error. The checkpoint stays intact.
    pub async fn pause_job(
        &self,
        job_id: Uuid,
        error_code: &str,
        error_message: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = 'QUEUED', next_retry_at = $2, error_code = $3,
                error_message = $4, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(retry_at)
        .bind(error_code)
        .bind(error_message)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Stores the error of a job. Retryable errors stay queued with a retry time,
    /// permanent errors finish the job as failed.
    pub async fn fail_job(
        &self,
        job_id: Uuid,
        error_code: &str,
        error_message: &str,
        retry_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        match retry_at {
            Some(retry_at) => {
                sqlx::query(
                    r#"
                    UPDATE scan_jobs
                    SET status = 'QUEUED', next_retry_at = $2, error_code = $3,
                        error_message = $4, updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(job_id)
                .bind(retry_at)
                .bind(error_code)
                .bind(error_message)
                .execute(self.pool())
                .await?;
            }
            None => {
                sqlx::query(
                    r#"
                    UPDATE scan_jobs
                    SET status = 'FAILED', next_retry_at = NULL, error_code = $2,
                        error_message = $3, finished_at = now(), updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(job_id)
                .bind(error_code)
                .bind(error_message)
                .execute(self.pool())
                .await?;
            }
        }
        Ok(())
    }

    /// Recomputes the counters of a run and stores its status.
    pub async fn refresh_run(&self, run_id: Uuid) -> Result<ScanRunRecord> {
        let counts = sqlx::query(
            r#"
            SELECT
                count(*) FILTER (WHERE status = 'COMPLETED') AS completed,
                count(*) FILTER (WHERE status = 'QUEUED') AS queued,
                count(*) FILTER (WHERE status = 'RUNNING') AS running,
                count(*) FILTER (WHERE status = 'FAILED') AS failed,
                count(*) FILTER (WHERE status = 'CANCELLED') AS cancelled
            FROM scan_jobs WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(self.pool())
        .await?;

        let completed: i64 = counts.get("completed");
        let queued: i64 = counts.get("queued");
        let running: i64 = counts.get("running");
        let failed: i64 = counts.get("failed");
        let cancelled: i64 = counts.get("cancelled");
        let total = completed + queued + running + failed + cancelled;

        let status = if total == 0 {
            JobStatus::Failed
        } else if cancelled == total {
            JobStatus::Cancelled
        } else if completed == total {
            JobStatus::Completed
        } else if failed == total {
            JobStatus::Failed
        } else if failed > 0 || queued > 0 || running > 0 {
            JobStatus::Partial
        } else {
            JobStatus::Partial
        };

        sqlx::query(
            r#"
            UPDATE scan_runs
            SET processed_count = $2, pending_count = $3, failed_count = $4,
                status = $5, updated_at = now(),
                finished_at = CASE WHEN $5 IN ('COMPLETED','FAILED','CANCELLED') THEN now() ELSE finished_at END
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .bind(completed)
        .bind(queued + running)
        .bind(failed)
        .bind(status.as_str())
        .execute(self.pool())
        .await?;

        self.get_run(run_id)
            .await?
            .ok_or(crate::StoreError::NotFound("scan run"))
    }

    /// Stores the rate limit that was observed during the run.
    pub async fn set_run_rate_limit(
        &self,
        run_id: Uuid,
        limit: i32,
        remaining: i32,
        reset_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scan_runs
            SET rate_limit_limit = $2, rate_limit_remaining = $3, rate_limit_reset_at = $4,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .bind(limit)
        .bind(remaining)
        .bind(reset_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Stores a note on the run, for example a rate limit that stopped the work.
    pub async fn set_run_error(&self, run_id: Uuid, message: &str) -> Result<()> {
        sqlx::query("UPDATE scan_runs SET last_error = $2, updated_at = now() WHERE id = $1")
            .bind(run_id)
            .bind(message)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_run_status(&self, run_id: Uuid, status: JobStatus) -> Result<()> {
        sqlx::query(
            "UPDATE scan_runs SET status = $2, updated_at = now(), finished_at = CASE WHEN $2 IN ('COMPLETED','FAILED','CANCELLED') THEN now() ELSE finished_at END WHERE id = $1",
        )
        .bind(run_id)
        .bind(status.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Lists the jobs of a run.
    pub async fn list_jobs(&self, run_id: Uuid) -> Result<Vec<ScanJob>> {
        let sql = format!(
            "SELECT {JOB_COLUMNS} FROM scan_jobs WHERE run_id = $1 ORDER BY depth ASC, created_at ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(run_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(job_from_row).collect())
    }

    /// Lists the jobs of a run for the API.
    pub async fn list_job_views(&self, run_id: Uuid) -> Result<Vec<ScanJobView>> {
        let rows = sqlx::query(
            r#"
            SELECT j.id, j.repository_github_id, j.job_type, j.depth, j.status, j.attempt_count,
                   j.cursor_page, j.pages_done, j.forks_found, j.last_success_at, j.next_retry_at,
                   j.error_code, j.error_message, j.created_at, j.updated_at, j.finished_at,
                   r.full_name
            FROM scan_jobs j
            JOIN repositories r ON r.github_id = j.repository_github_id
            WHERE j.run_id = $1
            ORDER BY j.depth ASC, j.created_at ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ScanJobView {
                id: row.get("id"),
                repository_full_name: row.get("full_name"),
                repository_github_id: row.get("repository_github_id"),
                job_type: row.get("job_type"),
                depth: row.get("depth"),
                status: JobStatus::parse(row.get::<&str, _>("status")).unwrap_or(JobStatus::Failed),
                attempt_count: row.get("attempt_count"),
                cursor_page: row.get("cursor_page"),
                pages_done: row.get("pages_done"),
                forks_found: row.get("forks_found"),
                last_success_at: row.get("last_success_at"),
                next_retry_at: row.get("next_retry_at"),
                error_code: row.get("error_code"),
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                finished_at: row.get("finished_at"),
            })
            .collect())
    }

    /// Builds the graph payload of a run.
    pub async fn graph_payload(&self, run_id: Uuid) -> Result<GraphPayload> {
        let node_rows = sqlx::query(
            r#"
            SELECT r.github_id, r.owner, r.name, r.full_name, r.is_fork, r.language, r.pushed_at,
                   r.archived, s.depth, s.relation_kind, s.verification
            FROM scan_repositories s
            JOIN repositories r ON r.github_id = s.github_id
            WHERE s.run_id = $1
            ORDER BY s.depth, r.full_name
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        let nodes: Vec<GraphNode> = node_rows
            .into_iter()
            .map(|row| GraphNode {
                id: row.get("github_id"),
                label: row.get("full_name"),
                owner: row.get("owner"),
                name: row.get("name"),
                depth: row.get("depth"),
                is_fork: row.get("is_fork"),
                language: row.get("language"),
                pushed_at: row.get("pushed_at"),
                archived: row.get("archived"),
                relation_kind: RelationshipKind::parse(row.get::<&str, _>("relation_kind"))
                    .unwrap_or(RelationshipKind::Unclear),
                verification: RelationshipVerification::parse(row.get::<&str, _>("verification"))
                    .unwrap_or(RelationshipVerification::Unverified),
            })
            .collect();

        let edge_rows = sqlx::query(
            r#"
            SELECT rel.from_github_id, rel.to_github_id, rel.relation_kind, rel.verification, rel.depth
            FROM repository_relationships rel
            JOIN scan_repositories source ON source.run_id = rel.run_id AND source.github_id = rel.from_github_id
            JOIN scan_repositories target ON target.run_id = rel.run_id AND target.github_id = rel.to_github_id
            WHERE rel.run_id = $1
            ORDER BY rel.depth NULLS LAST, rel.discovered_at
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;

        let edges: Vec<GraphEdge> = edge_rows
            .into_iter()
            .map(|row| GraphEdge {
                source: row.get("from_github_id"),
                target: row.get("to_github_id"),
                kind: RelationshipKind::parse(row.get::<&str, _>("relation_kind"))
                    .unwrap_or(RelationshipKind::Unclear),
                verification: RelationshipVerification::parse(row.get::<&str, _>("verification"))
                    .unwrap_or(RelationshipVerification::Unverified),
                depth: row.get("depth"),
            })
            .collect();

        Ok(GraphPayload { nodes, edges })
    }

    /// Moves failed jobs of a run back into the queue.
    pub async fn requeue_failed_jobs(&self, run_id: Uuid) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = 'QUEUED', next_retry_at = NULL, finished_at = NULL, updated_at = now()
            WHERE run_id = $1 AND status = 'FAILED'
            "#,
        )
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Cancels the open jobs of a run.
    pub async fn cancel_run(&self, run_id: Uuid) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE scan_jobs
            SET status = 'CANCELLED', next_retry_at = NULL, finished_at = now(), updated_at = now()
            WHERE run_id = $1 AND status IN ('QUEUED','RUNNING')
            "#,
        )
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
}

async fn lookup_repository_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    full_name: &str,
) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar("SELECT github_id FROM repositories WHERE full_name = $1")
        .bind(full_name)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(id)
}

async fn insert_relationship(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    from_github_id: i64,
    to_github_id: i64,
    kind: RelationshipKind,
    verification: RelationshipVerification,
    depth: Option<i32>,
) -> Result<i64> {
    let result = sqlx::query(
        r#"
        INSERT INTO repository_relationships
            (id, run_id, from_github_id, to_github_id, relation_kind, verification, depth)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (run_id, from_github_id, to_github_id, relation_kind) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(run_id)
    .bind(from_github_id)
    .bind(to_github_id)
    .bind(kind.as_str())
    .bind(verification.as_str())
    .bind(depth)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() as i64)
}

fn run_from_row(row: sqlx::postgres::PgRow) -> ScanRunRecord {
    ScanRunRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        root_github_id: row.get("root_github_id"),
        status: JobStatus::parse(row.get::<&str, _>("status")).unwrap_or(JobStatus::Failed),
        max_depth: row.get("max_depth"),
        discovery_mode: row.get("discovery_mode"),
        config_version: row.get("config_version"),
        discovered_count: row.get("discovered_count"),
        processed_count: row.get("processed_count"),
        pending_count: row.get("pending_count"),
        failed_count: row.get("failed_count"),
        max_reached_depth: row.get("max_reached_depth"),
        pages_fetched: row.get("pages_fetched"),
        rate_limit_limit: row.get("rate_limit_limit"),
        rate_limit_remaining: row.get("rate_limit_remaining"),
        rate_limit_reset_at: row.get("rate_limit_reset_at"),
        last_success_at: row.get("last_success_at"),
        last_error: row.get("last_error"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        updated_at: row.get("updated_at"),
        finished_at: row.get("finished_at"),
    }
}

fn job_from_row(row: sqlx::postgres::PgRow) -> ScanJob {
    ScanJob {
        id: row.get("id"),
        run_id: row.get("run_id"),
        repository_github_id: row.get("repository_github_id"),
        job_type: row.get("job_type"),
        depth: row.get("depth"),
        status: JobStatus::parse(row.get::<&str, _>("status")).unwrap_or(JobStatus::Failed),
        attempt_count: row.get("attempt_count"),
        cursor_page: row.get("cursor_page"),
        pages_done: row.get("pages_done"),
        forks_found: row.get("forks_found"),
        last_success_at: row.get("last_success_at"),
        next_retry_at: row.get("next_retry_at"),
        error_code: row.get("error_code"),
        error_message: row.get("error_message"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        finished_at: row.get("finished_at"),
    }
}
