-- Applied transactionally by the platform migration runner (SQLx owns the transaction).
CREATE TABLE app.todos (
    id uuid PRIMARY KEY,
    owner_id text NOT NULL CHECK (length(owner_id) > 0),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200 AND title = btrim(title)),
    completed boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX todos_owner_created_idx ON app.todos (owner_id, created_at DESC, id DESC);
