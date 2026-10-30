//! Authentication of the API server.
//!
//! The server uses a real GitHub OAuth flow with `state` and PKCE. The
//! session lives in the database, the browser only holds a random session
//! token in an `HttpOnly` cookie.

use axum::extract::{FromRequestParts, Query, State};
use axum::http::header::{ACCEPT, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Duration;
use forkradar_domain::api::SessionUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::config::GithubOAuthConfig;
use crate::crypto::{hash_token, pkce_challenge, random_token};
use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "forkradar_session";
const OAUTH_STATE_TTL_MINUTES: i64 = 10;
const DEFAULT_OAUTH_SCOPES: &str = "read:user";

/// The authenticated user of a request.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub session: forkradar_store::SessionUser,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or_else(|| {
                ApiError::unauthorized(
                    "AUTHENTICATION_REQUIRED",
                    "This endpoint needs a session. Sign in with GitHub first.",
                )
            })?;

        let session = state
            .store
            .find_session(&hash_token(&token))
            .await
            .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?
            .ok_or_else(|| {
                ApiError::unauthorized(
                    "SESSION_EXPIRED",
                    "The session is unknown or expired. Sign in with GitHub again.",
                )
            })?;

        Ok(Self { session })
    }
}

/// Starts the GitHub OAuth flow.
pub async fn start(State(state): State<AppState>) -> Result<Response, ApiError> {
    let oauth = oauth_config(&state)?;
    require_cipher(&state)?;

    let state_value = random_token()
        .map_err(|error| ApiError::internal_from("RANDOMNESS_UNAVAILABLE", error))?;
    let verifier = random_token()
        .map_err(|error| ApiError::internal_from("RANDOMNESS_UNAVAILABLE", error))?;
    let challenge = pkce_challenge(&verifier);

    state
        .store
        .store_oauth_state(
            &state_value,
            &verifier,
            None,
            Duration::minutes(OAUTH_STATE_TTL_MINUTES),
        )
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    let scopes = oauth_scopes();
    let mut authorize_url = url::Url::parse(&oauth.authorize_url)
        .map_err(|error| ApiError::internal_from("GITHUB_OAUTH_URL_INVALID", error))?;
    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", &oauth.client_id)
        .append_pair("redirect_uri", &oauth.callback_url)
        .append_pair("scope", &scopes)
        .append_pair("state", &state_value)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("allow_signup", "false");

    Ok(Redirect::to(authorize_url.as_str()).into_response())
}

/// Query parameters of the OAuth callback.
#[derive(Debug, Deserialize)]
pub struct OAuthCallback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// Handles the OAuth callback of GitHub.
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<OAuthCallback>,
) -> Result<Response, ApiError> {
    let oauth = oauth_config(&state)?;
    let cipher = require_cipher(&state)?;

    if let Some(error) = params.error.as_deref() {
        let description = params
            .error_description
            .as_deref()
            .unwrap_or("GitHub refused the authorization request.");
        return Err(ApiError::bad_request(
            "GITHUB_OAUTH_ERROR",
            format!("{error}: {description}"),
        ));
    }

    let code = params.code.ok_or_else(|| {
        ApiError::bad_request(
            "GITHUB_OAUTH_ERROR",
            "The callback of GitHub carries no authorization code.",
        )
    })?;
    let state_value = params.state.ok_or_else(|| {
        ApiError::bad_request(
            "GITHUB_OAUTH_ERROR",
            "The callback of GitHub carries no state value.",
        )
    })?;

    // The state is single use. A replayed callback finds no row.
    let stored_state = state
        .store
        .take_oauth_state(&state_value)
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?
        .ok_or_else(|| {
            ApiError::bad_request(
                "GITHUB_OAUTH_STATE_INVALID",
                "The state of the authorization request is unknown or expired.",
            )
        })?;

    let token = exchange_code(&oauth, &code, &stored_state.code_verifier).await?;
    let profile = fetch_user(&oauth, &token.access_token).await?;

    let user = state
        .store
        .upsert_user(
            profile.id,
            &profile.login,
            profile.avatar_url.as_deref(),
            profile.language.as_deref(),
        )
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    let encrypted = cipher
        .encrypt(&token.access_token)
        .map_err(|error| ApiError::internal_from("TOKEN_ENCRYPTION_FAILED", error))?;
    let scopes: Vec<String> = token
        .scope
        .split(',')
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect();
    state
        .store
        .store_github_token(user.id, &encrypted, &scopes)
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    let session_token = random_token()
        .map_err(|error| ApiError::internal_from("RANDOMNESS_UNAVAILABLE", error))?;
    let ttl = Duration::hours(state.config.session_ttl_hours);
    state
        .store
        .create_session(user.id, &hash_token(&session_token), ttl)
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    state
        .store
        .write_audit_event(
            Some(user.id),
            "github_login",
            Some(&profile.login),
            Some(serde_json::json!({ "scopes": scopes })),
        )
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    let cookie = Cookie::build((SESSION_COOKIE, session_token))
        .path("/")
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(ttl)
        .build();

    let mut headers = HeaderMap::new();
    headers.append(
        SET_COOKIE,
        cookie
            .to_string()
            .parse()
            .map_err(|error| ApiError::internal_from("COOKIE_INVALID", error))?,
    );
    Ok((headers, Redirect::to("/")).into_response())
}

/// Deletes the session of the browser.
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Response, ApiError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let removed = state
            .store
            .delete_session(&hash_token(cookie.value()))
            .await
            .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;
        if removed {
            tracing::info!("session deleted");
        }
    }
    let cookie = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(Duration::zero())
        .build();
    Ok((jar.remove(cookie), StatusCode::NO_CONTENT).into_response())
}

/// Returns the user of the current session.
pub async fn session(State(state): State<AppState>, user: AuthUser) -> Result<Json<SessionUser>, ApiError> {
    let scopes = state
        .store
        .github_scopes(user.session.user_id)
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;

    Ok(Json(SessionUser {
        id: user.session.user_id,
        github_id: user.session.github_id,
        login: user.session.login.clone(),
        avatar_url: user.session.avatar_url.clone(),
        scopes,
    }))
}

/// Returns the GitHub OAuth configuration of the server.
pub fn oauth_config(state: &AppState) -> Result<&GithubOAuthConfig, ApiError> {
    state.config.github_oauth.as_ref().ok_or_else(|| {
        ApiError::not_configured(
            "GITHUB_OAUTH_NOT_CONFIGURED",
            "GITHUB_OAUTH_CLIENT_ID, GITHUB_OAUTH_CLIENT_SECRET and GITHUB_OAUTH_CALLBACK_URL are not set.",
        )
    })
}

/// Returns the cipher for GitHub tokens.
fn require_cipher(state: &AppState) -> Result<&crate::crypto::TokenCipher, ApiError> {
    state.cipher.as_ref().ok_or_else(|| {
        ApiError::not_configured(
            "GITHUB_TOKEN_ENCRYPTION_NOT_CONFIGURED",
            "FORKRADAR_TOKEN_ENCRYPTION_KEY is not set. No GitHub token can be stored safely.",
        )
    })
}

/// Scopes that the server requests from GitHub.
fn oauth_scopes() -> String {
    std::env::var("GITHUB_OAUTH_SCOPES")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_OAUTH_SCOPES.to_string())
}

/// Answer of the GitHub token endpoint.
#[derive(Debug, Deserialize)]
struct TokenExchange {
    access_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Exchanges the authorization code for an access token.
async fn exchange_code(
    oauth: &GithubOAuthConfig,
    code: &str,
    verifier: &str,
) -> Result<TokenExchange, ApiError> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| ApiError::internal_from("HTTP_CLIENT_FAILED", error))?;

    let response = http
        .post(&oauth.token_url)
        .header(ACCEPT, "application/json")
        .form(&[
            ("client_id", oauth.client_id.as_str()),
            ("client_secret", oauth.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", oauth.callback_url.as_str()),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|error| ApiError::internal_from("GITHUB_UNREACHABLE", error))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| ApiError::internal_from("GITHUB_UNREACHABLE", error))?;

    let parsed = parse_token_response(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "GITHUB_TOKEN_RESPONSE_INVALID",
            format!("GitHub returned an answer that could not be read: {error}"),
        )
    })?;

    if let Some(error) = parsed.error.as_deref() {
        let description = parsed
            .error_description
            .as_deref()
            .unwrap_or("the authorization code was refused");
        return Err(ApiError::bad_request(
            "GITHUB_TOKEN_EXCHANGE_FAILED",
            format!("GitHub refused the authorization: {error} ({description})"),
        ));
    }

    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "GITHUB_TOKEN_EXCHANGE_FAILED",
            format!("GitHub returned status {status} for the token exchange."),
        ));
    }

    if parsed.access_token.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "GITHUB_TOKEN_EXCHANGE_FAILED",
            "GitHub returned no access token.",
        ));
    }

    Ok(parsed)
}

/// Reads the answer of the token endpoint. GitHub answers as JSON or as form.
fn parse_token_response(body: &str) -> Result<TokenExchange, String> {
    if let Ok(parsed) = serde_json::from_str::<TokenExchange>(body) {
        return Ok(parsed);
    }
    let mut fields = std::collections::HashMap::new();
    for (key, value) in url::form_urlencoded::parse(body.as_bytes()) {
        fields.insert(key.into_owned(), value.into_owned());
    }
    if fields.is_empty() {
        return Err("the answer is neither JSON nor form encoded".to_string());
    }
    Ok(TokenExchange {
        access_token: fields.get("access_token").cloned(),
        scope: fields.get("scope").cloned(),
        error: fields.get("error").cloned(),
        error_description: fields.get("error_description").cloned(),
    })
}

/// Profile of the authenticated GitHub user.
#[derive(Debug, Deserialize)]
struct GithubProfile {
    id: i64,
    login: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

/// Reads the profile of the authenticated GitHub user.
async fn fetch_user(oauth: &GithubOAuthConfig, token: &str) -> Result<GithubProfile, ApiError> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| ApiError::internal_from("HTTP_CLIENT_FAILED", error))?;

    let response = http
        .get(&oauth.user_url)
        .header(ACCEPT, "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| ApiError::internal_from("GITHUB_UNREACHABLE", error))?;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::unauthorized(
            "GITHUB_UNAUTHORIZED",
            "GitHub rejected the access token of this session.",
        ));
    }
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "GITHUB_PROFILE_FAILED",
            format!("GitHub returned status {status} for the user profile."),
        ));
    }

    response
        .json::<GithubProfile>()
        .await
        .map_err(|error| ApiError::new(
            StatusCode::BAD_GATEWAY,
            "GITHUB_PROFILE_INVALID",
            format!("GitHub returned a profile that could not be read: {error}"),
        ))
}

/// Returns the user id of an optional session.
pub async fn optional_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<Uuid>, ApiError> {
    let jar = CookieJar::from_headers(headers);
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return Ok(None);
    };
    let session = state
        .store
        .find_session(&hash_token(cookie.value()))
        .await
        .map_err(|error| ApiError::internal_from("DATABASE_ERROR", error))?;
    Ok(session.map(|session| session.user_id))
}
