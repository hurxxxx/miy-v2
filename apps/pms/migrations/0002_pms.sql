-- PMS (spaces, folders, task lists, tasks), ported from AI-DO 0002af062:
-- apps/api/src/ai_do_api/domains/pms/models.py, with the app-space part of
-- domains/auth/models.py (Workspace, WorkspaceUserBinding) that PMS uses.
-- Table and column names, types and constraints follow AI-DO unless noted.
--
-- Differences from AI-DO, all forced by miy's boundaries (docs/PORTING.md §3):
-- * User references (reporter_id, author_id, user_id, ...) hold the miy user
--   id (core users.id is text). Users live in the core DB: no foreign key.
-- * AI-DO PMS spaces were rows of the shared `workspaces` table (kind
--   'app_space', owner_app_id 'pms') with `workspace_user_bindings` for their
--   members. Here they are the app's own pms_spaces / pms_space_members with
--   the same columns PMS read. `team_id` keeps its AI-DO name and now points
--   at pms_spaces.
-- * pms_view_preferences is per user (AI-DO keyed it by the PMS workspace too;
--   miy has one app installation per environment instead of workspaces).
-- * pms_tasks has no retrieval_partition_id (RAG is not ported).
-- * pms_task_assignees, pms_task_followers and pms_task_labels have a
--   `position`: AI-DO kept their order in the ORM collection (the first
--   assignee is the primary one); SQL needs the order stored.
-- * pms_attachments keep the platform file id instead of an object-storage key
--   (miy small attachments, `platform:files:*`), with the reservation request
--   ids and an upload status.
-- * pms_task_user_access.granted_by_meeting_id has no foreign key (no meeting
--   app yet).
-- * AI-DO's in-app pms_notifications is replaced by the platform notification
--   inbox; pms_notification_deliveries only remembers what this app published
--   so it can retract it (miy addition).
-- * pms_task_doc_links is not ported (Docs app later).
-- Timestamps stay naive UTC (`timestamp`) and ids are UUID strings, as in AI-DO.

CREATE TABLE pms_spaces (
    id varchar(36) PRIMARY KEY,
    key varchar(48) NOT NULL UNIQUE,
    name varchar(120) NOT NULL,
    description text NOT NULL DEFAULT '',
    active boolean NOT NULL DEFAULT true,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_pms_spaces_active ON pms_spaces (active);
CREATE INDEX ix_pms_spaces_name ON pms_spaces (name);

CREATE TABLE pms_space_members (
    id varchar(36) PRIMARY KEY,
    space_id varchar(36) NOT NULL REFERENCES pms_spaces (id),
    user_id text NOT NULL,
    role varchar(24) NOT NULL DEFAULT 'member'
        CHECK (role IN ('viewer', 'member', 'admin', 'owner')),
    created_at timestamp NOT NULL,
    CONSTRAINT uq_pms_space_member UNIQUE (space_id, user_id)
);
CREATE INDEX ix_pms_space_members_user ON pms_space_members (user_id, space_id);

CREATE TABLE pms_folders (
    id varchar(36) PRIMARY KEY,
    team_id varchar(36) REFERENCES pms_spaces (id),
    name varchar(140) NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_folders_team_id ON pms_folders (team_id);

CREATE TABLE pms_view_preferences (
    user_id text PRIMARY KEY,
    task_list_group_by varchar(16) NOT NULL DEFAULT 'status',
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT ck_pms_view_preferences_task_list_group_by
        CHECK (task_list_group_by IN ('none', 'status', 'assignee', 'label'))
);

CREATE TABLE pms_space_preferences (
    team_id varchar(36) NOT NULL REFERENCES pms_spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    pinned boolean NOT NULL DEFAULT false,
    hidden boolean NOT NULL DEFAULT false,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    PRIMARY KEY (team_id, user_id),
    CONSTRAINT ck_pms_space_preferences_pinned_or_hidden CHECK (NOT (pinned AND hidden))
);
CREATE INDEX ix_pms_space_preferences_user_id ON pms_space_preferences (user_id);

CREATE TABLE pms_space_statuses (
    id varchar(36) PRIMARY KEY,
    team_id varchar(36) NOT NULL REFERENCES pms_spaces (id),
    slug varchar(40) NOT NULL,
    name varchar(60) NOT NULL,
    color varchar(24) NOT NULL DEFAULT '#6b7280',
    category varchar(24) NOT NULL DEFAULT 'active',
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    CONSTRAINT uq_pms_space_status_slug UNIQUE (team_id, slug)
);
CREATE INDEX ix_pms_space_statuses_team_id ON pms_space_statuses (team_id);

CREATE TABLE pms_task_lists (
    id varchar(36) PRIMARY KEY,
    key varchar(24) NOT NULL UNIQUE,
    name varchar(140) NOT NULL,
    description text NOT NULL DEFAULT '',
    status varchar(24) NOT NULL DEFAULT 'active',
    status_mode varchar(16) NOT NULL DEFAULT 'custom',
    archived boolean NOT NULL DEFAULT false,
    team_id varchar(36) REFERENCES pms_spaces (id),
    folder_id varchar(36) REFERENCES pms_folders (id),
    sort_order integer NOT NULL DEFAULT 0,
    created_by_id text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_pms_task_lists_name ON pms_task_lists (name);
CREATE INDEX ix_pms_task_lists_team_id ON pms_task_lists (team_id);
CREATE INDEX ix_pms_task_lists_folder_id ON pms_task_lists (folder_id);
CREATE INDEX ix_pms_task_lists_archived ON pms_task_lists (archived);

CREATE TABLE pms_task_list_statuses (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    slug varchar(40) NOT NULL,
    name varchar(60) NOT NULL,
    color varchar(24) NOT NULL DEFAULT '#6b7280',
    category varchar(24) NOT NULL DEFAULT 'active',
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    CONSTRAINT uq_pms_task_list_status_slug UNIQUE (list_id, slug)
);
CREATE INDEX ix_pms_task_list_statuses_list_id ON pms_task_list_statuses (list_id);

CREATE TABLE pms_milestones (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    title varchar(140) NOT NULL,
    description text NOT NULL DEFAULT '',
    status varchar(24) NOT NULL DEFAULT 'planned',
    start_date date,
    due_date date,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_pms_milestones_list_id ON pms_milestones (list_id);

CREATE TABLE pms_labels (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    name varchar(48) NOT NULL,
    color varchar(24) NOT NULL DEFAULT '#1f2d38',
    created_at timestamp NOT NULL,
    CONSTRAINT uq_pms_label_name UNIQUE (list_id, name)
);
CREATE INDEX ix_pms_labels_list_id ON pms_labels (list_id);

CREATE TABLE pms_tasks (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    task_number integer NOT NULL,
    title varchar(180) NOT NULL,
    description text NOT NULL DEFAULT '',
    description_blocks json,
    status varchar(24) NOT NULL DEFAULT 'todo',
    priority varchar(24) NOT NULL DEFAULT 'medium',
    assignee_id text,
    reporter_id text NOT NULL,
    parent_id varchar(36) REFERENCES pms_tasks (id) ON DELETE CASCADE,
    milestone_id varchar(36) REFERENCES pms_milestones (id) ON DELETE SET NULL,
    start_date date,
    due_date date,
    completed_date date,
    board_position integer NOT NULL DEFAULT 0,
    recurrence_rule varchar(120),
    archived boolean NOT NULL DEFAULT false,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_pms_task_number UNIQUE (list_id, task_number)
);
CREATE INDEX ix_pms_tasks_list_id ON pms_tasks (list_id);
CREATE INDEX ix_pms_tasks_title ON pms_tasks (title);
CREATE INDEX ix_pms_tasks_status ON pms_tasks (status);
CREATE INDEX ix_pms_tasks_priority ON pms_tasks (priority);
CREATE INDEX ix_pms_tasks_assignee_id ON pms_tasks (assignee_id);
CREATE INDEX ix_pms_tasks_reporter_id ON pms_tasks (reporter_id);
CREATE INDEX ix_pms_tasks_parent_id ON pms_tasks (parent_id);
CREATE INDEX ix_pms_tasks_milestone_id ON pms_tasks (milestone_id);
CREATE INDEX ix_pms_tasks_archived ON pms_tasks (archived);
CREATE INDEX ix_pms_tasks_due_date ON pms_tasks (due_date);
CREATE INDEX ix_pms_tasks_start_date ON pms_tasks (start_date);
CREATE INDEX ix_pms_tasks_list_archived_board
    ON pms_tasks (list_id, archived, board_position, task_number);
CREATE INDEX ix_pms_tasks_list_archived_updated ON pms_tasks (list_id, archived, updated_at);
CREATE INDEX ix_pms_tasks_reporter_archived_created ON pms_tasks (reporter_id, archived, created_at);

CREATE TABLE pms_task_labels (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    label_id varchar(36) NOT NULL REFERENCES pms_labels (id) ON DELETE CASCADE,
    position integer NOT NULL DEFAULT 0,
    CONSTRAINT uq_pms_task_label UNIQUE (task_id, label_id)
);
CREATE INDEX ix_pms_task_labels_task_id ON pms_task_labels (task_id);
CREATE INDEX ix_pms_task_labels_label_task ON pms_task_labels (label_id, task_id);

CREATE TABLE pms_task_comments (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    author_id text NOT NULL,
    body text NOT NULL,
    body_blocks json,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_task_comments_task_id ON pms_task_comments (task_id);
CREATE INDEX ix_pms_task_comments_author_created ON pms_task_comments (author_id, created_at);

CREATE TABLE pms_task_activity_logs (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    actor_id text,
    action varchar(40) NOT NULL,
    field_name varchar(40),
    from_value text,
    to_value text,
    message text NOT NULL,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_task_activity_logs_task_id ON pms_task_activity_logs (task_id);
CREATE INDEX ix_pms_task_activity_logs_action ON pms_task_activity_logs (action);
CREATE INDEX ix_pms_task_activity_logs_actor_created ON pms_task_activity_logs (actor_id, created_at);

CREATE TABLE pms_attachments (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    filename varchar(255) NOT NULL,
    content_type varchar(120) NOT NULL DEFAULT 'application/octet-stream',
    size_bytes integer NOT NULL DEFAULT 0,
    file_id uuid UNIQUE,
    request_id uuid NOT NULL UNIQUE,
    attach_request_id uuid NOT NULL UNIQUE,
    status varchar(16) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'attached')),
    uploaded_by_id text NOT NULL,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_attachments_task_id ON pms_attachments (task_id);
CREATE INDEX ix_pms_attachments_uploaded_created ON pms_attachments (uploaded_by_id, created_at);

-- Platform files whose app reference is gone (attachment or task deleted) and
-- that still have to be detached from the core (miy addition).
CREATE TABLE pms_attachment_releases (
    file_id uuid PRIMARY KEY,
    reference text NOT NULL,
    created_at timestamp NOT NULL
);

CREATE TABLE pms_checklist_items (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    text varchar(500) NOT NULL,
    completed boolean NOT NULL DEFAULT false,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_checklist_items_task_id ON pms_checklist_items (task_id);

CREATE TABLE pms_task_templates (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    name varchar(140) NOT NULL,
    description text NOT NULL DEFAULT '',
    default_status varchar(40) NOT NULL DEFAULT 'todo',
    default_priority varchar(24) NOT NULL DEFAULT 'medium',
    checklist_items json,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_task_templates_list_id ON pms_task_templates (list_id);

CREATE TABLE pms_custom_fields (
    id varchar(36) PRIMARY KEY,
    list_id varchar(36) NOT NULL REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    name varchar(100) NOT NULL,
    field_type varchar(24) NOT NULL,
    options json,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamp NOT NULL
);
CREATE INDEX ix_pms_custom_fields_list_id ON pms_custom_fields (list_id);

CREATE TABLE pms_custom_field_values (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    field_id varchar(36) NOT NULL REFERENCES pms_custom_fields (id) ON DELETE CASCADE,
    value text NOT NULL DEFAULT '',
    CONSTRAINT uq_pms_cf_value UNIQUE (task_id, field_id)
);
CREATE INDEX ix_pms_custom_field_values_task_id ON pms_custom_field_values (task_id);
CREATE INDEX ix_pms_custom_field_values_field_id ON pms_custom_field_values (field_id);

CREATE TABLE pms_task_assignees (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    position integer NOT NULL DEFAULT 0,
    CONSTRAINT uq_pms_task_assignee UNIQUE (task_id, user_id)
);
CREATE INDEX ix_pms_task_assignees_task_id ON pms_task_assignees (task_id);
CREATE INDEX ix_pms_task_assignees_user_task ON pms_task_assignees (user_id, task_id);

CREATE TABLE pms_task_followers (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    position integer NOT NULL DEFAULT 0,
    CONSTRAINT uq_pms_task_follower UNIQUE (task_id, user_id)
);
CREATE INDEX ix_pms_task_followers_task_id ON pms_task_followers (task_id);
CREATE INDEX ix_pms_task_followers_user_id ON pms_task_followers (user_id);

CREATE TABLE pms_task_user_access (
    id varchar(36) PRIMARY KEY,
    task_id varchar(36) NOT NULL REFERENCES pms_tasks (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    access_level varchar(16) NOT NULL DEFAULT 'read',
    granted_by_meeting_id varchar(36),
    granted_by_user_id text NOT NULL,
    reason varchar(24) NOT NULL DEFAULT 'meeting_attendee',
    expires_at timestamp,
    revoked_at timestamp,
    revoked_by_user_id text,
    revoke_reason varchar(24),
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_pms_task_user_access_task_id ON pms_task_user_access (task_id);
CREATE INDEX ix_pms_task_user_access_user_revoked ON pms_task_user_access (user_id, revoked_at);
CREATE INDEX ix_pms_task_user_access_meeting_revoked
    ON pms_task_user_access (granted_by_meeting_id, revoked_at);
CREATE INDEX ix_pms_task_user_access_expires_active
    ON pms_task_user_access (expires_at) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX uq_pms_task_user_access_active
    ON pms_task_user_access (task_id, user_id, granted_by_meeting_id) WHERE revoked_at IS NULL;

CREATE TABLE pms_notification_deliveries (
    id varchar(36) PRIMARY KEY,
    request_id uuid NOT NULL,
    task_id varchar(36) NOT NULL,
    user_id text NOT NULL,
    kind varchar(24) NOT NULL,
    group_key text NOT NULL,
    created_at timestamp NOT NULL,
    read_at timestamp
);
CREATE INDEX ix_pms_notification_deliveries_task ON pms_notification_deliveries (task_id);
CREATE INDEX ix_pms_notification_deliveries_user ON pms_notification_deliveries (user_id, task_id);
