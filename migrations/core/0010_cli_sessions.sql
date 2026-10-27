-- Core users.enabled is the authoritative local access switch. OIDC/Keycloak
-- changes do not automatically update it; no upstream propagation is implied.
ALTER TABLE users ADD COLUMN enabled boolean NOT NULL DEFAULT true;

CREATE TABLE cli_device_authorizations (
    id uuid PRIMARY KEY,
    client_id uuid NOT NULL,
    client_name text NOT NULL CHECK (length(client_name) BETWEEN 1 AND 80),
    device_secret_hash text NOT NULL UNIQUE,
    user_code_hash text NOT NULL,
    user_id text REFERENCES users,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','approved','denied','consumed')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '5 minutes',
    last_poll_at timestamptz,
    poll_interval integer NOT NULL DEFAULT 5 CHECK (poll_interval BETWEEN 5 AND 60),
    CHECK (status NOT IN ('approved','consumed') OR user_id IS NOT NULL),
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '5 minutes')
);
CREATE INDEX cli_device_expiry ON cli_device_authorizations(expires_at);

CREATE TABLE cli_sessions (
    id uuid PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    client_id uuid NOT NULL,
    client_name text NOT NULL CHECK (length(client_name) BETWEEN 1 AND 80),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- Never extended by rotation: this is the original issue time plus 30 days.
    expires_at timestamptz NOT NULL DEFAULT now() + interval '720 hours',
    last_used_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    revocation_reason text,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '720 hours')
);
CREATE INDEX cli_sessions_user ON cli_sessions(user_id,created_at DESC,id);

CREATE TABLE cli_access_tokens (
    token_hash text PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES cli_sessions ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '15 minutes')
);
CREATE INDEX cli_access_session ON cli_access_tokens(session_id);
CREATE INDEX cli_access_expiry ON cli_access_tokens(expires_at);

-- Used refresh hashes must remain until the family expires, even after rotation.
-- A known used token revokes the entire family, including all access tokens.
CREATE TABLE cli_refresh_tokens (
    token_hash text PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES cli_sessions ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    used_at timestamptz
);
CREATE INDEX cli_refresh_session ON cli_refresh_tokens(session_id);
CREATE UNIQUE INDEX cli_refresh_current ON cli_refresh_tokens(session_id) WHERE used_at IS NULL;

-- Only hashed client/actor identifiers are retained. Old minute buckets are
-- removed under one global lock, and the global limit bounds new subject rows.
CREATE TABLE cli_auth_rate_limits (
    scope text NOT NULL,
    subject_hash text NOT NULL,
    window_start timestamptz NOT NULL,
    attempts integer NOT NULL CHECK (attempts > 0),
    PRIMARY KEY(scope,subject_hash,window_start)
);

-- Re-enabling an account never resurrects credentials issued before disable.
-- This also applies to direct administrative SQL updates to the Core source.
CREATE FUNCTION revoke_disabled_user_sessions() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.enabled AND NOT NEW.enabled THEN
        UPDATE cli_sessions
           SET revoked_at=COALESCE(revoked_at,now()),
               revocation_reason=COALESCE(revocation_reason,'user_disabled')
         WHERE user_id=NEW.id;
        DELETE FROM cli_tokens WHERE user_id=NEW.id;
        DELETE FROM sessions WHERE user_id=NEW.id;
        DELETE FROM development_sessions WHERE user_id=NEW.id;
        DELETE FROM cli_authorizations WHERE user_id=NEW.id;
        UPDATE cli_device_authorizations SET status='denied'
         WHERE user_id=NEW.id AND status IN ('pending','approved');
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER users_disable_sessions
AFTER UPDATE OF enabled ON users FOR EACH ROW
EXECUTE FUNCTION revoke_disabled_user_sessions();
