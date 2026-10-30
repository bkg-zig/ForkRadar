//! Error handling of the API.
//!
//! Every failed request returns the same JSON body with a stable code and a
//! message that never contains a secret.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use forkradar_domain::api::ErrorResponse;

/// An error of the API layer.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message)
    }

    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn not_configured(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, code, message)
    }

    /// Logs the cause and returns a message without internal details.
    pub fn internal_from<E: std::fmt::Display>(code: &'static str, cause: E) -> Self {
        tracing::error!(error = %cause, code, "request failed");
        Self::internal(code, "The request could not be processed.")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            code: self.code.to_string(),
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<forkradar_store::StoreError> for ApiError {
    fn from(error: forkradar_store::StoreError) -> Self {
        match error {
            forkradar_store::StoreError::NotFound(what) => {
                ApiError::not_found("RESOURCE_NOT_FOUND", format!("{what} not found"))
            }
            forkradar_store::StoreError::Invalid(message) => {
                ApiError::bad_request("INVALID_INPUT", message)
            }
            other => ApiError::internal_from("DATABASE_ERROR", other),
        }
    }
}

impl From<forkradar_github::GitHubError> for ApiError {
    fn from(error: forkradar_github::GitHubError) -> Self {
        use forkradar_github::GitHubError;
        match error {
            GitHubError::NotFound { owner, name } => ApiError::not_found(
                "REPOSITORY_NOT_FOUND",
                format!("Repository {owner}/{name} was not found or is not accessible."),
            ),
            GitHubError::Unauthorized => ApiError::unauthorized(
                "GITHUB_UNAUTHORIZED",
                "GitHub rejected the credentials of this session.",
            ),
            GitHubError::RateLimited { reset_at, .. } => ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "GITHUB_RATE_LIMITED",
                match reset_at {
                    Some(reset_at) => {
                        format!("The GitHub rate limit is reached. It resets at {reset_at}.")
                    }
                    None => "The GitHub rate limit is reached.".to_string(),
                },
            ),
            GitHubError::Transport(cause) => {
                ApiError::internal_from("GITHUB_TRANSPORT_ERROR", cause)
            }
            GitHubError::InvalidResponse(message) => {
                ApiError::internal_from("GITHUB_INVALID_RESPONSE", message)
            }
            GitHubError::Status { status, body } => ApiError::new(
                StatusCode::BAD_GATEWAY,
                "GITHUB_STATUS_ERROR",
                format!("GitHub returned status {status}: {body}"),
            ),
        }
    }
}

impl From<forkradar_discovery::DiscoveryError> for ApiError {
    fn from(error: forkradar_discovery::DiscoveryError) -> Self {
        match error {
            forkradar_discovery::DiscoveryError::Store(inner) => inner.into(),
            forkradar_discovery::DiscoveryError::Github(inner) => inner.into(),
            forkradar_discovery::DiscoveryError::RunNotFound(_) => {
                ApiError::not_found("SCAN_NOT_FOUND", "The scan run does not exist.")
            }
            forkradar_discovery::DiscoveryError::RepositoryNotFound(_) => ApiError::not_found(
                "REPOSITORY_NOT_FOUND",
                "The repository metadata is missing.",
            ),
        }
    }
}
