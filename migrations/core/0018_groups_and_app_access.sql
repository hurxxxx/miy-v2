-- Users and groups receive app use permissions; apps own their internal data access
-- (docs/PRD.md, miy v1 ADR 0012). The core database is the source of truth for groups.
-- Membership is computed on every check and never copied into user grants.
CREATE TABLE groups (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    -- hr: organisation unit, members are users whose primary group is exactly this one.
    -- local: manually assigned users. Neither kind inherits through the hierarchy.
    source text NOT NULL CHECK (source IN ('hr', 'local')),
    parent_id uuid REFERENCES groups(id),
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (parent_id IS NULL OR source = 'hr'),
    CHECK (parent_id IS DISTINCT FROM id)
);
CREATE UNIQUE INDEX groups_name_key ON groups (lower(btrim(name)));

ALTER TABLE users ADD COLUMN primary_group_id uuid REFERENCES groups(id);

CREATE TABLE group_members (
    group_id uuid NOT NULL REFERENCES groups(id),
    user_id text NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);

-- Production deployment stays a per-user permission until deployment approval and
-- privileged group protection exist (2026-11-03 decision).
CREATE TABLE group_grants (
    group_id uuid NOT NULL REFERENCES groups(id),
    app_id text NOT NULL REFERENCES apps(id),
    environment text NOT NULL CHECK (environment IN ('dev', 'prod')),
    permission text NOT NULL CHECK (permission IN ('use', 'develop')),
    PRIMARY KEY (group_id, app_id, environment, permission)
);

-- Absent row = 'selected': only explicit user and group grants admit users.
CREATE TABLE app_admission (
    app_id text NOT NULL REFERENCES apps(id),
    environment text NOT NULL CHECK (environment IN ('dev', 'prod')),
    mode text NOT NULL CHECK (mode IN ('selected', 'all')),
    changed_by text REFERENCES users(id),
    changed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (app_id, environment)
);

-- Company switch: a disabled app cannot be used, even by administrators.
CREATE TABLE app_controls (
    app_id text PRIMARY KEY REFERENCES apps(id),
    disabled boolean NOT NULL,
    changed_by text REFERENCES users(id),
    changed_at timestamptz NOT NULL DEFAULT now()
);

CREATE VIEW effective_grants AS
    SELECT g.user_id, g.app_id, g.environment, g.permission
    FROM grants g
    UNION
    SELECT u.id, gg.app_id, gg.environment, gg.permission
    FROM group_grants gg
    JOIN groups grp ON grp.id = gg.group_id AND grp.active
    JOIN users u ON u.enabled AND (
        (grp.source = 'hr' AND u.primary_group_id = grp.id)
        OR (grp.source = 'local' AND EXISTS (
            SELECT 1 FROM group_members m WHERE m.group_id = grp.id AND m.user_id = u.id)))
    UNION
    SELECT u.id, a.app_id, a.environment, 'use'
    FROM app_admission a
    JOIN users u ON u.enabled
    WHERE a.mode = 'all';
