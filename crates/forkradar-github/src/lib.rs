use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use forkradar_domain::model::{RepositoryNode, RepositoryRef};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, LINK, USER_AGENT};
use reqwest::StatusCode;
use url::Url;

mod wire;

pub use wire::{ForkPage, GitHubRepository, RateLimitResource, RateLimitResponse, RateLimitState};

/// Errors returned by the GitHub client.
#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("github returned status {status}")]
    Status {
        status: u16,
        /// Response body, truncated and redacted.
        body: String,
    },
    #[error("github rate limit reached")]
    RateLimited {
        reset_at: Option<DateTime<Utc>>,
        retry_after: Option<Duration>,
    },
    #[error("repository {owner}/{name} not found or not accessible")]
    NotFound { owner: String, name: String },
    #[error("github rejected the provided credentials")]
    Unauthorized,
    #[error("transport error while calling github: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("invalid response from github: {0}")]
    InvalidResponse(String),
}

impl GitHubError {
    /// Errors that may succeed later without any change to the request.
    pub fn is_retryable(&self) -> bool {
        match self {
            GitHubError::RateLimited { .. } | GitHubError::Transport(_) => true,
            GitHubError::Status { status, .. } => *status >= 500 || *status == 408,
            GitHubError::NotFound { .. }
            | GitHubError::Unauthorized
            | GitHubError::InvalidResponse(_) => false,
        }
    }

    /// Adds the repository coordinates to errors that do not carry them yet.
    pub fn with_repository_context(self, owner: &str, name: &str) -> Self {
        match self {
            GitHubError::NotFound {
                owner: found_owner,
                name: found_name,
            } => GitHubError::NotFound {
                owner: if found_owner.is_empty() {
                    owner.to_string()
                } else {
                    found_owner
                },
                name: if found_name.is_empty() {
                    name.to_string()
                } else {
                    found_name
                },
            },
            other => other,
        }
    }

    /// Stable error code for storage in the job record.
    pub fn code(&self) -> &'static str {
        match self {
            GitHubError::Status { .. } => "GITHUB_STATUS_ERROR",
            GitHubError::RateLimited { .. } => "GITHUB_RATE_LIMITED",
            GitHubError::NotFound { .. } => "REPOSITORY_NOT_FOUND",
            GitHubError::Unauthorized => "GITHUB_UNAUTHORIZED",
            GitHubError::Transport(_) => "GITHUB_TRANSPORT_ERROR",
            GitHubError::InvalidResponse(_) => "GITHUB_INVALID_RESPONSE",
        }
    }
}

/// HTTP client for the GitHub REST API.
///
/// The base URL is configurable so the client can talk to a GitHub Enterprise
/// instance and so the test suite can point it at a local fixture server.
#[derive(Debug, Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
    base_url: Url,
    token: Option<String>,
    user_agent: String,
}

impl GitHubClient {
    pub fn new(base_url: &str, token: Option<String>) -> Result<Self, GitHubError> {
        let base_url = Url::parse(base_url).map_err(|err| {
            GitHubError::InvalidResponse(format!("invalid github base url: {err}"))
        })?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("ForkRadar/0.1")
            .build()
            .map_err(GitHubError::Transport)?;
        Ok(Self {
            http,
            base_url,
            token,
            user_agent: "ForkRadar/0.1".to_string(),
        })
    }

    /// Returns a copy of the client that uses the given token.
    pub fn with_token(&self, token: Option<String>) -> Self {
        Self {
            http: self.http.clone(),
            base_url: self.base_url.clone(),
            token,
            user_agent: self.user_agent.clone(),
        }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Fetches the metadata of one repository.
    pub async fn repository(&self, owner: &str, name: &str) -> Result<RepositoryNode, GitHubError> {
        let url = self
            .base_url
            .join(&format!("repos/{owner}/{name}"))
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        let response = self.send(self.http.get(url)).await?;
        let body = response
            .json::<GitHubRepository>()
            .await
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        Ok(body.into())
    }

    /// Fetches one page of the fork listing of a repository.
    pub async fn list_forks(
        &self,
        owner: &str,
        name: &str,
        page: u32,
        per_page: u32,
    ) -> Result<ForkPage, GitHubError> {
        let url = self
            .base_url
            .join(&format!("repos/{owner}/{name}/forks"))
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        let response = self
            .send(self.http.get(url).query(&[
                ("page", page.to_string()),
                ("per_page", per_page.to_string()),
            ]))
            .await?;
        let next_page = response
            .headers()
            .get(LINK)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_next_page);
        let body = response
            .json::<Vec<GitHubRepository>>()
            .await
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        Ok(ForkPage {
            page,
            per_page,
            items: body.into_iter().map(|repo| repo.into()).collect(),
            next_page,
        })
    }

    /// Reads the current rate limit of the authenticated context.
    pub async fn rate_limit(&self) -> Result<RateLimitState, GitHubError> {
        let url = self
            .base_url
            .join("rate_limit")
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        let response = self.send(self.http.get(url)).await?;
        let body = response
            .json::<RateLimitResponse>()
            .await
            .map_err(|err| GitHubError::InvalidResponse(err.to_string()))?;
        Ok(RateLimitState {
            limit: body.resources.core.limit,
            remaining: body.resources.core.remaining,
            reset_at: reset_to_datetime(body.resources.core.reset),
        })
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, GitHubError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static("2022-11-28"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("ForkRadar/0.1"));
        let mut request = request.headers(headers);
        if let Some(token) = &self.token {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| GitHubError::InvalidResponse("invalid token encoding".to_string()))?;
            request = request.header(AUTHORIZATION, value);
        }

        let response = request.send().await.map_err(GitHubError::Transport)?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        Err(self.map_error(status, response).await)
    }

    async fn map_error(&self, status: StatusCode, response: reqwest::Response) -> GitHubError {
        let headers = response.headers().clone();
        let body = response
            .text()
            .await
            .map(|text| redact(&text, self.token.as_deref()))
            .unwrap_or_default();
        let body = truncate(&body, 512);

        match status {
            StatusCode::UNAUTHORIZED => GitHubError::Unauthorized,
            StatusCode::NOT_FOUND => GitHubError::NotFound {
                owner: String::new(),
                name: String::new(),
            },
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
                let remaining = header_i64(&headers, "x-ratelimit-remaining");
                let reset = header_i64(&headers, "x-ratelimit-reset");
                let retry_after = header_i64(&headers, "retry-after")
                    .and_then(|seconds| u64::try_from(seconds).ok())
                    .map(Duration::from_secs);
                let secondary = body.to_ascii_lowercase().contains("secondary rate limit");
                if remaining == Some(0) || secondary || status == StatusCode::TOO_MANY_REQUESTS {
                    GitHubError::RateLimited {
                        reset_at: reset.and_then(reset_to_datetime),
                        retry_after,
                    }
                } else {
                    GitHubError::Status {
                        status: status.as_u16(),
                        body,
                    }
                }
            }
            _ => GitHubError::Status {
                status: status.as_u16(),
                body,
            },
        }
    }
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn reset_to_datetime(reset: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(reset, 0).single()
}

/// Extracts the page number of the `rel="next"` link from a Link header.
pub fn parse_next_page(link: &str) -> Option<u32> {
    for part in link.split(',') {
        let mut segments = part.split(';');
        let target = segments.next()?.trim();
        let mut is_next = false;
        for segment in segments {
            let segment = segment.trim();
            if let Some(value) = segment.strip_prefix("rel=") {
                is_next = value.trim_matches('"') == "next";
            }
        }
        if !is_next {
            continue;
        }
        let url = target.trim_start_matches('<').trim_end_matches('>');
        let parsed = Url::parse(url).ok()?;
        let page = parsed
            .query_pairs()
            .find(|(key, _)| key == "page")
            .and_then(|(_, value)| value.parse::<u32>().ok())?;
        return Some(page);
    }
    None
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

/// Removes a token from text before it is stored in a log or an error message.
fn redact(text: &str, token: Option<&str>) -> String {
    match token {
        Some(token) if !token.is_empty() => text.replace(token, "[REDACTED]"),
        _ => text.to_string(),
    }
}

impl From<GitHubRepository> for RepositoryNode {
    fn from(repo: GitHubRepository) -> Self {
        let owner = if repo.owner.login.is_empty() {
            repo.full_name
                .split_once('/')
                .map(|(owner, _)| owner.to_string())
                .unwrap_or_default()
        } else {
            repo.owner.login
        };
        RepositoryNode {
            id: repo.id,
            owner,
            name: repo.name,
            full_name: repo.full_name,
            html_url: repo.html_url,
            is_fork: repo.fork,
            parent: repo.parent.map(RepositoryRef::from),
            source: repo.source.map(RepositoryRef::from),
            default_branch: repo.default_branch,
            language: repo.language,
            description: repo.description,
            stargazers_count: repo.stargazers_count,
            forks_count: repo.forks_count,
            open_issues_count: repo.open_issues_count,
            size_kb: repo.size,
            archived: repo.archived,
            disabled: repo.disabled,
            visibility: repo.visibility,
            created_at: parse_timestamp(repo.created_at.as_deref()),
            updated_at: parse_timestamp(repo.updated_at.as_deref()),
            pushed_at: parse_timestamp(repo.pushed_at.as_deref()),
        }
    }
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_next_page_from_link_header() {
        let link = "<https://api.github.com/repositories/1/forks?page=2>; rel=\"next\", <https://api.github.com/repositories/1/forks?page=9>; rel=\"last\"";
        assert_eq!(parse_next_page(link), Some(2));
    }

    #[test]
    fn returns_none_without_next_link() {
        let link = "<https://api.github.com/repositories/1/forks?page=1>; rel=\"prev\", <https://api.github.com/repositories/1/forks?page=9>; rel=\"last\"";
        assert_eq!(parse_next_page(link), None);
    }

    #[test]
    fn redacts_token_from_body() {
        let body = "token abc123 leaked";
        assert_eq!(redact(body, Some("abc123")), "token [REDACTED] leaked");
    }
}
