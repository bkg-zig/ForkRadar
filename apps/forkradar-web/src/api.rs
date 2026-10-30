//! Typed client for the ForkRadar API.
//!
//! Every call uses a relative path. In production the API server serves the
//! built bundle itself, in development the Dioxus dev server forwards the same
//! paths. The session lives in the cookie `forkradar_session`, which the
//! browser sends automatically with every same origin request.

use forkradar_domain::api::{
    AddRepositoryRequest, CreateFeatureRequest, CreateScanRequest, DiscoveredRepositoryPage,
    ErrorResponse, FeatureAuditSummary, FeatureSuggestion, GraphPayload, OverviewSummary, ScanJobView,
    ScanRun, SessionUser, StatusResponse, UpdateFeatureStatusRequest, WatchedRepository,
};
use forkradar_domain::quality::{QualityScore, ToolInventory};
use gloo_net::http::RequestBuilder;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A scan run with its jobs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRunDetail {
    #[serde(flatten)]
    pub run: ScanRun,
    pub jobs: Vec<ScanJobView>,
}

/// Whether the browser holds a valid session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub authenticated: bool,
    pub user: Option<SessionUser>,
}

/// Answer of a vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteResponse {
    pub votes: i64,
    pub voted_by_me: bool,
}

/// A failed request.
///
/// The message never contains a secret. The server sends its own message in
/// the `message` field, the client only adds a fallback for answers that do
/// not follow the error contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    /// Stable code of the error, for example `GITHUB_OAUTH_NOT_CONFIGURED`.
    pub code: String,
    /// Text that can be shown to the user.
    pub message: String,
    /// HTTP status of the answer, zero when the request did not reach the server.
    pub status: u16,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

/// Result of an API call.
pub type ApiResult<T> = Result<T, ApiError>;

/// Builds an error for a request that never reached the server.
fn transport_error(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "NETWORK_ERROR".to_string(),
        message: message.into(),
        status: 0,
    }
}

/// Sends a request and reads the answer.
///
/// The server returns the error body of the domain crate for every failed
/// request. An answer that does not follow the contract keeps its HTTP status
/// and gets a generic message.
async fn send<T: serde::de::DeserializeOwned>(request: RequestBuilder) -> ApiResult<T> {
    let response = request
        .send()
        .await
        .map_err(|error| transport_error(format!("Der Server ist nicht erreichbar: {error}")))?;

    let status = response.status();
    if !response.ok() {
        let body = response.text().await.unwrap_or_default();
        let parsed: Option<ErrorResponse> = serde_json::from_str(&body).ok();
        return Err(ApiError {
            code: parsed
                .as_ref()
                .map(|error| error.code.clone())
                .unwrap_or_else(|| "HTTP_ERROR".to_string()),
            message: parsed
                .map(|error| error.message)
                .unwrap_or_else(|| format!("Der Server antwortete mit dem Status {status}.")),
            status,
        });
    }

    response
        .json::<T>()
        .await
        .map_err(|error| ApiError {
            code: "DECODE_ERROR".to_string(),
            message: format!("Die Antwort des Servers war nicht lesbar: {error}"),
            status,
        })
}

/// Sends a request whose successful answer has no body.
async fn send_without_body(request: RequestBuilder) -> ApiResult<()> {
    let response = request
        .send()
        .await
        .map_err(|error| transport_error(format!("Der Server ist nicht erreichbar: {error}")))?;

    let status = response.status();
    if !response.ok() {
        let body = response.text().await.unwrap_or_default();
        let parsed: Option<ErrorResponse> = serde_json::from_str(&body).ok();
        return Err(ApiError {
            code: parsed
                .as_ref()
                .map(|error| error.code.clone())
                .unwrap_or_else(|| "HTTP_ERROR".to_string()),
            message: parsed
                .map(|error| error.message)
                .unwrap_or_else(|| format!("Der Server antwortete mit dem Status {status}.")),
            status,
        });
    }

    Ok(())
}

/// Reports the state of the server and of its integrations.
pub async fn status() -> ApiResult<StatusResponse> {
    send(RequestBuilder::get("/api/status")).await
}

/// Reports whether the browser holds a session.
pub async fn session_state() -> ApiResult<SessionState> {
    send(RequestBuilder::get("/api/auth/state")).await
}

/// Returns the personal summary of the signed in user.
pub async fn overview() -> ApiResult<OverviewSummary> {
    send(RequestBuilder::get("/api/overview")).await
}

/// Lists the watched repositories of the user.
pub async fn repositories() -> ApiResult<Vec<WatchedRepository>> {
    send(RequestBuilder::get("/api/repositories")).await
}

/// Adds a repository to the watchlist.
pub async fn add_repository(repository: &str) -> ApiResult<WatchedRepository> {
    let body = AddRepositoryRequest {
        repository: repository.to_string(),
    };
    send(RequestBuilder::post("/api/repositories").json(&body).unwrap_or_default()).await
}

/// Removes a repository from the watchlist.
pub async fn remove_repository(id: Uuid) -> ApiResult<()> {
    send_without_body(RequestBuilder::delete(&format!("/api/repositories/{id}"))).await
}

/// Lists the scan runs of the user.
pub async fn scan_runs() -> ApiResult<Vec<ScanRun>> {
    send(RequestBuilder::get("/api/scans")).await
}

/// Starts a scan for a watched repository.
pub async fn create_scan(repository: &str, max_depth: Option<i32>) -> ApiResult<ScanRun> {
    let body = CreateScanRequest {
        repository: repository.to_string(),
        max_depth,
    };
    send(RequestBuilder::post("/api/scans").json(&body).unwrap_or_default()).await
}

/// Reads one scan run with its jobs.
pub async fn scan_detail(id: Uuid) -> ApiResult<ScanRunDetail> {
    send(RequestBuilder::get(&format!("/api/scans/{id}"))).await
}

/// Reads one page of the repositories discovered by a scan.
pub async fn discovered_repositories(
    id: Uuid,
    offset: i64,
    limit: i64,
) -> ApiResult<DiscoveredRepositoryPage> {
    send(
        RequestBuilder::get(&format!("/api/scans/{id}/repositories"))
            .query(&[("offset", offset.to_string()), ("limit", limit.to_string())]),
    )
    .await
}

/// Reads the fork graph of a scan.
pub async fn graph(id: Uuid) -> ApiResult<GraphPayload> {
    send(RequestBuilder::get(&format!("/api/scans/{id}/graph"))).await
}

/// Resumes a paused or failed scan.
pub async fn resume_scan(id: Uuid) -> ApiResult<ScanRun> {
    send(RequestBuilder::post(&format!("/api/scans/{id}/resume"))).await
}

/// Cancels a running scan.
pub async fn cancel_scan(id: Uuid) -> ApiResult<ScanRun> {
    send(RequestBuilder::post(&format!("/api/scans/{id}/cancel"))).await
}

/// Lists the analysis tools of the runtime of the server.
pub async fn analysis_tools() -> ApiResult<ToolInventory> {
    send(RequestBuilder::get("/api/analysis/tools")).await
}

/// Reads the quality score of a repository.
pub async fn quality_score(github_id: i64) -> ApiResult<QualityScore> {
    send(RequestBuilder::get(&format!("/api/analysis/score/{github_id}"))).await
}

/// Lists the feature suggestions with the filters and the sort mode.
pub async fn features(
    sort: &str,
    query: &str,
    area: &str,
    status: &str,
    min_impact: Option<i32>,
    max_effort: Option<i32>,
    min_votes: Option<i64>,
) -> ApiResult<Vec<FeatureSuggestion>> {
    let mut parameters: Vec<(&str, String)> = vec![("sort", sort.to_string())];
    if !query.trim().is_empty() {
        parameters.push(("q", query.to_string()));
    }
    if !area.trim().is_empty() {
        parameters.push(("area", area.to_string()));
    }
    if !status.trim().is_empty() {
        parameters.push(("status", status.to_string()));
    }
    if let Some(value) = min_impact {
        parameters.push(("min_impact", value.to_string()));
    }
    if let Some(value) = max_effort {
        parameters.push(("max_effort", value.to_string()));
    }
    if let Some(value) = min_votes {
        parameters.push(("min_votes", value.to_string()));
    }

    send(RequestBuilder::get("/api/features").query(&parameters)).await
}

/// Creates a feature suggestion.
pub async fn create_feature(
    title: &str,
    description: &str,
    area: &str,
    impact: i32,
    effort: i32,
) -> ApiResult<FeatureSuggestion> {
    let body = CreateFeatureRequest {
        title: title.to_string(),
        description: description.to_string(),
        area: area.to_string(),
        impact,
        effort,
    };
    send(RequestBuilder::post("/api/features").json(&body).unwrap_or_default()).await
}

/// Votes for a suggestion.
pub async fn vote(id: Uuid) -> ApiResult<VoteResponse> {
    send(RequestBuilder::post(&format!("/api/features/{id}/vote"))).await
}

/// Withdraws the vote of the user.
pub async fn withdraw_vote(id: Uuid) -> ApiResult<VoteResponse> {
    send(RequestBuilder::delete(&format!("/api/features/{id}/vote"))).await
}

/// Changes the lifecycle state of a suggestion.
pub async fn update_feature_status(id: Uuid, status: &str) -> ApiResult<FeatureSuggestion> {
    let body = UpdateFeatureStatusRequest {
        status: status.to_string(),
    };
    send(
        RequestBuilder::put(&format!("/api/features/{id}/status"))
            .json(&body)
            .unwrap_or_default(),
    )
    .await
}

/// Reads the audit summary of the last thirty days.
pub async fn feature_audit() -> ApiResult<FeatureAuditSummary> {
    send(RequestBuilder::get("/api/features/audit")).await
}
