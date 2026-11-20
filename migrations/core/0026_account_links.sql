-- One-time links an administrator hands to one person. An invitation creates the
-- sign-in account and registers it; a password reset sets a new password.
-- Only the SHA-256 of the secret is stored: the link itself is shown once.
CREATE TABLE account_links (
    id uuid PRIMARY KEY,
    token_hash text NOT NULL UNIQUE CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    kind text NOT NULL CHECK (kind IN ('invitation','password_reset')),
    -- Invitation: the display name the administrator expects. Reset: unused.
    name text CHECK (name IS NULL OR char_length(name) BETWEEN 1 AND 120),
    can_develop boolean NOT NULL DEFAULT false,
    -- Reset: the account whose password changes. Invitation: the account it created.
    user_id text REFERENCES users(id),
    created_by text NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','completed','revoked')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    completed_at timestamptz,
    CHECK (kind <> 'password_reset' OR user_id IS NOT NULL),
    CHECK (kind <> 'invitation' OR name IS NOT NULL),
    CHECK (kind <> 'invitation' OR (status = 'completed') = (user_id IS NOT NULL)),
    CHECK ((status = 'completed') = (completed_at IS NOT NULL))
);
CREATE INDEX account_links_created ON account_links(created_at DESC);
CREATE INDEX account_links_pending_user ON account_links(user_id) WHERE status = 'pending';
