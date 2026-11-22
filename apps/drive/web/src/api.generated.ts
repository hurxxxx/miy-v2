// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/files/items/{file_id}/preview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /files/items/{file_id}/preview` (fileApi; AI-DO `get_file_preview`
         *     returned an inline URL): the image bytes for a frame Blob URL. Raster
         *     types only, signature checked, up to 10 MiB.
         */
        get: operations["previewDriveImage"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/files/uploads/{upload_id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** `PUT /files/uploads/{upload_id}/content` (fileApi): a single upload. */
        put: operations["putDriveUploadContent"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/files/uploads/{upload_id}/parts": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * `PUT /files/uploads/{upload_id}/parts?offset=N` (fileApi): one range of a
         *     large upload, at the committed offset. The answer carries the new
         *     `received`; after a failure it carries the offset to resume from.
         */
        put: operations["putDriveUploadPart"];
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
         * `POST /platform/notification-check`: a `space:<id>` notification stays
         *     while the person may still open that space (a direct grant or one of the
         *     current groups the call carries). Read-only.
         */
        post: operations["checkDriveNotifications"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/file-access": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/file-access` (platformEndpoints.fileAccess): before sending the
         *     bytes of a download link, may this person still read the file? The same
         *     rule as the screen (`require_file_access`): the file is not in the trash,
         *     the person still holds a role in its space (directly or through the
         *     current groups the call carries), the file is visible to them and every
         *     folder above it is.
         */
        post: operations["checkDriveDownloadAccess"];
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
        /**
         * `POST /v1/search` (platformEndpoints.search): file names the person may
         *     open now. The call cannot ask the directory; the person's current groups
         *     come signed with it (`identity.context.groups`).
         */
        post: operations["searchDriveForPlatform"];
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
        /** `GET /v1/spaces`: the spaces the caller may open, by name. */
        get: operations["listDriveSpaces"];
        put?: never;
        /** `POST /v1/spaces`. */
        post: operations["createDriveSpace"];
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
         * `DELETE /v1/spaces/{space_id}` (space admins): the space, its folders,
         *     files and trash. The Core files are released after the commit.
         */
        delete: operations["deleteDriveSpace"];
        options?: never;
        head?: never;
        /** `PATCH /v1/spaces/{space_id}`: rename (space admins). */
        patch: operations["renameDriveSpace"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/files": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/spaces/{space_id}/files?folder_id=` (AI-DO `GET /files`). */
        get: operations["browseDriveFiles"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/archive": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/spaces/{space_id}/files/archive`: AI-DO `list_archive_entries`
         *     and its limits. AI-DO streamed a ZIP; miy answers the planned entries and
         *     the screen downloads each file with its own link (docs/PORTING.md §3.5).
         */
        post: operations["planDriveDownloads"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/bulk-delete": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/bulk-delete` (AI-DO `bulk_delete`). */
        post: operations["trashDriveSelection"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/folders": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/folders` (AI-DO `create_folder`). */
        post: operations["createDriveFolder"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/folders/{folder_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/spaces/{space_id}/files/folders/{folder_id}`: to the trash. */
        delete: operations["trashDriveFolder"];
        options?: never;
        head?: never;
        /**
         * `PATCH /v1/spaces/{space_id}/files/folders/{folder_id}` (AI-DO
         *     `update_folder`: rename; visibility is fixed; miy adds a move).
         */
        patch: operations["updateDriveFolder"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/move": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/move` (보강): move a selection. */
        post: operations["moveDriveSelection"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/search": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/search` (AI-DO `search_files`). */
        post: operations["searchDriveFiles"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/uploads": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/uploads`. */
        post: operations["reserveDriveUpload"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/uploads/{upload_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/spaces/{space_id}/files/uploads/{upload_id}`: the committed
         *     offset to resume from.
         */
        get: operations["getDriveUpload"];
        put?: never;
        post?: never;
        /**
         * `DELETE /v1/spaces/{space_id}/files/uploads/{upload_id}`: cancel an
         *     upload that has not completed. A completed upload is a file: delete it.
         */
        delete: operations["cancelDriveUpload"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/uploads/{upload_id}/complete": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/files/uploads/{upload_id}/complete`. */
        post: operations["completeDriveUpload"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/{file_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/spaces/{space_id}/files/{file_id}` (AI-DO `delete_file`). */
        delete: operations["trashDriveFile"];
        options?: never;
        head?: never;
        /** `PATCH /v1/spaces/{space_id}/files/{file_id}` (보강): rename or move. */
        patch: operations["updateDriveFile"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/{file_id}/download": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/spaces/{space_id}/files/{file_id}/download` (AI-DO
         *     `get_file_download`): a five-minute link for `runtime.download`.
         */
        get: operations["getDriveDownloadLink"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/{file_id}/versions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/spaces/{space_id}/files/{file_id}/versions`. */
        get: operations["listDriveFileVersions"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/files/{file_id}/versions/{version_id}/download": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/spaces/{space_id}/files/{file_id}/versions/{version_id}/download`. */
        get: operations["getDriveVersionDownloadLink"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/sharing": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/spaces/{space_id}/sharing` (space admins). */
        get: operations["getDriveSpaceSharing"];
        /**
         * `PUT /v1/spaces/{space_id}/sharing` (space admins). People and groups
         *     added get a `share` notification; people taken off lose it.
         */
        put: operations["replaceDriveSpaceSharing"];
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
        /**
         * `GET /v1/spaces/{space_id}/sharing/requests/{request_id}`: the result of a
         *     replacement whose answer was lost.
         */
        get: operations["getDriveSharingResult"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/trash": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/spaces/{space_id}/trash`: the trashed items the caller may
         *     restore or delete (their own; everything for a space admin).
         */
        get: operations["listDriveTrash"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/trash/delete": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/spaces/{space_id}/trash/delete`: delete permanently. */
        post: operations["purgeDriveTrash"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/trash/empty": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/spaces/{space_id}/trash/empty`: permanently delete every trashed
         *     item the caller may delete.
         */
        post: operations["emptyDriveTrash"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/trash/restore": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/spaces/{space_id}/trash/restore`: back where they were, or to
         *     the space root when that folder is gone or still in the trash.
         */
        post: operations["restoreDriveTrash"];
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
        ArchiveEntryOut: {
            file_id: string | null;
            path: string;
            /** Format: int64 */
            size_bytes: number;
        };
        ArchivePlanResponse: {
            entries: components["schemas"]["ArchiveEntryOut"][];
            file_count: number;
            /** Format: int64 */
            total_size: number;
        };
        /**
         * Format: binary
         * @description Binary bodies flow through the file gateway. This only describes their wire format.
         */
        BinaryBody: string;
        CheckItem: {
            groupKey: string;
            resourceId?: string | null;
        };
        ErrorDetails: {
            code: string;
            message: string;
        };
        ErrorResponse: {
            error: components["schemas"]["ErrorDetails"];
        };
        FileAccessRequest: {
            fileId: string;
            reference?: string | null;
        };
        FileAccessResponse: {
            allowed: boolean;
        };
        FileBrowseResponse: {
            all_folders: components["schemas"]["FileFolderItem"][];
            breadcrumbs: components["schemas"]["FileFolderItem"][];
            can_ingest: boolean;
            current_folder: components["schemas"]["FileFolderItem"] | null;
            files: components["schemas"]["FileItem"][];
            folders: components["schemas"]["FileFolderItem"][];
            ingest_block_reason: components["schemas"]["IngestBlockReason"] | null;
            space: components["schemas"]["SpaceSummary"];
        };
        FileBulkRequest: {
            file_ids?: string[];
            folder_ids?: string[];
        };
        FileDownloadResponse: {
            expires_at: string;
            url: string;
        };
        FileFolderItem: {
            can_manage: boolean;
            created_at: string;
            id: string;
            name: string;
            owner_id: string;
            owner_name: string;
            parent_id: string | null;
            updated_at: string;
            visibility: components["schemas"]["FileVisibility"];
        };
        FileItem: {
            can_delete: boolean;
            content_type: string;
            created_at: string;
            filename: string;
            folder_id: string | null;
            id: string;
            owner_id: string;
            owner_name: string;
            /** Format: int64 */
            size_bytes: number;
            updated_at: string;
            /** Format: int64 */
            version_count: number;
            /** Format: int32 */
            version_number: number;
            visibility: components["schemas"]["FileVisibility"];
        };
        FileSearchHit: {
            author: string | null;
            authored_at: string | null;
            content_type: string;
            department: string | null;
            document_type: string | null;
            file_id: string;
            filename: string;
            folder_id: string | null;
            methods: string[];
            /** Format: int32 */
            rank: number;
            /** Format: double */
            score: number;
            /** Format: int64 */
            size_bytes: number;
            snippet: components["schemas"]["Snippet"];
            source_kind: string;
            source_updated_at: string | null;
            title: string;
            updated_at: string;
        };
        FileSearchRequest: {
            /** @description Owner name (AI-DO: the source author). */
            author?: string | null;
            /** @description Upload time range (AI-DO: the source authored time). */
            authored_from?: string | null;
            authored_to?: string | null;
            department?: string | null;
            /** @description A content type or its prefix (AI-DO: the source document type). */
            document_type?: string | null;
            /**
             * Format: int32
             * @description One-based page. The product of `page` and `page_size` must be at most 100.
             * @default 1
             */
            page: number;
            /**
             * Format: int32
             * @default 20
             */
            page_size: number;
            /** @description Search text containing at least one non-whitespace character. */
            query: string;
            source_kind?: string | null;
            /**
             * @description Searches file names and metadata. Legacy requests that omit this field
             *     default to `hybrid` and receive HTTP 422, as do explicit `semantic` and `hybrid` requests.
             */
            strategy: components["schemas"]["FileSearchStrategy"];
        };
        FileSearchResponse: {
            has_more: boolean;
            hits: components["schemas"]["FileSearchHit"][];
            /** Format: int64 */
            latency_ms: number;
            /** Format: int32 */
            max_ranked_results: number;
            /** Format: int32 */
            page: number;
            /** Format: int32 */
            page_size: number;
            query: string;
            strategy: components["schemas"]["FileSearchStrategy"];
            trace_id: string | null;
        };
        /** @enum {string} */
        FileSearchStrategy: "keyword";
        FileUpdateRequest: {
            filename?: string | null;
            folder_id?: string | null;
        };
        /** @enum {string} */
        FileVisibility: "private" | "workspace";
        FolderCreateRequest: {
            name: string;
            parent_id?: string | null;
            visibility?: components["schemas"]["FileVisibility"];
        };
        FolderUpdateRequest: {
            name?: string | null;
            /** @description miy (보강): move under another folder (`null` = the space root). */
            parent_id?: string | null;
            visibility?: components["schemas"]["FileVisibility"] | null;
        };
        Highlight: {
            end: number;
            start: number;
        };
        IdentityErrorDetails: {
            code: string;
        };
        IdentityErrorResponse: {
            error: components["schemas"]["IdentityErrorDetails"];
        };
        /** @enum {string} */
        IngestBlockReason: "workspace_access_required" | "parent_manage_required";
        MoveRequest: {
            file_ids?: string[];
            folder_ids?: string[];
            target_folder_id?: string | null;
        };
        NotificationCheckRequest: {
            items: components["schemas"]["CheckItem"][];
        };
        NotificationCheckResponse: {
            allowed: string[];
        };
        PlatformSearchItem: {
            at: string;
            kind: string;
            path: string;
            snippet: string;
            title: string;
        };
        PlatformSearchRequest: {
            /** Format: int32 */
            limit: number;
            locale?: string | null;
            query: string;
        };
        PlatformSearchResponse: {
            items: components["schemas"]["PlatformSearchItem"][];
        };
        /** @enum {string} */
        Role: "viewer" | "member" | "admin";
        SharingEntry: {
            protected: boolean;
            role: string;
            subject: components["schemas"]["SharingSubject"];
        };
        SharingGrant: {
            role: string;
            subject: components["schemas"]["SharingSubjectRef"];
        };
        SharingRequest: {
            entries: components["schemas"]["SharingGrant"][];
            /** Format: int64 */
            expectedRevision: number;
            requestId: string;
        };
        SharingRole: {
            id: string;
            label: string;
        };
        SharingSnapshot: {
            canManage: boolean;
            defaultRole: string | null;
            entries: components["schemas"]["SharingEntry"][];
            /** Format: int64 */
            revision: number;
            roles: components["schemas"]["SharingRole"][];
        };
        SharingSubject: {
            id: string;
            kind: components["schemas"]["SharingSubjectKind"];
            name: string;
        };
        /** @enum {string} */
        SharingSubjectKind: "user" | "group";
        SharingSubjectRef: {
            id: string;
            kind: components["schemas"]["SharingSubjectKind"];
        };
        Snippet: {
            highlights: components["schemas"]["Highlight"][];
            text: string;
        };
        SpaceOut: {
            can_manage: boolean;
            can_upload: boolean;
            created_at: string;
            created_by: string;
            id: string;
            name: string;
            role: components["schemas"]["Role"];
            updated_at: string;
        };
        SpaceRequest: {
            name: string;
        };
        SpaceSummary: {
            can_manage: boolean;
            id: string;
            name: string;
            role: components["schemas"]["Role"];
        };
        SpacesResponse: {
            spaces: components["schemas"]["SpaceOut"][];
            /** @description The caller (for the share editor's "you" marker). */
            user_id: string;
        };
        TrashItem: {
            content_type: string | null;
            deleted_at: string;
            /** Format: int64 */
            file_count: number;
            id: string;
            kind: components["schemas"]["TrashKind"];
            name: string;
            /** @description The folder path the item was in (`""` for the space root). */
            original_path: string;
            owner_id: string;
            owner_name: string;
            /** Format: int64 */
            size_bytes: number;
            trashed_by: string;
            trashed_by_name: string;
            visibility: components["schemas"]["FileVisibility"];
        };
        /** @enum {string} */
        TrashKind: "file" | "folder";
        TrashResponse: {
            items: components["schemas"]["TrashItem"][];
            space: components["schemas"]["SpaceSummary"];
        };
        UploadOut: {
            content_type: string;
            file_id: string;
            filename: string;
            id: string;
            /** Format: int64 */
            part_size: number;
            /** Format: int64 */
            received: number;
            request_id: string;
            /** Format: int64 */
            size_bytes: number;
            /**
             * @description `reserving`, `uploading`, `ready` (all bytes stored), `completed`
             *     (the file shows this version) or `cancelled`.
             */
            status: components["schemas"]["UploadStatus"];
            transfer: components["schemas"]["UploadTransfer"];
        };
        UploadRequest: {
            content_type?: string | null;
            /** @description miy (versions): upload a new version of this file. */
            file_id?: string | null;
            filename: string;
            folder_id?: string | null;
            request_id: string;
            /** Format: int64 */
            size_bytes: number;
            visibility?: components["schemas"]["FileVisibility"];
        };
        /** @enum {string} */
        UploadStatus: "reserving" | "uploading" | "ready" | "completed" | "cancelled";
        /** @enum {string} */
        UploadTransfer: "single" | "parts";
        VersionOut: {
            content_type: string;
            created_at: string;
            filename: string;
            id: string;
            is_current: boolean;
            sha256: string | null;
            /** Format: int64 */
            size_bytes: number;
            uploaded_by: string;
            uploader_name: string;
            /** Format: int32 */
            version_number: number;
        };
        VersionsResponse: {
            file: components["schemas"]["FileItem"];
            versions: components["schemas"]["VersionOut"][];
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    previewDriveImage: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                file_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Verified raster image up to 10 MiB */
            200: {
                headers: {
                    /** @description private, no-store */
                    "Cache-Control"?: string;
                    /** @description inline */
                    "Content-Disposition"?: string;
                    /** @description nosniff */
                    "X-Content-Type-Options"?: string;
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File or selection exceeds the supported limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Unsupported content type */
            415: {
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
            /** @description Storage rejected the operation */
            502: {
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
    putDriveUploadContent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                upload_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": components["schemas"]["BinaryBody"];
            };
        };
        responses: {
            /** @description Committed upload state after sending at most 10 MiB */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadOut"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File or selection exceeds the supported limit */
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
            /** @description Storage rejected the operation */
            502: {
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
    putDriveUploadPart: {
        parameters: {
            query: {
                /** @description Committed byte offset at which the part starts */
                offset: number;
            };
            header?: never;
            path: {
                upload_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": components["schemas"]["BinaryBody"];
            };
        };
        responses: {
            /** @description Upload state after sending a part of at most 8 MiB, including the committed next offset */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadOut"];
                };
            };
            /** @description Missing or invalid offset query parameter */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Offset does not match the committed position, or upload state changed */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Part exceeds 8 MiB */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Part is empty or would extend beyond the reserved file size */
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
    checkDriveNotifications: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NotificationCheckRequest"];
            };
        };
        responses: {
            /** @description Notification group keys still visible to the user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NotificationCheckResponse"];
                };
            };
            /** @description Malformed request */
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
            /** @description Only the platform may call this endpoint */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Unsupported content type */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Invalid or unsupported request */
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
    checkDriveDownloadAccess: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FileAccessRequest"];
            };
        };
        responses: {
            /** @description Current permission to stream this Storage file */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileAccessResponse"];
                };
            };
            /** @description Malformed request */
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
            /** @description Only the platform may call this endpoint */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Unsupported content type */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Invalid or unsupported request */
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
    searchDriveForPlatform: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PlatformSearchRequest"];
            };
        };
        responses: {
            /** @description Readable links for platform search */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlatformSearchResponse"];
                };
            };
            /** @description Malformed request */
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
            /** @description Only the platform may call this endpoint */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Unsupported content type */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Invalid or unsupported request */
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
    listDriveSpaces: {
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Directory service is unavailable */
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
    createDriveSpace: {
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
            /** @description Resource ACL is unavailable */
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
    deleteDriveSpace: {
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
            /** @description Space deleted */
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
            /** @description Resource was not found or is not visible */
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
    renameDriveSpace: {
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
            /** @description Renamed space */
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
            /** @description Current permissions do not allow this operation */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    browseDriveFiles: {
        parameters: {
            query?: {
                /** @description Omit for the space root */
                folder_id?: string;
            };
            header?: never;
            path: {
                space_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Visible files, folders and current ingest policy */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileBrowseResponse"];
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
            /** @description Resource was not found or is not visible */
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
    planDriveDownloads: {
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
                "application/json": components["schemas"]["FileBulkRequest"];
            };
        };
        responses: {
            /** @description Plan of accessible files for separate downloads */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ArchivePlanResponse"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File or selection exceeds the supported limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    trashDriveSelection: {
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
                "application/json": components["schemas"]["FileBulkRequest"];
            };
        };
        responses: {
            /** @description Selection moved to the trash */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    createDriveFolder: {
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
                "application/json": components["schemas"]["FolderCreateRequest"];
            };
        };
        responses: {
            /** @description Created folder */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileFolderItem"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    trashDriveFolder: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                folder_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Folder and its contents moved to the trash */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
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
    updateDriveFolder: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
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
            /** @description Updated folder */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileFolderItem"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    moveDriveSelection: {
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
                "application/json": components["schemas"]["MoveRequest"];
            };
        };
        responses: {
            /** @description Selection moved */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    searchDriveFiles: {
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
                "application/json": components["schemas"]["FileSearchRequest"];
            };
        };
        responses: {
            /** @description Ranked results containing only readable files */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileSearchResponse"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid input returns `validation_request_invalid`. Legacy requests that omit `strategy` or specify `semantic` or `hybrid` return `files_search_strategy_unsupported` when the other fields pass validation. */
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
    reserveDriveUpload: {
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
                "application/json": components["schemas"]["UploadRequest"];
            };
        };
        responses: {
            /** @description Same reservation recovered by request ID */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadOut"];
                };
            };
            /** @description New reservation */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadOut"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request ID conflict or Storage quota exceeded */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description File or selection exceeds the supported limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
            /** @description Storage rejected the operation */
            502: {
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
    getDriveUpload: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                upload_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Upload and its committed byte offset */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadOut"];
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
            /** @description Resource was not found or is not visible */
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
    cancelDriveUpload: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                upload_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Unfinished upload cancelled */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
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
    completeDriveUpload: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                upload_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Published file after successful completion */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileItem"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Upload cancelled, incomplete or still finalizing. Retry finalizing with the same upload ID */
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
    trashDriveFile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                file_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description File moved to the trash */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
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
    updateDriveFile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                file_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FileUpdateRequest"];
            };
        };
        responses: {
            /** @description Updated file */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileItem"];
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    getDriveDownloadLink: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                file_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Short-lived link bound to the current app frame */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileDownloadResponse"];
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
            /** @description Current file access and an app frame are required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
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
            /** @description Storage rejected the operation */
            502: {
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
    listDriveFileVersions: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                file_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description File and version list */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VersionsResponse"];
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
            /** @description Resource was not found or is not visible */
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
    getDriveVersionDownloadLink: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                file_id: string;
                version_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Short-lived version link bound to the current app frame */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FileDownloadResponse"];
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
            /** @description Current file access and an app frame are required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
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
            /** @description Storage rejected the operation */
            502: {
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
    getDriveSpaceSharing: {
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
            /** @description Sharing rules and current revision */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingSnapshot"];
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
            /** @description Sharing management permission is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
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
    replaceDriveSpaceSharing: {
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
            /** @description Updated sharing snapshot */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingSnapshot"];
                };
            };
            /** @description Malformed request */
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
            /** @description Sharing management permission is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description ACL revision or request ID conflict */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    getDriveSharingResult: {
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
            /** @description Recorded result, or null when no result was recorded */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingSnapshot"] | null;
                };
            };
            /** @description Malformed request */
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
            /** @description Sharing management permission is required */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource was not found or is not visible */
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
    listDriveTrash: {
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
            /** @description Trash entries the caller may manage */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrashResponse"];
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
            /** @description Resource was not found or is not visible */
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
    purgeDriveTrash: {
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
                "application/json": components["schemas"]["FileBulkRequest"];
            };
        };
        responses: {
            /** @description Trash selection permanently deleted */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
    emptyDriveTrash: {
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
            /** @description Accessible trash emptied */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
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
    restoreDriveTrash: {
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
                "application/json": components["schemas"]["FileBulkRequest"];
            };
        };
        responses: {
            /** @description Trash selection restored */
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
            /** @description Resource was not found or is not visible */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Resource state changed or conflicts with this request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Invalid or unsupported request */
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
