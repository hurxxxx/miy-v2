-- PMS reinforcement (COMMON-FEATURES-PLAN §2.2 "보강", docs/PORTING.md §10):
-- screens for milestones, custom fields and templates, and saved filters kept
-- on the server. Earlier migrations stay as they were applied.

-- Milestones belong to a space. A list milestone also names its list; a
-- space-wide milestone (list_id NULL) can be set on tasks of any list in the
-- space. Lists never move between spaces, so team_id is fixed.
ALTER TABLE pms_milestones
    ADD COLUMN team_id varchar(36) REFERENCES pms_spaces (id) ON DELETE CASCADE;
UPDATE pms_milestones m SET team_id = l.team_id FROM pms_task_lists l WHERE l.id = m.list_id;
ALTER TABLE pms_milestones ALTER COLUMN team_id SET NOT NULL;
ALTER TABLE pms_milestones ALTER COLUMN list_id DROP NOT NULL;
CREATE INDEX ix_pms_milestones_team_id ON pms_milestones (team_id);

-- Task templates belong to a space (AI-DO kept them per list) and hold the
-- whole task: title, description, priority, status, checklist, labels (by
-- name and color, since labels are per list) and direct subtasks.
ALTER TABLE pms_task_templates
    ADD COLUMN team_id varchar(36) REFERENCES pms_spaces (id) ON DELETE CASCADE;
UPDATE pms_task_templates t SET team_id = l.team_id FROM pms_task_lists l WHERE l.id = t.list_id;
ALTER TABLE pms_task_templates ALTER COLUMN team_id SET NOT NULL;
ALTER TABLE pms_task_templates DROP COLUMN list_id;
ALTER TABLE pms_task_templates
    ADD COLUMN title varchar(180) NOT NULL DEFAULT '',
    ADD COLUMN description_blocks json,
    ADD COLUMN labels json,
    ADD COLUMN subtasks json,
    ADD COLUMN created_by_id text,
    ADD COLUMN updated_at timestamp;
UPDATE pms_task_templates SET updated_at = created_at;
ALTER TABLE pms_task_templates ALTER COLUMN updated_at SET NOT NULL;
CREATE INDEX ix_pms_task_templates_team_id ON pms_task_templates (team_id);

-- Saved filters (AI-DO kept them in browser storage). A filter belongs to its
-- owner and to one view: a list (list_id) or a space's combined task view
-- (list_id NULL). `shared` shows it to every member of the space; only space
-- owners and admins share. Nothing is migrated from browser storage.
CREATE TABLE pms_saved_filters (
    id varchar(36) PRIMARY KEY,
    owner_id text NOT NULL,
    team_id varchar(36) NOT NULL REFERENCES pms_spaces (id) ON DELETE CASCADE,
    list_id varchar(36) REFERENCES pms_task_lists (id) ON DELETE CASCADE,
    name varchar(80) NOT NULL,
    params json NOT NULL,
    sort json,
    group_by varchar(24),
    shared boolean NOT NULL DEFAULT false,
    created_at timestamp NOT NULL,
    updated_at timestamp NOT NULL
);
CREATE INDEX ix_pms_saved_filters_owner_scope ON pms_saved_filters (owner_id, team_id, list_id);
CREATE INDEX ix_pms_saved_filters_shared_scope ON pms_saved_filters (team_id, list_id) WHERE shared;

-- Which saved filter a person opens a view with (AI-DO's "default" star).
-- `scope` is `list:<id>` or `space:<id>`.
CREATE TABLE pms_saved_filter_defaults (
    user_id text NOT NULL,
    scope varchar(80) NOT NULL,
    filter_id varchar(36) NOT NULL REFERENCES pms_saved_filters (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, scope)
);
CREATE INDEX ix_pms_saved_filter_defaults_filter ON pms_saved_filter_defaults (filter_id);
