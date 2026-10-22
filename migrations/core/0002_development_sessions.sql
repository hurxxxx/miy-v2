CREATE TABLE development_sessions (
    id_hash text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    csrf text NOT NULL,
    expires_at timestamptz NOT NULL
);
