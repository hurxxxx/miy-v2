-- Install through an explicitly numbered app migration.
CREATE TABLE app.miy_installation_stewards (
    installation_id uuid NOT NULL,
    environment text NOT NULL CHECK (environment IN ('dev','prod')),
    user_id text NOT NULL,
    PRIMARY KEY (installation_id, environment, user_id)
);

-- Keep this receipt after a handover. A lost reply can never restore an old role.
CREATE TABLE app.miy_steward_receipts (
    assignment_id uuid PRIMARY KEY,
    installation_id uuid NOT NULL,
    environment text NOT NULL CHECK (environment IN ('dev','prod')),
    user_id text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
