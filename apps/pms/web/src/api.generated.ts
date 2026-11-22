// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/files/pms/attachments/{attachment_id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * Step 3: `GET /files/pms/attachments/{id}/content`.
         * @description Rechecks task read access before streaming the stored attachment through the authenticated frame file gateway. The response uses the stored media type. Attachments are up to 10 MiB.
         */
        get: operations["downloadPmsTaskAttachment"];
        /**
         * Step 2: `PUT /files/pms/attachments/{id}/content` (bytes via `fileApi`).
         * @description Send the exact reserved file bytes, up to 10 MiB, using the authenticated frame file gateway. Only the uploader may do this while they can edit the task's list.
         */
        put: operations["uploadPmsTaskAttachment"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/platform/notification-check": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `platformEndpoints.notificationCheck`: which group keys still point at a
         *     task this person may read. Read only; platform calls only.
         * @description Platform-only, read-only callback. Returns notification groups for tasks the verified caller can still read.
         */
        post: operations["checkPmsNotifications"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/home-card": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/home-card`.
         * @description Platform-only, read-only home card for open tasks due today or overdue in the caller’s time zone.
         */
        post: operations["getPmsHomeCard"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/personal-widgets/pms/tasks/assigned": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/personal-widgets/pms/tasks/assigned`: the floating PMS widget's
         *     list across every space, each task with its space.
         */
        get: operations["listPmsPersonalWidgetAssignedTasks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/attachments/{attachment_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/attachments/{attachment_id}`: also cancels a pending upload. */
        delete: operations["deletePmsTaskAttachment"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/checklist/{item_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/checklist/{item_id}`. */
        delete: operations["deletePmsChecklistItem"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/checklist/{item_id}`. */
        patch: operations["updatePmsChecklistItem"];
        trace?: never;
    };
    "/v1/pms/custom-fields/{field_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/custom-fields/{field_id}`: its values go with it. */
        delete: operations["deletePmsCustomField"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/custom-fields/{field_id}` (miy): rename, or replace the
         *     options of a select field. `option_renames` (`[{from, to}]`) carries values
         *     over to a renamed option; values of removed options are cleared. An option
         *     edit must supply the unchanged options it was based on.
         */
        patch: operations["updatePmsCustomField"];
        trace?: never;
    };
    "/v1/pms/dashboard/summary": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/dashboard/summary`: AI-DO dashboard_summary.py over the
         *     caller's active (unarchived) lists.
         */
        get: operations["getPmsDashboardSummary"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/folders": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/folders`. */
        get: operations["listPmsFolders"];
        put?: never;
        /** `POST /v1/pms/folders`: also creates a default list "List" in the folder. */
        post: operations["createPmsFolder"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/folders/{folder_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/folders/{folder_id}`: its lists stay, outside any folder. */
        delete: operations["deletePmsFolder"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/folders/{folder_id}`. */
        patch: operations["updatePmsFolder"];
        trace?: never;
    };
    "/v1/pms/labels/{label_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/labels/{label_id}`. */
        delete: operations["deletePmsLabel"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/labels/{label_id}`. */
        patch: operations["updatePmsLabel"];
        trace?: never;
    };
    "/v1/pms/lists": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listPmsTaskLists"];
        put?: never;
        /** `POST /v1/pms/lists`: a new list inherits its space's workflow. */
        post: operations["createPmsTaskList"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/lists/{list_id}`. */
        get: operations["getPmsTaskList"];
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/lists/{list_id}`: only an archived list, with its tasks. */
        delete: operations["deletePmsTaskList"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/lists/{list_id}`: folder/sort order alone is an editor's
         *     reorder; anything else needs an owner or admin, and an archived list only
         *     accepts (un)archiving.
         */
        patch: operations["updatePmsTaskList"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/custom-fields": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/lists/{list_id}/custom-fields`. */
        get: operations["listPmsCustomFields"];
        put?: never;
        /**
         * `POST /v1/pms/lists/{list_id}/custom-fields`. miy: names are unique in a
         *     list (copying matches fields by name and type), select fields need options,
         *     and a field without an explicit order goes last.
         */
        post: operations["createPmsCustomField"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/custom-fields/reorder": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/lists/{list_id}/custom-fields/reorder` (miy): `field_ids`
         *     in the new order. Unknown ids are ignored; fields left out keep their
         *     relative order after the given ones.
         */
        patch: operations["reorderPmsCustomFields"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/export": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/lists/{list_id}/export`.
         *     The same task filters as the list apply, including cf_<field ID> keys.
         */
        get: operations["exportPmsTaskList"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/labels": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/lists/{list_id}/labels`. */
        get: operations["listPmsLabels"];
        put?: never;
        /** `POST /v1/pms/lists/{list_id}/labels`. */
        post: operations["createPmsLabel"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/milestones": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/lists/{list_id}/milestones`: the list's milestones and the
         *     space-wide milestones of its space (a task of the list may carry either).
         */
        get: operations["listPmsListMilestones"];
        put?: never;
        /** `POST /v1/pms/lists/{list_id}/milestones`: a list milestone. */
        post: operations["createPmsListMilestone"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/status-mode": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/lists/{list_id}/status-mode`.
         * @description Switches between the space workflow and a custom list workflow. Inheritance requires every in-use status to exist in the space workflow.
         */
        patch: operations["updatePmsTaskListStatusMode"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/statuses": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/lists/{list_id}/statuses`. */
        get: operations["listPmsTaskListStatuses"];
        put?: never;
        /**
         * `POST /v1/pms/lists/{list_id}/statuses`: switches an inheriting list to
         *     its own copy of the space workflow first.
         * @description Copies the current space workflow first when the list inherits it.
         */
        post: operations["createPmsTaskListStatus"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/tasks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/lists/{list_id}/tasks`.
         * @description Filter by fields with repeated status values and cf_<field ID>=value keys for custom fields.
         */
        get: operations["listPmsTasks"];
        put?: never;
        /**
         * `POST /v1/pms/lists/{list_id}/tasks`. miy: with `template_id` (a template
         *     of the list's space) the task also gets the template's checklist, labels and
         *     subtasks; the other fields come from the request as usual.
         */
        post: operations["createPmsTask"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/tasks/bulk": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/lists/{list_id}/tasks/bulk`. */
        patch: operations["bulkUpdatePmsTasks"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/tasks/copy": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/pms/lists/{list_id}/tasks/copy`. */
        post: operations["copyPmsTasks"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/tasks/reorder": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/lists/{list_id}/tasks/reorder`. */
        patch: operations["reorderPmsTasks"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/tasks/reorder-by-title": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/lists/{list_id}/tasks/reorder-by-title`: saves name order
         *     as the custom order (parents before their subtasks); `updated_at` kept.
         */
        patch: operations["reorderPmsTasksByTitle"];
        trace?: never;
    };
    "/v1/pms/lists/{list_id}/templates": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/lists/{list_id}/templates`: the templates of the list's space,
         *     newest first (the new-task dialog knows the list).
         */
        get: operations["listPmsListTemplates"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/me` (miy addition): AI-DO's frontend read the signed-in user
         *     from its auth context; the frame gets no user from the platform.
         */
        get: operations["getPmsMe"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/milestones/{milestone_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * `DELETE /v1/pms/milestones/{milestone_id}` (miy; AI-DO had no delete). Its
         *     tasks keep everything else and lose the milestone (the foreign key sets it
         *     to NULL); each gets an activity entry. The task rows are locked first, in
         *     id order, so a task write waiting on its row cannot deadlock with the
         *     cascade.
         */
        delete: operations["deletePmsMilestone"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/milestones/{milestone_id}`: AI-DO applies only the fields
         *     sent with a value (null leaves title, description, status and order as they
         *     are). miy: an explicit `null` start or due date clears it, so a date set by
         *     mistake can be removed from the screen.
         */
        patch: operations["updatePmsMilestone"];
        trace?: never;
    };
    "/v1/pms/saved-filters": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/saved-filters?list_id=` or `?space_id=`: `{items,
         *     default_filter_id}`, by name.
         */
        get: operations["listPmsSavedFilters"];
        put?: never;
        /**
         * `POST /v1/pms/saved-filters`: `{list_id | space_id, name, params, sort?,
         *     group_by?, shared?}` (201). Only a space owner or admin may share.
         */
        post: operations["createPmsSavedFilter"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/saved-filters/default": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT /v1/pms/saved-filters/default`: `{list_id | space_id, filter_id}` sets
         *     the filter the caller opens this view with (`null` clears it). The filter
         *     must be one the caller sees in that view. Answers the view's list.
         */
        put: operations["setPmsDefaultSavedFilter"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/saved-filters/{filter_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * `DELETE /v1/pms/saved-filters/{filter_id}`: same managers as an update.
         *     Anyone's "open with" choice of it goes with it.
         */
        delete: operations["deletePmsSavedFilter"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/saved-filters/{filter_id}`: rename or replace the content
         *     (`name`, `params`, `sort`, `group_by`), or share and unshare (`shared`, space
         *     owner or admin only). The owner manages a filter; space owners and admins
         *     also manage shared ones. A filter the caller cannot see is not found.
         */
        patch: operations["updatePmsSavedFilter"];
        trace?: never;
    };
    "/v1/pms/space-preferences": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/space-preferences`: the caller's pinned/hidden spaces among
         *     those they can still open.
         */
        get: operations["listPmsSpacePreferences"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/space-statuses/{status_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/space-statuses/{status_id}`. */
        delete: operations["deletePmsSpaceStatus"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/space-statuses/{status_id}`. */
        patch: operations["updatePmsSpaceStatus"];
        trace?: never;
    };
    "/v1/pms/spaces": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/spaces`: active spaces the caller is a member of, by name. */
        get: operations["listPmsSpaces"];
        put?: never;
        /**
         * `POST /v1/pms/spaces`: the creator becomes the only owner; the default
         *     statuses are seeded with the space (AI-DO `_initialize_pms_space`).
         */
        post: operations["createPmsSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * `DELETE /v1/pms/spaces/{space_id}`: archives the space (AI-DO
         *     `archive_app_space` sets it inactive; the data stays).
         * @description Archives the space and retracts its notifications. Archived space data is retained.
         */
        delete: operations["deletePmsSpace"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/spaces/{space_id}`: owners and admins. */
        patch: operations["updatePmsSpace"];
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/lists": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listPmsSpaceLists"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/lists/reorder": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/spaces/{space_id}/lists/reorder`. */
        patch: operations["reorderPmsSpaceLists"];
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/members": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/spaces/{space_id}/members`: owners and admins first, then by name.
         * @description Lists space managers first and sorts each group by display name.
         */
        get: operations["listPmsSpaceMembers"];
        put?: never;
        /** `POST /v1/pms/spaces/{space_id}/members`. */
        post: operations["addPmsSpaceMember"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/members/bulk": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/pms/spaces/{space_id}/members/bulk`: members already in the space
         *     are skipped, not rejected, so a department can be added again.
         * @description Skips users who are already space members and reports the number skipped.
         */
        post: operations["addPmsSpaceMembers"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/members/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * `DELETE /v1/pms/spaces/{space_id}/members/{user_id}`: managers remove
         *     others; anyone may leave; the last owner must remain. The removed person's
         *     notifications for this space are retracted.
         * @description Removes a space member or lets the caller leave. The last owner must remain.
         */
        delete: operations["removePmsSpaceMember"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/spaces/{space_id}/members/{user_id}`. */
        patch: operations["updatePmsSpaceMember"];
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/milestones": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/spaces/{space_id}/milestones` (miy): every milestone of the
         *     space, space-wide ones and those of its lists.
         */
        get: operations["listPmsSpaceMilestones"];
        put?: never;
        /** `POST /v1/pms/spaces/{space_id}/milestones` (miy): a space-wide milestone. */
        post: operations["createPmsSpaceMilestone"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/preference": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/spaces/{space_id}/preference`: pinning unhides, hiding unpins.
         * @description Pinning unhides the space and hiding unpins it. Both cannot be true.
         */
        patch: operations["updatePmsSpacePreference"];
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/statuses": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/spaces/{space_id}/statuses`. */
        get: operations["listPmsSpaceStatuses"];
        put?: never;
        /** `POST /v1/pms/spaces/{space_id}/statuses`. */
        post: operations["createPmsSpaceStatus"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/spaces/{space_id}/templates": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/spaces/{space_id}/templates`. */
        get: operations["listPmsSpaceTemplates"];
        put?: never;
        /** `POST /v1/pms/spaces/{space_id}/templates`: a template written by hand. */
        post: operations["createPmsTemplate"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/task-list-statuses/{status_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/task-list-statuses/{status_id}`. */
        delete: operations["deletePmsTaskListStatus"];
        options?: never;
        head?: never;
        /** `PATCH /v1/pms/task-list-statuses/{status_id}`. */
        patch: operations["updatePmsTaskListStatus"];
        trace?: never;
    };
    "/v1/pms/tasks/assigned": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/tasks/assigned`. */
        get: operations["listPmsAssignedTasks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/today-overdue": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/tasks/today-overdue?today=YYYY-MM-DD`. */
        get: operations["listPmsTodayOverdueTasks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/tasks/{task_id}`: the caller's notification line for this
         *     task is marked read (miy).
         */
        get: operations["getPmsTask"];
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/tasks/{task_id}`. */
        delete: operations["deletePmsTask"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/tasks/{task_id}`.
         * @description Omitted fields retain their current values. Null clears nullable values such as assignees and due dates.
         */
        patch: operations["updatePmsTask"];
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/activity-logs": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/tasks/{task_id}/activity-logs`: newest first, without the
         *     noisy description_blocks entries.
         */
        get: operations["listPmsTaskActivityLogs"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/assignees": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** `PUT /v1/pms/tasks/{task_id}/assignees`: an update of `assignee_ids`. */
        put: operations["setPmsTaskAssignees"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/attachments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Step 1: `POST /v1/pms/tasks/{task_id}/attachments`. Repeating a request id
         *     returns the same pending attachment (a lost response is looked up, never
         *     reserved twice).
         */
        post: operations["createPmsTaskAttachment"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/checklist": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/pms/tasks/{task_id}/checklist`. */
        post: operations["createPmsChecklistItem"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/checklist/reorder": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/tasks/{task_id}/checklist/reorder`: unknown ids are
         *     ignored; items left out keep their order value.
         */
        patch: operations["reorderPmsChecklist"];
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/comments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/pms/tasks/{task_id}/comments`. */
        post: operations["createPmsTaskComment"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/custom-field-values": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/tasks/{task_id}/custom-field-values`. */
        get: operations["listPmsCustomFieldValues"];
        /**
         * `PUT /v1/pms/tasks/{task_id}/custom-field-values`: `{field_id, value}`.
         *     miy: the task row is locked and the caller's edit right checked after the
         *     lock; the value is checked against the field type and stored in one
         *     spelling (a multi select is a JSON list in option order, a user field a
         *     current member of the space); an empty value removes it.
         */
        put: operations["setPmsCustomFieldValue"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/followers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** `PUT /v1/pms/tasks/{task_id}/followers`. */
        put: operations["setPmsTaskFollowers"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/tasks/{task_id}/template": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/pms/tasks/{task_id}/template` (miy): `{name}`. Saves the task in
         *     its space: title, description, status, priority, checklist, labels and its
         *     direct subtasks that are not archived (each with its description, priority,
         *     checklist and labels), at most 50 subtasks and 100 checklist items each.
         *     Assignees, dates, attachments and comments are not part of a template.
         */
        post: operations["savePmsTaskAsTemplate"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/templates/{template_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/templates/{template_id}` (miy). */
        get: operations["getPmsTemplate"];
        put?: never;
        post?: never;
        /** `DELETE /v1/pms/templates/{template_id}`. */
        delete: operations["deletePmsTemplate"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/templates/{template_id}`: the fields sent with a value;
         *     `description_blocks: null` clears the formatted description.
         */
        patch: operations["updatePmsTemplate"];
        trace?: never;
    };
    "/v1/pms/users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/pms/users`: AI-DO listed active users with PMS access
         *     (`SpaceUserItem`). The directory lists active users of the platform; who
         *     may use this app is checked by the platform when they open it. The
         *     directory has no e-mail, so `email` is empty.
         * @description Active people in directory name order. The directory does not provide email addresses.
         */
        get: operations["listPmsUsers"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/pms/view-preferences": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/pms/view-preferences`. */
        get: operations["getPmsViewPreferences"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/pms/view-preferences`.
         * @description Updates the caller’s task-list grouping preference.
         */
        patch: operations["updatePmsViewPreferences"];
        trace?: never;
    };
    "/v1/search": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/search`.
         * @description Platform-only, read-only search across tasks visible to the verified caller.
         */
        post: operations["searchPmsPlatform"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        AddSpaceMemberRequest: {
            /** @description Defaults to member. */
            role?: components["schemas"]["SpaceRole"];
            user_id: string;
        };
        AddSpaceMembersRequest: {
            /** @description Defaults to member. */
            role?: components["schemas"]["SpaceRole"];
            user_ids: string[];
        };
        BooleanInput: boolean | string | components["schemas"]["BooleanNumber"];
        /** Format: int32 */
        BooleanNumber: number;
        BulkUpdateTasksRequest: {
            add_label_ids?: string[] | null;
            archived?: boolean | null;
            assignee_id?: string | null;
            /** @description Defaults to false. If true, delete the selected tasks and their subtasks. */
            delete?: boolean | null;
            /** Format: date */
            due_date?: string | null;
            priority?: components["schemas"]["TaskPriorityInput"] | null;
            remove_label_ids?: string[] | null;
            /** Format: date */
            start_date?: string | null;
            status?: string | null;
            task_ids: string[];
        };
        BulkUpdateTasksResponse: {
            deleted_count: number;
            updated_count: number;
        };
        CheckItem: {
            groupKey: string;
            resourceId?: string | null;
        };
        CheckRequest: {
            /** @description Only the first 1000 entries are considered. */
            items?: components["schemas"]["CheckItem"][];
        };
        CheckResponse: {
            allowed: string[];
        };
        CopyTasksRequest: {
            /** @description Defaults to false. When true, copy without dates, assignees or checklist progress. */
            reset_progress?: boolean | null;
            target_list_id: string;
            task_ids: string[];
        };
        CopyTasksResponse: {
            copied_count: number;
            copied_task_ids: string[];
            target_list_id: string;
            unmatched: components["schemas"]["UnmatchedTaskCopyFields"];
        };
        CreateChecklistItemRequest: {
            /**
             * Format: int64
             * @description Defaults to 0.
             */
            sort_order?: number | null;
            text: string;
        };
        CreateCustomFieldRequest: {
            /** @description Defaults to `text`. */
            field_type?: components["schemas"]["CustomFieldKind"];
            /** @description Trimmed name of 1 to 100 characters, unique in the list. */
            name: string;
            /** @description Required for `select` and `multi_select`. These fields allow 1 to 50 distinct options. */
            options?: string[] | null;
            /**
             * Format: int64
             * @description A field without an explicit order goes last.
             */
            sort_order?: number | null;
        };
        CreateSpaceRequest: {
            /** @description Defaults to an empty description. Trimmed to at most 1000 characters. */
            description?: string;
            /** @description Trimmed to 2 through 120 characters. */
            name: string;
        };
        CreateTaskAttachmentRequest: {
            /** @description Defaults to `application/octet-stream` if null, blank or omitted. */
            content_type?: string | null;
            /** @description Trimmed filename, up to 255 characters. An empty or omitted value becomes `unnamed`. */
            filename?: string;
            /** Format: uuid */
            request_id: string;
            /**
             * Format: int64
             * @description Exact number of bytes. The legacy API also accepts an integer string.
             */
            size_bytes: number;
        };
        CreateTaskCommentRequest: {
            /** @description Defaults to an empty string. */
            body?: string;
            body_blocks?: {
                [key: string]: unknown;
            }[] | null;
        };
        CreateTaskRequest: {
            assignee_id?: string | null;
            assignee_ids?: string[] | null;
            /** Format: date */
            completed_date?: string | null;
            description?: string;
            description_blocks?: {
                [key: string]: unknown;
            }[] | null;
            /** Format: date */
            due_date?: string | null;
            label_ids?: string[] | null;
            milestone_id?: string | null;
            parent_id?: string | null;
            /** @description Defaults to `medium`. */
            priority?: components["schemas"]["TaskPriorityInput"];
            recurrence_rule?: string | null;
            /** Format: date */
            start_date?: string | null;
            /** @description Defaults to `todo`. */
            status?: string;
            template_id?: string | null;
            title: string;
        };
        CreateWorkflowStatusRequest: {
            category?: components["schemas"]["WorkflowStatusCategory"];
            color?: string;
            name: string;
            sort_order?: components["schemas"]["StatusSortOrder"] | null;
        };
        CustomFieldDefinition: {
            field_type: components["schemas"]["CustomFieldKind"];
            id: string;
            list_id: string;
            name: string;
            options: string[] | null;
            /** Format: int32 */
            sort_order: number;
        };
        /** @enum {string} */
        CustomFieldKind: "text" | "number" | "date" | "select" | "multi_select" | "user";
        CustomFieldOptionRename: {
            from: string;
            to: string;
        };
        CustomFieldPage: {
            items: components["schemas"]["CustomFieldDefinition"][];
        };
        CustomFieldValue: {
            field_id: string;
            /** @description Stored string. Multi-select values contain a JSON-encoded string array. */
            value: string;
        };
        DashboardActivity: {
            actor_name: string | null;
            created_at: string;
            id: string;
            message: string;
            task_id: string;
            task_reference: string;
        };
        DashboardPriorityCount: {
            count: number;
            label: string;
            priority: string;
        };
        DashboardStatusCount: {
            count: number;
            label: string;
            status: string;
        };
        DashboardSummary: {
            active_task_count: number;
            list_count: number;
            lists: components["schemas"]["DashboardTaskList"][];
            milestone_due_soon_count: number;
            my_task_count: number;
            overdue_task_count: number;
            priority_counts: components["schemas"]["DashboardPriorityCount"][];
            recent_activity: components["schemas"]["DashboardActivity"][];
            status_counts: components["schemas"]["DashboardStatusCount"][];
        };
        DashboardTaskList: {
            key: string;
            list_id: string;
            name: string;
            /** Format: date */
            next_due_date: string | null;
            open_task_count: number;
            overdue_task_count: number;
            /** Format: double */
            progress: number;
        };
        ErrorDetails: {
            code: string;
            message: string;
        };
        ErrorResponse: {
            error: components["schemas"]["ErrorDetails"];
        };
        /** @enum {string} */
        FilterArchivedState: "active" | "archived" | "all";
        /** @enum {string} */
        FilterGroup: "none" | "status" | "assignee" | "label";
        /** @enum {string} */
        FilterSortDirection: "asc" | "desc";
        FolderCreateRequest: {
            name: string;
            sort_order?: components["schemas"]["IntegerInput"] | null;
            team_id: string;
        };
        FolderItem: {
            id: string;
            /** Format: int64 */
            list_count: number;
            name: string;
            /** Format: int32 */
            sort_order: number;
            team_id: string | null;
        };
        FolderUpdateRequest: {
            name?: string | null;
            sort_order?: components["schemas"]["IntegerInput"] | null;
        };
        FoldersResponse: {
            items: components["schemas"]["FolderItem"][];
        };
        HomeCardItem: {
            path: string;
            subtitle: string;
            title: string;
        };
        HomeCardRequest: {
            locale?: string | null;
            /** @description IANA time zone. Missing and unknown values use Asia/Seoul. */
            timeZone?: string | null;
        };
        HomeCardResponse: {
            count: number;
            items: components["schemas"]["HomeCardItem"][];
            title: string;
        };
        IdentityErrorDetails: {
            code: string;
        };
        /** @description Error produced by the SDK gateway's signed-identity check. */
        IdentityErrorResponse: {
            error: components["schemas"]["IdentityErrorDetails"];
        };
        IntegerInput: number | string;
        LabelCreateRequest: {
            color?: string;
            name: string;
        };
        LabelItem: {
            color: string;
            id: string;
            name: string;
        };
        LabelUpdateRequest: {
            color?: string | null;
            name?: string | null;
        };
        LabelsResponse: {
            items: components["schemas"]["LabelItem"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        MilestoneCreateRequest: {
            description?: string;
            /**
             * Format: date
             * @description Must be on or after start_date when both are present.
             */
            due_date?: string | null;
            sort_order?: components["schemas"]["IntegerInput"] | null;
            /** Format: date */
            start_date?: string | null;
            status?: components["schemas"]["MilestoneStatus"];
            title: string;
        };
        MilestoneItem: {
            completed_task_count: number;
            description: string;
            /** Format: date */
            due_date: string | null;
            id: string;
            list_id: string | null;
            list_name: string | null;
            /** Format: double */
            progress: number;
            scope: components["schemas"]["MilestoneScope"];
            /** Format: int32 */
            sort_order: number;
            /** Format: date */
            start_date: string | null;
            status: components["schemas"]["MilestoneStatus"];
            task_count: number;
            team_id: string;
            title: string;
            updated_at: string;
        };
        MilestonePage: {
            items: components["schemas"]["MilestoneItem"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        /** @enum {string} */
        MilestoneScope: "list" | "space";
        /** @enum {string} */
        MilestoneStatus: "planned" | "active" | "complete";
        MilestoneUpdateRequest: {
            description?: string | null;
            /**
             * Format: date
             * @description Omit to keep this date; null clears it.
             */
            due_date?: string | null;
            sort_order?: components["schemas"]["IntegerInput"] | null;
            /**
             * Format: date
             * @description Omit to keep this date; null clears it.
             */
            start_date?: string | null;
            status?: components["schemas"]["MilestoneStatus"] | null;
            title?: string | null;
        };
        PmsMe: {
            id: string;
            name: string;
            primary_org_unit_id: string | null;
            primary_org_unit_name: string | null;
        };
        PmsViewPreferencesResponse: {
            task_list_group_by: components["schemas"]["TaskListGroupBy"];
        };
        ReorderChecklistRequest: {
            item_ids: string[];
        };
        ReorderCustomFieldsRequest: {
            field_ids: string[];
        };
        ReorderTasksByTitleRequest: {
            /** @description Defaults to `asc`. */
            sort_dir?: components["schemas"]["TaskSortDirection"];
        };
        ReorderTasksByTitleResponse: {
            updated_count: number;
        };
        ReorderTasksRequest: {
            items: components["schemas"]["TaskMoveRequest"][];
        };
        ReorderTasksResponse: {
            items: components["schemas"]["TaskSummary"][];
            updated_count: number;
        };
        /** @enum {string} */
        ReservationState: "pending";
        SaveTaskAsTemplateRequest: {
            name: string;
        };
        /** @description Specify exactly one of list_id and space_id. */
        SavedFilterCreateRequest: {
            group_by?: components["schemas"]["FilterGroup"] | null;
            list_id?: string | null;
            name: string;
            /** @description The normalized filter is at most 8 KiB of JSON. */
            params: components["schemas"]["SavedFilterParams"];
            shared?: components["schemas"]["BooleanInput"] | null;
            sort?: components["schemas"]["SavedFilterSortInput"] | null;
            space_id?: string | null;
        };
        /** @description Specify exactly one of list_id and space_id. */
        SavedFilterDefaultRequest: {
            /** @description Omit or send null to clear the current default. */
            filter_id?: string | null;
            list_id?: string | null;
            space_id?: string | null;
        };
        SavedFilterItem: {
            can_manage: boolean;
            created_at: string;
            group_by: components["schemas"]["FilterGroup"] | null;
            id: string;
            is_default: boolean;
            is_mine: boolean;
            list_id: string | null;
            name: string;
            owner_id: string;
            owner_name: string;
            params: components["schemas"]["SavedFilterParams"];
            shared: boolean;
            sort: components["schemas"]["SavedFilterSort"] | null;
            space_id: string;
            updated_at: string;
        };
        SavedFilterParams: {
            archived_state?: components["schemas"]["FilterArchivedState"] | null;
            assignee_id?: string | null;
            /** @description Up to 50 custom field IDs mapped to their conditions. */
            custom_fields?: {
                [key: string]: string;
            } | null;
            /** Format: date */
            due_date_from?: string | null;
            /** Format: date */
            due_date_to?: string | null;
            label_id?: string | null;
            milestone_id?: string | null;
            priority?: string | null;
            /** @description Accepted for compatibility and never persisted. */
            q?: unknown;
            /** Format: date */
            start_date_from?: string | null;
            /** Format: date */
            start_date_to?: string | null;
            status?: string[] | null;
        };
        SavedFilterSort: {
            direction: components["schemas"]["FilterSortDirection"];
            field: string;
        };
        SavedFilterSortInput: {
            direction?: components["schemas"]["FilterSortDirection"] | null;
            /** @description A task sort key or cf_<custom field ID>. */
            field: string;
        };
        SavedFilterUpdateRequest: {
            group_by?: components["schemas"]["FilterGroup"] | null;
            name?: string | null;
            params?: components["schemas"]["SavedFilterParams"] | null;
            shared?: components["schemas"]["BooleanInput"] | null;
            sort?: components["schemas"]["SavedFilterSortInput"] | null;
        };
        SavedFiltersResponse: {
            default_filter_id: string | null;
            items: components["schemas"]["SavedFilterItem"][];
        };
        SearchItem: {
            /** Format: date-time */
            at: string;
            kind: string;
            path: string;
            snippet: string;
            title: string;
        };
        SearchRequest: {
            /**
             * Format: int64
             * @description Defaults to 10. Values outside 1 through 10 are clamped.
             */
            limit?: number | null;
            locale?: string | null;
            /** @description Matches task titles and references. Use 1 to 200 characters after trimming. */
            query: string;
        };
        SearchResponse: {
            items: components["schemas"]["SearchItem"][];
        };
        SetCustomFieldValueRequest: {
            field_id: string;
            /** @description An empty or omitted value clears the field. Multi-select values use a JSON-encoded string array. */
            value?: string;
        };
        SetTaskAssigneesRequest: {
            user_ids: string[];
        };
        SetTaskFollowersRequest: {
            user_ids: string[];
        };
        SpaceItem: {
            created_at: string;
            current_user_role: components["schemas"]["SpaceRole"] | null;
            description: string;
            id: string;
            key: string;
            /** Format: int64 */
            member_count: number;
            name: string;
            updated_at: string;
            workspace_id: string;
            workspace_key: string;
        };
        SpaceMemberItem: {
            /** @description The platform does not provide an email address here. */
            email: string;
            full_name: string;
            /** @description Administrative access is represented by the app space role. */
            is_admin: boolean;
            joined_at: string;
            primary_org_unit_name: string | null;
            role: components["schemas"]["SpaceRole"];
            user_id: string;
        };
        SpaceMemberListResponse: {
            items: components["schemas"]["SpaceMemberItem"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        SpaceMembersBulkCreateResponse: {
            items: components["schemas"]["SpaceMemberItem"][];
            skipped_count: number;
        };
        SpacePreferenceItem: {
            hidden: boolean;
            pinned: boolean;
            space_id: string;
        };
        SpacePreferenceListResponse: {
            items: components["schemas"]["SpacePreferenceItem"][];
        };
        SpacePreferenceUpdateRequest: {
            hidden?: components["schemas"]["BooleanInput"] | null;
            pinned?: components["schemas"]["BooleanInput"] | null;
        };
        /** @enum {string} */
        SpaceRole: "owner" | "admin" | "viewer" | "member";
        SpaceStatusesResponse: {
            items: components["schemas"]["TaskListStatusItem"][];
        };
        SpaceUserItem: {
            /** @description The platform directory does not provide an email address. */
            email: string;
            full_name: string;
            id: string;
            primary_org_unit_id: string | null;
            primary_org_unit_name: string | null;
        };
        StatusModeRequest: {
            mode: components["schemas"]["TaskListStatusMode"];
        };
        StatusSortOrder: number | string;
        TaskActivityLog: {
            action: string;
            actor_id: string | null;
            actor_name: string | null;
            created_at: string;
            field_name: string | null;
            from_value: string | null;
            id: string;
            message: string;
            task_id: string;
            to_value: string | null;
        };
        TaskActivityPage: {
            items: components["schemas"]["TaskActivityLog"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        /** @description AI-DO `AttachmentItem`. */
        TaskAttachment: {
            content_type: string;
            created_at: string;
            /** @description App-relative path for `runtime.fileApi`. */
            download_url: string;
            filename: string;
            id: string;
            /** Format: int32 */
            size_bytes: number;
            task_id: string;
            uploaded_by_id: string;
            uploaded_by_name: string;
        };
        /** Format: binary */
        TaskAttachmentBytes: string;
        TaskAttachmentReservation: {
            content_type: string;
            filename: string;
            id: string;
            /** Format: int32 */
            size_bytes: number;
            status: components["schemas"]["ReservationState"];
            task_id: string;
            /** @description App-relative path for `runtime.fileApi`. */
            upload_path: string;
        };
        TaskChecklistItem: {
            completed: boolean;
            created_at: string;
            id: string;
            /** Format: int32 */
            sort_order: number;
            task_id: string;
            text: string;
        };
        TaskComment: {
            author_id: string;
            author_name: string;
            body: string;
            body_blocks: {
                [key: string]: unknown;
            }[] | null;
            created_at: string;
            id: string;
            task_id: string;
        };
        TaskDetail: {
            attachments: components["schemas"]["TaskAttachment"][];
            checklist_items: components["schemas"]["TaskChecklistItem"][];
            comments: components["schemas"]["TaskComment"][];
            /** @description Currently empty. The task detail does not populate linked documents. */
            linked_docs: unknown[];
            subtasks: components["schemas"]["TaskSummary"][];
            task: components["schemas"]["TaskSummary"];
        };
        TaskLabel: {
            color: string;
            id: string;
            name: string;
        };
        TaskListCreateRequest: {
            description?: string;
            folder_id?: string | null;
            /** @description Omit to generate a key from the name. */
            key?: string | null;
            name: string;
            team_id: string;
        };
        /** @enum {string} */
        TaskListGroupBy: "none" | "status" | "assignee" | "label";
        TaskListItem: {
            active_task_count: number;
            archived: boolean;
            created_at: string;
            description: string;
            folder_id: string | null;
            folder_name: string | null;
            id: string;
            key: string;
            /** Format: int64 */
            member_count: number;
            /** Format: int64 */
            milestone_count: number;
            name: string;
            open_task_count: number;
            overdue_task_count: number;
            /** Format: double */
            progress: number;
            role: components["schemas"]["TaskListRole"];
            /** Format: int32 */
            sort_order: number;
            status: components["schemas"]["TaskListState"];
            status_mode: components["schemas"]["TaskListWorkflowMode"];
            task_count: number;
            team_id: string | null;
            team_name: string | null;
            updated_at: string;
            workspace_id: string | null;
            workspace_key: string | null;
        };
        TaskListPage: {
            items: components["schemas"]["TaskListItem"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        TaskListReorderItem: {
            folder_id?: string | null;
            id: string;
            sort_order?: components["schemas"]["IntegerInput"] | null;
        };
        TaskListReorderRequest: {
            items: components["schemas"]["TaskListReorderItem"][];
        };
        /** @enum {string} */
        TaskListRole: "viewer" | "member" | "admin" | "owner";
        /** @enum {string} */
        TaskListState: "planned" | "active" | "on_hold" | "done";
        TaskListStatusItem: {
            category: components["schemas"]["WorkflowStatusCategory"];
            color: string;
            id: string;
            name: string;
            slug: string;
            /** Format: int32 */
            sort_order: number;
        };
        /** @enum {string} */
        TaskListStatusMode: "inherit" | "custom";
        /** @enum {string} */
        TaskListStatusSource: "space" | "list";
        TaskListStatusesResponse: {
            items: components["schemas"]["TaskListStatusItem"][];
            mode: components["schemas"]["TaskListStatusMode"];
            source: components["schemas"]["TaskListStatusSource"];
        };
        TaskListUpdateRequest: {
            archived?: components["schemas"]["BooleanInput"] | null;
            description?: string | null;
            /** @description Omit to keep the folder, or send null to move to the space root. */
            folder_id?: string | null;
            name?: string | null;
            sort_order?: components["schemas"]["IntegerInput"] | null;
            status?: components["schemas"]["TaskListState"] | null;
        };
        /** @enum {string} */
        TaskListWorkflowMode: "inherit" | "custom";
        TaskMoveRequest: {
            /** Format: int64 */
            board_position: number;
            /** @description Omit to keep the current parent. Use null to move the task to the top level. */
            parent_id?: string | null;
            task_id: string;
        };
        TaskPage: {
            items: components["schemas"]["TaskSummary"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            /** Format: int64 */
            total: number;
        };
        /** @enum {string} */
        TaskPriorityInput: "low" | "medium" | "high" | "critical";
        /** @enum {string} */
        TaskSortDirection: "asc" | "desc";
        TaskSummary: {
            archived: boolean;
            assignee_id: string | null;
            assignee_ids: string[];
            assignee_name: string | null;
            assignee_names: string[];
            /** Format: int32 */
            board_position: number;
            /** Format: int64 */
            checklist_done: number;
            /** Format: int64 */
            checklist_total: number;
            /** Format: int64 */
            comments_count: number;
            /** Format: date */
            completed_date: string | null;
            custom_fields: {
                [key: string]: string;
            };
            description: string;
            description_blocks: {
                [key: string]: unknown;
            }[] | null;
            /** Format: date */
            due_date: string | null;
            follower_ids: string[];
            follower_names: string[];
            id: string;
            labels: components["schemas"]["TaskLabel"][];
            list_id: string;
            milestone_id: string | null;
            milestone_title: string | null;
            parent_id: string | null;
            priority: string;
            priority_label: string;
            /** Format: double */
            progress: number | null;
            recurrence_rule: string | null;
            reference: string;
            reporter_id: string;
            reporter_name: string;
            /** Format: date */
            start_date: string | null;
            status: string;
            status_label: string;
            /** Format: int64 */
            subtask_count: number;
            title: string;
            updated_at: string;
        };
        TaskTemplateItem: {
            checklist_count: number;
            checklist_items: components["schemas"]["TemplateChecklistItem"][] | null;
            created_at: string;
            created_by_id: string | null;
            default_priority: components["schemas"]["TemplatePriority"];
            default_status: string;
            description: string;
            description_blocks: {
                [key: string]: unknown;
            }[] | null;
            id: string;
            labels: components["schemas"]["TemplateLabelItem"][] | null;
            name: string;
            subtask_count: number;
            subtasks: components["schemas"]["TemplateSubtaskItem"][] | null;
            team_id: string;
            title: string;
            updated_at: string;
        };
        TaskTemplatesResponse: {
            items: components["schemas"]["TaskTemplateItem"][];
        };
        TaskUserAssignment: {
            full_name: string;
            user_id: string;
        };
        TemplateChecklistItem: {
            text: string;
        };
        TemplateCreateRequest: {
            checklist_items?: components["schemas"]["TemplateChecklistItem"][] | null;
            default_priority?: components["schemas"]["TemplatePriority"];
            default_status?: string;
            description?: string;
            description_blocks?: {
                [key: string]: unknown;
            }[] | null;
            labels?: components["schemas"]["TemplateLabelInput"][] | null;
            name: string;
            subtasks?: components["schemas"]["TemplateSubtaskInput"][] | null;
            title?: string;
        };
        TemplateLabelInput: {
            color?: string | null;
            name: string;
        };
        TemplateLabelItem: {
            color: string;
            name: string;
        };
        /** @enum {string} */
        TemplatePriority: "low" | "medium" | "high" | "critical";
        TemplateSubtaskInput: {
            checklist_items?: components["schemas"]["TemplateChecklistItem"][] | null;
            description?: string | null;
            description_blocks?: unknown[] | null;
            labels?: components["schemas"]["TemplateLabelInput"][] | null;
            priority?: components["schemas"]["TemplatePriority"] | null;
            title: string;
        };
        TemplateSubtaskItem: {
            checklist_items: components["schemas"]["TemplateChecklistItem"][];
            description: string;
            description_blocks: unknown[] | null;
            labels: components["schemas"]["TemplateLabelItem"][];
            priority: components["schemas"]["TemplatePriority"];
            title: string;
        };
        TemplateUpdateRequest: {
            checklist_items?: components["schemas"]["TemplateChecklistItem"][] | null;
            default_priority?: components["schemas"]["TemplatePriority"] | null;
            default_status?: string | null;
            description?: string | null;
            /** @description Omit to keep the existing blocks, or send null to clear them. */
            description_blocks?: {
                [key: string]: unknown;
            }[] | null;
            labels?: components["schemas"]["TemplateLabelInput"][] | null;
            name?: string | null;
            subtasks?: components["schemas"]["TemplateSubtaskInput"][] | null;
            title?: string | null;
        };
        UnmatchedTaskCopyFields: {
            assignees: number;
            custom_fields: number;
            labels: number;
            milestones: number;
        };
        UpdateChecklistItemRequest: {
            completed?: boolean | null;
            /** Format: int64 */
            sort_order?: number | null;
            text?: string | null;
        };
        UpdateCustomFieldRequest: {
            /** @description The complete option list read before editing, in order. */
            expected_options?: string[] | null;
            /** @description Trimmed name of 1 to 100 characters, unique in the list. */
            name?: string | null;
            option_renames?: components["schemas"]["CustomFieldOptionRename"][] | null;
            /** @description New select options. Supply `expected_options` in the same request. */
            options?: string[] | null;
        };
        UpdateSpaceMemberRequest: {
            role: components["schemas"]["SpaceRole"];
        };
        UpdateSpaceRequest: {
            /** @description Omit or send null to keep the description. Trimmed to at most 1000 characters. */
            description?: string | null;
            /** @description Omit or send null to keep the name. Trimmed to 2 through 120 characters. */
            name?: string | null;
        };
        UpdateTaskRequest: {
            archived?: boolean | null;
            assignee_id?: string | null;
            assignee_ids?: string[] | null;
            /** Format: int64 */
            board_position?: number | null;
            /** Format: date */
            completed_date?: string | null;
            description?: string | null;
            description_blocks?: {
                [key: string]: unknown;
            }[] | null;
            /** Format: date */
            due_date?: string | null;
            label_ids?: string[] | null;
            milestone_id?: string | null;
            parent_id?: string | null;
            priority?: components["schemas"]["TaskPriorityInput"] | null;
            recurrence_rule?: string | null;
            /** Format: date */
            start_date?: string | null;
            status?: string | null;
            title?: string | null;
        };
        UpdateWorkflowStatusRequest: {
            category?: components["schemas"]["WorkflowStatusCategory"] | null;
            color?: string | null;
            name?: string | null;
            sort_order?: components["schemas"]["StatusSortOrder"] | null;
        };
        WidgetAssignedTask: components["schemas"]["TaskSummary"] & {
            workspace?: components["schemas"]["WidgetWorkspace"];
        };
        WidgetAssignmentsPage: {
            items: components["schemas"]["WidgetAssignedTask"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
            workspaces: components["schemas"]["WidgetWorkspaceMembership"][];
        };
        WidgetWorkspace: {
            id: string;
            name: string;
            slug: string;
        };
        WidgetWorkspaceMembership: components["schemas"]["WidgetWorkspace"] & {
            role: components["schemas"]["WidgetWorkspaceRole"];
        };
        /** @enum {string} */
        WidgetWorkspaceRole: "viewer" | "member" | "admin" | "owner";
        /** @enum {string} */
        WorkflowStatusCategory: "not_started" | "active" | "done" | "closed";
        /** Format: binary */
        XlsxWorkbook: string;
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    downloadPmsTaskAttachment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Stored attachment bytes */
            200: {
                headers: {
                    /** @description Stored filename and download disposition */
                    "Content-Disposition"?: string;
                    /** @description Size of the response in bytes */
                    "Content-Length"?: number;
                    /** @description sandbox */
                    "Content-Security-Policy"?: string;
                    /** @description Storage object entity tag */
                    ETag?: string;
                    /** @description nosniff */
                    "X-Content-Type-Options"?: string;
                    /** @description Core storage file ID */
                    "X-Miy-File-Id"?: string;
                    /** @description SHA-256 digest of the stored file */
                    "X-Miy-File-Sha256"?: string;
                    /** @description Core storage file version */
                    "X-Miy-File-Version"?: string;
                    [name: string]: unknown;
                };
                content: {
                    "*/*": components["schemas"]["TaskAttachmentBytes"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Task read access is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Attachment, task, or list not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform file could not be downloaded */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    uploadPmsTaskAttachment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": components["schemas"]["TaskAttachmentBytes"];
            };
        };
        responses: {
            /** @description Completed attachment */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskAttachment"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller may not upload this attachment */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Attachment, task, or list not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The list is archived or has no space, the attachment is not pending, or the file is being uploaded */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The file reservation expired */
            410: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The attachment exceeds 10 MiB */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform file could not be uploaded or attached */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    checkPmsNotifications: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["CheckRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CheckResponse"];
                };
            };
            /** @description Malformed JSON */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description JSON body exceeds the extractor limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Expected application/json */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description JSON fields could not be decoded */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsHomeCard: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["HomeCardRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HomeCardResponse"];
                };
            };
            /** @description Malformed JSON */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description JSON body exceeds the extractor limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Expected application/json */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description JSON fields could not be decoded */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsPersonalWidgetAssignedTasks: {
        parameters: {
            query?: {
                /** @description Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Assigned tasks and active workspace memberships */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WidgetAssignmentsPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsTaskAttachment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Attachment removed */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Task edit access is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Attachment, task, or list not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The list is archived or has no space */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsChecklistItem: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Checklist item deleted */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsChecklistItem: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateChecklistItemRequest"];
            };
        };
        responses: {
            /** @description Updated checklist item */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskChecklistItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsCustomField: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                field_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Custom field deleted */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsCustomField: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                field_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateCustomFieldRequest"];
            };
        };
        responses: {
            /** @description Updated custom field definition */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldDefinition"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The list is read-only, the name is already used, or the options changed since the editor loaded them */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsDashboardSummary: {
        parameters: {
            query?: {
                /** @description Limit the dashboard to this task list */
                list_id?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DashboardSummary"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsFolders: {
        parameters: {
            query?: {
                /** @description Omit to read all accessible spaces */
                team_id?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FoldersResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsFolder: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FolderCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FolderItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsFolder: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                folder_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsFolder: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                folder_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FolderUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FolderItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsLabel: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                label_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsLabel: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                label_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LabelUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LabelItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsTaskLists: {
        parameters: {
            query?: {
                /** @description Defaults to 1 */
                page?: number;
                /** @description Defaults to 20 */
                page_size?: number;
                /** @description name, key, progress, sort_order or updated_at; omitted uses updated_at */
                sort_by?: string;
                /** @description asc or desc; omitted uses desc */
                sort_dir?: string;
                q?: string;
                /** @description Omit for both archived and active lists */
                archived?: boolean;
                /** @description Omit to read all accessible spaces */
                team_id?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListPage"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTaskList: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TaskListCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListItem"];
                };
            };
            /** @description The folder must belong to the task list’s space */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsTaskList: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsTaskList: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsTaskList: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TaskListUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListItem"];
                };
            };
            /** @description The folder must belong to the task list’s space */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsCustomFields: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description List custom field definitions */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsCustomField: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateCustomFieldRequest"];
            };
        };
        responses: {
            /** @description Created custom field definition */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldDefinition"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The list is read-only, reached its field limit, or already has this field name */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    reorderPmsCustomFields: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReorderCustomFieldsRequest"];
            };
        };
        responses: {
            /** @description Custom field definitions in the new order */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    exportPmsTaskList: {
        parameters: {
            query?: {
                /** @description Match task title, description or reference */
                q?: string;
                /** @description Repeat status for multiple values */
                status?: string[];
                assignee_id?: string;
                priority?: string;
                label_id?: string;
                milestone_id?: string;
                archived?: boolean;
                due_date_from?: string;
                due_date_to?: string;
                start_date_from?: string;
                start_date_to?: string;
            };
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Excel workbook */
            200: {
                headers: {
                    /** @description private, no-store */
                    "Cache-Control"?: string;
                    /** @description Attachment filename, including UTF-8 filename* */
                    "Content-Disposition"?: string;
                    /** @description nosniff */
                    "X-Content-Type-Options"?: string;
                    [name: string]: unknown;
                };
                content: {
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": components["schemas"]["XlsxWorkbook"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsLabels: {
        parameters: {
            query?: {
                /** @description Defaults to 1 */
                page?: number;
                /** @description Defaults to 100 */
                page_size?: number;
            };
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LabelsResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsLabel: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LabelCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LabelItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsListMilestones: {
        parameters: {
            query?: {
                /** @description Defaults to 1 */
                page?: number;
                /** @description Defaults to 20 */
                page_size?: number;
                /** @description sort_order, due_date or updated_at; omitted uses sort_order */
                sort_by?: string;
                /** @description asc or desc; omitted uses asc */
                sort_dir?: string;
            };
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MilestonePage"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsListMilestone: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["MilestoneCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MilestoneItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsTaskListStatusMode: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["StatusModeRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusesResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsTaskListStatuses: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusesResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTaskListStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateWorkflowStatusRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsTasks: {
        parameters: {
            query?: {
                /** @description Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
                /** @description Sort key, including cf_<field ID> for a custom field. Defaults to board_position. */
                sort_by?: string;
                /** @description Defaults to asc. */
                sort_dir?: components["schemas"]["TaskSortDirection"];
                /** @description Search task titles, descriptions and references. */
                q?: string;
                /** @description Repeat status for multiple values. */
                status?: string[];
                assignee_id?: string;
                priority?: string;
                label_id?: string;
                milestone_id?: string;
                archived?: boolean;
                due_date_from?: string;
                due_date_to?: string;
                start_date_from?: string;
                start_date_to?: string;
            };
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Visible tasks in the list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTask: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateTaskRequest"];
            };
        };
        responses: {
            /** @description Created task */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskSummary"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    bulkUpdatePmsTasks: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["BulkUpdateTasksRequest"];
            };
        };
        responses: {
            /** @description Numbers of updated and deleted tasks */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BulkUpdateTasksResponse"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    copyPmsTasks: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CopyTasksRequest"];
            };
        };
        responses: {
            /** @description Copied task IDs and counts of unmatched related records */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CopyTasksResponse"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    reorderPmsTasks: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReorderTasksRequest"];
            };
        };
        responses: {
            /** @description Number and current state of moved tasks */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ReorderTasksResponse"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    reorderPmsTasksByTitle: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReorderTasksByTitleRequest"];
            };
        };
        responses: {
            /** @description Number of changed positions */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ReorderTasksByTitleResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsListTemplates: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                list_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplatesResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsMe: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description PMS profile of the current user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PmsMe"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
        };
    };
    deletePmsMilestone: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                milestone_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsMilestone: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                milestone_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["MilestoneUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MilestoneItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSavedFilters: {
        parameters: {
            query?: {
                /** @description Provide exactly one of list_id and space_id */
                list_id?: string;
                /** @description Provide exactly one of list_id and space_id */
                space_id?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SavedFiltersResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsSavedFilter: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SavedFilterCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SavedFilterItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    setPmsDefaultSavedFilter: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SavedFilterDefaultRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SavedFiltersResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsSavedFilter: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                filter_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsSavedFilter: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                filter_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SavedFilterUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SavedFilterItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpacePreferences: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpacePreferenceListResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsSpaceStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                status_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsSpaceStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                status_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateWorkflowStatusRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaces: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceItem"][];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsSpace: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateSpaceRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateSpaceRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaceLists: {
        parameters: {
            query?: {
                /** @description Defaults to 1 */
                page?: number;
                /** @description Defaults to 20 */
                page_size?: number;
                /** @description name, key, progress, sort_order or updated_at; omitted uses updated_at */
                sort_by?: string;
                /** @description asc or desc; omitted uses desc */
                sort_dir?: string;
                q?: string;
                /** @description Omit for both archived and active lists */
                archived?: boolean;
            };
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListPage"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    reorderPmsSpaceLists: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TaskListReorderRequest"];
            };
        };
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The request conflicts with the route constraints */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaceMembers: {
        parameters: {
            query?: {
                /** @description Page number (default 1) */
                page?: number;
                /** @description Page size (default 20) */
                page_size?: number;
            };
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceMemberListResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    addPmsSpaceMember: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AddSpaceMemberRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceMemberItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform directory is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    addPmsSpaceMembers: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AddSpaceMembersRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceMembersBulkCreateResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform directory is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    removePmsSpaceMember: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                user_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsSpaceMember: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                user_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateSpaceMemberRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceMemberItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform directory is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaceMilestones: {
        parameters: {
            query?: {
                /** @description Defaults to 1 */
                page?: number;
                /** @description Defaults to 20 */
                page_size?: number;
                /** @description sort_order, due_date or updated_at; omitted uses sort_order */
                sort_by?: string;
                /** @description asc or desc; omitted uses asc */
                sort_dir?: string;
            };
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MilestonePage"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsSpaceMilestone: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["MilestoneCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MilestoneItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsSpacePreference: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SpacePreferenceUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpacePreferenceItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaceStatuses: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceStatusesResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsSpaceStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateWorkflowStatusRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsSpaceTemplates: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplatesResponse"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTemplate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TemplateCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplateItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsTaskListStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                status_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsTaskListStatus: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                status_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateWorkflowStatusRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskListStatusItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space, task list, status, or user was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsAssignedTasks: {
        parameters: {
            query?: {
                /** @description Omit to use limit and return page 1. */
                page?: number;
                /** @description Used with an explicit page. Defaults to 50. */
                page_size?: number;
                /** @description Used when page is omitted. Defaults to 10. */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Open tasks assigned to the caller */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsTodayOverdueTasks: {
        parameters: {
            query: {
                today: string;
                /** @description Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Assigned tasks due today or overdue */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsTask: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Task and its visible related records */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskDetail"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsTask: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Task and its subtask tree deleted */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsTask: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateTaskRequest"];
            };
        };
        responses: {
            /** @description Updated task */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskSummary"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsTaskActivityLogs: {
        parameters: {
            query?: {
                /** @description Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
            };
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Activity history, newest first */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskActivityPage"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    setPmsTaskAssignees: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetTaskAssigneesRequest"];
            };
        };
        responses: {
            /** @description Current assignees */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskUserAssignment"][];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTaskAttachment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateTaskAttachmentRequest"];
            };
        };
        responses: {
            /** @description A completed attachment for this request ID */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskAttachment"];
                };
            };
            /** @description The attachment is reserved for upload */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskAttachmentReservation"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Task edit access is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Task or list not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The list is archived or has no space, the request ID is already in use, or the file is being uploaded */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The attachment exceeds 10 MiB */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid attachment reservation */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The platform file could not be reserved */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsChecklistItem: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateChecklistItemRequest"];
            };
        };
        responses: {
            /** @description Created checklist item */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskChecklistItem"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    reorderPmsChecklist: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReorderChecklistRequest"];
            };
        };
        responses: {
            /** @description Checklist positions updated */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    createPmsTaskComment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateTaskCommentRequest"];
            };
        };
        responses: {
            /** @description Created comment */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskComment"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsCustomFieldValues: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Values stored on the task */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldValue"][];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    setPmsCustomFieldValue: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetCustomFieldValueRequest"];
            };
        };
        responses: {
            /** @description Current value, or an empty string if cleared */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CustomFieldValue"];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    setPmsTaskFollowers: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SetTaskFollowersRequest"];
            };
        };
        responses: {
            /** @description Current followers */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskUserAssignment"][];
                };
            };
            /** @description The selection or related fields are invalid */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The caller lacks access */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The current list or resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    savePmsTaskAsTemplate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                task_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SaveTaskAsTemplateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplateItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current resource state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsTemplate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                template_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplateItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    deletePmsTemplate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                template_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Completed with no response body */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsTemplate: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                template_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TemplateUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TaskTemplateItem"];
                };
            };
            /** @description Valid platform identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource is unavailable or does not exist */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation failed */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Could not complete the request */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    listPmsUsers: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceUserItem"][];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description The platform directory is unavailable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    getPmsViewPreferences: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PmsViewPreferencesResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    updatePmsViewPreferences: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PmsViewPreferencesResponse"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PmsViewPreferencesResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Malformed input or invalid field value */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
    searchPmsPlatform: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SearchRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SearchResponse"];
                };
            };
            /** @description Malformed JSON */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IdentityErrorResponse"];
                };
            };
            /** @description Current permissions do not allow the operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description JSON body exceeds the extractor limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Expected application/json */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Invalid JSON payload or invalid query */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                    "text/plain": string;
                };
            };
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
        };
    };
}
