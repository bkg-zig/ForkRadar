//! PostgreSQL persistence for ForkRadar.
//!
//! The database holds the durable job queue, the discovered fork network and
//! the user isolation boundary. Every read and write of user data goes through
//! the user id of the authenticated session.

pub mod accounts;
pub mod features;
pub mod overview;
pub mod repos;
pub mod scans;

pub use accounts::{OAuthState, SessionUser};
pub use features::{FeatureFilter, FeatureSuggestionRecord};
pub use repos::WatchedRepositoryRecord;
pub use scans::{ScanJob, ScanRunRecord};

use sqlx::postgres::{PgPool, PgPoolOptions};

/// Errors of the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Handle to the PostgreSQL database.
#[derive(Clone, Debug)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Applies the embedded migrations.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
