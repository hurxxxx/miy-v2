CREATE TABLE app.assets (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (char_length(regexp_replace(name, '[[:space:]]', '', 'g')) BETWEEN 1 AND 100),
    quantity INTEGER NOT NULL CHECK (quantity BETWEEN 0 AND 100000),
    owner TEXT NOT NULL
);
CREATE INDEX assets_owner_idx ON app.assets (owner);
