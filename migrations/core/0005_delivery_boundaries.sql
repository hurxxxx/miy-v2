-- Deployment authority is separate from development and runtime data access.
ALTER TABLE grants DROP CONSTRAINT grants_permission_check;
ALTER TABLE grants ADD CONSTRAINT grants_permission_check CHECK(permission IN ('use','develop','deploy'));
CREATE TABLE release_checks (
    release_id uuid PRIMARY KEY REFERENCES releases,
    installation_id uuid NOT NULL REFERENCES installations,
    actor text NOT NULL REFERENCES users,
    note text NOT NULL,
    checked_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cli_authorizations (
    id uuid PRIMARY KEY,
    challenge text NOT NULL,
    user_id text REFERENCES users,
    expires_at timestamptz NOT NULL DEFAULT now() + interval '5 minutes'
);
CREATE TABLE cli_tokens (
    token_hash text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '8 hours'
);
CREATE TABLE image_uploads (
    id uuid PRIMARY KEY,
    app_id text NOT NULL REFERENCES apps,
    actor text NOT NULL REFERENCES users,
    image_id text NOT NULL,
    size_bytes bigint NOT NULL,
    archive_hash text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now()
);
