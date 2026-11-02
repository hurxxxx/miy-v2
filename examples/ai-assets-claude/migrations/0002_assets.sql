CREATE TABLE app.assets (
    id uuid PRIMARY KEY,
    owner text NOT NULL CHECK (owner <> ''),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100 AND name = btrim(name)),
    quantity integer NOT NULL CHECK (quantity BETWEEN 0 AND 100000),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX assets_owner_created_idx ON app.assets (owner, created_at, id);
