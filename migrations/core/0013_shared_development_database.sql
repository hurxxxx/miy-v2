-- Database identity belongs to an app/environment, independently of deployments.
-- Existing physical databases, role names and encrypted passwords are retained.
ALTER TABLE db_resources ADD COLUMN id uuid;
ALTER TABLE db_resources ADD COLUMN app_id text REFERENCES apps;
ALTER TABLE db_resources ADD COLUMN environment text CHECK(environment IN ('dev','prod'));
UPDATE db_resources r SET id=r.installation_id,app_id=i.app_id,environment=i.environment
FROM installations i WHERE i.id=r.installation_id;
ALTER TABLE db_resources ALTER COLUMN id SET NOT NULL;
ALTER TABLE db_resources ALTER COLUMN app_id SET NOT NULL;
ALTER TABLE db_resources ALTER COLUMN environment SET NOT NULL;
ALTER TABLE db_resources DROP CONSTRAINT db_resources_pkey;
ALTER TABLE db_resources ALTER COLUMN installation_id DROP NOT NULL;
ALTER TABLE db_resources ADD PRIMARY KEY(id);
ALTER TABLE db_resources ADD UNIQUE(installation_id);
ALTER TABLE db_resources ADD UNIQUE(app_id,environment);
ALTER TABLE db_resources ADD COLUMN status text NOT NULL DEFAULT 'ready'
    CHECK(status IN ('not_prepared','provisioning','ready','failed'));
ALTER TABLE db_resources ADD COLUMN endpoint_host text;
ALTER TABLE db_resources ADD COLUMN endpoint_port integer CHECK(endpoint_port BETWEEN 1 AND 65535);
ALTER TABLE db_resources ADD COLUMN schema_fingerprint text;
ALTER TABLE db_resources ADD COLUMN schema_generation bigint NOT NULL DEFAULT 0;
ALTER TABLE release_checks ADD COLUMN schema_fingerprint text;

CREATE TABLE app_dev_sessions (
    id uuid PRIMARY KEY,
    app_id text NOT NULL REFERENCES apps,
    database_id uuid NOT NULL REFERENCES db_resources,
    actor text NOT NULL REFERENCES users,
    parent_session_id uuid NOT NULL REFERENCES cli_sessions,
    idempotency_key text NOT NULL,
    input_hash text NOT NULL,
    request_data jsonb NOT NULL,
    status text NOT NULL DEFAULT 'queued'
        CHECK(status IN ('queued','preparing','ready','failed','closed','expired','revoked')),
    role_name text NOT NULL UNIQUE,
    connection_secret text NOT NULL,
    tunnel_token_hash text NOT NULL UNIQUE,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '120 seconds',
    cleaned_at timestamptz,
    UNIQUE(actor,parent_session_id,idempotency_key)
);
CREATE UNIQUE INDEX app_dev_sessions_one_active ON app_dev_sessions(app_id)
    WHERE status IN ('queued','preparing','ready');
CREATE INDEX app_dev_sessions_cleanup ON app_dev_sessions(expires_at) WHERE cleaned_at IS NULL;
