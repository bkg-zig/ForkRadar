//! Shared domain model for ForkRadar.
//!
//! This crate is compiled for the host and for WebAssembly. It therefore contains
//! no server dependencies: no database, no HTTP client, no secret handling.

pub mod api;
pub mod model;

pub use model::{
    GithubId, JobStatus, RelationshipKind, RelationshipVerification, RepositoryNode, RepositoryRef,
};
