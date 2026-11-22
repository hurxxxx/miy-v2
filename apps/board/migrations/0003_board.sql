-- Board (게시판), ported from AI-DO 0002af062 (docs/PORTING.md §3.1).
-- Names, types and constraints follow AI-DO's SQLAlchemy models. User
-- references are miy user IDs (text, no foreign key: users live in the core).
-- AI-DO workspaces become board spaces; their membership is the SDK resource
-- ACL (0002_acl.sql, kind "space").

-- AI-DO domains/community/models.py
CREATE TABLE community_channels (
    id varchar(36) PRIMARY KEY,
    key varchar(64) NOT NULL,
    name varchar(120) NOT NULL,
    description text NOT NULL DEFAULT '',
    position integer NOT NULL DEFAULT 0,
    active boolean NOT NULL DEFAULT true,
    read_only boolean NOT NULL DEFAULT false,
    force_anonymous boolean NOT NULL DEFAULT false,
    admin_only_content boolean NOT NULL DEFAULT false,
    template_title varchar(240) NOT NULL DEFAULT '',
    template_body text NOT NULL DEFAULT '',
    last_post_number integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE UNIQUE INDEX ix_community_channels_key ON community_channels (key);
CREATE INDEX ix_community_channels_position ON community_channels (position);

CREATE TABLE community_posts (
    id varchar(36) PRIMARY KEY,
    channel_id varchar(36) NOT NULL REFERENCES community_channels (id),
    number integer NOT NULL,
    author_id text NOT NULL,
    is_anonymous boolean NOT NULL DEFAULT false,
    is_secret boolean NOT NULL DEFAULT false,
    password_hash varchar(255),
    title varchar(240) NOT NULL,
    body text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_community_posts_channel_number UNIQUE (channel_id, number)
);
CREATE INDEX ix_community_posts_channel_id ON community_posts (channel_id);
CREATE INDEX ix_community_posts_author_id ON community_posts (author_id);
CREATE INDEX ix_community_posts_channel_created ON community_posts (channel_id, created_at);

CREATE TABLE community_post_reads (
    post_id varchar(36) NOT NULL REFERENCES community_posts (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    read_at timestamp NOT NULL,
    PRIMARY KEY (post_id, user_id)
);
CREATE INDEX ix_community_post_reads_user_id ON community_post_reads (user_id);

CREATE TABLE community_comments (
    id varchar(36) PRIMARY KEY,
    post_id varchar(36) NOT NULL REFERENCES community_posts (id) ON DELETE CASCADE,
    author_id text NOT NULL,
    parent_comment_id varchar(36) REFERENCES community_comments (id) ON DELETE CASCADE,
    is_anonymous boolean NOT NULL DEFAULT false,
    is_deleted boolean NOT NULL DEFAULT false,
    body text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_community_comments_post_id ON community_comments (post_id);
CREATE INDEX ix_community_comments_author_id ON community_comments (author_id);
CREATE INDEX ix_community_comments_parent_comment_id ON community_comments (parent_comment_id);
CREATE INDEX ix_community_comments_post_created ON community_comments (post_id, created_at);

-- AI-DO workspaces, reduced to what notices need. Members and roles are the
-- SDK ACL grants of resource kind "space" with the space ID.
CREATE TABLE board_spaces (
    id varchar(36) PRIMARY KEY,
    name varchar(120) NOT NULL,
    description text NOT NULL DEFAULT '',
    created_by text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);

-- AI-DO domains/announcements/models.py. workspace_id is the board space; a
-- company notice may have none. migrated_from_workspace_id and
-- announcement_management_grants belonged to an AI-DO data migration and are
-- not ported.
CREATE TABLE announcements (
    id varchar(36) PRIMARY KEY,
    workspace_id varchar(36) REFERENCES board_spaces (id) ON DELETE CASCADE,
    author_id text NOT NULL,
    scope varchar(24) NOT NULL DEFAULT 'workspace',
    title varchar(200) NOT NULL,
    body text NOT NULL DEFAULT '',
    is_pinned boolean NOT NULL DEFAULT false,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT ck_announcements_scope CHECK (scope IN ('workspace', 'company')),
    CONSTRAINT ck_announcements_workspace_scope CHECK (scope = 'company' OR workspace_id IS NOT NULL)
);
CREATE INDEX ix_announcements_workspace_id ON announcements (workspace_id);
CREATE INDEX ix_announcements_author_id ON announcements (author_id);
CREATE INDEX ix_announcements_scope ON announcements (scope);
CREATE INDEX ix_announcements_is_pinned ON announcements (is_pinned);
CREATE INDEX ix_announcements_workspace_pinned_created ON announcements (workspace_id, is_pinned, created_at);
CREATE INDEX ix_announcements_scope_pinned_created ON announcements (scope, is_pinned, created_at);

-- Replaces AI-DO's platform administrator for this app: board managers manage
-- channels, banned words, company notices and spaces, and see what AI-DO
-- showed platform administrators (anonymous authors, admin-only posts).
CREATE TABLE board_managers (
    user_id text PRIMARY KEY,
    granted_by text NOT NULL,
    created_at timestamp NOT NULL
);

-- AI-DO domains/ugc_policy/models.py UgcFilterTerm (banned words).
CREATE TABLE ugc_filter_terms (
    id varchar(36) PRIMARY KEY,
    term varchar(50) NOT NULL,
    normalized varchar(50) NOT NULL,
    created_by_id text,
    created_at timestamp NOT NULL,
    CONSTRAINT ck_ugc_filter_terms_term_length CHECK (char_length(term) BETWEEN 1 AND 50),
    CONSTRAINT ck_ugc_filter_terms_normalized_length CHECK (char_length(normalized) BETWEEN 1 AND 50)
);
CREATE UNIQUE INDEX uq_ugc_filter_terms_normalized ON ugc_filter_terms (normalized);

-- AI-DO domains/media/models.py MediaFile for images embedded in posts and
-- comments (`media:<id>` in the Markdown). The bytes live in the core storage
-- (platform:files): storage_key is replaced by the reservation request ID and
-- the core file ID. A file is attached in the core once, with the reference
-- `board_media:<id>`, when a post or comment first links it; it is detached
-- (bytes removed) when that post or comment is deleted. `detaching` records
-- that work with the delete until the core confirms it.
CREATE TABLE media_files (
    id varchar(36) PRIMARY KEY,
    request_id uuid NOT NULL UNIQUE,
    file_id varchar(36) UNIQUE,
    filename varchar(255) NOT NULL,
    content_type varchar(120) NOT NULL DEFAULT 'application/octet-stream',
    size_bytes integer NOT NULL DEFAULT 0,
    uploaded_by_id text NOT NULL,
    status varchar(16) NOT NULL DEFAULT 'pending',
    resource_type varchar(40),
    resource_id varchar(36),
    created_at timestamp NOT NULL,
    CONSTRAINT ck_media_files_status CHECK (status IN ('pending', 'ready', 'attached', 'detaching', 'detached'))
);
CREATE INDEX ix_media_files_uploaded_by_id ON media_files (uploaded_by_id);
CREATE INDEX ix_media_files_resource ON media_files (resource_type, resource_id);
