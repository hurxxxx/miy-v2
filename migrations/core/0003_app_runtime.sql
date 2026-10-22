ALTER TABLE releases ADD COLUMN image_id text;
ALTER TABLE releases ADD COLUMN source_commit text;
ALTER TABLE installations ADD COLUMN runtime_image text;
ALTER TABLE installations ADD COLUMN runtime_container text;
ALTER TABLE installations ADD COLUMN runtime_port integer;
ALTER TABLE installations ADD COLUMN runtime_secret text;
CREATE TABLE runtime_jobs (
    id uuid PRIMARY KEY, app_id text NOT NULL REFERENCES apps,
    environment text NOT NULL CHECK (environment IN ('dev','prod')),
    actor text NOT NULL REFERENCES users, action text NOT NULL,
    image_id text, installation_id uuid REFERENCES installations,
    release_id uuid REFERENCES releases, idempotency_key text NOT NULL,
    input_hash text NOT NULL, status text NOT NULL DEFAULT 'queued',
    step text NOT NULL DEFAULT 'queued', error_code text,
    created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(actor,idempotency_key)
);
