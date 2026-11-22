-- User switch (docs/CORE-PERMISSIONS.md "사용자 전환"), as AI-DO did it: a
-- console administrator signs in as a person. The result is that person's
-- ordinary portal session; the session keeps who started it, so it ends with
-- that administrator. The start itself is a management audit row.
ALTER TABLE sessions
    ADD COLUMN impersonator_user_id text REFERENCES users(id) ON DELETE CASCADE;

-- One-time codes the console hands to the target portal (60 seconds).
CREATE TABLE user_switch_codes (
    code_hash text PRIMARY KEY,
    target_user_id text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    impersonator_user_id text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    browser_purpose text NOT NULL CHECK (browser_purpose IN ('portal','dev_portal')),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX user_switch_codes_expires ON user_switch_codes(expires_at);
