use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Stable numeric GitHub repository id. Used as the primary deduplication key,
/// because owner/name pairs can change while the id stays the same.
pub type GithubId = i64;

/// `owner/name` pair of a GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryRef {
    pub owner: String,
    pub name: String,
}

impl RepositoryRef {
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// Lifecycle state of a scan run and of a single discovery job.
///
/// `COMPLETED` is only valid when every job of the run and every API page in
/// the documented scope was processed. Anything else that stopped early is
/// `PARTIAL` or `FAILED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Discovering,
    Queued,
    Running,
    Partial,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Discovering => "DISCOVERING",
            JobStatus::Queued => "QUEUED",
            JobStatus::Running => "RUNNING",
            JobStatus::Partial => "PARTIAL",
            JobStatus::Completed => "COMPLETED",
            JobStatus::Failed => "FAILED",
            JobStatus::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DISCOVERING" => Some(JobStatus::Discovering),
            "QUEUED" => Some(JobStatus::Queued),
            "RUNNING" => Some(JobStatus::Running),
            "PARTIAL" => Some(JobStatus::Partial),
            "COMPLETED" => Some(JobStatus::Completed),
            "FAILED" => Some(JobStatus::Failed),
            "CANCELLED" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Kind of an edge between two repositories.
///
/// The kinds are kept apart because GitHub does not report a fork network as a
/// tree with a provable intermediate parent. Only `VerifiedImmediateParent`
/// carries evidence collected by ForkRadar itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipKind {
    /// The watched main repository of the user.
    MainSource,
    /// GitHub reports this repository as the immediate parent.
    GithubReportedParent,
    /// GitHub reports this repository as the root of the fork network.
    GithubReportedSource,
    /// ForkRadar found this repository in the fork listing of its parent.
    VerifiedImmediateParent,
    /// Repository belongs to the same fork network as the watched repository.
    ForkNetworkMember,
    /// Both repositories contain the same commit. Requires commit ingestion.
    SharedCommitAncestry,
    /// Commits look similar without being identical.
    SimilarContent,
    /// Relationship could not be determined.
    Unclear,
}

impl RelationshipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipKind::MainSource => "MAIN_SOURCE",
            RelationshipKind::GithubReportedParent => "GITHUB_REPORTED_PARENT",
            RelationshipKind::GithubReportedSource => "GITHUB_REPORTED_SOURCE",
            RelationshipKind::VerifiedImmediateParent => "VERIFIED_IMMEDIATE_PARENT",
            RelationshipKind::ForkNetworkMember => "FORK_NETWORK_MEMBER",
            RelationshipKind::SharedCommitAncestry => "SHARED_COMMIT_ANCESTRY",
            RelationshipKind::SimilarContent => "SIMILAR_CONTENT",
            RelationshipKind::Unclear => "UNCLEAR",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "MAIN_SOURCE" => Some(RelationshipKind::MainSource),
            "GITHUB_REPORTED_PARENT" => Some(RelationshipKind::GithubReportedParent),
            "GITHUB_REPORTED_SOURCE" => Some(RelationshipKind::GithubReportedSource),
            "VERIFIED_IMMEDIATE_PARENT" => Some(RelationshipKind::VerifiedImmediateParent),
            "FORK_NETWORK_MEMBER" => Some(RelationshipKind::ForkNetworkMember),
            "SHARED_COMMIT_ANCESTRY" => Some(RelationshipKind::SharedCommitAncestry),
            "SIMILAR_CONTENT" => Some(RelationshipKind::SimilarContent),
            "UNCLEAR" => Some(RelationshipKind::Unclear),
            _ => None,
        }
    }
}

/// How reliable a relationship is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipVerification {
    /// ForkRadar saw the repository in the fork listing of the parent repository.
    ConfirmedByForkListing,
    /// The statement comes from the GitHub API response.
    ReportedByGithub,
    /// Derived by following the reported source chain.
    DerivedFromSourceChain,
    /// Not verified.
    Unverified,
}

impl RelationshipVerification {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipVerification::ConfirmedByForkListing => "CONFIRMED_BY_FORK_LISTING",
            RelationshipVerification::ReportedByGithub => "REPORTED_BY_GITHUB",
            RelationshipVerification::DerivedFromSourceChain => "DERIVED_FROM_SOURCE_CHAIN",
            RelationshipVerification::Unverified => "UNVERIFIED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CONFIRMED_BY_FORK_LISTING" => Some(RelationshipVerification::ConfirmedByForkListing),
            "REPORTED_BY_GITHUB" => Some(RelationshipVerification::ReportedByGithub),
            "DERIVED_FROM_SOURCE_CHAIN" => Some(RelationshipVerification::DerivedFromSourceChain),
            "UNVERIFIED" => Some(RelationshipVerification::Unverified),
            _ => None,
        }
    }
}

/// Repository metadata as returned by the GitHub API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepositoryNode {
    pub id: GithubId,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub is_fork: bool,
    pub parent: Option<RepositoryRef>,
    pub source: Option<RepositoryRef>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub stargazers_count: i32,
    pub forks_count: i32,
    pub open_issues_count: i32,
    pub size_kb: i32,
    pub archived: bool,
    pub disabled: bool,
    pub visibility: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub pushed_at: Option<DateTime<Utc>>,
}

impl RepositoryNode {
    pub fn repository_ref(&self) -> RepositoryRef {
        RepositoryRef::new(self.owner.clone(), self.name.clone())
    }
}
