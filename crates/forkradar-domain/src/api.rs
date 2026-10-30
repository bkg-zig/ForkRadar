use chrono::{DateTime, Utc};
use crate::model::{GithubId, JobStatus, RelationshipKind, RelationshipVerification};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// State of an optional integration. The frontend renders the value verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureStatus {
    /// `READY`, `NOT_CONFIGURED`, `BLOCKED` or `NOT_RUN`.
    pub state: String,
    /// Human readable explanation without secrets.
    pub message: String,
}

impl FeatureStatus {
    pub fn ready(message: impl Into<String>) -> Self {
        Self {
            state: "READY".to_string(),
            message: message.into(),
        }
    }

    pub fn not_configured(message: impl Into<String>) -> Self {
        Self {
            state: "NOT_CONFIGURED".to_string(),
            message: message.into(),
        }
    }
}

/// Status of the internal BKG-NIM model backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NimStatus {
    /// `NIM_NOT_CONFIGURED`, `NIM_UNREACHABLE`, `MODEL_NOT_AVAILABLE`,
    /// `INFERENCE_NOT_TESTED` or `READY`.
    pub state: String,
    pub base_url: String,
    /// Configured model name. Never contains a secret.
    pub model: Option<String>,
    /// Names returned by the authenticated models endpoint, when the list was fetched.
    pub available_models: Vec<String>,
    pub message: String,
}

/// Result of `GET /api/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub application: String,
    pub version: String,
    pub github_oauth: FeatureStatus,
    pub github_api: FeatureStatus,
    pub nim: NimStatus,
    pub database: FeatureStatus,
    pub worker: FeatureStatus,
    /// Rate limit of the GitHub API as last observed by the server.
    pub github_rate_limit: Option<RateLimitState>,
}

/// GitHub rate limit state as last observed by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitState {
    pub limit: i32,
    pub remaining: i32,
    pub reset_at: Option<DateTime<Utc>>,
    /// `AUTHENTICATED` when requests carry a user token, `ANONYMOUS` otherwise.
    pub scope: String,
}

/// The authenticated user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: Uuid,
    pub github_id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub scopes: Vec<String>,
}

/// A repository the user added to the watchlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedRepository {
    pub id: Uuid,
    pub github_id: GithubId,
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub is_fork: bool,
    pub created_at: DateTime<Utc>,
    pub last_scan_id: Option<Uuid>,
}

/// A repository discovered by a scan run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredRepository {
    pub github_id: GithubId,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub is_fork: bool,
    pub parent_full_name: Option<String>,
    pub source_full_name: Option<String>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: i32,
    pub forks_count: i32,
    pub pushed_at: Option<DateTime<Utc>>,
    pub archived: bool,
    pub visibility: Option<String>,
    /// Fork generation relative to the watched repository. `0` is the watched repository.
    pub depth: i32,
    pub relation_kind: RelationshipKind,
    pub verification: RelationshipVerification,
    pub discovered_at: DateTime<Utc>,
}

/// State of a scan run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRun {
    pub id: Uuid,
    pub root_github_id: GithubId,
    pub root_full_name: String,
    pub status: JobStatus,
    /// `None` means no depth limit, which is the default.
    pub max_depth: Option<i32>,
    pub discovery_mode: String,
    pub config_version: i32,
    pub discovered_count: i64,
    pub processed_count: i64,
    pub pending_count: i64,
    pub failed_count: i64,
    pub max_reached_depth: i32,
    pub pages_fetched: i64,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub rate_limit: Option<RateLimitState>,
}

/// A single discovery job of a scan run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanJobView {
    pub id: Uuid,
    pub repository_full_name: String,
    pub repository_github_id: GithubId,
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

/// Node of the fork graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: GithubId,
    pub label: String,
    pub owner: String,
    pub name: String,
    pub depth: i32,
    pub is_fork: bool,
    pub language: Option<String>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub archived: bool,
    pub relation_kind: RelationshipKind,
    pub verification: RelationshipVerification,
}

/// Edge of the fork graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: GithubId,
    pub target: GithubId,
    pub kind: RelationshipKind,
    pub verification: RelationshipVerification,
    pub depth: Option<i32>,
}

/// Payload of the fork graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPayload {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Payload for creating a scan run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateScanRequest {
    /// `owner/name` of the watched repository.
    pub repository: String,
    /// Optional depth limit. `None` means exhaustive without a fixed limit.
    #[serde(default)]
    pub max_depth: Option<i32>,
}

/// Payload for adding a repository to the watchlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddRepositoryRequest {
    /// `owner/name` of the repository.
    pub repository: String,
}

/// Error body returned for every failed request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Stable error code such as `GITHUB_OAUTH_NOT_CONFIGURED`.
    pub code: String,
    /// Human readable explanation without secrets.
    pub message: String,
}
