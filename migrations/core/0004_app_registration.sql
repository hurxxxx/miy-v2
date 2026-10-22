ALTER TABLE users ADD COLUMN can_develop boolean NOT NULL DEFAULT false;
-- The validation realm has one explicitly provisioned developer.
UPDATE users SET can_develop=true WHERE id='00000000-0000-4000-8000-000000000002';
CREATE TABLE app_registrations (
    id uuid PRIMARY KEY,
    app_id text NOT NULL UNIQUE,
    name text NOT NULL,
    actor text NOT NULL REFERENCES users,
    status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
    decided_by text REFERENCES users,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
