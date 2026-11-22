// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/files/community/posts/{post_id}/media/{media_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /files/community/posts/{post_id}/media/{id}`: AI-DO's
         *     `resolve_post_media_urls` for one image, answering the bytes. A secret
         *     post accepts its password here.
         */
        post: operations["getCommunityPostMedia"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/files/media/{media_id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /files/media/{id}/content`: the uploader's own image (editor preview)
         *     or an image of a post or comment the caller can read without a password.
         */
        get: operations["getBoardMedia"];
        /**
         * `PUT /files/media/{id}/content`: the uploader streams the reserved bytes.
         * @description Streams the raw bytes of a reserved image through the app file gateway. The reservation and stored image are limited to 10 MiB.
         */
        put: operations["uploadBoardMedia"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/platform/home-card": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /platform/home-card`: the person's notice card on the portal home.
         *     At most five notices they may read, pinned first then newest. Read-only,
         *     app DB only (space membership through user grants or signed groups, as
         *     above). The body
         *     carries the person's display settings; only the language is used, for the
         *     card and company labels (Korean unless `en-US`).
         */
        post: operations["getBoardHomeCard"];
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
         *     groups still point at something they may open. Read-only and answered from
         *     the app DB: a platform call carries no token for directory calls, so space
         *     membership through a group uses the group IDs the Core signed into the
         *     call (`identity.context.groups`; none without directory approval).
         */
        post: operations["checkBoardNotifications"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/announcements": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listCompanyAnnouncements"];
        put?: never;
        post: operations["createCompanyAnnouncement"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/announcements/{announcement_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getCompanyAnnouncement"];
        put?: never;
        post?: never;
        delete: operations["deleteCompanyAnnouncement"];
        options?: never;
        head?: never;
        patch: operations["updateCompanyAnnouncement"];
        trace?: never;
    };
    "/v1/board/managers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listBoardManagers"];
        put?: never;
        post: operations["addBoardManager"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/board/managers/bootstrap": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["bootstrapBoardManager"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/board/managers/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["removeBoardManager"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/board/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getBoardMe"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/board/notices/recent": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/board/notices/recent?limit=`: the newest notices the caller may
         *     read (company notices and notices of spaces they belong to), for the
         *     portal home notice card (M5). Read-only.
         */
        get: operations["listRecentBoardNotices"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/admin/channels": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listAdminCommunityChannels"];
        put?: never;
        post: operations["createCommunityChannel"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/admin/channels/{channel_key}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["deleteCommunityChannel"];
        options?: never;
        head?: never;
        patch: operations["updateCommunityChannel"];
        trace?: never;
    };
    "/v1/community/admin/filter-terms": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listCommunityFilterTerms"];
        put?: never;
        post: operations["createCommunityFilterTerm"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/admin/filter-terms/{term_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["deleteCommunityFilterTerm"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/channels": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listCommunityChannels"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/channels/{channel_key}/posts": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listCommunityPosts"];
        put?: never;
        post: operations["createCommunityPost"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/posts/{post_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getCommunityPost"];
        put?: never;
        post?: never;
        delete: operations["deleteCommunityPost"];
        options?: never;
        head?: never;
        patch: operations["updateCommunityPost"];
        trace?: never;
    };
    "/v1/community/posts/{post_id}/comments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["createCommunityComment"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/posts/{post_id}/comments/{comment_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["deleteCommunityComment"];
        options?: never;
        head?: never;
        patch: operations["updateCommunityComment"];
        trace?: never;
    };
    "/v1/community/posts/{post_id}/read": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["markCommunityPostRead"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/community/posts/{post_id}/unlock": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["unlockCommunityPost"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/media/upload": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["reserveBoardMedia"];
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
        get: operations["listBoardSpaces"];
        put?: never;
        post: operations["createBoardSpace"];
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
        delete: operations["deleteBoardSpace"];
        options?: never;
        head?: never;
        patch: operations["updateBoardSpace"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/announcements": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listSpaceAnnouncements"];
        put?: never;
        post: operations["createSpaceAnnouncement"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/spaces/{space_id}/announcements/{announcement_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getSpaceAnnouncement"];
        put?: never;
        post?: never;
        delete: operations["deleteSpaceAnnouncement"];
        options?: never;
        head?: never;
        patch: operations["updateSpaceAnnouncement"];
        trace?: never;
    };
    "/v1/spaces/{space_id}/sharing": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getBoardSpaceSharing"];
        put: operations["replaceBoardSpaceSharing"];
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
        get: operations["getBoardSpaceSharingResult"];
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
        AnnouncementOut: {
            authorId: string;
            authorName: string;
            body: string;
            canManage: boolean;
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            id: string;
            isPinned: boolean;
            scope: components["schemas"]["AnnouncementScope"];
            title: string;
            /** @example 2026-05-04T01:02:03 */
            updatedAt: string;
            workspaceId: string | null;
        };
        /** @enum {string} */
        AnnouncementScope: "company" | "workspace";
        AnnouncementsResponse: {
            hasMore: boolean;
            items: components["schemas"]["AnnouncementOut"][];
        };
        BoardMe: {
            canBootstrap: boolean;
            isManager: boolean;
            userId: string;
        };
        /** @enum {string} */
        BoardMediaStatus: "pending" | "ready" | "attached" | "detaching" | "detached";
        ChannelOut: {
            active: boolean;
            adminOnlyContent: boolean;
            description: string;
            forceAnonymous: boolean;
            id: string;
            key: string;
            name: string;
            /** Format: int32 */
            position: number;
            readOnly: boolean;
            templateBody: string;
            templateTitle: string;
        };
        ChannelRequest: {
            active?: boolean;
            adminOnlyContent?: boolean;
            description?: string;
            forceAnonymous?: boolean;
            key: string;
            name: string;
            /** Format: int64 */
            position?: number;
            readOnly?: boolean;
            templateBody?: string;
            templateTitle?: string;
        };
        ChannelsResponse: {
            channels: components["schemas"]["ChannelOut"][];
        };
        CheckItem: {
            groupKey: string;
            resourceId?: string | null;
        };
        CheckRequest: {
            /** @description At most the first 500 items are considered. */
            items?: components["schemas"]["CheckItem"][];
        };
        CheckResponse: {
            allowed: string[];
        };
        CommentCreateRequest: {
            body: string;
            isAnonymous?: boolean;
            parentCommentId?: string | null;
            password?: string | null;
        };
        CommentOut: {
            /** Format: int64 */
            anonSeq: number | null;
            authorName: string | null;
            body: string;
            canModify: boolean;
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            id: string;
            isAnonymous: boolean;
            isDeleted: boolean;
            parentCommentId: string | null;
            /** @example 2026-05-04T01:02:03 */
            updatedAt: string;
        };
        CommentUpdateRequest: {
            body: string;
        };
        /** @enum {string} */
        CompanyAnnouncementScope: "company";
        CreateCompanyAnnouncementRequest: {
            body?: string;
            isPinned?: boolean;
            scope: components["schemas"]["CompanyAnnouncementScope"];
            title: string;
        };
        CreateRequest: {
            body?: string;
            isPinned?: boolean;
            scope?: components["schemas"]["AnnouncementScope"] | null;
            title: string;
        };
        ErrorDetail: {
            code: string;
            message: string;
        };
        ErrorResponse: {
            error: components["schemas"]["ErrorDetail"];
        };
        FilterTermCreateRequest: {
            term: string;
        };
        FilterTermItem: {
            /** @example 2026-05-04T01:02:03 */
            created_at: string;
            id: string;
            term: string;
        };
        FilterTermList: {
            items: components["schemas"]["FilterTermItem"][];
            /** Format: int64 */
            limit: number;
        };
        GatewayIdentityError: {
            error: components["schemas"]["GatewayIdentityErrorDetail"];
        };
        GatewayIdentityErrorDetail: {
            /** @example invalid_gateway_identity */
            code: string;
        };
        HomeCard: {
            items: components["schemas"]["HomeCardItem"][];
            title: string;
        };
        HomeCardItem: {
            /** Format: date-time */
            at: string;
            path: string;
            subtitle: string;
            title: string;
        };
        HomeCardRequest: {
            /** @description English labels for en-US and Korean labels otherwise. */
            locale?: string | null;
            /** @description Time zone preference supplied by the platform. */
            timeZone?: string | null;
        };
        /** @enum {string} */
        LockedReason: "password" | "admin_only";
        ManagerAddRequest: {
            userId: string;
        };
        ManagerOut: {
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            grantedBy: string;
            name: string | null;
            userId: string;
        };
        ManagersResponse: {
            items: components["schemas"]["ManagerOut"][];
        };
        PostCreateRequest: {
            body: string;
            isAnonymous?: boolean;
            isSecret?: boolean;
            password?: string | null;
            title: string;
        };
        PostDetail: components["schemas"]["PostOut"] & {
            comments: components["schemas"]["CommentOut"][];
        };
        PostListResponse: {
            /** Format: int64 */
            page: number;
            /** Format: int64 */
            pageSize: number;
            posts: components["schemas"]["PostOut"][];
            /** Format: int64 */
            total: number;
        };
        PostMediaRequest: {
            password?: string | null;
        };
        PostOut: {
            authorName: string | null;
            body: string;
            canModify: boolean;
            channelKey: string;
            /** Format: int64 */
            commentCount: number;
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            id: string;
            isAnonymous: boolean;
            isMine: boolean;
            isRead: boolean;
            isSecret: boolean;
            locked: boolean;
            lockedReason: components["schemas"]["LockedReason"] | null;
            /** Format: int32 */
            number: number;
            title: string;
            /** @example 2026-05-04T01:02:03 */
            updatedAt: string;
        };
        PostUpdateRequest: {
            body: string;
            title: string;
        };
        RecentNotice: {
            authorName: string;
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            id: string;
            isPinned: boolean;
            path: string;
            scope: components["schemas"]["AnnouncementScope"];
            spaceId: string | null;
            spaceName: string | null;
            title: string;
        };
        RecentNoticesResponse: {
            items: components["schemas"]["RecentNotice"][];
        };
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
            /** Format: uuid */
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
        SpaceOut: {
            canManage: boolean;
            /** @example 2026-05-04T01:02:03 */
            createdAt: string;
            createdBy: string;
            description: string;
            id: string;
            name: string;
            /** @example 2026-05-04T01:02:03 */
            updatedAt: string;
        };
        SpaceRequest: {
            description?: string;
            name: string;
        };
        SpacesResponse: {
            spaces: components["schemas"]["SpaceOut"][];
        };
        UnlockRequest: {
            password: string;
        };
        UpdateRequest: {
            body?: string | null;
            isPinned?: boolean | null;
            title?: string | null;
        };
        UploadRequest: {
            /** @description JPEG, PNG, GIF or WebP. */
            mime: string;
            name: string;
            /** Format: uuid */
            requestId: string;
            /** Format: int64 */
            size: number;
        };
        UploadResponse: {
            id: string;
            status: components["schemas"]["BoardMediaStatus"];
            /** @description A media reference of the form `media:<id>`. */
            url: string;
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
    getCommunityPostMedia: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
                media_id: string;
            };
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["PostMediaRequest"];
            };
        };
        responses: {
            /** @description Image bytes */
            200: {
                headers: {
                    /** @description Attachment filename */
                    "Content-Disposition"?: string;
                    /** @description Number of image bytes */
                    "Content-Length"?: number;
                    /** @description Sandbox policy */
                    "Content-Security-Policy"?: string;
                    /** @description Stored file entity tag */
                    ETag?: string;
                    /** @description nosniff */
                    "X-Content-Type-Options"?: string;
                    /** @description Core file ID */
                    "X-Miy-File-Id"?: string;
                    /** @description SHA-256 digest */
                    "X-Miy-File-Sha256"?: string;
                    /** @description Core file version */
                    "X-Miy-File-Version"?: string;
                    [name: string]: unknown;
                };
                content: {
                    "image/png": string;
                    "image/jpeg": string;
                    "image/gif": string;
                    "image/webp": string;
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request body exceeds the size limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    getBoardMedia: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                media_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Image bytes */
            200: {
                headers: {
                    /** @description Attachment filename */
                    "Content-Disposition"?: string;
                    /** @description Number of image bytes */
                    "Content-Length"?: number;
                    /** @description Sandbox policy */
                    "Content-Security-Policy"?: string;
                    /** @description Stored file entity tag */
                    ETag?: string;
                    /** @description nosniff */
                    "X-Content-Type-Options"?: string;
                    /** @description Core file ID */
                    "X-Miy-File-Id"?: string;
                    /** @description SHA-256 digest */
                    "X-Miy-File-Sha256"?: string;
                    /** @description Core file version */
                    "X-Miy-File-Version"?: string;
                    [name: string]: unknown;
                };
                content: {
                    "image/png": string;
                    "image/jpeg": string;
                    "image/gif": string;
                    "image/webp": string;
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    uploadBoardMedia: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                media_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": string;
            };
        };
        responses: {
            /** @description Image stored; status is ready */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    getBoardHomeCard: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["HomeCardRequest"] | null;
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HomeCard"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request body exceeds the size limit */
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
            /** @description A required service is unavailable */
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
    checkBoardNotifications: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CheckResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    listCompanyAnnouncements: {
        parameters: {
            query: {
                /** @description Supply company for the company-wide route */
                scope: components["schemas"]["CompanyAnnouncementScope"];
                /** @description Page size (default 20) */
                limit?: number;
                /** @description Results to skip (default 0) */
                offset?: number;
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
                    "application/json": components["schemas"]["AnnouncementsResponse"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
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
    createCompanyAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateCompanyAnnouncementRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
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
    getCompanyAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                announcement_id: string;
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
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Requested resource was not found */
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
    deleteCompanyAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                announcement_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    updateCompanyAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                announcement_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    listBoardManagers: {
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
                    "application/json": components["schemas"]["ManagersResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
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
        };
    };
    addBoardManager: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ManagerAddRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
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
    bootstrapBoardManager: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BoardMe"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Current state conflicts with the request */
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
    removeBoardManager: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                user_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    getBoardMe: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Board access for the current user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BoardMe"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
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
    listRecentBoardNotices: {
        parameters: {
            query?: {
                /** @description Number of notices to return (default 5) */
                limit?: number;
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
                    "application/json": components["schemas"]["RecentNoticesResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    listAdminCommunityChannels: {
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
                    "application/json": components["schemas"]["ChannelsResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
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
        };
    };
    createCommunityChannel: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ChannelRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ChannelOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    deleteCommunityChannel: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                channel_key: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    updateCommunityChannel: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                channel_key: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ChannelRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ChannelOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    listCommunityFilterTerms: {
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
                    "application/json": components["schemas"]["FilterTermList"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
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
        };
    };
    createCommunityFilterTerm: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FilterTermCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FilterTermItem"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    deleteCommunityFilterTerm: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                term_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    listCommunityChannels: {
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
                    "application/json": components["schemas"]["ChannelsResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
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
    listCommunityPosts: {
        parameters: {
            query?: {
                /** @description Page number (default 1) */
                page?: number;
                /** @description Page size (default 10) */
                page_size?: number;
                /** @description Post title, author, or number */
                q?: string;
            };
            header?: never;
            path: {
                channel_key: string;
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
                    "application/json": components["schemas"]["PostListResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Requested resource was not found */
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
    createCommunityPost: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                channel_key: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PostCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PostOut"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
    getCommunityPost: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
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
                    "application/json": components["schemas"]["PostDetail"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Requested resource was not found */
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
    deleteCommunityPost: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    updateCommunityPost: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PostUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PostOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    createCommunityComment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CommentCreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CommentOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    deleteCommunityComment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
                comment_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    updateCommunityComment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
                comment_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CommentUpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CommentOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    markCommunityPostRead: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    unlockCommunityPost: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                post_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UnlockRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PostDetail"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
    reserveBoardMedia: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UploadRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UploadResponse"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Current state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Image exceeds 10 MiB */
            413: {
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    listBoardSpaces: {
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
                    "application/json": components["schemas"]["SpacesResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
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
            /** @description A required service is unavailable */
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
    createBoardSpace: {
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
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceOut"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    deleteBoardSpace: {
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
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    updateBoardSpace: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SpaceOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    listSpaceAnnouncements: {
        parameters: {
            query?: {
                /** @description Select workspace (default) or company notices in this space context */
                scope?: components["schemas"]["AnnouncementScope"];
                /** @description Page size (default 20) */
                limit?: number;
                /** @description Results to skip (default 0) */
                offset?: number;
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
                    "application/json": components["schemas"]["AnnouncementsResponse"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    createSpaceAnnouncement: {
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
                "application/json": components["schemas"]["CreateRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    getSpaceAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                announcement_id: string;
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
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    deleteSpaceAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                announcement_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description No content */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    updateSpaceAnnouncement: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                space_id: string;
                announcement_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnnouncementOut"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    getBoardSpaceSharing: {
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
                    "application/json": components["schemas"]["SharingSnapshot"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
    replaceBoardSpaceSharing: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingSnapshot"];
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Current state conflicts with the request */
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
            /** @description Internal error */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description A required service is unavailable */
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
    getBoardSpaceSharingResult: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SharingSnapshot"] | null;
                };
            };
            /** @description Invalid request for this resource */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Valid gateway identity required */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayIdentityError"];
                };
            };
            /** @description Permission denied */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Requested resource was not found */
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
            /** @description A required service is unavailable */
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
