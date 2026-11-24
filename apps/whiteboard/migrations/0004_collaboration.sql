CREATE TABLE collab_rooms (
    resource_id varchar(36) PRIMARY KEY REFERENCES whiteboards(id) ON DELETE CASCADE,
    generation uuid NOT NULL,
    owner uuid,
    lease_until timestamptz NOT NULL,
    state bytea NOT NULL,
    saved_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE whiteboard_versions (
    id uuid PRIMARY KEY,
    board_id varchar(36) NOT NULL REFERENCES whiteboards(id) ON DELETE CASCADE,
    revision bigint NOT NULL,
    scene jsonb NOT NULL,
    author text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (board_id,revision)
);
