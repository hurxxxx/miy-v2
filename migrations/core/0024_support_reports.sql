-- Anonymous, manually submitted reports are unverified input, never identities.
CREATE TABLE support_reports (
    sequence bigint GENERATED ALWAYS AS IDENTITY UNIQUE NOT NULL,
    id uuid PRIMARY KEY,
    payload jsonb NOT NULL CHECK (octet_length(payload::text) <= 20000),
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','investigating','resolved','closed')),
    note text NOT NULL DEFAULT '' CHECK (char_length(note) <= 4000),
    revision bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text REFERENCES users(id)
);
CREATE INDEX support_reports_created ON support_reports(created_at);
CREATE INDEX support_reports_status_sequence ON support_reports(status,sequence DESC);
