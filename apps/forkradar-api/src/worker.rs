//! Background worker of the API server.
//!
//! The worker processes the durable job queue of the database. It does not
//! depend on a browser session and resumes the work of a previous process.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use forkradar_discovery::{DiscoveryConfig, DiscoveryEngine};
use forkradar_github::GitHubClient;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::state::AppState;

/// Observable state of the worker.
#[derive(Debug, Default)]
pub struct WorkerState {
    started_at: Mutex<Option<DateTime<Utc>>>,
    last_activity: Mutex<Option<DateTime<Utc>>>,
    last_error: Mutex<Option<String>>,
    active_runs: AtomicUsize,
    processed_runs: AtomicU64,
}

impl WorkerState {
    /// Description of the worker for the status endpoint.
    pub fn describe(&self) -> forkradar_domain::api::FeatureStatus {
        let started_at = self.started_at.lock().ok().and_then(|value| *value);
        let Some(started_at) = started_at else {
            return forkradar_domain::api::FeatureStatus::new(
                "NOT_RUN",
                "The worker has not started yet.",
            );
        };
        forkradar_domain::api::FeatureStatus::new(
            "READY",
            format!(
                "The worker runs since {started_at}. {} runs processed, {} active.",
                self.processed_runs.load(Ordering::Relaxed),
                self.active_runs.load(Ordering::Relaxed)
            ),
        )
    }

    fn note_activity(&self) {
        if let Ok(mut guard) = self.last_activity.lock() {
            *guard = Some(Utc::now());
        }
    }

    fn note_error(&self, message: String) {
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(message);
        }
    }
}

/// Starts the worker loop.
pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    let config = state.config.clone();
    let engine = DiscoveryEngine::new(
        state.store.clone(),
        config.github_api_base_url.clone(),
        DiscoveryConfig {
            max_pages_per_job: config.max_pages_per_job,
            ..DiscoveryConfig::default()
        },
    );
    let concurrency = config.worker_concurrency;

    tokio::spawn(async move {
        if let Ok(mut guard) = state.worker.started_at.lock() {
            *guard = Some(Utc::now());
        }
        tracing::info!(concurrency, "discovery worker started");

        let semaphore = Arc::new(Semaphore::new(concurrency));
        let in_flight = Arc::new(Mutex::new(HashSet::<Uuid>::new()));

        loop {
            match state.store.list_open_runs().await {
                Ok(runs) => {
                    for run in runs {
                        let mut guard = match in_flight.lock() {
                            Ok(guard) => guard,
                            Err(_) => break,
                        };
                        if !guard.insert(run.id) {
                            continue;
                        }
                        drop(guard);

                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => break,
                        };

                        let state = state.clone();
                        let engine = engine.clone();
                        let in_flight = in_flight.clone();
                        let run_id = run.id;
                        let user_id = run.user_id;

                        tokio::spawn(async move {
                            let _permit = permit;
                            state.worker.active_runs.fetch_add(1, Ordering::Relaxed);
                            let result = process_run(&state, &engine, run_id, user_id).await;
                            state.worker.active_runs.fetch_sub(1, Ordering::Relaxed);
                            state.worker.processed_runs.fetch_add(1, Ordering::Relaxed);
                            state.worker.note_activity();
                            if let Err(message) = result {
                                state.worker.note_error(message);
                            }
                            if let Ok(mut guard) = in_flight.lock() {
                                guard.remove(&run_id);
                            }
                        });
                    }
                }
                Err(error) => {
                    tracing::error!(error = %error, "worker could not read the open runs");
                    state.worker.note_error(error.to_string());
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

/// Processes one run and stores the observed rate limit.
async fn process_run(
    state: &AppState,
    engine: &DiscoveryEngine,
    run_id: Uuid,
    user_id: Uuid,
) -> Result<(), String> {
    let token = state
        .github_token_for(user_id)
        .await
        .or_else(|| state.service_token());

    let run = engine
        .process_run(run_id, token.clone())
        .await
        .map_err(|error| error.to_string())?;

    // The rate limit belongs to the context that did the requests.
    let client = GitHubClient::new(&state.config.github_api_base_url, token)
        .map_err(|error| error.to_string())?;
    if let Ok(rate_limit) = client.rate_limit().await {
        state
            .store
            .set_run_rate_limit(
                run_id,
                rate_limit.limit,
                rate_limit.remaining,
                rate_limit.reset_at,
            )
            .await
            .map_err(|error| error.to_string())?;
    }

    tracing::info!(
        run_id = %run_id,
        status = %run.status,
        discovered = run.discovered_count,
        processed = run.processed_count,
        "scan run processed"
    );
    Ok(())
}
