CREATE TABLE users (
    id UUID PRIMARY KEY,
    github_id BIGINT NOT NULL UNIQUE,
    login TEXT NOT NULL,
    avatar_url TEXT,
    language TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE oauth_states (
    state TEXT PRIMARY KEY,
    code_verifier TEXT NOT NULL,
    redirect_to TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE github_connections (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    encrypted_token BYTEA NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE repositories (
    github_id BIGINT PRIMARY KEY,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL UNIQUE,
    html_url TEXT NOT NULL,
    is_fork BOOLEAN NOT NULL DEFAULT false,
    parent_full_name TEXT,
    source_full_name TEXT,
    default_branch TEXT,
    language TEXT,
    description TEXT,
    stargazers_count INTEGER NOT NULL DEFAULT 0,
    forks_count INTEGER NOT NULL DEFAULT 0,
    open_issues_count INTEGER NOT NULL DEFAULT 0,
    size_kb INTEGER NOT NULL DEFAULT 0,
    archived BOOLEAN NOT NULL DEFAULT false,
    disabled BOOLEAN NOT NULL DEFAULT false,
    visibility TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    pushed_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE watched_repositories (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    github_id BIGINT NOT NULL REFERENCES repositories(github_id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, github_id)
);

CREATE INDEX watched_repositories_user_idx ON watched_repositories (user_id);

CREATE TABLE scan_runs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    root_github_id BIGINT NOT NULL REFERENCES repositories(github_id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    max_depth INTEGER,
    discovery_mode TEXT NOT NULL DEFAULT 'exhaustive',
    config_version INTEGER NOT NULL DEFAULT 1,
    discovered_count BIGINT NOT NULL DEFAULT 0,
    processed_count BIGINT NOT NULL DEFAULT 0,
    pending_count BIGINT NOT NULL DEFAULT 0,
    failed_count BIGINT NOT NULL DEFAULT 0,
    max_reached_depth INTEGER NOT NULL DEFAULT 0,
    pages_fetched BIGINT NOT NULL DEFAULT 0,
    rate_limit_limit INTEGER,
    rate_limit_remaining INTEGER,
    rate_limit_reset_at TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX scan_runs_user_idx ON scan_runs (user_id, created_at DESC);

CREATE TABLE scan_jobs (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    repository_github_id BIGINT NOT NULL REFERENCES repositories(github_id) ON DELETE CASCADE,
    job_type TEXT NOT NULL DEFAULT 'discover_forks',
    depth INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ,
    cursor_page INTEGER,
    pages_done INTEGER NOT NULL DEFAULT 0,
    forks_found INTEGER NOT NULL DEFAULT 0,
    last_success_at TIMESTAMPTZ,
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    UNIQUE (run_id, repository_github_id)
);

CREATE INDEX scan_jobs_claim_idx ON scan_jobs (run_id, status, depth, created_at);

CREATE TABLE scan_repositories (
    run_id UUID NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    github_id BIGINT NOT NULL REFERENCES repositories(github_id) ON DELETE CASCADE,
    depth INTEGER NOT NULL,
    relation_kind TEXT NOT NULL,
    verification TEXT NOT NULL,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, github_id)
);

CREATE TABLE repository_relationships (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES scan_runs(id) ON DELETE CASCADE,
    from_github_id BIGINT NOT NULL,
    to_github_id BIGINT NOT NULL,
    relation_kind TEXT NOT NULL,
    verification TEXT NOT NULL,
    depth INTEGER,
    source_endpoint TEXT,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, from_github_id, to_github_id, relation_kind)
);

CREATE INDEX repository_relationships_run_idx ON repository_relationships (run_id);

CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    subject TEXT,
    detail JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_user_idx ON audit_events (user_id, created_at DESC);
