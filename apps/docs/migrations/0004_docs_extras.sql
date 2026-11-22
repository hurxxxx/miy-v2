-- Docs 보강 (COMMON-FEATURES-PLAN §2.2: trash restore, version history,
-- comments, templates). These were not in AI-DO 0002af062; see
-- docs/PORTING.md §8.

-- Trash: who moved a doc or page to the trash (shown in the trash list).
ALTER TABLE docs_native_docs ADD COLUMN trashed_by_id text;
ALTER TABLE docs_native_doc_pages ADD COLUMN trashed_by_id text;

-- Permanent deletion removes page rows; an attachment row must outlive its
-- page until the core confirms the detach (files.rs keeps the work there).
ALTER TABLE docs_page_attachments ALTER COLUMN page_id DROP NOT NULL;
ALTER TABLE docs_page_attachments DROP CONSTRAINT docs_page_attachments_page_id_fkey;
ALTER TABLE docs_page_attachments
    ADD CONSTRAINT docs_page_attachments_page_id_fkey
    FOREIGN KEY (page_id) REFERENCES docs_native_doc_pages (id) ON DELETE SET NULL;

-- Version history: one row per saved body revision, pruned to the latest 50
-- plus the last revision of each older UTC day.
CREATE TABLE docs_page_versions (
    id varchar(36) PRIMARY KEY,
    page_id varchar(36) NOT NULL REFERENCES docs_native_doc_pages (id) ON DELETE CASCADE,
    revision bigint NOT NULL,
    title varchar(200) NOT NULL,
    content_blocks json,
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    restored_from_revision bigint,
    CONSTRAINT uq_docs_page_versions_page_revision UNIQUE (page_id, revision)
);
CREATE INDEX ix_docs_page_versions_page_created ON docs_page_versions (page_id, created_at);

-- Comments on a page: threads of one level (a root and its replies). A
-- deleted comment keeps its row (body cleared) while replies remain.
CREATE TABLE docs_page_comments (
    id varchar(36) PRIMARY KEY,
    page_id varchar(36) NOT NULL REFERENCES docs_native_doc_pages (id) ON DELETE CASCADE,
    parent_id varchar(36) REFERENCES docs_page_comments (id) ON DELETE CASCADE,
    author_id text NOT NULL,
    body text NOT NULL,
    mention_ids json NOT NULL DEFAULT '[]',
    request_id uuid NOT NULL UNIQUE,
    resolved_at timestamp,
    resolved_by_id text,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    deleted_at timestamp
);
CREATE INDEX ix_docs_page_comments_page_created ON docs_page_comments (page_id, created_at);
CREATE INDEX ix_docs_page_comments_parent ON docs_page_comments (parent_id);

-- Who received which comment notification, so a person who loses access to
-- the page loses the notification too.
CREATE TABLE docs_comment_notifications (
    comment_id varchar(36) NOT NULL REFERENCES docs_page_comments (id) ON DELETE CASCADE,
    recipient_id text NOT NULL,
    kind varchar(16) NOT NULL,
    PRIMARY KEY (comment_id, recipient_id, kind),
    CONSTRAINT ck_docs_comment_notifications_kind CHECK (kind IN ('comment', 'mention'))
);
CREATE INDEX ix_docs_comment_notifications_recipient ON docs_comment_notifications (recipient_id);

-- Page templates of a space.
CREATE TABLE docs_templates (
    id varchar(36) PRIMARY KEY,
    space_id varchar(36) NOT NULL REFERENCES docs_spaces (id) ON DELETE CASCADE,
    title varchar(200) NOT NULL,
    description text NOT NULL DEFAULT '',
    content_blocks json NOT NULL,
    source_page_id varchar(36),
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT ck_docs_templates_title_not_blank CHECK (length(trim(title)) > 0)
);
CREATE INDEX ix_docs_templates_space ON docs_templates (space_id, title);

-- Pages saved before this migration start their history at their current revision.
INSERT INTO docs_page_versions (id, page_id, revision, title, content_blocks, created_by_id, created_at)
SELECT gen_random_uuid()::text, p.id, p.revision, p.title, p.content_blocks, p.created_by_id, p.updated_at
FROM docs_native_doc_pages p;
