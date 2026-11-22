-- Docs (문서): AI-DO apps/api/src/ai_do_api/domains/docs/models.py at 0002af062.
-- Users are miy user IDs (text, no foreign key: the people live in the core).
-- AI-DO workspaces/app spaces become docs_spaces; the global space is seeded
-- below. Left out with their features (docs/PORTING.md §1): collections, link
-- shares, meeting access grants, the Yjs collaboration store and RAG columns.
-- miy additions: docs_spaces, docs_managers, pages.revision (single-editor
-- saving with optimistic concurrency) and pages.search_text (integrated search).

CREATE TABLE docs_spaces (
    id varchar(36) PRIMARY KEY,
    kind varchar(24) NOT NULL,
    name varchar(120) NOT NULL,
    description text NOT NULL DEFAULT '',
    created_by text,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT ck_docs_spaces_kind CHECK (kind IN ('app_global', 'app_space')),
    CONSTRAINT ck_docs_spaces_name_not_blank CHECK (length(trim(name)) > 0)
);
CREATE UNIQUE INDEX uq_docs_spaces_app_global ON docs_spaces (kind) WHERE kind = 'app_global';

-- AI-DO seeded one empty global space per app at installation
-- (auth/app_global_spaces.py seed_app_global_spaces).
INSERT INTO docs_spaces (id, kind, name, description, created_by, created_at, updated_at)
VALUES ('global', 'app_global', '문서', '', NULL,
        (now() AT TIME ZONE 'utc'), (now() AT TIME ZONE 'utc'));

CREATE TABLE docs_managers (
    user_id text PRIMARY KEY,
    granted_by text NOT NULL,
    created_at timestamp NOT NULL
);

CREATE TABLE docs_native_docs (
    id varchar(36) PRIMARY KEY,
    workspace_id varchar(36) NOT NULL REFERENCES docs_spaces (id) ON DELETE CASCADE,
    owner_id text NOT NULL,
    title varchar(200) NOT NULL,
    doc_type varchar(40) NOT NULL DEFAULT 'general',
    source_app varchar(64) NOT NULL DEFAULT 'docs',
    source_kind varchar(64) NOT NULL DEFAULT 'manual',
    source_ref varchar(128),
    generation_kind varchar(32) NOT NULL DEFAULT 'human',
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    trashed_at timestamp,
    CONSTRAINT ck_docs_native_docs_doc_type CHECK (
        doc_type IN ('general', 'meeting_notes', 'project_brief', 'spec', 'policy', 'guide', 'memo')
    )
);
CREATE INDEX ix_docs_native_docs_workspace_id ON docs_native_docs (workspace_id);
CREATE INDEX ix_docs_native_docs_owner_id ON docs_native_docs (owner_id);
CREATE INDEX ix_docs_native_docs_owner_created ON docs_native_docs (owner_id, created_at);
CREATE INDEX ix_docs_native_docs_workspace_updated ON docs_native_docs (workspace_id, updated_at);
CREATE INDEX ix_docs_native_docs_trashed_at ON docs_native_docs (trashed_at);

CREATE TABLE docs_doc_targets (
    id varchar(36) PRIMARY KEY,
    doc_id varchar(36) NOT NULL REFERENCES docs_native_docs (id) ON DELETE CASCADE,
    target_app varchar(64) NOT NULL,
    target_type varchar(64) NOT NULL,
    target_id varchar(128) NOT NULL,
    is_primary boolean NOT NULL DEFAULT true,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_docs_doc_targets_doc_target UNIQUE (doc_id, target_app, target_type, target_id)
);
CREATE INDEX ix_docs_doc_targets_doc_id ON docs_doc_targets (doc_id);
CREATE INDEX ix_docs_doc_targets_target_lookup ON docs_doc_targets (target_app, target_type, target_id);
CREATE UNIQUE INDEX uq_docs_doc_targets_primary ON docs_doc_targets (doc_id) WHERE is_primary IS TRUE;

CREATE TABLE docs_native_doc_pages (
    id varchar(36) PRIMARY KEY,
    doc_id varchar(36) NOT NULL REFERENCES docs_native_docs (id) ON DELETE CASCADE,
    parent_id varchar(36) REFERENCES docs_native_doc_pages (id) ON DELETE CASCADE,
    title varchar(200) NOT NULL,
    content_format varchar(24) NOT NULL DEFAULT 'block',
    content_blocks json,
    content_text text,
    sort_order integer NOT NULL DEFAULT 0,
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    trashed_at timestamp,
    revision bigint NOT NULL DEFAULT 1,
    search_text text NOT NULL DEFAULT '',
    -- AI-DO also allowed 'html'; HTML pages are excluded (frame frame-src 'none').
    CONSTRAINT ck_docs_native_doc_pages_content_format CHECK (content_format IN ('block')),
    CONSTRAINT ck_docs_native_doc_pages_revision CHECK (revision > 0)
);
CREATE INDEX ix_docs_native_doc_pages_doc_id ON docs_native_doc_pages (doc_id);
CREATE INDEX ix_docs_native_doc_pages_parent_id ON docs_native_doc_pages (parent_id);
CREATE INDEX ix_docs_native_doc_pages_created_by_id ON docs_native_doc_pages (created_by_id);
CREATE INDEX ix_docs_native_doc_pages_trashed_at ON docs_native_doc_pages (trashed_at);

-- AI-DO keeps storage_key (its object store); miy keeps the core file and the
-- reservation intent (request_id) written before the core reservation.
CREATE TABLE docs_page_attachments (
    id varchar(36) PRIMARY KEY,
    page_id varchar(36) NOT NULL REFERENCES docs_native_doc_pages (id) ON DELETE CASCADE,
    filename varchar(255) NOT NULL,
    content_type varchar(120) NOT NULL DEFAULT 'application/octet-stream',
    size_bytes integer NOT NULL DEFAULT 0,
    request_id uuid NOT NULL UNIQUE,
    file_id text,
    status varchar(16) NOT NULL DEFAULT 'pending',
    uploaded_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    CONSTRAINT ck_docs_page_attachments_status CHECK (
        status IN ('pending', 'ready', 'attached', 'detaching', 'detached')
    )
);
CREATE INDEX ix_docs_page_attachments_page_id ON docs_page_attachments (page_id);
CREATE INDEX ix_docs_page_attachments_page_created ON docs_page_attachments (page_id, created_at);
CREATE INDEX ix_docs_page_attachments_uploaded_by_id ON docs_page_attachments (uploaded_by_id);
CREATE INDEX ix_docs_page_attachments_detaching ON docs_page_attachments (created_at) WHERE status = 'detaching';

CREATE TABLE docs_native_doc_user_shares (
    id varchar(36) PRIMARY KEY,
    doc_id varchar(36) NOT NULL REFERENCES docs_native_docs (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    access_level varchar(16) NOT NULL DEFAULT 'read',
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    CONSTRAINT uq_docs_native_doc_user_share UNIQUE (doc_id, user_id),
    CONSTRAINT ck_docs_native_doc_user_shares_access_level CHECK (access_level IN ('read', 'edit'))
);
CREATE INDEX ix_docs_native_doc_user_shares_doc_id ON docs_native_doc_user_shares (doc_id);
CREATE INDEX ix_docs_native_doc_user_shares_user_id ON docs_native_doc_user_shares (user_id);

CREATE TABLE docs_user_item_prefs (
    id varchar(36) PRIMARY KEY,
    user_id text NOT NULL,
    source_type varchar(40) NOT NULL,
    source_doc_id varchar(36) NOT NULL,
    is_favorite boolean NOT NULL DEFAULT false,
    last_viewed_at timestamp,
    last_viewed_page_source_id varchar(36),
    last_viewed_page_title varchar(200),
    CONSTRAINT uq_docs_user_item_pref UNIQUE (user_id, source_type, source_doc_id)
);
CREATE INDEX ix_docs_user_item_prefs_user_id ON docs_user_item_prefs (user_id);
CREATE INDEX ix_docs_user_item_prefs_source_doc_id ON docs_user_item_prefs (source_doc_id);

-- Body images (AI-DO domains/media MediaFile), linked to a page by
-- resource_type 'docs_native_page'.
CREATE TABLE media_files (
    id varchar(36) PRIMARY KEY,
    request_id uuid NOT NULL UNIQUE,
    file_id text,
    filename varchar(255) NOT NULL,
    content_type varchar(120) NOT NULL,
    size_bytes integer NOT NULL DEFAULT 0,
    uploaded_by_id text NOT NULL,
    resource_type varchar(40),
    resource_id varchar(36),
    status varchar(16) NOT NULL DEFAULT 'pending',
    created_at timestamp NOT NULL,
    CONSTRAINT ck_media_files_status CHECK (
        status IN ('pending', 'ready', 'attached', 'detaching', 'detached')
    )
);
CREATE INDEX ix_media_files_resource ON media_files (resource_type, resource_id);
CREATE INDEX ix_media_files_uploaded_by_id ON media_files (uploaded_by_id);
CREATE INDEX ix_media_files_detaching ON media_files (created_at) WHERE status = 'detaching';
