-- Nominations survive release changes. Business roles and steward succession live in the app DB.
CREATE TABLE installation_steward_assignments (
    id uuid PRIMARY KEY,
    installation_id uuid NOT NULL REFERENCES installations ON DELETE CASCADE,
    subject_id text NOT NULL REFERENCES users,
    assigned_by text NOT NULL REFERENCES users,
    recovery_reason text CHECK (length(recovery_reason) BETWEEN 1 AND 500),
    status text NOT NULL CHECK (status IN ('pending','issued','applied','blocked')),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    superseded_at timestamptz
);
CREATE UNIQUE INDEX installation_current_steward_assignment ON installation_steward_assignments (installation_id)
    WHERE superseded_at IS NULL;
