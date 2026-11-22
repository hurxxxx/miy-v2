// Schema shapes copied verbatim from AI-DO packages/contracts/src/
// openapi.generated.d.ts (commit 0002af062) for the PMS API types in
// pms-api.ts. Only the schemas pms-api.ts names and what they reference.
// The miy backend answers the same JSON (apps/pms/src).
/* eslint-disable */
export interface PmsApiSchemas {
    ActivityLogItem: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Actor Id */
        actor_id: string | null;
        /** Actor Name */
        actor_name: string | null;
        /** Action */
        action: string;
        /** Field Name */
        field_name: string | null;
        /** From Value */
        from_value: string | null;
        /** To Value */
        to_value: string | null;
        /** Message */
        message: string;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    ActivityLogListResponse: {
        /** Items */
        items: PmsApiSchemas["ActivityLogItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    AttachmentItem: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Filename */
        filename: string;
        /** Content Type */
        content_type: string;
        /** Size Bytes */
        size_bytes: number;
        /** Download Url */
        download_url: string;
        /** Uploaded By Id */
        uploaded_by_id: string;
        /** Uploaded By Name */
        uploaded_by_name: string;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    BulkUpdateRequest: {
        /** Task Ids */
        task_ids: string[];
        /** Status */
        status?: string | null;
        /** Priority */
        priority?: ("low" | "medium" | "high" | "critical") | null;
        /** Assignee Id */
        assignee_id?: string | null;
        /** Add Label Ids */
        add_label_ids?: string[];
        /** Remove Label Ids */
        remove_label_ids?: string[];
        /** Archived */
        archived?: boolean | null;
        /** Start Date */
        start_date?: string | null;
        /** Due Date */
        due_date?: string | null;
        /**
         * Delete
         * @default false
         */
        delete: boolean;
    };
    BulkUpdateResponse: {
        /** Updated Count */
        updated_count: number;
        /** Deleted Count */
        deleted_count: number;
    };
    ChecklistItemResponse: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Text */
        text: string;
        /** Completed */
        completed: boolean;
        /** Sort Order */
        sort_order: number;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    CustomFieldItem: {
        /** Id */
        id: string;
        /** List Id */
        list_id: string;
        /** Name */
        name: string;
        /** Field Type */
        field_type: string;
        /** Options */
        options?: string[] | null;
        /** Sort Order */
        sort_order: number;
    };
    CustomFieldListResponse: {
        /** Items */
        items: PmsApiSchemas["CustomFieldItem"][];
    };
    CustomFieldValueItem: {
        /** Field Id */
        field_id: string;
        /** Value */
        value: string;
    };
    DashboardSummaryResponse: {
        /** List Count */
        list_count: number;
        /** Active Task Count */
        active_task_count: number;
        /** Overdue Task Count */
        overdue_task_count: number;
        /** My Task Count */
        my_task_count: number;
        /** Milestone Due Soon Count */
        milestone_due_soon_count: number;
        /** Status Counts */
        status_counts: PmsApiSchemas["StatusCountItem"][];
        /** Priority Counts */
        priority_counts: PmsApiSchemas["PriorityCountItem"][];
        /** Lists */
        lists: PmsApiSchemas["DashboardTaskListItem"][];
        /** Recent Activity */
        recent_activity: PmsApiSchemas["RecentActivityItem"][];
    };
    DashboardTaskListItem: {
        /** List Id */
        list_id: string;
        /** Key */
        key: string;
        /** Name */
        name: string;
        /** Progress */
        progress: number;
        /** Open Task Count */
        open_task_count: number;
        /** Overdue Task Count */
        overdue_task_count: number;
        /** Next Due Date */
        next_due_date: string | null;
    };
    FolderItem: {
        /** Id */
        id: string;
        /** Team Id */
        team_id: string | null;
        /** Name */
        name: string;
        /** Sort Order */
        sort_order: number;
        /**
         * List Count
         * @default 0
         */
        list_count: number;
    };
    FolderListResponse: {
        /** Items */
        items: PmsApiSchemas["FolderItem"][];
    };
    LabelItem: {
        /** Id */
        id: string;
        /** Name */
        name: string;
        /** Color */
        color: string;
    };
    LabelListResponse: {
        /** Items */
        items: PmsApiSchemas["LabelItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    MilestoneItem: {
        /** Id */
        id: string;
        /** List Id */
        list_id: string;
        /** Title */
        title: string;
        /** Description */
        description: string;
        /** Status */
        status: string;
        /** Start Date */
        start_date: string | null;
        /** Due Date */
        due_date: string | null;
        /** Sort Order */
        sort_order: number;
        /** Progress */
        progress: number;
        /** Task Count */
        task_count: number;
        /** Completed Task Count */
        completed_task_count: number;
        /**
         * Updated At
         * Format: date-time
         */
        updated_at: string;
    };
    MilestoneListResponse: {
        /** Items */
        items: PmsApiSchemas["MilestoneItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    PmsViewPreferencesResponse: {
        /**
         * Task List Group By
         * @enum {string}
         */
        task_list_group_by: "none" | "status" | "assignee" | "label";
    };
    PriorityCountItem: {
        /** Priority */
        priority: string;
        /** Label */
        label: string;
        /** Count */
        count: number;
    };
    RecentActivityItem: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Task Reference */
        task_reference: string;
        /** Message */
        message: string;
        /** Actor Name */
        actor_name: string | null;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    SpaceItem: {
        /** Id */
        id: string;
        /** Workspace Id */
        workspace_id: string;
        /** Workspace Key */
        workspace_key: string;
        /** Key */
        key: string;
        /** Name */
        name: string;
        /** Description */
        description: string;
        /** Member Count */
        member_count: number;
        /** Current User Role */
        current_user_role: string | null;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
        /**
         * Updated At
         * Format: date-time
         */
        updated_at: string;
    };
    SpaceMemberItem: {
        /** User Id */
        user_id: string;
        /** Email */
        email: string;
        /** Full Name */
        full_name: string;
        /** Primary Org Unit Name */
        primary_org_unit_name?: string | null;
        /** Is Admin */
        is_admin: boolean;
        /** Role */
        role: string;
        /**
         * Joined At
         * Format: date-time
         */
        joined_at: string;
    };
    SpaceMemberListResponse: {
        /** Items */
        items: PmsApiSchemas["SpaceMemberItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    SpaceMembersBulkCreateResponse: {
        /** Items */
        items: PmsApiSchemas["SpaceMemberItem"][];
        /** Skipped Count */
        skipped_count: number;
    };
    SpacePreferenceItem: {
        /** Space Id */
        space_id: string;
        /** Pinned */
        pinned: boolean;
        /** Hidden */
        hidden: boolean;
    };
    SpacePreferenceUpdateRequest: {
        /** Pinned */
        pinned?: boolean | null;
        /** Hidden */
        hidden?: boolean | null;
    };
    SpaceUserItem: {
        /** Id */
        id: string;
        /** Email */
        email: string;
        /** Full Name */
        full_name: string;
        /** Primary Org Unit Id */
        primary_org_unit_id?: string | null;
        /** Primary Org Unit Name */
        primary_org_unit_name?: string | null;
    };
    StatusCountItem: {
        /** Status */
        status: string;
        /** Label */
        label: string;
        /** Count */
        count: number;
    };
    TaskAssigneeItem: {
        /** User Id */
        user_id: string;
        /** Full Name */
        full_name: string;
    };
    TaskCommentItem: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Author Id */
        author_id: string;
        /** Author Name */
        author_name: string;
        /** Body */
        body: string;
        /** Body Blocks */
        body_blocks?: {
            [key: string]: unknown;
        }[] | null;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    TaskCopyRequest: {
        /** Task Ids */
        task_ids: string[];
        /** Target List Id */
        target_list_id: string;
        /**
         * Reset Progress
         * @default false
         */
        reset_progress: boolean;
    };
    TaskCopyResponse: {
        /** Target List Id */
        target_list_id: string;
        /** Copied Count */
        copied_count: number;
        /** Copied Task Ids */
        copied_task_ids: string[];
        unmatched: PmsApiSchemas["TaskCopyUnmatchedCounts"];
    };
    TaskCopyUnmatchedCounts: {
        /** Assignees */
        assignees: number;
        /** Labels */
        labels: number;
        /** Milestones */
        milestones: number;
        /** Custom Fields */
        custom_fields: number;
    };
    TaskDetailResponse: {
        task: PmsApiSchemas["TaskItem"];
        /** Comments */
        comments: PmsApiSchemas["TaskCommentItem"][];
        /**
         * Linked Docs
         * @default []
         */
        linked_docs: PmsApiSchemas["TaskDocLinkItem"][];
        /**
         * Subtasks
         * @default []
         */
        subtasks: PmsApiSchemas["TaskItem"][];
        /**
         * Attachments
         * @default []
         */
        attachments: PmsApiSchemas["AttachmentItem"][];
        /**
         * Checklist Items
         * @default []
         */
        checklist_items: PmsApiSchemas["ChecklistItemResponse"][];
    };
    TaskDocLinkItem: {
        /** Id */
        id: string;
        /** Task Id */
        task_id: string;
        /** Doc Id */
        doc_id: string;
        /** Workspace Key */
        workspace_key?: string | null;
        /** Doc Title */
        doc_title: string;
        /** Doc Type */
        doc_type: string;
        /** Source App */
        source_app: string;
        /** Source Kind */
        source_kind: string;
        /**
         * Updated At
         * Format: date-time
         */
        updated_at: string;
        /** Created By Id */
        created_by_id: string;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    TaskDocLinksResponse: {
        /** Items */
        items: PmsApiSchemas["TaskDocLinkItem"][];
    };
    TaskFollowerItem: {
        /** User Id */
        user_id: string;
        /** Full Name */
        full_name: string;
    };
    TaskItem: {
        /** Id */
        id: string;
        /** List Id */
        list_id: string;
        /** Reference */
        reference: string;
        /** Title */
        title: string;
        /** Description */
        description: string;
        /** Description Blocks */
        description_blocks?: {
            [key: string]: unknown;
        }[] | null;
        /** Parent Id */
        parent_id?: string | null;
        /**
         * Subtask Count
         * @default 0
         */
        subtask_count: number;
        /** Status */
        status: string;
        /** Status Label */
        status_label: string;
        /** Priority */
        priority: string;
        /** Priority Label */
        priority_label: string;
        /** Assignee Id */
        assignee_id: string | null;
        /** Assignee Name */
        assignee_name: string | null;
        /**
         * Assignee Ids
         * @default []
         */
        assignee_ids: string[];
        /**
         * Assignee Names
         * @default []
         */
        assignee_names: string[];
        /**
         * Follower Ids
         * @default []
         */
        follower_ids: string[];
        /**
         * Follower Names
         * @default []
         */
        follower_names: string[];
        /** Reporter Id */
        reporter_id: string;
        /** Reporter Name */
        reporter_name: string;
        /** Milestone Id */
        milestone_id: string | null;
        /** Milestone Title */
        milestone_title: string | null;
        /** Start Date */
        start_date: string | null;
        /** Due Date */
        due_date: string | null;
        /** Completed Date */
        completed_date: string | null;
        /** Board Position */
        board_position: number;
        /** Archived */
        archived: boolean;
        /** Progress */
        progress: number | null;
        /** Comments Count */
        comments_count: number;
        /**
         * Checklist Total
         * @default 0
         */
        checklist_total: number;
        /**
         * Checklist Done
         * @default 0
         */
        checklist_done: number;
        /** Recurrence Rule */
        recurrence_rule?: string | null;
        /** Labels */
        labels: PmsApiSchemas["LabelItem"][];
        /**
         * Updated At
         * Format: date-time
         */
        updated_at: string;
    };
    TaskItemsResponse: {
        /** Items */
        items: PmsApiSchemas["TaskItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    TaskListItem: {
        /** Workspace Id */
        workspace_id: string | null;
        /** Workspace Key */
        workspace_key: string | null;
        /** Id */
        id: string;
        /** Key */
        key: string;
        /** Name */
        name: string;
        /** Description */
        description: string;
        /** Status */
        status: string;
        /**
         * Status Mode
         * @default custom
         */
        status_mode: string;
        /** Archived */
        archived: boolean;
        /** Team Id */
        team_id: string | null;
        /** Team Name */
        team_name: string | null;
        /** Folder Id */
        folder_id?: string | null;
        /** Folder Name */
        folder_name?: string | null;
        /**
         * Sort Order
         * @default 0
         */
        sort_order: number;
        /** Role */
        role: string;
        /** Progress */
        progress: number;
        /** Member Count */
        member_count: number;
        /** Milestone Count */
        milestone_count: number;
        /** Task Count */
        task_count: number;
        /** Active Task Count */
        active_task_count: number;
        /** Open Task Count */
        open_task_count: number;
        /** Overdue Task Count */
        overdue_task_count: number;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
        /**
         * Updated At
         * Format: date-time
         */
        updated_at: string;
    };
    TaskListStatusItem: {
        /** Id */
        id: string;
        /** Slug */
        slug: string;
        /** Name */
        name: string;
        /** Color */
        color: string;
        /**
         * Category
         * @enum {string}
         */
        category: "not_started" | "active" | "done" | "closed";
        /** Sort Order */
        sort_order: number;
    };
    TaskListsResponse: {
        /** Items */
        items: PmsApiSchemas["TaskListItem"][];
        /** Total */
        total: number;
        /** Page */
        page: number;
        /** Page Size */
        page_size: number;
    };
    TaskReorderItem: {
        /** Task Id */
        task_id: string;
        /** Board Position */
        board_position: number;
        /** Parent Id */
        parent_id?: string | null;
    };
    TaskReorderRequest: {
        /** Items */
        items: PmsApiSchemas["TaskReorderItem"][];
    };
    TaskReorderResponse: {
        /** Updated Count */
        updated_count: number;
        /** Items */
        items: PmsApiSchemas["TaskItem"][];
    };
    TaskTemplateItem: {
        /** Id */
        id: string;
        /** List Id */
        list_id: string;
        /** Name */
        name: string;
        /** Description */
        description: string;
        /** Default Status */
        default_status: string;
        /** Default Priority */
        default_priority: string;
        /** Checklist Items */
        checklist_items?: {
            [key: string]: unknown;
        }[] | null;
        /**
         * Created At
         * Format: date-time
         */
        created_at: string;
    };
    TaskTemplateListResponse: {
        /** Items */
        items: PmsApiSchemas["TaskTemplateItem"][];
    };
    TaskTitleOrderResponse: {
        /** Updated Count */
        updated_count: number;
    };
}

export type ApiSchema<K extends keyof PmsApiSchemas> = PmsApiSchemas[K];
