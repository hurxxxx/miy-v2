-- Business app UIs run in per-installation cross-origin frames (candidate design
-- docs/APP-FRAMES.md). An origin belongs to one installation and is never given to
-- another one, even after the installation is gone. Frame credentials are opaque,
-- hash-only and die with the parent portal session (logout or disabled user).
CREATE TABLE app_frame_origins (
    origin text PRIMARY KEY,
    installation_id uuid UNIQUE REFERENCES installations ON DELETE SET NULL,
    port integer UNIQUE CHECK (port BETWEEN 1 AND 65535),
    allocated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE app_frame_sessions (
    id uuid PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    parent_session_hash text REFERENCES sessions(id_hash) ON DELETE CASCADE,
    parent_development_hash text REFERENCES development_sessions(id_hash) ON DELETE CASCADE,
    browser_purpose text NOT NULL CHECK (browser_purpose IN ('portal', 'dev_portal')),
    installation_id uuid NOT NULL REFERENCES installations ON DELETE CASCADE,
    release_id uuid NOT NULL REFERENCES releases,
    environment text NOT NULL CHECK (environment IN ('dev', 'prod')),
    origin text NOT NULL REFERENCES app_frame_origins,
    token_hash text NOT NULL UNIQUE,
    token_expires_at timestamptz NOT NULL,
    asset_hash text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    CHECK ((parent_session_hash IS NULL) <> (parent_development_hash IS NULL)),
    CHECK (token_expires_at <= expires_at),
    CHECK (expires_at <= created_at + interval '8 hours')
);
CREATE INDEX app_frame_session_expiry ON app_frame_sessions(expires_at);
CREATE INDEX app_frame_session_parent ON app_frame_sessions(parent_session_hash);
CREATE INDEX app_frame_session_development ON app_frame_sessions(parent_development_hash);

-- One-time launch and renew codes. The portal knows the frame a code opens or
-- renews from its own launch, never from a message of the app.
CREATE TABLE app_frame_codes (
    code_hash text PRIMARY KEY,
    purpose text NOT NULL CHECK (purpose IN ('launch', 'renew')),
    frame_id uuid NOT NULL,
    user_id text NOT NULL REFERENCES users,
    parent_session_hash text REFERENCES sessions(id_hash) ON DELETE CASCADE,
    parent_development_hash text REFERENCES development_sessions(id_hash) ON DELETE CASCADE,
    browser_purpose text NOT NULL CHECK (browser_purpose IN ('portal', 'dev_portal')),
    frame_session_id uuid REFERENCES app_frame_sessions ON DELETE CASCADE,
    installation_id uuid NOT NULL REFERENCES installations ON DELETE CASCADE,
    release_id uuid NOT NULL REFERENCES releases,
    environment text NOT NULL CHECK (environment IN ('dev', 'prod')),
    portal_origin text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    CHECK ((parent_session_hash IS NULL) <> (parent_development_hash IS NULL)),
    CHECK ((purpose = 'launch' AND frame_session_id IS NULL)
        OR (purpose = 'renew' AND frame_session_id = frame_id)),
    CHECK (expires_at <= created_at + interval '60 seconds')
);
CREATE INDEX app_frame_code_expiry ON app_frame_codes(expires_at);
