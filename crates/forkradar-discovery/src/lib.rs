//! Recursive fork discovery.
//!
//! The engine walks the fork network of a watched repository. It keeps the
//! traversal state in PostgreSQL, so a scan survives a restart of the server
//! and a rate limit pauses the work instead of losing it.

use chrono::{DateTime, Duration, Utc};
use forkradar_domain::model::RepositoryNode;
use forkradar_github::{GitHubClient, GitHubError};
use forkradar_store::{PgStore, ScanJob, ScanRunRecord};
use rand::Rng;
use uuid::Uuid;

/// Errors of the discovery engine.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Store(#[from] forkradar_store::StoreError),
    #[error("github client error: {0}")]
    Github(#[from] GitHubError),
    #[error("run {0} not found")]
    RunNotFound(Uuid),
    #[error("repository {0} not found")]
    RepositoryNotFound(i64),
}

/// Configuration of the engine.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Number of forks requested per API page.
    pub per_page: u32,
    /// Number of pages one job processes before it pauses and is resumed later.
    pub max_pages_per_job: u32,
    /// Number of attempts after which a job is marked as failed.
    pub max_attempts: i32,
    /// Base delay for the retry backoff.
    pub retry_base_delay: Duration,
    /// Upper bound for the retry backoff.
    pub retry_max_delay: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            per_page: 100,
            max_pages_per_job: 50,
            max_attempts: 5,
            retry_base_delay: Duration::seconds(30),
            retry_max_delay: Duration::minutes(15),
        }
    }
}

/// Engine that processes discovery jobs of a run.
#[derive(Debug, Clone)]
pub struct DiscoveryEngine {
    store: PgStore,
    github_base_url: String,
    config: DiscoveryConfig,
}

impl DiscoveryEngine {
    pub fn new(store: PgStore, github_base_url: impl Into<String>, config: DiscoveryConfig) -> Self {
        Self {
            store,
            github_base_url: github_base_url.into(),
            config,
        }
    }

    /// Creates a run for a watched repository and queues the first job.
    pub async fn start_run(
        &self,
        user_id: Uuid,
        root: &RepositoryNode,
        max_depth: Option<i32>,
    ) -> Result<Uuid, DiscoveryError> {
        self.store.upsert_repository(root).await?;
        let run_id = self.store.create_run(user_id, root.id, max_depth).await?;
        self.store
            .insert_root_repository(run_id, root.id)
            .await?;
        self.store.enqueue_job(run_id, root.id, 0).await?;
        Ok(run_id)
    }

    /// Processes the open jobs of a run until no job can be claimed.
    ///
    /// The token is the GitHub access token of the user. Without a token the
    /// engine uses the anonymous read-only access of GitHub, which is limited
    /// to 60 requests per hour.
    pub async fn process_run(
        &self,
        run_id: Uuid,
        token: Option<String>,
    ) -> Result<ScanRunRecord, DiscoveryError> {
        let run = self
            .store
            .get_run(run_id)
            .await?
            .ok_or(DiscoveryError::RunNotFound(run_id))?;

        let client = GitHubClient::new(&self.github_base_url, token)?;

        loop {
            let Some(job) = self.store.claim_next_job(run_id).await? else {
                break;
            };
            self.process_job(&client, &run, job).await?;
        }

        let run = self.store.refresh_run(run_id).await?;
        Ok(run)
    }

    async fn process_job(
        &self,
        client: &GitHubClient,
        run: &ScanRunRecord,
        job: ScanJob,
    ) -> Result<(), DiscoveryError> {
        let Some(repo) = self.store.get_repository(job.repository_github_id).await? else {
            self.store
                .fail_job(
                    job.id,
                    "REPOSITORY_NOT_FOUND",
                    "repository metadata is missing",
                    None,
                )
                .await?;
            return Ok(());
        };

        let mut page = job.cursor_page.unwrap_or(1).max(1) as u32;
        let mut pages_this_round = 0_u32;

        loop {
            match client.list_forks(&repo.owner, &repo.name, page, self.config.per_page).await {
                Ok(fork_page) => {
                    self.store
                        .record_page(
                            &job,
                            &repo.full_name,
                            &fork_page.items,
                            fork_page.next_page,
                            run.root_github_id,
                            run.max_depth,
                        )
                        .await?;
                    pages_this_round += 1;

                    match fork_page.next_page {
                        Some(next_page) => {
                            page = next_page;
                            if pages_this_round >= self.config.max_pages_per_job {
                                self.store
                                    .pause_job(
                                        job.id,
                                        "PAGE_BUDGET_REACHED",
                                        "page budget of this job reached, work continues later",
                                        Utc::now() + Duration::seconds(30),
                                    )
                                    .await?;
                                return Ok(());
                            }
                        }
                        None => {
                            self.store.complete_job(job.id).await?;
                            return Ok(());
                        }
                    }
                }
                Err(error) => {
                    let retry_at = self.retry_time(&job, &error);
                    self.store
                        .fail_job(job.id, error.code(), &error.to_string(), retry_at)
                        .await?;
                    if retry_at.is_some() {
                        tracing::warn!(
                            job_id = %job.id,
                            repository = %repo.full_name,
                            code = error.code(),
                            "discovery job paused, retry scheduled"
                        );
                    } else {
                        tracing::warn!(
                            job_id = %job.id,
                            repository = %repo.full_name,
                            code = error.code(),
                            "discovery job failed"
                        );
                    }
                    return Ok(());
                }
            }
        }
    }

    /// Computes the retry time of a failed job.
    ///
    /// Permanent errors end the job. Transient errors and rate limits are
    /// retried with exponential backoff and jitter until the attempt limit is
    /// reached. A rate limit uses the reset time of the GitHub API when the
    /// response carries one.
    fn retry_time(&self, job: &ScanJob, error: &GitHubError) -> Option<DateTime<Utc>> {
        if !error.is_retryable() {
            return None;
        }
        if job.attempt_count >= self.config.max_attempts {
            return None;
        }
        if let GitHubError::RateLimited {
            reset_at: Some(reset_at),
            ..
        } = error
        {
            if reset_at > Utc::now() {
                return Some(reset_at);
            }
        }
        let exponent = (job.attempt_count - 1).clamp(0, 10) as u32;
        let multiplier = 2_i64.pow(exponent);
        let base = Duration::milliseconds(self.config.retry_base_delay.num_milliseconds() * multiplier);
        let capped = base.min(self.config.retry_max_delay);
        let jitter_seconds = rand::thread_rng().gen_range(0..=(capped.num_seconds() / 5).max(1));
        Some(Utc::now() + capped + Duration::seconds(jitter_seconds))
    }
}
