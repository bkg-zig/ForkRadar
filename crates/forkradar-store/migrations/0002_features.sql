-- Feature suggestions and their votes.
--
-- Every row carries the owner. The API filters by that owner on every read
-- and write, so one user never sees the suggestions of another user.

CREATE TABLE feature_suggestions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    area TEXT NOT NULL DEFAULT '',
    impact INTEGER NOT NULL CHECK (impact BETWEEN 1 AND 10),
    effort INTEGER NOT NULL CHECK (effort BETWEEN 1 AND 10),
    status TEXT NOT NULL DEFAULT 'PROPOSED'
        CHECK (status IN ('PROPOSED', 'PLANNED', 'IN_DEVELOPMENT', 'SHIPPED')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX feature_suggestions_user_idx
    ON feature_suggestions (user_id, created_at DESC);

CREATE TABLE feature_votes (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    suggestion_id UUID NOT NULL REFERENCES feature_suggestions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, suggestion_id)
);

CREATE INDEX feature_votes_suggestion_idx ON feature_votes (suggestion_id);
