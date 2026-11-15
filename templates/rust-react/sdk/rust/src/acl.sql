-- Apply explicitly through an application's own migration transaction.
-- These are application-owned tables, not platform grants or app Use rights.
-- The platform provisions schema app; the app migration role has no DB CREATE.
CREATE TABLE app.miy_acl_resources (
    installation_id uuid NOT NULL,
    environment text NOT NULL CHECK (environment IN ('dev','prod')),
    kind text NOT NULL,
    resource_id text NOT NULL,
    owner_id text NOT NULL,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    PRIMARY KEY (installation_id,environment,kind,resource_id)
);
CREATE TABLE app.miy_acl_grants (
    installation_id uuid NOT NULL,
    environment text NOT NULL,
    kind text NOT NULL,
    resource_id text NOT NULL,
    subject_kind text NOT NULL CHECK (subject_kind IN ('user','group')),
    subject_id text NOT NULL,
    role text NOT NULL,
    PRIMARY KEY (installation_id,environment,kind,resource_id,subject_kind,subject_id),
    FOREIGN KEY (installation_id,environment,kind,resource_id)
      REFERENCES app.miy_acl_resources ON DELETE CASCADE
);
CREATE INDEX miy_acl_grants_subject ON app.miy_acl_grants
    (installation_id,environment,kind,subject_kind,subject_id,role,resource_id);
CREATE TABLE app.miy_acl_changes (
    installation_id uuid NOT NULL,
    environment text NOT NULL,
    kind text NOT NULL,
    resource_id text NOT NULL,
    request_id uuid NOT NULL,
    actor_id text NOT NULL,
    payload jsonb NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (installation_id,environment,kind,resource_id,request_id)
);
-- Keep the audit even if the application later deletes its business resource.
CREATE TABLE app.miy_acl_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    installation_id uuid NOT NULL,
    environment text NOT NULL,
    kind text NOT NULL,
    resource_id text NOT NULL,
    actor_id text NOT NULL,
    action text NOT NULL CHECK (action IN ('create','replace')),
    request_id uuid,
    before_grants jsonb NOT NULL,
    after_grants jsonb NOT NULL,
    revision bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
