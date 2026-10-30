//! Shared state of the API server.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forkradar_github::GitHubClient;
use forkradar_nim::NimClient;
use forkradar_store::PgStore;

use crate::config::Config;
use crate::crypto::TokenCipher;
use crate::worker::WorkerState;

/// Cache for a value that is expensive to fetch.
#[derive(Debug)]
pub struct Cached<T> {
    entry: Mutex<Option<(Instant, T)>>,
    ttl: Duration,
}

impl<T: Clone> Cached<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entry: Mutex::new(None),
            ttl,
        }
    }

    /// Returns the cached value when it is still fresh.
    pub fn get(&self) -> Option<T> {
        let guard = self.entry.lock().ok()?;
        let (at, value) = guard.as_ref()?;
        if at.elapsed() < self.ttl {
            Some(value.clone())
        } else {
            None
        }
    }

    pub fn put(&self, value: T) {
        if let Ok(mut guard) = self.entry.lock() {
            *guard = Some((Instant::now(), value));
        }
    }
}

/// Rate limit of the GitHub API as last observed by the server.
#[derive(Debug, Clone)]
pub struct ObservedRateLimit {
    pub limit: i32,
    pub remaining: i32,
    pub reset_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `AUTHENTICATED` or `ANONYMOUS`.
    pub scope: String,
}

/// State shared by all request handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: PgStore,
    pub config: Arc<Config>,
    /// Client without a user token. It is the base for per request tokens.
    pub github: GitHubClient,
    pub nim: NimClient,
    /// Without the key no GitHub token is stored in the database.
    pub cipher: Option<TokenCipher>,
    pub rate_limit: Arc<Cached<ObservedRateLimit>>,
    pub nim_status: Arc<Cached<forkradar_domain::api::NimStatus>>,
    pub worker: Arc<WorkerState>,
}

impl AppState {
    pub fn new(
        store: PgStore,
        config: Arc<Config>,
        github: GitHubClient,
        nim: NimClient,
        cipher: Option<TokenCipher>,
    ) -> Self {
        Self {
            store,
            config,
            github,
            nim,
            cipher,
            rate_limit: Arc::new(Cached::new(Duration::from_secs(30))),
            nim_status: Arc::new(Cached::new(Duration::from_secs(30))),
            worker: Arc::new(WorkerState::default()),
        }
    }

    /// Returns the GitHub token of a user.
    ///
    /// The stored token of the user wins. Without a user token the server uses
    /// the optional service token, otherwise the anonymous public access.
    pub async fn github_token_for(&self, user_id: uuid::Uuid) -> Option<String> {
        let cipher = self.cipher.as_ref()?;
        let stored = self.store.load_github_token(user_id).await.ok().flatten()?;
        let (encrypted, _) = stored;
        match cipher.decrypt(&encrypted) {
            Ok(token) => Some(token),
            Err(error) => {
                tracing::error!(user_id = %user_id, error = %error, "stored token could not be decrypted");
                None
            }
        }
    }

    /// Returns the token that the server uses for anonymous work.
    pub fn service_token(&self) -> Option<String> {
        self.config.github_api_token.clone()
    }
}
