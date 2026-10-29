-- Build computation has its own queue: it must not block runtime health,
-- development sessions, installation reconciliation or lifecycle job admission.
CREATE TABLE app_builds (
    id uuid PRIMARY KEY,
    app_id text NOT NULL REFERENCES apps,
    actor text NOT NULL REFERENCES users,
    request_id uuid NOT NULL,
    source_commit text NOT NULL CHECK (source_commit ~ '^[0-9a-f]{40}$'),
    archive_hash text NOT NULL CHECK (archive_hash ~ '^[0-9a-f]{64}$'),
    size_bytes bigint NOT NULL CHECK (size_bytes BETWEEN 1 AND 67108864),
    platform text NOT NULL CHECK (platform IN ('linux/amd64','linux/arm64')),
    status text NOT NULL DEFAULT 'receiving' CHECK (status IN ('receiving','queued','running','succeeded','failed')),
    step text NOT NULL DEFAULT 'receiving',
    error_code text,
    image_id text CHECK (image_id IS NULL OR image_id ~ '^sha256:[0-9a-f]{64}$'),
    version text,
    bundle_version text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    finished_at timestamptz,
    upload_expires_at timestamptz NOT NULL DEFAULT (clock_timestamp()+interval '5 minutes'),
    log_bytes bigint NOT NULL DEFAULT 0 CHECK (log_bytes BETWEEN 0 AND 2097152),
    log_sequence bigint NOT NULL DEFAULT 0 CHECK (log_sequence >= 0),
    log_truncated boolean NOT NULL DEFAULT false,
    UNIQUE(actor,request_id)
);
-- New explicit requests may retry a terminal failed build. Existing request
-- aliases always retain their original receipt; successes and in-flight work dedupe.
CREATE UNIQUE INDEX app_build_content_identity ON app_builds(actor,app_id,source_commit,archive_hash,platform) WHERE status <> 'failed';
CREATE INDEX app_build_queue ON app_builds(created_at,id) WHERE status='queued';
CREATE INDEX app_build_active_actor ON app_builds(actor,status);
CREATE TABLE app_build_requests (
    actor text NOT NULL REFERENCES users,
    request_id uuid NOT NULL,
    build_id uuid NOT NULL REFERENCES app_builds(id) ON DELETE CASCADE,
    PRIMARY KEY(actor,request_id)
);
CREATE TABLE app_build_logs (
    build_id uuid NOT NULL REFERENCES app_builds(id) ON DELETE CASCADE,
    sequence bigint NOT NULL CHECK (sequence > 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    text text NOT NULL CHECK (octet_length(text) BETWEEN 1 AND 2097152),
    PRIMARY KEY(build_id,sequence)
);
