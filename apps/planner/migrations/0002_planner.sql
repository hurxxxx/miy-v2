-- Planner events and personal widgets (to-dos, memo), ported from AI-DO
-- 0002af062: apps/api/src/ai_do_api/domains/planner/models.py and
-- domains/personal_widgets/models.py (their Alembic revisions b4c1a2d9e6f0,
-- c1d2e3f4a5c8, c2e4f6a8b0d3, c3d4e5f6a7b8, d4e5f6a7b9c0 folded into one).
--
-- Differences from AI-DO, all forced by miy's boundaries (docs/PORTING.md):
-- * owner_id / user_id hold the miy user id (core users.id is text). The
--   users table lives in the core DB, so there is no foreign key.
-- * planner_events has no workspace_id, visibility or retrieval_partition_id:
--   events are personal per user and the RAG partitions are not ported.
-- Timestamps stay naive UTC (`timestamp`) as in AI-DO.

CREATE TABLE planner_events (
    id varchar(36) PRIMARY KEY,
    owner_id text NOT NULL,
    title varchar(200) NOT NULL,
    description text NOT NULL,
    location varchar(240) NOT NULL,
    time_zone varchar(64) NOT NULL DEFAULT 'Asia/Seoul',
    all_day boolean NOT NULL,
    start_has_time boolean NOT NULL,
    end_has_time boolean NOT NULL,
    start_at timestamp NOT NULL,
    end_at timestamp NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_planner_events_owner_id ON planner_events (owner_id);
CREATE INDEX ix_planner_events_owner_start ON planner_events (owner_id, start_at);
CREATE INDEX ix_planner_events_owner_end ON planner_events (owner_id, end_at);

CREATE TABLE personal_todo_items (
    id varchar(36) PRIMARY KEY,
    user_id text NOT NULL,
    title varchar(240) NOT NULL,
    completed boolean NOT NULL,
    sort_order integer NOT NULL,
    completed_at timestamp NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_personal_todo_items_user_id ON personal_todo_items (user_id);
CREATE INDEX ix_personal_todo_items_completed ON personal_todo_items (completed);
CREATE INDEX ix_personal_todo_items_user_completed_sort
    ON personal_todo_items (user_id, completed, sort_order);

CREATE TABLE personal_memos (
    id varchar(36) PRIMARY KEY,
    user_id text NOT NULL,
    body text NOT NULL,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL,
    CONSTRAINT uq_personal_memos_user UNIQUE (user_id)
);
CREATE INDEX ix_personal_memos_user_id ON personal_memos (user_id);
