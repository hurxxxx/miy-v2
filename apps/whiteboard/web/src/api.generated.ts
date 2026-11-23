// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/files/whiteboard/items/{item_id}/files/{file_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /files/whiteboard/items/{item_id}/files/{file_id}`: the bytes, for
         *     anyone who may currently see the board.
         * @description Use the authenticated frame file gateway. Rechecks current board access before streaming an available image, up to 10 MiB.
         */
        get: operations["downloadWhiteboardFile"];
        /**
         * `PUT /files/whiteboard/items/{item_id}/files/{file_id}`: the uploader
         *     streams the reserved bytes while they may still edit the board. A repeat
         *     after a lost answer reports the stored state.
         * @description Use the authenticated frame file gateway. Send the exact number of reserved bytes, up to 10 MiB. Only the person who reserved the file may upload it, while they can edit the board.
         */
        put: operations["uploadWhiteboardFile"];
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
         * `POST /platform/notification-check`: which of the person's notification
         *     groups still point at a board they may open now (also through a group's
         *     space membership). Read-only.
         */
        post: operations["checkWhiteboardNotifications"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
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
        /** @description Search titles of boards the person can open. Invalid JSON, missing or invalid search fields, and blank or overly long queries return an empty result. The platform's timeZone field is ignored. */
        post: operations["searchWhiteboardForPlatform"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listWhiteboardSpaces"];
        put?: never;
        post: operations["createWhiteboardSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}": {
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
         * A space that still holds boards (trashed ones too) is not deleted: its
         *     boards' images and shares would have to go with it. Empty the space first
         *     (docs/PORTING.md §4).
         */
        delete: operations["deleteWhiteboardSpace"];
        options?: never;
        head?: never;
        patch: operations["updateWhiteboardSpace"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/sharing": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getWhiteboardSpaceSharing"];
        put: operations["replaceWhiteboardSpaceSharing"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/sharing/requests/{request_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getWhiteboardSpaceSharingResult"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/hub": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET …/hub`: AI-DO `build_whiteboard_hub_response`. Boards of the current
         *     space the caller may see, filtered and sorted like AI-DO.
         */
        get: operations["listWhiteboardHub"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items`: AI-DO `create_whiteboard_item` / `create_whiteboard_for_user`.
         * @description The JSON request is limited to 4 MiB. Images can be reserved after the board is created.
         */
        post: operations["createWhiteboardItem"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET …/items/{item_id}`. */
        get: operations["getWhiteboardItem"];
        put?: never;
        post?: never;
        /**
         * `DELETE …/items/{item_id}`: move to the trash (AI-DO sets `trashed_at`).
         *     The people it was shared with lose its notification.
         */
        delete: operations["trashWhiteboardItem"];
        options?: never;
        head?: never;
        /**
         * `PATCH …/items/{item_id}`: AI-DO `update_whiteboard_item`
         *     (item_mutations.py). A scene save carries `base_revision`, the revision
         *     the editor loaded or last saved. A changed scene from an older revision is
         *     refused with 409 instead of overwriting (AI-DO merged concurrent editors
         *     with Yjs). A scene equal to the stored one (ignoring per-viewer editor
         *     state) changes nothing, whatever its revision.
         * @description The JSON request is limited to 4 MiB. Scene saves require base_revision and use 409 for conflicting newer changes.
         */
        patch: operations["updateWhiteboardItem"];
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/favorite": {
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
        /** `PATCH …/items/{item_id}/favorite`. */
        patch: operations["toggleWhiteboardFavorite"];
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/files": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items/{item_id}/files`: reserve a stored image of the board.
         *     Needs edit access to the board. Repeating the same request ID with the
         *     same file answers the same row (and its current status); another payload
         *     under that request ID is a conflict.
         */
        post: operations["reserveWhiteboardFile"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/permanent": {
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
         * `DELETE …/items/{item_id}/permanent`: only from the trash. Its stored
         *     images are released after the commit (files.rs).
         */
        delete: operations["deleteWhiteboardItemPermanently"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/reference": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getWhiteboardReference"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/restore": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST …/items/{item_id}/restore`. */
        post: operations["restoreWhiteboardItem"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/sharing": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET …/items/{item_id}/sharing`: AI-DO `load_whiteboard_for_share_or_403`. */
        get: operations["getWhiteboardSharing"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/sharing/users/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT …/items/{item_id}/sharing/users/{user_id}`: AI-DO
         *     `upsert_whiteboard_user_share`, plus a `share` notification to a person
         *     newly given the board (miy).
         */
        put: operations["upsertWhiteboardUserShare"];
        post?: never;
        /**
         * `DELETE …/items/{item_id}/sharing/users/{user_id}`: the person also loses
         *     the board's notification (miy).
         */
        delete: operations["deleteWhiteboardUserShare"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/target": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT …/items/{item_id}/target`: AI-DO `_upsert_primary_target`. The only
         *     target miy knows is the board's space ("워크스페이스 공개").
         */
        put: operations["updateWhiteboardTarget"];
        post?: never;
        /**
         * `DELETE …/items/{item_id}/target`: AI-DO `_delete_primary_target` (back to
         *     "비공개").
         */
        delete: operations["deleteWhiteboardTarget"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/items/{item_id}/view": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items/{item_id}/view`: last viewed (AI-DO), and the board's share
         *     notification for the caller is read (miy).
         */
        post: operations["recordWhiteboardView"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/whiteboard/shareable-users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET …/shareable-users?q=`: AI-DO `list_whiteboard_shareable_users`.
         *     People of the current space (an app space: its direct members), not the
         *     caller, at most 30.
         */
        get: operations["listWhiteboardShareableUsers"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/hub": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET …/hub`: AI-DO `build_whiteboard_hub_response`. Boards of the current
         *     space the caller may see, filtered and sorted like AI-DO.
         */
        get: operations["listWhiteboardHubInSpace"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items`: AI-DO `create_whiteboard_item` / `create_whiteboard_for_user`.
         * @description The JSON request is limited to 4 MiB. Images can be reserved after the board is created.
         */
        post: operations["createWhiteboardItemInSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET …/items/{item_id}`. */
        get: operations["getWhiteboardItemInSpace"];
        put?: never;
        post?: never;
        /**
         * `DELETE …/items/{item_id}`: move to the trash (AI-DO sets `trashed_at`).
         *     The people it was shared with lose its notification.
         */
        delete: operations["trashWhiteboardItemInSpace"];
        options?: never;
        head?: never;
        /**
         * `PATCH …/items/{item_id}`: AI-DO `update_whiteboard_item`
         *     (item_mutations.py). A scene save carries `base_revision`, the revision
         *     the editor loaded or last saved. A changed scene from an older revision is
         *     refused with 409 instead of overwriting (AI-DO merged concurrent editors
         *     with Yjs). A scene equal to the stored one (ignoring per-viewer editor
         *     state) changes nothing, whatever its revision.
         * @description The JSON request is limited to 4 MiB. Scene saves require base_revision and use 409 for conflicting newer changes.
         */
        patch: operations["updateWhiteboardItemInSpace"];
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/favorite": {
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
        /** `PATCH …/items/{item_id}/favorite`. */
        patch: operations["toggleWhiteboardFavoriteInSpace"];
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/files": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items/{item_id}/files`: reserve a stored image of the board.
         *     Needs edit access to the board. Repeating the same request ID with the
         *     same file answers the same row (and its current status); another payload
         *     under that request ID is a conflict.
         */
        post: operations["reserveWhiteboardFileInSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/permanent": {
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
         * `DELETE …/items/{item_id}/permanent`: only from the trash. Its stored
         *     images are released after the commit (files.rs).
         */
        delete: operations["deleteWhiteboardItemPermanentlyInSpace"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/reference": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getWhiteboardReferenceInSpace"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/restore": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST …/items/{item_id}/restore`. */
        post: operations["restoreWhiteboardItemInSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/sharing": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET …/items/{item_id}/sharing`: AI-DO `load_whiteboard_for_share_or_403`. */
        get: operations["getWhiteboardSharingInSpace"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/sharing/users/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT …/items/{item_id}/sharing/users/{user_id}`: AI-DO
         *     `upsert_whiteboard_user_share`, plus a `share` notification to a person
         *     newly given the board (miy).
         */
        put: operations["upsertWhiteboardUserShareInSpace"];
        post?: never;
        /**
         * `DELETE …/items/{item_id}/sharing/users/{user_id}`: the person also loses
         *     the board's notification (miy).
         */
        delete: operations["deleteWhiteboardUserShareInSpace"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/target": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT …/items/{item_id}/target`: AI-DO `_upsert_primary_target`. The only
         *     target miy knows is the board's space ("워크스페이스 공개").
         */
        put: operations["updateWhiteboardTargetInSpace"];
        post?: never;
        /**
         * `DELETE …/items/{item_id}/target`: AI-DO `_delete_primary_target` (back to
         *     "비공개").
         */
        delete: operations["deleteWhiteboardTargetInSpace"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/items/{item_id}/view": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST …/items/{item_id}/view`: last viewed (AI-DO), and the board's share
         *     notification for the caller is read (miy).
         */
        post: operations["recordWhiteboardViewInSpace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/workspaces/{space_id}/whiteboard/shareable-users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET …/shareable-users?q=`: AI-DO `list_whiteboard_shareable_users`.
         *     People of the current space (an app space: its direct members), not the
         *     caller, at most 30.
         */
        get: operations["listWhiteboardShareableUsersInSpace"];
        put?: never;
        post?: never;
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
        /**
         * Format: binary
         * @description Bodies streamed through the file gateway.
         */
        BinaryBody: string;
        BoardReference: {
            id: string;
            space_id: string;
            title: string;
        };
        CheckItem: {
            groupKey: string;
            resourceId?: string | null;
        };
        CheckRequest: {
            /** @description The first 500 items are checked. */
            items?: components["schemas"]["CheckItem"][];
        };
        CheckResponse: {
            allowed: string[];
        };
        /** @description AI-DO `CreateWhiteboardRequest`. */
        CreateRequest: {
            /** @description Defaults to `human`. */
            generation_kind?: string;
            primary_target?: components["schemas"]["TargetPayload"] | null;
            /** @description Excalidraw scene object. Omit or send null to start with an empty scene. */
            scene?: {
                [key: string]: unknown;
            } | null;
            /** @description Defaults to `whiteboard`. */
            source_app?: string;
            /** @description Defaults to `manual`. */
            source_kind?: string;
            source_ref?: string | null;
            title: string;
        };
        Detail: components["schemas"]["HubItem"] & {
            /** @description Excalidraw scene object. `files` references stored images by row ID. */
            scene: {
                [key: string]: unknown;
            };
        };
        ErrorDetails: {
            code: string;
            message: string;
        };
        ErrorResponse: {
            error: components["schemas"]["ErrorDetails"];
        };
        FavoriteResponse: {
            is_favorite: boolean;
        };
        FileResponse: {
            /** @description Excalidraw's file ID. */
            file_id: string;
            /** @description Board file row ID to save as `storage` in the scene. */
            id: string;
            status: components["schemas"]["WhiteboardFileStatus"];
        };
        /** @description AI-DO `WhiteboardHubItem` plus `revision` (miy). */
        HubItem: {
            can_edit: boolean;
            can_manage: boolean;
            can_share: boolean;
            can_view: boolean;
            created_at: string;
            created_by_id: string;
            created_by_name: string;
            generation_kind: string;
            id: string;
            is_favorite: boolean;
            is_private: boolean;
            last_viewed_at: string | null;
            location_label: string;
            primary_target: components["schemas"]["PrimaryTarget"] | null;
            /**
             * Format: int64
             * @description Stored scene revision used for concurrent saves.
             */
            revision: number;
            source_app: string;
            source_badge: string;
            source_deeplink: string | null;
            source_id: string;
            source_kind: string;
            source_ref: string | null;
            source_type: components["schemas"]["WhiteboardSourceType"];
            target_label: string;
            targets: components["schemas"]["TargetItem"][];
            title: string;
            trashed_at: string | null;
            updated_at: string;
            workspace_id: string | null;
            workspace_key: string | null;
        };
        HubResponse: {
            can_publish: boolean;
            items: components["schemas"]["HubItem"][];
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            page_size: number;
            total: number;
        };
        IdentityErrorDetails: {
            code: string;
        };
        IdentityErrorResponse: {
            error: components["schemas"]["IdentityErrorDetails"];
        };
        PrimaryTarget: {
            app: string;
            id: string;
            /** Format: int32 */
            sort_order: number;
            type: string;
        };
        ReserveRequest: {
            /** @description Excalidraw's file ID (the key in `scene.files`). */
            file_id: string;
            /** @description PNG, JPEG, GIF or WebP. The server trims and lowercases this value. */
            mime: string;
            /** @description The trimmed filename may contain up to 255 characters and no control characters. */
            name: string;
            /** Format: uuid */
            request_id: string;
            /**
             * Format: int64
             * @description Exact size in bytes, up to 10 MiB.
             */
            size: number;
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
             * @description Defaults to 5. Supplied values are clamped to the range 1 to 10.
             */
            limit?: number | null;
            locale?: string | null;
            /** @description A trimmed query of 1 to 200 characters returns eligible matches. */
            query: string;
        };
        SearchResponse: {
            items: components["schemas"]["SearchItem"][];
        };
        /**
         * @description AI-DO `ShareableUserItem`; `email` is empty (the directory has none) and
         *     `primary_group` names the person's group.
         */
        ShareableUser: {
            email: string;
            full_name: string;
            id: string;
            primary_group: string | null;
        };
        SharingRequest: {
            entries: components["schemas"]["SpaceGrant"][];
            /** Format: int64 */
            expectedRevision: number;
            /** Format: uuid */
            requestId: string;
        };
        /**
         * @description AI-DO `WhiteboardSharingResponse`. Link shares are not offered in miy, so
         *     `link_share` is always null.
         */
        SharingResponse: {
            link_share: null;
            owner_id: string;
            users: components["schemas"]["UserShareItem"][];
            whiteboard_id: string;
        };
        SpaceGrant: {
            role: string;
            subject: components["schemas"]["SpaceSubjectRef"];
        };
        SpaceOut: {
            canManage: boolean;
            createdAt: string;
            createdBy: string;
            description: string;
            id: string;
            name: string;
            updatedAt: string;
        };
        SpaceRequest: {
            description?: string;
            /** @description The space name must contain a non-whitespace character. */
            name: string;
        };
        SpaceShareEntry: {
            protected: boolean;
            role: string;
            subject: components["schemas"]["SpaceShareSubject"];
        };
        SpaceShareRole: {
            id: string;
            label: string;
        };
        SpaceShareSnapshot: {
            canManage: boolean;
            defaultRole: string | null;
            entries: components["schemas"]["SpaceShareEntry"][];
            /** Format: int64 */
            revision: number;
            roles: components["schemas"]["SpaceShareRole"][];
        };
        SpaceShareSubject: {
            id: string;
            kind: components["schemas"]["SpaceSubjectKind"];
            name: string;
        };
        /** @enum {string} */
        SpaceSubjectKind: "user" | "group";
        SpaceSubjectRef: {
            id: string;
            kind: components["schemas"]["SpaceSubjectKind"];
        };
        SpacesResponse: {
            spaces: components["schemas"]["SpaceOut"][];
        };
        TargetItem: {
            app: string;
            id: string;
            is_primary: boolean;
            /** Format: int32 */
            sort_order: number;
            type: string;
        };
        TargetPayload: {
            /** @description Use `whiteboard` for a board shared in its own workspace. */
            app: string;
            /** @description The board's space ID, or `global` for the app-wide space. */
            id: string;
            /**
             * Format: int32
             * @description Defaults to 0.
             */
            sort_order?: number;
            /** @description Use `workspace_sidebar` for a board shared in its own workspace. */
            type: string;
        };
        UpdateRequest: {
            /**
             * Format: int64
             * @description Integer scene revision. Required when `scene` is present, including null.
             */
            base_revision?: number | null;
            /** @description Omit to leave the scene unchanged. An explicit null clears it. */
            scene?: {
                [key: string]: unknown;
            } | null;
            /** @description Omit or send null to leave the title unchanged. */
            title?: string | null;
        };
        /** @enum {string} */
        UserShareAccess: "read" | "edit";
        UserShareItem: {
            access_level: components["schemas"]["UserShareAccess"];
            email: string;
            full_name: string;
            user_id: string;
        };
        UserShareRequest: {
            access_level: components["schemas"]["UserShareAccess"];
        };
        /** @enum {string} */
        WhiteboardFileStatus: "pending" | "ready" | "attached";
        /** @enum {string} */
        WhiteboardSourceType: "whiteboard";
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    downloadWhiteboardFile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Board file row ID returned as id by the reservation */
                file_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Stored image bytes */
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
                    "image/gif": components["schemas"]["BinaryBody"];
                    "image/jpeg": components["schemas"]["BinaryBody"];
                    "image/png": components["schemas"]["BinaryBody"];
                    "image/webp": components["schemas"]["BinaryBody"];
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
            /** @description The board or image is missing or is not visible */
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
            /** @description The image could not be read or a required platform service is unavailable */
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
    uploadWhiteboardFile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Board file row ID returned as id by the reservation */
                file_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": components["schemas"]["BinaryBody"];
            };
        };
        responses: {
            /** @description Current image file status */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileResponse"];
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
            /** @description The caller cannot edit this board */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The board or image is missing or is not available to the caller */
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
            /** @description The image could not be stored or a required platform service is unavailable */
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
    checkWhiteboardNotifications: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CheckRequest"];
            };
        };
        responses: {
            /** @description Notification group keys the caller may still open */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CheckResponse"];
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
            /** @description Only the platform may call this endpoint */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid notification check request */
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
            /** @description Signed directory information is unavailable */
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
    searchWhiteboardForPlatform: {
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
            /** @description Currently readable search results */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SearchResponse"];
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
            /** @description Only the platform may call this endpoint */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request body exceeds the application limit */
            413: {
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
            /** @description Signed directory information is unavailable */
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
    listWhiteboardSpaces: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Spaces visible to the current user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpacesResponse"];
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
            /** @description Current permissions do not allow space membership lookup */
            403: {
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
            /** @description A required platform service is unavailable */
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
    createWhiteboardSpace: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SpaceRequest"];
            };
        };
        responses: {
            /** @description Created space */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceOut"];
                };
            };
            /** @description The creator could not be added as an ACL subject */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description An ACL resource already exists for the new ID */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid space request */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardSpace: {
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
            /** @description Empty space deleted */
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
            /** @description The caller cannot manage this space */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The space still contains boards */
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
            /** @description A required platform service is unavailable */
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
    updateWhiteboardSpace: {
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
                "application/json": components["schemas"]["SpaceRequest"];
            };
        };
        responses: {
            /** @description Updated space */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceOut"];
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
            /** @description The caller cannot manage this space */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid space request */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardSpaceSharing: {
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
            /** @description Current space ACL and its revision */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceShareSnapshot"];
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
            /** @description The caller cannot manage this space */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
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
            /** @description A required platform service is unavailable */
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
    replaceWhiteboardSpaceSharing: {
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
                "application/json": components["schemas"]["SharingRequest"];
            };
        };
        responses: {
            /** @description Current space ACL and its revision */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceShareSnapshot"];
                };
            };
            /** @description Invalid ACL request, role or subject */
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
            /** @description The caller cannot manage this space */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request or revision conflict, protected owner, or last manager */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid JSON request */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardSpaceSharingResult: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                request_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Recorded ACL replacement result, or null if it has not been recorded */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceShareSnapshot"] | null;
                };
            };
            /** @description Invalid ACL request ID */
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
            /** @description The caller cannot manage this space */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
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
            /** @description A required platform service is unavailable */
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
    listWhiteboardHub: {
        parameters: {
            query?: {
                /** @description all, mine, recent, favorites, or archived. Takes precedence over category. */
                view?: string;
                /** @description Legacy filter: my, mine, recent, favorite, favorites, archived, or all. */
                category?: string;
                /** @description One-based page. Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
                /** @description title, created_at, last_viewed_at, or target_sort_order. Defaults to updated_at for omitted or unrecognized values. */
                sort_by?: string;
                /** @description asc or desc. Defaults to desc. */
                sort_dir?: string;
                q?: string;
                source_app?: string;
                source_kind?: string;
                /** @description Filter by workspace ID. */
                space_id?: string;
                target_app?: string;
                target_type?: string;
                target_id?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Page of visible boards */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HubResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    createWhiteboardItem: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateRequest"];
            };
        };
        responses: {
            /** @description Created board */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The Excalidraw scene exceeds the supported size */
            413: {
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardItem: {
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
            /** @description Board detail and Excalidraw scene */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    trashWhiteboardItem: {
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
            /** @description Board moved to the trash */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    updateWhiteboardItem: {
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
                "application/json": components["schemas"]["UpdateRequest"];
            };
        };
        responses: {
            /** @description Board detail after saving */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A different scene was saved since base_revision (whiteboard_revision_conflict) */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The Excalidraw scene exceeds the supported size */
            413: {
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
            /** @description A required platform service is unavailable */
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
    toggleWhiteboardFavorite: {
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
            /** @description Current favorite state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FavoriteResponse"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    reserveWhiteboardFile: {
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
                "application/json": components["schemas"]["ReserveRequest"];
            };
        };
        responses: {
            /** @description Image reservation and its current status */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileResponse"];
                };
            };
            /** @description The image media type is unsupported */
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
            /** @description The caller cannot edit this board */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description This request ID already belongs to a different reservation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File size must be between 1 byte and 10 MiB */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid image reservation */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardItemPermanently: {
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
            /** @description Board permanently deleted */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Move the board to the trash before permanently deleting it */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardReference: {
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
            /** @description A readable, active board's ID and title */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BoardReference"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    restoreWhiteboardItem: {
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
            /** @description Restored board detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardSharing: {
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
            /** @description Current board shares */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    upsertWhiteboardUserShare: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                user_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UserShareRequest"];
            };
        };
        responses: {
            /** @description Current board shares */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The user already owns the board or is not a member of its workspace */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardUserShare: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                user_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current board shares after removal */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    updateWhiteboardTarget: {
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
                "application/json": components["schemas"]["TargetPayload"];
            };
        };
        responses: {
            /** @description Board detail after sharing with its workspace */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardTarget: {
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
            /** @description Board detail after removing the workspace target */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    recordWhiteboardView: {
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
            /** @description View recorded */
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    listWhiteboardShareableUsers: {
        parameters: {
            query?: {
                /** @description Filter workspace members by name. The trimmed query may contain up to 200 characters. */
                q?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Up to 30 eligible people */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ShareableUser"][];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The query exceeds 200 characters after trimming */
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
            /** @description A required platform service is unavailable */
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
    listWhiteboardHubInSpace: {
        parameters: {
            query?: {
                /** @description all, mine, recent, favorites, or archived. Takes precedence over category. */
                view?: string;
                /** @description Legacy filter: my, mine, recent, favorite, favorites, archived, or all. */
                category?: string;
                /** @description One-based page. Defaults to 1. */
                page?: number;
                /** @description Defaults to 50. */
                page_size?: number;
                /** @description title, created_at, last_viewed_at, or target_sort_order. Defaults to updated_at for omitted or unrecognized values. */
                sort_by?: string;
                /** @description asc or desc. Defaults to desc. */
                sort_dir?: string;
                q?: string;
                source_app?: string;
                source_kind?: string;
                /** @description Filter by workspace ID. */
                space_id?: string;
                target_app?: string;
                target_type?: string;
                target_id?: string;
            };
            header?: never;
            path: {
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Page of visible boards */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HubResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    createWhiteboardItemInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateRequest"];
            };
        };
        responses: {
            /** @description Created board */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The Excalidraw scene exceeds the supported size */
            413: {
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardItemInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Board detail and Excalidraw scene */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    trashWhiteboardItemInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Board moved to the trash */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    updateWhiteboardItemInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateRequest"];
            };
        };
        responses: {
            /** @description Board detail after saving */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A different scene was saved since base_revision (whiteboard_revision_conflict) */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The Excalidraw scene exceeds the supported size */
            413: {
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
            /** @description A required platform service is unavailable */
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
    toggleWhiteboardFavoriteInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current favorite state */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FavoriteResponse"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    reserveWhiteboardFileInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReserveRequest"];
            };
        };
        responses: {
            /** @description Image reservation and its current status */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileResponse"];
                };
            };
            /** @description The image media type is unsupported */
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
            /** @description The caller cannot edit this board */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description This request ID already belongs to a different reservation */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File size must be between 1 byte and 10 MiB */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid image reservation */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardItemPermanentlyInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Board permanently deleted */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Move the board to the trash before permanently deleting it */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardReferenceInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description A readable, active board's ID and title */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BoardReference"];
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    restoreWhiteboardItemInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Restored board detail */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    getWhiteboardSharingInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current board shares */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    upsertWhiteboardUserShareInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                user_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UserShareRequest"];
            };
        };
        responses: {
            /** @description Current board shares */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The user already owns the board or is not a member of its workspace */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardUserShareInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                user_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Current board shares after removal */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingResponse"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    updateWhiteboardTargetInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TargetPayload"];
            };
        };
        responses: {
            /** @description Board detail after sharing with its workspace */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    deleteWhiteboardTargetInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Board detail after removing the workspace target */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Detail"];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    recordWhiteboardViewInSpace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                item_id: string;
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description View recorded */
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
            /** @description The workspace or board is missing or is not visible */
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
            /** @description A required platform service is unavailable */
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
    listWhiteboardShareableUsersInSpace: {
        parameters: {
            query?: {
                /** @description Filter workspace members by name. The trimmed query may contain up to 200 characters. */
                q?: string;
            };
            header?: never;
            path: {
                /** @description Whiteboard app space ID */
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Up to 30 eligible people */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ShareableUser"][];
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Space not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description The query exceeds 200 characters after trimming */
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
            /** @description A required platform service is unavailable */
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
}
