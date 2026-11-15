-- Core provisions schema app; the migration owner cannot create databases or schemas.
CREATE TABLE app.projects (
    id uuid PRIMARY KEY,
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200)
);
CREATE TABLE app.tasks (
    id uuid PRIMARY KEY,
    project_id uuid NOT NULL REFERENCES app.projects,
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    completed boolean NOT NULL DEFAULT false
);
CREATE INDEX tasks_project ON app.tasks(project_id,id);
-- Intent survives a Core response loss. An app reference and Core attach marker
-- are distinct commits; ready bytes are never garbage-collected automatically.
CREATE TABLE app.attachments (
    request_id uuid PRIMARY KEY,
    project_id uuid NOT NULL REFERENCES app.projects,
    actor_id text NOT NULL,
    name text NOT NULL,
    mime text NOT NULL,
    size bigint NOT NULL CHECK (size BETWEEN 0 AND 10485760),
    file_id uuid UNIQUE,
    attached boolean NOT NULL DEFAULT false,
    attach_request uuid,
    CHECK (NOT attached OR (file_id IS NOT NULL AND attach_request IS NOT NULL))
);
CREATE INDEX attachments_project ON app.attachments(project_id,request_id);
