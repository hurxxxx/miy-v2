-- Persist the consumed link before sending a password change to the identity provider.
CREATE TABLE password_resets (
    user_id text PRIMARY KEY REFERENCES users(id),
    link_id uuid NOT NULL UNIQUE REFERENCES account_links(id),
    password_changed boolean NOT NULL DEFAULT false,
    retry_at timestamptz NOT NULL DEFAULT clock_timestamp() + interval '30 seconds'
);

CREATE INDEX account_links_pending_expiry ON account_links(expires_at) WHERE status='pending';

-- A callback begun before session revocation must start a fresh authorization flow.
ALTER TABLE users ADD COLUMN browser_login_after timestamptz;
ALTER TABLE login_flows ADD COLUMN created_at timestamptz NOT NULL DEFAULT clock_timestamp();
