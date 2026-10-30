use serde::Deserialize;

/// Repository object as returned by `GET /repos/{owner}/{repo}`.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    #[serde(default)]
    pub owner: GitHubOwner,
    pub html_url: String,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub parent: Option<GitHubParentRef>,
    #[serde(default)]
    pub source: Option<GitHubParentRef>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub stargazers_count: i32,
    #[serde(default)]
    pub forks_count: i32,
    #[serde(default)]
    pub open_issues_count: i32,
    #[serde(default)]
    pub size: i32,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GitHubOwner {
    #[serde(default)]
    pub login: String,
}

/// `parent` and `source` objects of a fork.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubParentRef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub owner: Option<GitHubOwner>,
}

impl From<GitHubParentRef> for forkradar_domain::model::RepositoryRef {
    fn from(value: GitHubParentRef) -> Self {
        let owner = value
            .owner
            .map(|owner| owner.login)
            .filter(|login| !login.is_empty())
            .unwrap_or_else(|| {
                value
                    .full_name
                    .split_once('/')
                    .map(|(owner, _)| owner.to_string())
                    .unwrap_or_default()
            });
        forkradar_domain::model::RepositoryRef::new(owner, value.name)
    }
}

/// One page of the fork listing.
#[derive(Debug, Clone)]
pub struct ForkPage {
    pub page: u32,
    pub per_page: u32,
    pub items: Vec<forkradar_domain::model::RepositoryNode>,
    /// Page number of the next page, `None` when the listing is exhausted.
    pub next_page: Option<u32>,
}

/// Rate limit state of the GitHub API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitState {
    pub limit: i32,
    pub remaining: i32,
    pub reset_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Rate limit response of `GET /rate_limit`.
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitResponse {
    pub resources: RateLimitResources,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitResources {
    pub core: RateLimitResource,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitResource {
    pub limit: i32,
    pub remaining: i32,
    pub reset: i64,
}
