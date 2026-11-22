-- AI-DO 0002af062 domains/whiteboard/models.py, ported (docs/PORTING.md §3.1).
-- User IDs are miy user IDs (the people live in the core, so no foreign key).
-- Times are naive UTC like AI-DO's utcnow_naive. AI-DO workspaces become the
-- app's own spaces: the reserved ID 'global' (every person who may use the
-- app, AI-DO's app_global space) or a row of whiteboard_spaces (AI-DO
-- app_space, members in the SDK resource ACL, kind 'space').

CREATE TABLE whiteboard_spaces (
    id varchar(36) PRIMARY KEY,
    name varchar(120) NOT NULL,
    description text NOT NULL DEFAULT '',
    created_by text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);

CREATE TABLE whiteboards (
    id varchar(36) PRIMARY KEY,
    workspace_id varchar(36) NOT NULL,
    owner_id text NOT NULL,
    title varchar(200) NOT NULL,
    source_app varchar(64) NOT NULL DEFAULT 'whiteboard',
    source_kind varchar(64) NOT NULL DEFAULT 'manual',
    source_ref varchar(128),
    generation_kind varchar(32) NOT NULL DEFAULT 'human',
    scene jsonb NOT NULL DEFAULT '{"elements": [], "appState": {}, "files": {}}'::jsonb,
    -- miy: single-editor saving with optimistic concurrency replaces AI-DO's
    -- Yjs collaboration. Every stored scene change adds one.
    revision bigint NOT NULL DEFAULT 1,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    trashed_at timestamp,
    CONSTRAINT ck_whiteboards_workspace CHECK (workspace_id <> '')
);
CREATE INDEX ix_whiteboards_owner_created ON whiteboards (owner_id, created_at);
CREATE INDEX ix_whiteboards_workspace_updated ON whiteboards (workspace_id, updated_at);
CREATE INDEX ix_whiteboards_trashed_at ON whiteboards (trashed_at);

CREATE TABLE whiteboard_targets (
    id varchar(36) PRIMARY KEY,
    whiteboard_id varchar(36) NOT NULL REFERENCES whiteboards (id) ON DELETE CASCADE,
    target_app varchar(64) NOT NULL,
    target_type varchar(64) NOT NULL,
    target_id varchar(128) NOT NULL,
    is_primary boolean NOT NULL DEFAULT true,
    sort_order integer NOT NULL DEFAULT 0,
    created_by_id text,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_whiteboard_targets_board_target UNIQUE (whiteboard_id, target_app, target_type, target_id)
);
CREATE INDEX ix_whiteboard_targets_target_lookup ON whiteboard_targets (target_app, target_type, target_id);
CREATE UNIQUE INDEX uq_whiteboard_targets_primary ON whiteboard_targets (whiteboard_id) WHERE is_primary IS TRUE;

CREATE TABLE whiteboard_user_shares (
    id varchar(36) PRIMARY KEY,
    whiteboard_id varchar(36) NOT NULL REFERENCES whiteboards (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    access_level varchar(16) NOT NULL DEFAULT 'read',
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    CONSTRAINT uq_whiteboard_user_share UNIQUE (whiteboard_id, user_id),
    CONSTRAINT ck_whiteboard_user_share_level CHECK (access_level IN ('read', 'edit'))
);
CREATE INDEX ix_whiteboard_user_shares_user ON whiteboard_user_shares (user_id);

CREATE TABLE whiteboard_user_item_prefs (
    id varchar(36) PRIMARY KEY,
    user_id text NOT NULL,
    whiteboard_id varchar(36) NOT NULL REFERENCES whiteboards (id) ON DELETE CASCADE,
    is_favorite boolean NOT NULL DEFAULT false,
    last_viewed_at timestamp,
    CONSTRAINT uq_whiteboard_user_item_pref UNIQUE (user_id, whiteboard_id)
);

-- miy (보강): images of a board live in the core storage, not inline in the
-- scene (AI-DO kept Excalidraw data URLs in scene.files). The scene's file
-- entry names the row (`storage`). The row stays after its board is deleted
-- until the core confirms the bytes are gone (status 'detaching' → 'detached').
CREATE TABLE whiteboard_files (
    id varchar(36) PRIMARY KEY,
    whiteboard_id varchar(36) REFERENCES whiteboards (id) ON DELETE SET NULL,
    excalidraw_file_id varchar(128) NOT NULL,
    request_id uuid NOT NULL UNIQUE,
    file_id text,
    filename varchar(255) NOT NULL,
    content_type varchar(64) NOT NULL,
    size_bytes integer NOT NULL,
    uploaded_by_id text NOT NULL,
    status varchar(16) NOT NULL DEFAULT 'pending',
    created_at timestamp NOT NULL,
    CONSTRAINT ck_whiteboard_files_status CHECK (status IN ('pending', 'ready', 'attached', 'detaching', 'detached'))
);
CREATE INDEX ix_whiteboard_files_board ON whiteboard_files (whiteboard_id);
CREATE INDEX ix_whiteboard_files_detaching ON whiteboard_files (status) WHERE status = 'detaching';
