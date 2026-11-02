-- A manifest requests access; it never approves itself. Existing installations
-- intentionally receive no external-call approvals on migration.
CREATE TABLE installation_permissions (
    installation_id uuid NOT NULL REFERENCES installations,
    release_id uuid NOT NULL REFERENCES releases,
    revision bigint NOT NULL CHECK (revision > 0),
    permissions jsonb NOT NULL CHECK (jsonb_typeof(permissions)='array'),
    approved_by text NOT NULL REFERENCES users,
    approved_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (installation_id,release_id)
);
CREATE TABLE installation_permission_history (
    installation_id uuid NOT NULL REFERENCES installations,
    release_id uuid NOT NULL REFERENCES releases,
    revision bigint NOT NULL,
    permissions jsonb NOT NULL CHECK (jsonb_typeof(permissions)='array'),
    actor text NOT NULL REFERENCES users,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (installation_id,release_id,revision)
);
-- Short-lived continuation capabilities are opaque, hash-only and independent
-- of the HS256 identity key held by an application. No legacy JWT is a core token.
CREATE TABLE app_call_tokens (
    token_hash text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    cli_session_id uuid REFERENCES cli_sessions,
    delegation_id uuid,
    origin jsonb,
    installation_id uuid NOT NULL REFERENCES installations,
    release_id uuid NOT NULL REFERENCES releases,
    environment text NOT NULL CHECK(environment IN ('dev','prod')),
    callers jsonb NOT NULL CHECK(jsonb_typeof(callers)='array'),
    permissions_used jsonb NOT NULL CHECK(jsonb_typeof(permissions_used)='array'),
    effective_permissions jsonb NOT NULL CHECK(jsonb_typeof(effective_permissions)='array'),
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (expires_at <= created_at + interval '30 seconds')
);
CREATE INDEX app_call_expiry ON app_call_tokens(expires_at);
CREATE INDEX app_call_user ON app_call_tokens(user_id);
