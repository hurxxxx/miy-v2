-- Messenger (메신저): AI-DO DM tables at 0002af062 (domains/dm/models.py and
-- domains/ugc_policy/models.py), ported as in docs/PORTING.md §3.1.
--
-- User references are miy user IDs (text, no foreign key: users live in the
-- platform). Timestamps are naive UTC like AI-DO's utcnow_naive. Additions
-- for miy are marked "miy:".

CREATE TABLE dm_conversations (
    id varchar(36) PRIMARY KEY,
    conversation_type varchar(16) NOT NULL,
    -- miy: text (AI-DO varchar(96)); miy user IDs may be longer than 36.
    direct_key text,
    title varchar(140),
    created_by_id text NOT NULL,
    message_seq integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_dm_conversations_direct_key UNIQUE (direct_key)
);
CREATE INDEX ix_dm_conversations_type ON dm_conversations (conversation_type);
CREATE INDEX ix_dm_conversations_updated_at ON dm_conversations (updated_at);
CREATE INDEX ix_dm_conversations_created_by_id ON dm_conversations (created_by_id);

CREATE TABLE dm_messages (
    id varchar(36) PRIMARY KEY,
    conversation_id varchar(36) NOT NULL REFERENCES dm_conversations (id) ON DELETE CASCADE,
    sequence integer NOT NULL,
    sender_id text NOT NULL,
    reply_to_message_id varchar(36) REFERENCES dm_messages (id) ON DELETE SET NULL,
    body text NOT NULL,
    created_at timestamp NOT NULL,
    moderation_hidden_at timestamp,
    moderation_hidden_by_id text,
    -- miy: author edit and delete (COMMON-FEATURES-PLAN §2.2).
    edited_at timestamp,
    deleted_at timestamp,
    CONSTRAINT uq_dm_messages_conversation_sequence UNIQUE (conversation_id, sequence)
);
CREATE INDEX ix_dm_messages_conversation_id ON dm_messages (conversation_id);
CREATE INDEX ix_dm_messages_sender_id ON dm_messages (sender_id);
CREATE INDEX ix_dm_messages_reply_to_message_id ON dm_messages (reply_to_message_id);
CREATE INDEX ix_dm_messages_conversation_created ON dm_messages (conversation_id, created_at);
CREATE INDEX ix_dm_messages_conversation_sequence ON dm_messages (conversation_id, sequence);
CREATE INDEX ix_dm_messages_conversation_created_sequence
    ON dm_messages (conversation_id, created_at, sequence);

CREATE TABLE dm_conversation_participants (
    id varchar(36) PRIMARY KEY,
    conversation_id varchar(36) NOT NULL REFERENCES dm_conversations (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    role varchar(16) NOT NULL DEFAULT 'member',
    joined_at timestamp NOT NULL,
    left_at timestamp,
    muted_at timestamp,
    last_read_message_id varchar(36) REFERENCES dm_messages (id),
    created_at timestamp NOT NULL
);
CREATE UNIQUE INDEX uq_dm_conversation_participants_active_user
    ON dm_conversation_participants (conversation_id, user_id) WHERE left_at IS NULL;
CREATE INDEX ix_dm_conversation_participants_user_active
    ON dm_conversation_participants (user_id, left_at, conversation_id);
CREATE INDEX ix_dm_conversation_participants_conversation_id
    ON dm_conversation_participants (conversation_id);
CREATE INDEX ix_dm_conversation_participants_user_id ON dm_conversation_participants (user_id);
CREATE INDEX ix_dm_conversation_participants_left_at ON dm_conversation_participants (left_at);

-- AI-DO stored bytes in its own object storage (storage_key). miy keeps them
-- in the platform Storage API: the reservation request ID, the file ID and
-- the app-side storage state replace storage_key.
CREATE TABLE dm_message_attachments (
    id varchar(36) PRIMARY KEY,
    conversation_id varchar(36) NOT NULL REFERENCES dm_conversations (id) ON DELETE CASCADE,
    message_id varchar(36) REFERENCES dm_messages (id) ON DELETE SET NULL,
    uploader_id text NOT NULL,
    filename varchar(512) NOT NULL,
    content_type varchar(160) NOT NULL DEFAULT 'application/octet-stream',
    size_bytes integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    -- miy: platform Storage.
    storage_request_id uuid NOT NULL,
    storage_file_id uuid,
    storage_status varchar(16) NOT NULL DEFAULT 'reserved',
    attach_request_id uuid,
    released_at timestamp,
    CONSTRAINT uq_dm_message_attachments_storage_request UNIQUE (uploader_id, storage_request_id),
    CONSTRAINT uq_dm_message_attachments_storage_file UNIQUE (storage_file_id),
    CONSTRAINT ck_dm_message_attachments_storage_status
        CHECK (storage_status IN ('reserved', 'uploaded', 'attached', 'released', 'cancelled'))
);
CREATE INDEX ix_dm_message_attachments_conversation ON dm_message_attachments (conversation_id, created_at);
CREATE INDEX ix_dm_message_attachments_message ON dm_message_attachments (message_id);
CREATE INDEX ix_dm_message_attachments_uploader ON dm_message_attachments (uploader_id, created_at);

CREATE TABLE dm_user_blocks (
    blocker_user_id text NOT NULL,
    blocked_user_id text NOT NULL,
    source varchar(16) NOT NULL,
    created_at timestamp NOT NULL,
    PRIMARY KEY (blocker_user_id, blocked_user_id),
    CONSTRAINT ck_dm_user_blocks_not_self CHECK (blocker_user_id <> blocked_user_id),
    CONSTRAINT ck_dm_user_blocks_source CHECK (source IN ('web', 'commute_app'))
);
CREATE INDEX ix_dm_user_blocks_blocked ON dm_user_blocks (blocked_user_id, blocker_user_id);

CREATE TABLE dm_abuse_reports (
    id varchar(36) PRIMARY KEY,
    reporter_user_id text NOT NULL,
    reported_user_id text NOT NULL,
    conversation_id varchar(36) NOT NULL REFERENCES dm_conversations (id),
    message_id varchar(36) REFERENCES dm_messages (id),
    category varchar(32) NOT NULL,
    note text,
    source varchar(16) NOT NULL,
    client_report_id varchar(36) NOT NULL,
    request_hash varchar(64) NOT NULL,
    open_dedupe_key varchar(160),
    status varchar(16) NOT NULL DEFAULT 'open',
    action varchar(32),
    resolution_note text,
    resolved_by_id text,
    resolved_at timestamp,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_dm_abuse_reports_reporter_client UNIQUE (reporter_user_id, client_report_id),
    CONSTRAINT uq_dm_abuse_reports_reporter_open_target UNIQUE (reporter_user_id, open_dedupe_key),
    CONSTRAINT ck_dm_abuse_reports_note_length CHECK (note IS NULL OR char_length(note) <= 1000),
    CONSTRAINT ck_dm_abuse_reports_resolution_note_length
        CHECK (resolution_note IS NULL OR char_length(resolution_note) <= 1000),
    CONSTRAINT ck_dm_abuse_reports_source CHECK (source IN ('web', 'commute_app')),
    CONSTRAINT ck_dm_abuse_reports_status CHECK (status IN ('open', 'actioned', 'dismissed')),
    CONSTRAINT ck_dm_abuse_reports_action CHECK (action IS NULL OR action IN ('none', 'hide_message')),
    CONSTRAINT ck_dm_abuse_reports_resolution_state CHECK (
        (status = 'open' AND open_dedupe_key IS NOT NULL AND resolved_at IS NULL AND action IS NULL)
        OR (status <> 'open' AND open_dedupe_key IS NULL AND resolved_at IS NOT NULL AND action IS NOT NULL)
    )
);
CREATE INDEX ix_dm_abuse_reports_status_created ON dm_abuse_reports (status, created_at);
CREATE INDEX ix_dm_abuse_reports_reported_created ON dm_abuse_reports (reported_user_id, created_at);
CREATE INDEX ix_dm_abuse_reports_reporter_created ON dm_abuse_reports (reporter_user_id, created_at);
CREATE INDEX ix_dm_abuse_reports_conversation ON dm_abuse_reports (conversation_id);
CREATE INDEX ix_dm_abuse_reports_message ON dm_abuse_reports (message_id);

CREATE TABLE dm_abuse_report_requests (
    reporter_user_id text NOT NULL,
    client_report_id varchar(36) NOT NULL,
    report_id varchar(36) NOT NULL REFERENCES dm_abuse_reports (id) ON DELETE CASCADE,
    request_hash varchar(64) NOT NULL,
    created_at timestamp NOT NULL,
    PRIMARY KEY (reporter_user_id, client_report_id)
);

CREATE TABLE ugc_terms_acceptances (
    user_id text NOT NULL,
    terms_version varchar(32) NOT NULL,
    accepted_at timestamp NOT NULL,
    source varchar(16) NOT NULL,
    PRIMARY KEY (user_id, terms_version),
    CONSTRAINT ck_ugc_terms_acceptances_source CHECK (source IN ('web', 'commute_app'))
);

CREATE TABLE ugc_filter_terms (
    id varchar(36) PRIMARY KEY,
    term varchar(50) NOT NULL,
    normalized varchar(50) NOT NULL UNIQUE,
    created_by_id text,
    created_at timestamp NOT NULL,
    CONSTRAINT ck_ugc_filter_terms_term_length CHECK (char_length(term) BETWEEN 1 AND 50),
    CONSTRAINT ck_ugc_filter_terms_normalized_length CHECK (char_length(normalized) BETWEEN 1 AND 50)
);

-- miy: AI-DO read the terms version, switch, document URL and contact from
-- its server settings (AI_DO_UGC_*). An app has no such settings, so one row
-- holds them with AI-DO's defaults; messenger managers change it in the app.
CREATE TABLE dm_terms_settings (
    id smallint PRIMARY KEY CHECK (id = 1),
    version varchar(32) NOT NULL CHECK (char_length(btrim(version)) BETWEEN 1 AND 32),
    required boolean NOT NULL DEFAULT false,
    document_url text,
    contact_email text,
    contact_url text,
    updated_by_id text,
    updated_at timestamp NOT NULL
);
INSERT INTO dm_terms_settings (id, version, required, updated_at)
VALUES (1, '2026-11-ugc-v1', false, (now() AT TIME ZONE 'UTC'));

-- miy: AI-DO let its platform administrators review reports. Messenger
-- declares its own manager role, granted inside the app (docs/PORTING.md §4).
CREATE TABLE dm_safety_managers (
    user_id text PRIMARY KEY,
    granted_by_id text,
    created_at timestamp NOT NULL
);

-- miy: AI-DO wrote these actions to its platform audit log (record_audit_log).
-- Business content stays in the app; payloads carry no message body, note or
-- file name, as in AI-DO.
CREATE TABLE dm_audit_logs (
    id bigserial PRIMARY KEY,
    action varchar(64) NOT NULL,
    entity_kind varchar(32) NOT NULL,
    entity_id text,
    summary text NOT NULL,
    actor_user_id text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_dm_audit_logs_created ON dm_audit_logs (created_at);
CREATE INDEX ix_dm_audit_logs_entity ON dm_audit_logs (entity_kind, entity_id);

-- miy: edit and delete history (COMMON-FEATURES-PLAN §2.2 "수정·삭제 이력은
-- 메신저 앱 DB에 남긴다"). body_before keeps the text an edit replaced, and
-- for a delete the body copy only while a report on the message is open.
CREATE TABLE dm_message_revisions (
    id bigserial PRIMARY KEY,
    message_id varchar(36) NOT NULL REFERENCES dm_messages (id) ON DELETE CASCADE,
    action varchar(16) NOT NULL CHECK (action IN ('edit', 'delete')),
    body_before text,
    actor_user_id text NOT NULL,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_dm_message_revisions_message ON dm_message_revisions (message_id, id);

-- miy: display names from the platform directory, remembered so that people
-- who left the directory keep a name in old conversations and so that every
-- poll does not call the directory. Never an access input.
CREATE TABLE dm_user_directory_cache (
    user_id text PRIMARY KEY,
    name text NOT NULL,
    primary_group_name text,
    active boolean NOT NULL,
    refreshed_at timestamp NOT NULL
);
