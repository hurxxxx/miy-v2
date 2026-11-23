-- PMS owns these associations. The linked app owns all business data and access.
CREATE TABLE pms_task_doc_refs (
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    task_id varchar(36) NOT NULL REFERENCES pms_tasks(id) ON DELETE CASCADE,
    resource_id uuid NOT NULL,
    PRIMARY KEY (task_id, resource_id)
);

CREATE TABLE pms_space_whiteboard_refs (
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    space_id varchar(36) NOT NULL REFERENCES pms_spaces(id) ON DELETE CASCADE,
    resource_id uuid NOT NULL,
    PRIMARY KEY (space_id, resource_id)
);

CREATE TABLE pms_list_whiteboard_refs (
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    list_id varchar(36) PRIMARY KEY REFERENCES pms_task_lists(id) ON DELETE CASCADE,
    resource_id uuid NOT NULL
);
