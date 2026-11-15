-- App databases own file references and ACLs. Core owns immutable bytes and quotas.
-- No cascading deletion: app removal must not erase stored files.
CREATE TABLE app_files (
    id uuid PRIMARY KEY,
    installation_id uuid NOT NULL REFERENCES installations(id),
    request_id uuid NOT NULL,
    created_by text NOT NULL REFERENCES users(id),
    input_hash text NOT NULL,
    version integer NOT NULL DEFAULT 1 CHECK (version = 1),
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    mime text NOT NULL CHECK (length(mime) BETWEEN 3 AND 128),
    size bigint NOT NULL CHECK (size BETWEEN 0 AND 10485760),
    sha256 text CHECK (sha256 ~ '^[a-f0-9]{64}$'),
    status text NOT NULL CHECK (status IN ('pending','uploading','ready','attached','cancelling','cancelled','expired')),
    reference text CHECK (length(reference) BETWEEN 1 AND 512),
    attach_request_id uuid,
    expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(installation_id,request_id),
    CHECK ((status IN ('pending','uploading')) = (expires_at IS NOT NULL)),
    CHECK (status NOT IN ('ready','attached') OR sha256 IS NOT NULL),
    CHECK ((status = 'attached') = (reference IS NOT NULL)),
    CHECK ((status = 'attached') = (attach_request_id IS NOT NULL))
);
CREATE INDEX app_files_reserved ON app_files(installation_id,status);
CREATE INDEX app_files_expiring ON app_files(expires_at) WHERE status IN ('pending','uploading');
