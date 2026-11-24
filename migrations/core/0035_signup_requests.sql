CREATE TABLE signup_requests (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    email text NOT NULL CHECK (char_length(email) BETWEEN 3 AND 254),
    created_at timestamptz NOT NULL DEFAULT now(),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    decided_at timestamptz,
    decided_by text REFERENCES users(id),
    link_id uuid UNIQUE REFERENCES account_links(id),
    CHECK ((status = 'pending') = (decided_at IS NULL)),
    CHECK ((status = 'pending') = (decided_by IS NULL)),
    CHECK ((status = 'approved') = (link_id IS NOT NULL))
);
CREATE UNIQUE INDEX signup_pending_email ON signup_requests (lower(email)) WHERE status = 'pending';
CREATE INDEX signup_requests_created ON signup_requests (created_at);
