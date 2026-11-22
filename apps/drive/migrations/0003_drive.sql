-- Drive (드라이브): AI-DO's Files module (domains/files/models.py at 0002af062).
-- File bytes live in platform Storage; these rows hold the business data:
-- spaces (AI-DO workspaces), folders, files, their versions (each pointing at
-- a Core file ID), trash and pending Storage cleanup. Space membership is the
-- SDK resource ACL (0002_acl.sql, kind `space`).

CREATE TABLE drive_spaces (
    id varchar(36) PRIMARY KEY,
    name varchar(120) NOT NULL,
    created_by text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);

-- AI-DO file_manager_folders without the RAG corpus/partition columns.
-- deleted_at marks a row in the trash; trash_id groups the rows one delete
-- moved there (the selected item and its subtree) and trash_root marks the
-- selected item itself.
CREATE TABLE file_manager_folders (
    id varchar(36) PRIMARY KEY,
    workspace_id varchar(36) NOT NULL REFERENCES drive_spaces(id) ON DELETE CASCADE,
    parent_id varchar(36) REFERENCES file_manager_folders(id) ON DELETE CASCADE,
    owner_id text NOT NULL,
    name varchar(255) NOT NULL,
    visibility varchar(24) NOT NULL DEFAULT 'private',
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    deleted_at timestamp,
    trash_id varchar(36),
    trash_root boolean NOT NULL DEFAULT false,
    trashed_by text,
    CONSTRAINT ck_file_manager_folders_visibility CHECK (visibility IN ('private','workspace')),
    CONSTRAINT ck_file_manager_folders_trash CHECK (
        (deleted_at IS NULL AND trash_id IS NULL AND NOT trash_root)
        OR (deleted_at IS NOT NULL AND trash_id IS NOT NULL)
    )
);
CREATE INDEX ix_file_manager_folders_workspace_parent ON file_manager_folders (workspace_id, parent_id);
CREATE INDEX ix_file_manager_folders_workspace_owner ON file_manager_folders (workspace_id, owner_id);
CREATE INDEX ix_file_manager_folders_workspace_visibility ON file_manager_folders (workspace_id, visibility);
CREATE INDEX ix_file_manager_folders_parent_id ON file_manager_folders (parent_id);
CREATE INDEX ix_file_manager_folders_deleted_at ON file_manager_folders (deleted_at);
CREATE INDEX ix_file_manager_folders_trash_id ON file_manager_folders (trash_id);

-- AI-DO file_manager_files without storage_key (bytes are versions) and the
-- RAG extraction columns. A file appears once its first version is ready
-- (current_version_id). An abandoned first upload is deleted_at without trash_id.
CREATE TABLE file_manager_files (
    id varchar(36) PRIMARY KEY,
    workspace_id varchar(36) NOT NULL REFERENCES drive_spaces(id) ON DELETE CASCADE,
    folder_id varchar(36) REFERENCES file_manager_folders(id) ON DELETE SET NULL,
    owner_id text NOT NULL,
    filename varchar(512) NOT NULL,
    content_type varchar(160) NOT NULL DEFAULT 'application/octet-stream',
    size_bytes bigint NOT NULL DEFAULT 0,
    visibility varchar(24) NOT NULL DEFAULT 'private',
    current_version_id varchar(36),
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    deleted_at timestamp,
    trash_id varchar(36),
    trash_root boolean NOT NULL DEFAULT false,
    trashed_by text,
    CONSTRAINT ck_file_manager_files_visibility CHECK (visibility IN ('private','workspace')),
    CONSTRAINT ck_file_manager_files_trash CHECK (
        (trash_id IS NULL AND NOT trash_root) OR deleted_at IS NOT NULL
    )
);
CREATE INDEX ix_file_manager_files_workspace_folder ON file_manager_files (workspace_id, folder_id);
CREATE INDEX ix_file_manager_files_workspace_owner ON file_manager_files (workspace_id, owner_id);
CREATE INDEX ix_file_manager_files_workspace_visibility ON file_manager_files (workspace_id, visibility);
CREATE INDEX ix_file_manager_files_folder_id ON file_manager_files (folder_id);
CREATE INDEX ix_file_manager_files_deleted_at ON file_manager_files (deleted_at);
CREATE INDEX ix_file_manager_files_trash_id ON file_manager_files (trash_id);

-- One uploaded content of a file. request_id is written before the Core
-- reservation so a lost answer is recovered by the same ID; attach_request_id
-- is the Core attach request. storage_file_id is the Core file.
CREATE TABLE file_manager_file_versions (
    id varchar(36) PRIMARY KEY,
    file_id varchar(36) NOT NULL REFERENCES file_manager_files(id) ON DELETE CASCADE,
    version_number integer NOT NULL,
    request_id uuid NOT NULL UNIQUE,
    attach_request_id uuid NOT NULL UNIQUE,
    storage_file_id uuid UNIQUE,
    filename varchar(512) NOT NULL,
    content_type varchar(160) NOT NULL,
    size_bytes bigint NOT NULL,
    transfer varchar(8) NOT NULL,
    status varchar(16) NOT NULL,
    attached boolean NOT NULL DEFAULT false,
    sha256 varchar(64),
    uploaded_by text NOT NULL,
    created_at timestamp NOT NULL,
    completed_at timestamp,
    CONSTRAINT uq_file_manager_file_versions_number UNIQUE (file_id, version_number),
    CONSTRAINT ck_file_manager_file_versions_transfer CHECK (transfer IN ('single','parts')),
    CONSTRAINT ck_file_manager_file_versions_status CHECK (status IN ('reserving','uploading','ready','cancelled')),
    CONSTRAINT ck_file_manager_file_versions_size CHECK (size_bytes >= 0)
);
CREATE INDEX ix_file_manager_file_versions_file ON file_manager_file_versions (file_id, version_number);

-- AI-DO file_manager_storage_cleanup_jobs: Core files to release after the
-- app commit that removed their references (detach when attached, else cancel).
CREATE TABLE file_manager_storage_cleanup_jobs (
    storage_file_id uuid PRIMARY KEY,
    reference varchar(128) NOT NULL,
    attempts integer NOT NULL DEFAULT 0,
    last_error varchar(120),
    created_at timestamp NOT NULL,
    next_retry_at timestamp NOT NULL
);
CREATE INDEX ix_file_manager_storage_cleanup_jobs_retry ON file_manager_storage_cleanup_jobs (next_retry_at);
