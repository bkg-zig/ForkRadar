//! Aggregated numbers for the personal dashboard.
//!
//! Every value is read from the stored rows of the user. The module never
//! estimates a missing number. Areas that are not collected yet stay empty
//! and are reported as `NOT_RUN` by the API.

use chrono::{DateTime, Utc};
use forkradar_domain::api::OverviewSummary;
use sqlx::Row;
use uuid::Uuid;

use crate::{PgStore, Result};

impl PgStore {
    /// Builds the dashboard summary of one user.
    pub async fn overview_summary(&self, user_id: Uuid) -> Result<OverviewSummary> {
        let watched_repository_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM watched_repositories WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(self.pool())
                .await?;

        let run_row = sqlx::query(
            r#"
            SELECT
                count(*) AS run_count,
                count(*) FILTER (WHERE status = 'COMPLETED') AS completed,
                count(*) FILTER (WHERE status IN ('DISCOVERING','QUEUED','RUNNING')) AS running,
                count(*) FILTER (WHERE status = 'FAILED') AS failed,
                count(*) FILTER (WHERE status = 'PARTIAL') AS partial,
                COALESCE(max(max_reached_depth), 0) AS max_depth,
                max(last_success_at) AS last_success_at
            FROM scan_runs
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(self.pool())
        .await?;

        let discovered_repository_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(DISTINCT s.github_id)
            FROM scan_repositories s
            JOIN scan_runs r ON r.id = s.run_id
            WHERE r.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(self.pool())
        .await?;

        let job_row = sqlx::query(
            r#"
            SELECT
                count(*) FILTER (WHERE j.status IN ('QUEUED','RUNNING')) AS open_jobs,
                count(*) FILTER (WHERE j.status = 'FAILED') AS failed_jobs
            FROM scan_jobs j
            JOIN scan_runs r ON r.id = j.run_id
            WHERE r.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(self.pool())
        .await?;

        let max_reached_depth: i32 = run_row.get("max_depth");
        let last_success_at: Option<DateTime<Utc>> = run_row.get("last_success_at");

        Ok(OverviewSummary {
            watched_repository_count,
            discovered_repository_count,
            // The root of a run has depth 0, so the number of generations is
            // the highest depth plus one.
            fork_generation_count: if discovered_repository_count > 0 {
                max_reached_depth + 1
            } else {
                0
            },
            max_reached_depth,
            run_count: run_row.get("run_count"),
            completed_run_count: run_row.get("completed"),
            running_run_count: run_row.get("running"),
            failed_run_count: run_row.get("failed"),
            partial_run_count: run_row.get("partial"),
            open_job_count: job_row.get("open_jobs"),
            failed_job_count: job_row.get("failed_jobs"),
            last_success_at,
            generated_at: Utc::now(),
        })
    }
}
