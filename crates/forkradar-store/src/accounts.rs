use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::{PgStore, Result, StoreError};

/// A user row.
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub github_id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub language: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Session with the resolved user.
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub github_id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// Pending OAuth state including the PKCE verifier.
#[derive(Debug, Clone)]
pub struct OAuthState {
    pub state: String,
    pub code_verifier: String,
    pub redirect_to: Option<String>,
}

impl PgStore {
    /// Creates the user for a GitHub id or updates the existing row.
    pub async fn upsert_user(
        &self,
        github_id: i64,
        login: &str,
        avatar_url: Option<&str>,
        language: Option<&str>,
    ) -> Result<UserRecord> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO users (id, github_id, login, avatar_url, language)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (github_id) DO UPDATE
            SET login = EXCLUDED.login,
                avatar_url = EXCLUDED.avatar_url,
                language = EXCLUDED.language,
                updated_at = now()
            RETURNING id, github_id, login, avatar_url, language, created_at
            "#,
        )
        .bind(id)
        .bind(github_id)
        .bind(login)
        .bind(avatar_url)
        .bind(language)
        .fetch_one(self.pool())
        .await?;

        Ok(UserRecord {
            id: row.get("id"),
            github_id: row.get("github_id"),
            login: row.get("login"),
            avatar_url: row.get("avatar_url"),
            language: row.get("language"),
            created_at: row.get("created_at"),
        })
    }

    pub async fn find_user_by_id(&self, user_id: Uuid) -> Result<Option<UserRecord>> {
        let row = sqlx::query(
            "SELECT id, github_id, login, avatar_url, language, created_at FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|row| UserRecord {
            id: row.get("id"),
            github_id: row.get("github_id"),
            login: row.get("login"),
            avatar_url: row.get("avatar_url"),
            language: row.get("language"),
            created_at: row.get("created_at"),
        }))
    }

    /// Stores a session. The token is stored as hash, the plain token never
    /// reaches the database.
    pub async fn create_session(
        &self,
        user_id: Uuid,
        token_hash: &str,
        ttl: Duration,
    ) -> Result<Uuid> {
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(token_hash)
        .bind(Utc::now() + ttl)
        .execute(self.pool())
        .await?;
        Ok(session_id)
    }

    /// Resolves a session token hash to the session and its user.
    pub async fn find_session(&self, token_hash: &str) -> Result<Option<SessionUser>> {
        let row = sqlx::query(
            r#"
            SELECT s.id AS session_id, s.expires_at, u.id AS user_id, u.github_id, u.login, u.avatar_url
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1 AND s.expires_at > now()
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|row| SessionUser {
            session_id: row.get("session_id"),
            user_id: row.get("user_id"),
            github_id: row.get("github_id"),
            login: row.get("login"),
            avatar_url: row.get("avatar_url"),
            expires_at: row.get("expires_at"),
        }))
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_sessions_for_user(&self, user_id: Uuid) -> Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected())
    }

    /// Stores a pending OAuth state with its PKCE verifier.
    pub async fn store_oauth_state(
        &self,
        state: &str,
        code_verifier: &str,
        redirect_to: Option<&str>,
        ttl: Duration,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oauth_states (state, code_verifier, redirect_to, expires_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state) DO UPDATE
            SET code_verifier = EXCLUDED.code_verifier,
                redirect_to = EXCLUDED.redirect_to,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(state)
        .bind(code_verifier)
        .bind(redirect_to)
        .bind(Utc::now() + ttl)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Reads and deletes a pending OAuth state. The state is single use.
    pub async fn take_oauth_state(&self, state: &str) -> Result<Option<OAuthState>> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query(
            r#"
            DELETE FROM oauth_states
            WHERE state = $1 AND expires_at > now()
            RETURNING state, code_verifier, redirect_to
            "#,
        )
        .bind(state)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(row.map(|row| OAuthState {
            state: row.get("state"),
            code_verifier: row.get("code_verifier"),
            redirect_to: row.get("redirect_to"),
        }))
    }

    /// Stores the encrypted GitHub access token of a user.
    pub async fn store_github_token(
        &self,
        user_id: Uuid,
        encrypted_token: &[u8],
        scopes: &[String],
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO github_connections (user_id, encrypted_token, scopes, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (user_id) DO UPDATE
            SET encrypted_token = EXCLUDED.encrypted_token,
                scopes = EXCLUDED.scopes,
                updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(encrypted_token)
        .bind(scopes)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Loads the encrypted GitHub access token of a user.
    pub async fn load_github_token(&self, user_id: Uuid) -> Result<Option<(Vec<u8>, Vec<String>)>> {
        let row = sqlx::query("SELECT encrypted_token, scopes FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(self.pool())
            .await?;

        Ok(row.map(|row| {
            let encrypted: Vec<u8> = row.get("encrypted_token");
            let scopes: Vec<String> = row.get("scopes");
            (encrypted, scopes)
        }))
    }

    /// Appends an audit event. Tokens and other secrets never belong into the detail.
    pub async fn write_audit_event(
        &self,
        user_id: Option<Uuid>,
        event_type: &str,
        subject: Option<&str>,
        detail: Option<serde_json::Value>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_events (id, user_id, event_type, subject, detail) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(event_type)
        .bind(subject)
        .bind(detail)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Reads the scopes of the stored GitHub connection.
    pub async fn github_scopes(&self, user_id: Uuid) -> Result<Vec<String>> {
        let row = sqlx::query("SELECT scopes FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row
            .map(|row| row.get::<Vec<String>, _>("scopes"))
            .unwrap_or_default())
    }

    /// Fails when the given user id does not exist.
    pub async fn ensure_user_exists(&self, user_id: Uuid) -> Result<()> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(self.pool())
            .await?;
        if exists {
            Ok(())
        } else {
            Err(StoreError::NotFound("user"))
        }
    }
}
