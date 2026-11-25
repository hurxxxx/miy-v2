-- Only digests of the browser code, preview grant and portal session are retained.
CREATE TABLE preview_browser_logins (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id text NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    instance_id uuid NOT NULL,
    host_user text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cli_session uuid REFERENCES cli_sessions(id) ON DELETE CASCADE,
    delegation uuid REFERENCES delegations(id) ON DELETE CASCADE,
    port integer NOT NULL CHECK (port BETWEEN 1 AND 65535),
    state text NOT NULL,
    challenge text NOT NULL,
    code_hash text UNIQUE,
    grant_hash text UNIQUE,
    session_hash text,
    development boolean,
    expires_at timestamptz NOT NULL DEFAULT now() + interval '5 minutes',
    CHECK (num_nonnulls(cli_session, delegation) >= 1)
);
CREATE INDEX preview_browser_expiry ON preview_browser_logins(expires_at);
