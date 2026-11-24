CREATE TABLE collab_rooms (
    resource_id varchar(36) PRIMARY KEY REFERENCES docs_native_doc_pages(id) ON DELETE CASCADE,
    generation uuid NOT NULL,
    owner uuid,
    lease_until timestamptz NOT NULL,
    state bytea NOT NULL,
    saved_at timestamptz NOT NULL DEFAULT now()
);
