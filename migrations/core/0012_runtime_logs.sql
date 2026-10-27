CREATE TABLE runtime_log_requests (
    id uuid PRIMARY KEY,
    actor text NOT NULL REFERENCES users,
    app_id text NOT NULL REFERENCES apps,
    environment text NOT NULL CHECK(environment IN ('dev','prod')),
    installation_id uuid NOT NULL REFERENCES installations,
    generation bigint NOT NULL,
    expected_container text NOT NULL,
    container_id text,
    tail integer NOT NULL CHECK(tail BETWEEN 1 AND 1000),
    since timestamptz,
    status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','ready','failed')),
    text text,
    truncated boolean NOT NULL DEFAULT false,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL DEFAULT (clock_timestamp()+interval '5 minutes')
);
CREATE INDEX runtime_logs_queue ON runtime_log_requests(created_at,id) WHERE status='queued';
CREATE INDEX runtime_logs_expiry ON runtime_log_requests(expires_at);
