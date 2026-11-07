-- Existing browser credentials have no trustworthy purpose. Require sign-in again;
-- keep independent CLI credentials and all app/user data intact.
DELETE FROM sessions;
DELETE FROM development_sessions;
DELETE FROM login_flows;
DELETE FROM cli_authorizations;
DELETE FROM cli_device_authorizations WHERE status IN ('pending', 'approved');

ALTER TABLE sessions ADD COLUMN browser_purpose text NOT NULL
    CHECK (browser_purpose IN ('portal', 'console'));
ALTER TABLE development_sessions ADD COLUMN browser_purpose text NOT NULL
    CHECK (browser_purpose IN ('portal', 'console'));
ALTER TABLE login_flows ADD COLUMN browser_purpose text NOT NULL
    CHECK (browser_purpose IN ('portal', 'console'));
ALTER TABLE login_flows ADD COLUMN callback_uri text NOT NULL;

-- A public, non-secret identifier for listing/revoking older CLI credentials.
-- Never expose token_hash as a credential-management identifier.
ALTER TABLE cli_tokens ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid();
CREATE UNIQUE INDEX cli_tokens_public_id ON cli_tokens(id);
