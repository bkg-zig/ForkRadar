//! Server configuration of the ForkRadar API.
//!
//! Every value is read from the environment. Secrets are read once at startup
//! and are never written to a log line or an HTTP response.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub const DEFAULT_GITHUB_API_BASE_URL: &str = "https://api.github.com";
pub const DEFAULT_GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
pub const DEFAULT_GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// Configuration of the GitHub OAuth app.
#[derive(Debug, Clone)]
pub struct GithubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub callback_url: String,
    pub authorize_url: String,
    pub token_url: String,
    /// Endpoint that returns the authenticated user.
    pub user_url: String,
}

/// Configuration of the whole server.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    /// Directory with the built Dioxus bundle. The API serves it when present.
    pub web_dist: Option<PathBuf>,
    pub cookie_secure: bool,
    pub cors_allowed_origin: Option<String>,
    pub database_url: String,
    pub github_oauth: Option<GithubOAuthConfig>,
    /// 32 bytes for AES-256-GCM. Without the key no OAuth token is stored.
    pub token_encryption_key: Option<[u8; 32]>,
    pub github_api_base_url: String,
    /// Optional token for read-only access to public repositories.
    pub github_api_token: Option<String>,
    pub nim: forkradar_nim::NimConfig,
    pub worker_concurrency: usize,
    pub max_pages_per_job: u32,
    pub session_ttl_hours: i64,
}

impl Config {
    /// Reads the configuration from the environment.
    ///
    /// `DATABASE_URL` is required. Every other value has a documented default
    /// or stays unset, which the API reports as `NOT_CONFIGURED`.
    pub fn from_env() -> Result<Self> {
        let database_url = required("DATABASE_URL")
            .context("DATABASE_URL must point to the PostgreSQL database of ForkRadar")?;

        let github_api_base_url = optional("GITHUB_API_BASE_URL")
            .unwrap_or_else(|| DEFAULT_GITHUB_API_BASE_URL.to_string());

        let github_oauth = match (
            optional("GITHUB_OAUTH_CLIENT_ID"),
            optional("GITHUB_OAUTH_CLIENT_SECRET"),
            optional("GITHUB_OAUTH_CALLBACK_URL"),
        ) {
            (Some(client_id), Some(client_secret), Some(callback_url)) => Some(GithubOAuthConfig {
                client_id,
                client_secret,
                callback_url,
                authorize_url: optional("GITHUB_OAUTH_AUTHORIZE_URL")
                    .unwrap_or_else(|| DEFAULT_GITHUB_AUTHORIZE_URL.to_string()),
                token_url: optional("GITHUB_OAUTH_TOKEN_URL")
                    .unwrap_or_else(|| DEFAULT_GITHUB_TOKEN_URL.to_string()),
                user_url: format!("{}/user", github_api_base_url.trim_end_matches('/')),
            }),
            _ => None,
        };

        let token_encryption_key = match optional("FORKRADAR_TOKEN_ENCRYPTION_KEY") {
            Some(encoded) => Some(decode_key(&encoded)?),
            None => None,
        };

        let worker_concurrency = optional("FORKRADAR_WORKER_CONCURRENCY")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(4);

        let max_pages_per_job = optional("FORKRADAR_MAX_PAGES_PER_JOB")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(50);

        let session_ttl_hours = optional("FORKRADAR_SESSION_TTL_HOURS")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(72);

        Ok(Self {
            bind_addr: optional("FORKRADAR_BIND_ADDR")
                .and_then(|value| value.parse::<SocketAddr>().ok())
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3000))),
            web_dist: optional("FORKRADAR_WEB_DIST").map(PathBuf::from),
            cookie_secure: optional("FORKRADAR_COOKIE_SECURE")
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            cors_allowed_origin: optional("FORKRADAR_CORS_ALLOWED_ORIGIN"),
            database_url,
            github_oauth,
            token_encryption_key,
            github_api_base_url,
            github_api_token: optional("GITHUB_API_TOKEN"),
            nim: forkradar_nim::NimConfig::from_env(),
            worker_concurrency,
            max_pages_per_job,
            session_ttl_hours,
        })
    }
}

fn required(name: &str) -> Option<String> {
    optional(name)
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Decodes the base64 encoded 32 byte key for AES-256-GCM.
fn decode_key(encoded: &str) -> Result<[u8; 32]> {
    let bytes = STANDARD
        .decode(encoded)
        .context("FORKRADAR_TOKEN_ENCRYPTION_KEY must be base64 encoded")?;
    if bytes.len() != 32 {
        bail!(
            "FORKRADAR_TOKEN_ENCRYPTION_KEY must decode to 32 bytes, got {}",
            bytes.len()
        );
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}
