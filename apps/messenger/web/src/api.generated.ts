// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/files/dm-safety/reports/{report_id}/evidence/attachments/{attachment_id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /files/dm-safety/reports/{id}/evidence/attachments/{attachment_id}/content`.
         * @description Streams the attachment of a reported message for an authorized moderator.
         */
        get: operations["evidenceAttachmentContent"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/files/dm/attachments/{attachment_id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /files/dm/attachments/{id}/content` (AI-DO's signed `content` URL).
         * @description Streams an authorized attachment with the stored media type.
         */
        get: operations["downloadDmAttachmentContent"];
        /**
         * `PUT /files/dm/attachments/{id}/content`: the bytes of a reservation.
         * @description Streams the reserved bytes for the uploader. Content-Length must match the reservation.
         */
        put: operations["uploadDmAttachmentContent"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/live": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /live`: the WebSocket the platform relays from `runtime.live`.
         * @description HTTP WebSocket handshake for runtime.live. See the Messenger README live protocol for text messages.
         */
        get: operations["live"];
        put?: never;
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
         * `POST /platform/home-card` (manifest `platformEndpoints.homeCard`): the
         *     person's unread conversations for the portal home, newest first, at most
         *     five, without message bodies. Read-only; names come from the app's cache
         *     (this call carries no token for further platform calls).
         * @description Read-only platform callback. Lists unread conversations for the verified caller.
         */
        post: operations["homeCard"];
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
         * `POST /platform/notification-check` (manifest `platformEndpoints`): the
         *     groups that still point at something this person may open. Read-only.
         * @description Read-only platform callback. Confirms notification groups remain accessible.
         */
        post: operations["notificationCheck"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/platform/search": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /platform/search` (manifest `platformEndpoints.search`): the same
         *     rule as `GET /v1/dm/search`, at most `limit` (1-10) items, read-only.
         * @description Read-only platform callback. Searches visible messages of the verified caller.
         */
        post: operations["platformSearch"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/filter-terms": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm-safety/filter-terms`. */
        get: operations["listFilterTerms"];
        put?: never;
        /** `POST /v1/dm-safety/filter-terms`. */
        post: operations["addFilterTerm"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/filter-terms/{term_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/dm-safety/filter-terms/{id}`. */
        delete: operations["removeFilterTerm"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/managers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** miy: `GET /v1/dm-safety/managers`. */
        get: operations["listManagers"];
        put?: never;
        /** miy: `POST /v1/dm-safety/managers`. */
        post: operations["grantManager"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/managers/claim": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** miy: `POST /v1/dm-safety/managers/claim`: only while the role is empty. */
        post: operations["claimManager"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/managers/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** miy: `DELETE /v1/dm-safety/managers/{user_id}`. The last manager stays. */
        delete: operations["revokeManager"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** miy: `GET /v1/dm-safety/me`. */
        get: operations["managerStatus"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/reports": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm-safety/reports`. */
        get: operations["listReports"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/reports/{report_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm-safety/reports/{id}`. */
        get: operations["getReport"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/reports/{report_id}/evidence": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm-safety/reports/{id}/evidence`: the reported message only. */
        get: operations["getEvidence"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/reports/{report_id}/message-visibility": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/dm-safety/reports/{id}/message-visibility`. */
        post: operations["setMessageVisibility"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/reports/{report_id}/resolve": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/dm-safety/reports/{id}/resolve`. */
        post: operations["resolveReport"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm-safety/terms-settings": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** miy: `GET /v1/dm-safety/terms-settings`. */
        get: operations["getTermsSettings"];
        /** miy: `PUT /v1/dm-safety/terms-settings` (AI-DO `AI_DO_UGC_*` settings). */
        put: operations["putTermsSettings"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/attachments/{attachment_id}": {
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
         * miy: `DELETE /v1/dm/attachments/{id}` cancels an unsent attachment the
         *     caller reserved (AI-DO left removed composer files unreferenced).
         */
        delete: operations["cancelDmAttachment"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/attachments/{attachment_id}/download": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/attachments/{id}/download`. */
        get: operations["getDmAttachmentDownloadUrl"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/attachments/{attachment_id}/preview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/attachments/{id}/preview`: images only (415 otherwise). */
        get: operations["getDmAttachmentPreviewUrl"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/blocks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/blocks`. */
        get: operations["listDmBlocks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/blocks/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** `PUT /v1/dm/blocks/{user_id}`. */
        put: operations["blockDmUser"];
        post?: never;
        /** `DELETE /v1/dm/blocks/{user_id}`. */
        delete: operations["unblockDmUser"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/conversations`. */
        get: operations["listDmConversations"];
        put?: never;
        /** `POST /v1/dm/conversations`. */
        post: operations["createDmConversation"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}": {
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
        /** `PATCH /v1/dm/conversations/{id}`: `conversation_settings.update_conversation`. */
        patch: operations["updateDmConversation"];
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/attachments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/dm/conversations/{id}/attachments` (miy: JSON reservation).
         * @description Reserves one file up to 10 MiB. Use the same request_id when a reservation reply is lost.
         */
        post: operations["reserveDmAttachment"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/messages": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * `GET /v1/dm/conversations/{id}/messages`: the newest `limit` messages the
         *     caller may see (joined before), older than `before` when given.
         */
        get: operations["listDmMessages"];
        put?: never;
        /** `POST /v1/dm/conversations/{id}/messages` (`message_delivery.send_message`). */
        post: operations["sendDmMessage"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/messages/{message_id}": {
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
         * miy: `DELETE /v1/dm/conversations/{id}/messages/{message_id}`. Only the
         *     author. The flow keeps a "삭제된 메시지" placeholder; the body is cleared
         *     and the files are released after commit. While a report on the message
         *     is open, the body copy stays in the revision and the files stay attached
         *     until the report is resolved.
         */
        delete: operations["deleteDmMessage"];
        options?: never;
        head?: never;
        /**
         * miy: `PATCH /v1/dm/conversations/{id}/messages/{message_id}`. Only the
         *     author; not a hidden or deleted message. The replaced text stays in
         *     `dm_message_revisions`; the message shows "수정됨" (`edited_at`).
         */
        patch: operations["editDmMessage"];
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/messages/{message_id}/reactions/{reaction}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** `PUT /v1/dm/conversations/{id}/messages/{message_id}/reactions/{reaction}`. */
        put: operations["addReaction"];
        post?: never;
        /** `DELETE /v1/dm/conversations/{id}/messages/{message_id}/reactions/{reaction}`. */
        delete: operations["removeReaction"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/mute": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * miy: `PUT /v1/dm/conversations/{id}/mute` sets the caller's `muted_at`
         *     (AI-DO kept the column for its commute-app relay). Muted conversations
         *     publish no platform notifications to that person.
         */
        put: operations["muteDmConversation"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/participants": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * `POST /v1/dm/conversations/{id}/participants`: any participant of a group
         *     may invite (AI-DO `add_participants`).
         */
        post: operations["addDmConversationParticipants"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/participants/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/dm/conversations/{id}/participants/me`. */
        delete: operations["leaveDmConversation"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/participants/{user_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** `DELETE /v1/dm/conversations/{id}/participants/{user_id}`. */
        delete: operations["removeDmConversationParticipant"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/conversations/{conversation_id}/read": {
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
         * `PATCH /v1/dm/conversations/{id}/read`: read up to the latest visible
         *     message, then mark this conversation's platform notification read.
         */
        patch: operations["markDmConversationRead"];
        trace?: never;
    };
    "/v1/dm/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Messenger profile of the verified caller and their app manager status. */
        get: operations["getMessengerMe"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/reports": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/dm/reports`. */
        post: operations["reportDm"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/search": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/search?q&conversation_id&cursor&limit`. */
        get: operations["searchMessages"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/terms": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/terms`. */
        get: operations["getDmTerms"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/terms/accept": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** `POST /v1/dm/terms/accept`. */
        post: operations["acceptDmTerms"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/threads": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * @deprecated
         * @description Deprecated alias for GET /v1/dm/conversations.
         */
        get: operations["legacyListDmThreads"];
        put?: never;
        /**
         * @deprecated
         * @description Deprecated alias for POST /v1/dm/conversations.
         */
        post: operations["legacyCreateDmThread"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/threads/{conversation_id}/messages": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * @deprecated
         * @description Deprecated alias for GET /v1/dm/conversations/{conversation_id}/messages.
         */
        get: operations["legacyListDmThreadMessages"];
        put?: never;
        /**
         * @deprecated
         * @description Deprecated alias for POST /v1/dm/conversations/{conversation_id}/messages.
         */
        post: operations["legacySendDmThreadMessage"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/dm/threads/{conversation_id}/read": {
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
         * @deprecated
         * @description Deprecated alias for PATCH /v1/dm/conversations/{conversation_id}/read.
         */
        patch: operations["legacyMarkDmThreadRead"];
        trace?: never;
    };
    "/v1/dm/users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** `GET /v1/dm/users`. */
        get: operations["searchDmUsers"];
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
        CheckItem: {
            groupKey: string;
            resourceId?: string | null;
        };
        CheckRequest: {
            items: components["schemas"]["CheckItem"][];
        };
        CheckResponse: {
            allowed: string[];
        };
        /** @enum {string} */
        ConversationType: "direct" | "group";
        DmAddParticipantsRequest: {
            user_ids: string[];
        };
        /**
         * @description miy: attachment reservation. AI-DO received the multipart file here; miy
         *     reserves platform storage first and the bytes follow through the file
         *     gateway (`runtime.fileApi`).
         */
        DmAttachmentReserveRequest: {
            content_type?: string | null;
            filename: string;
            /** Format: uuid */
            request_id: string;
            /** Format: int64 */
            size_bytes: number;
        };
        DmAttachmentUrlResponse: {
            url: string;
        };
        DmBlockItem: {
            blocked_at: string;
            user: components["schemas"]["DmUserItem"];
        };
        DmBlockListResponse: {
            items: components["schemas"]["DmBlockItem"][];
        };
        DmBlockResult: {
            blocked: boolean;
            changed: boolean;
            user_id: string;
        };
        DmConversationItem: {
            conversation_type: components["schemas"]["ConversationType"];
            created_at: string;
            created_by_id: string;
            display_name: string;
            id: string;
            last_message: components["schemas"]["DmMessageItem"] | null;
            last_read_message_id: string | null;
            muted_at: string | null;
            other_user: components["schemas"]["DmUserItem"] | null;
            /** Format: int64 */
            participant_count: number;
            participants: components["schemas"]["DmConversationParticipantItem"][];
            peer_blocked: boolean;
            thread_type: components["schemas"]["ConversationType"];
            title: string | null;
            /** Format: int64 */
            unread_count: number;
            updated_at: string;
        };
        DmConversationListResponse: {
            items: components["schemas"]["DmConversationItem"][];
            safety: components["schemas"]["DmSafetyCapability"] | null;
        };
        DmConversationParticipantItem: {
            joined_at: string;
            last_read_message_id: string | null;
            left_at: string | null;
            muted_at: string | null;
            role: components["schemas"]["ParticipantRole"];
            user: components["schemas"]["DmUserItem"];
        };
        DmCreateConversationRequest: {
            participant_user_ids?: string[] | null;
            recipient_user_id?: string | null;
            title?: string | null;
        };
        /** @description miy: author edit (`PATCH .../messages/{id}`). */
        DmEditMessageRequest: {
            body?: string | null;
            /** @description Mention ranges of the new trimmed body (replace the old ones). */
            mentions?: components["schemas"]["DmMentionItem"][] | null;
        };
        DmMe: {
            manager: boolean;
            user: components["schemas"]["DmUserItem"];
        };
        DmMentionItem: {
            /** Format: int32 */
            length: number;
            /** Format: int32 */
            start: number;
            user_id: string;
        };
        DmMessageAttachmentItem: {
            content_type: string;
            conversation_id: string;
            created_at: string;
            /** @description miy: an app file path for `runtime.fileApi` (AI-DO: a signed URL). */
            download_url: string;
            filename: string;
            id: string;
            is_image: boolean;
            message_id: string | null;
            preview_url: string | null;
            /** Format: int64 */
            size_bytes: number;
        };
        DmMessageItem: {
            attachments: components["schemas"]["DmMessageAttachmentItem"][];
            body: string;
            conversation_id: string;
            created_at: string;
            /** @description miy: deleted by the author; body "" and no attachments. */
            deleted: boolean;
            /** @description miy: when the author last edited the body ("수정됨"). */
            edited_at: string | null;
            hidden: boolean;
            id: string;
            /** @description miy: mention ranges (UTF-16 code units of `body`). */
            mentions: components["schemas"]["DmMentionItem"][];
            /** @description miy: reactions in the fixed order, with who reacted. */
            reactions: components["schemas"]["DmReactionItem"][];
            read_state: components["schemas"]["DmMessageReadStateItem"];
            reply_to: components["schemas"]["DmMessageReplyToItem"] | null;
            sender_id: string;
            sender_name: string;
            /** Format: int32 */
            sequence: number;
            thread_id: string;
        };
        DmMessageListResponse: {
            items: components["schemas"]["DmMessageItem"][];
        };
        DmMessageReadStateItem: {
            read_by_all: boolean;
            /** Format: int64 */
            unread_count: number;
        };
        DmMessageReplyToItem: {
            /** Format: int64 */
            attachment_count: number;
            body_preview: string;
            created_at: string;
            /** @description miy: the quoted message was deleted by its author. */
            deleted: boolean;
            hidden: boolean;
            id: string;
            sender_id: string;
            sender_name: string;
        };
        /** @description miy: mute switch (`PUT .../mute`). */
        DmMuteRequest: {
            muted: boolean;
        };
        /** @description miy: one reaction of a message. */
        DmReactionItem: {
            /** Format: int64 */
            count: number;
            /** @description The caller is among the people who reacted. */
            reacted: boolean;
            reaction: components["schemas"]["ReactionKind"];
            users: components["schemas"]["DmUserItem"][];
        };
        DmReportRequest: {
            block?: boolean;
            category: components["schemas"]["ReportCategory"];
            client_report_id: string;
            conversation_id: string;
            message_id?: string | null;
            note?: string | null;
            reported_user_id?: string | null;
        };
        DmReportResult: {
            blocked: boolean;
            duplicate: boolean;
            report_id: string;
            status: components["schemas"]["ReportReceiptStatus"];
        };
        DmSafetyCapability: {
            block: boolean;
            /**
             * @description miy: the caller holds the messenger manager role (shows the report
             *     review screen link). Display only; the server rechecks every call.
             */
            manager: boolean;
            /** @description Nobody holds the role yet. The claim endpoint still checks atomically. */
            manager_claimable: boolean;
            report: boolean;
            report_categories: string[];
            report_note_max_length: number;
            terms: components["schemas"]["UgcTermsStatus"] | null;
            /** Format: int32 */
            version: number;
        };
        DmSendMessageRequest: {
            attachment_ids?: string[] | null;
            /** @description Provide a nonblank body, an attachment, or both. */
            body?: string | null;
            /** @description miy: mention ranges of the trimmed body. */
            mentions?: components["schemas"]["DmMentionItem"][] | null;
            reply_to_message_id?: string | null;
        };
        DmUpdateConversationRequest: {
            title?: string | null;
        };
        DmUserItem: {
            display_name: string | null;
            /** @description miy: the directory has no e-mail; always "". */
            email: string;
            full_name: string;
            id: string;
            job_title: string | null;
            primary_org_unit_name: string | null;
        };
        ErrorDetail: {
            code: string;
            message: string;
        };
        ErrorResponse: {
            error: components["schemas"]["ErrorDetail"];
        };
        EvidenceAttachment: {
            content_type: string;
            filename: string;
            id: string;
            is_image: boolean;
            /** Format: int64 */
            size_bytes: number;
        };
        EvidenceMessage: {
            attachments: components["schemas"]["EvidenceAttachment"][];
            body: string;
            conversation_id: string;
            created_at: string;
            deleted_at: string | null;
            /** @description miy: edits and deletion (with the texts kept while a report is open). */
            edited_at: string | null;
            hidden: boolean;
            hidden_at: string | null;
            id: string;
            revisions: components["schemas"]["EvidenceRevision"][];
            sender: components["schemas"]["DmUserItem"] | null;
            /** Format: int32 */
            sequence: number;
        };
        EvidenceResponse: {
            message: components["schemas"]["EvidenceMessage"] | null;
            report_id: string;
        };
        EvidenceRevision: {
            action: string;
            body_before: string | null;
            created_at: string;
        };
        FilterTermItem: {
            created_at: string;
            id: string;
            term: string;
        };
        FilterTermRequest: {
            term: string;
        };
        FilterTermsResponse: {
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
            /** @description Time zone supplied by the platform. Messenger uses the locale for labels. */
            timeZone?: string | null;
        };
        HomeCardResponse: {
            /** Format: int64 */
            count: number;
            items: components["schemas"]["HomeCardItem"][];
            title: string;
        };
        ManagerGrantRequest: {
            user_id: string;
        };
        ManagerItem: {
            created_at: string;
            granted_by_id: string | null;
            user: components["schemas"]["DmUserItem"];
        };
        ManagerStatus: {
            /** @description Nobody holds the role yet: the caller may claim it. */
            claimable: boolean;
            manager: boolean;
        };
        ManagersResponse: {
            items: components["schemas"]["ManagerItem"][];
        };
        MessengerLiveClientMessage: {
            conversationId?: string | null;
            /** @enum {string} */
            type: "open";
        } | {
            active?: boolean;
            conversationId: string;
            /** @enum {string} */
            type: "typing";
        };
        MessengerLiveServerHint: {
            conversationId: string;
            /** @enum {string} */
            type: "conversation.changed";
        } | {
            active: boolean;
            conversationId: string;
            /** @enum {string} */
            type: "typing";
            userId: string;
        } | {
            /** @enum {string} */
            type: "heartbeat";
        };
        /** @enum {string} */
        OpenReportStatus: "open";
        /** @enum {string} */
        ParticipantRole: "owner" | "admin" | "member";
        PlatformSearchItem: {
            /** Format: date-time */
            at: string;
            kind: string;
            path: string;
            snippet: string;
            title: string;
        };
        PlatformSearchRequest: {
            /** Format: int64 */
            limit?: number | null;
            locale?: string | null;
            query: string;
        };
        PlatformSearchResponse: {
            items: components["schemas"]["PlatformSearchItem"][];
        };
        /** @enum {string} */
        ReactionKind: "thumbs_up" | "heart" | "laugh" | "surprised" | "sad" | "pray" | "check";
        /** @enum {string} */
        ReportAction: "none" | "hide_message";
        /** @enum {string} */
        ReportCategory: "harassment" | "hate" | "sexual" | "violence" | "spam" | "privacy" | "other";
        ReportDetail: components["schemas"]["ReportSummary"] & {
            conversation_id: string;
            /** @description miy: the author deleted the reported message. */
            message_deleted: boolean | null;
            message_hidden: boolean | null;
            message_id: string | null;
            note: string | null;
            reported_user: components["schemas"]["DmUserItem"] | null;
            reporter: components["schemas"]["DmUserItem"] | null;
            resolution_note: string | null;
            resolved_by: components["schemas"]["DmUserItem"] | null;
        };
        /** @enum {string} */
        ReportOutcome: "actioned" | "dismissed";
        /** @enum {string} */
        ReportReceiptStatus: "received";
        /** @enum {string} */
        ReportStatus: "open" | "actioned" | "dismissed";
        ReportSummary: {
            action: components["schemas"]["ReportAction"] | null;
            category: components["schemas"]["ReportCategory"];
            conversation_type: string;
            created_at: string;
            has_message: boolean;
            id: string;
            resolved_at: string | null;
            source: string;
            status: components["schemas"]["ReportStatus"];
        };
        ReportsResponse: {
            has_more: boolean;
            items: components["schemas"]["ReportSummary"][];
        };
        ResolveRequest: {
            /** @description Dismissing a report requires `none`. */
            action: components["schemas"]["ReportAction"];
            expected_status: components["schemas"]["OpenReportStatus"];
            outcome: components["schemas"]["ReportOutcome"];
            resolution_note?: string | null;
        };
        SearchItem: {
            conversation_id: string;
            conversation_name: string;
            conversation_type: components["schemas"]["ConversationType"];
            created_at: string;
            message_id: string;
            sender_id: string;
            sender_name: string;
            /** Format: int32 */
            sequence: number;
            snippet: string;
        };
        SearchResponse: {
            items: components["schemas"]["SearchItem"][];
            next_cursor: string | null;
        };
        TermsSettingsItem: {
            contact_email: string | null;
            contact_url: string | null;
            document_url: string | null;
            required: boolean;
            version: string;
        };
        TermsSettingsRequest: {
            contact_email?: string | null;
            contact_url?: string | null;
            document_url?: string | null;
            required: boolean;
            version: string;
        };
        UgcContact: {
            support_email: string | null;
            support_url: string | null;
        };
        UgcTermsAcceptRequest: {
            version: string;
        };
        UgcTermsAcceptResult: {
            accepted_at: string;
            changed: boolean;
            version: string;
        };
        UgcTermsStatus: {
            accepted: boolean;
            accepted_at: string | null;
            contact: components["schemas"]["UgcContact"] | null;
            document_url: string | null;
            required: boolean;
            version: string;
        };
        VisibilityRequest: {
            hidden: boolean;
        };
        VisibilityResponse: {
            changed: boolean;
            hidden: boolean;
            message_id: string;
            report_id: string;
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
    evidenceAttachmentContent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                report_id: string;
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Attachment bytes with the stored media type */
            200: {
                headers: {
                    /** @description private, no-store */
                    "Cache-Control"?: string;
                    /** @description Attachment filename, with UTF-8 encoding */
                    "Content-Disposition"?: string;
                    /** @description Number of bytes */
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
                    "*/*": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    downloadDmAttachmentContent: {
        parameters: {
            query?: {
                /** @description Legacy hint. Response uses attachment disposition. */
                disposition?: string;
            };
            header?: never;
            path: {
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Attachment bytes with the stored media type */
            200: {
                headers: {
                    /** @description private, no-store */
                    "Cache-Control"?: string;
                    /** @description Attachment filename, with UTF-8 encoding */
                    "Content-Disposition"?: string;
                    /** @description Number of bytes */
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
                    "*/*": string;
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    uploadDmAttachmentContent: {
        parameters: {
            query?: never;
            header: {
                /** @description Must equal the reserved size */
                "Content-Length": number;
            };
            path: {
                attachment_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "*/*": string;
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmMessageAttachmentItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    live: {
        parameters: {
            query?: never;
            header: {
                /** @description Includes Upgrade */
                Connection: string;
                /** @description websocket */
                Upgrade: string;
                /** @description WebSocket client nonce */
                "Sec-WebSocket-Key": string;
                /** @description 13 */
                "Sec-WebSocket-Version": string;
            };
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Switching Protocols */
            101: {
                headers: {
                    /** @description Upgrade */
                    Connection?: string;
                    /** @description WebSocket handshake acceptance */
                    "Sec-WebSocket-Accept"?: string;
                    /** @description websocket */
                    Upgrade?: string;
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Invalid WebSocket handshake */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Connection cannot be upgraded */
            426: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
        };
    };
    homeCard: {
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
                    "application/json": components["schemas"]["HomeCardResponse"];
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
        };
    };
    notificationCheck: {
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
            /** @description Request validation or domain policy rejected the operation */
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
    platformSearch: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlatformSearchResponse"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    listFilterTerms: {
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
                    "application/json": components["schemas"]["FilterTermsResponse"];
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
    addFilterTerm: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FilterTermRequest"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    removeFilterTerm: {
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
            /** @description Request validation or domain policy rejected the operation */
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
    listManagers: {
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
    grantManager: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ManagerGrantRequest"];
            };
        };
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
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    claimManager: {
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
    revokeManager: {
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
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
    managerStatus: {
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
                    "application/json": components["schemas"]["ManagerStatus"];
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
    listReports: {
        parameters: {
            query?: {
                /** @description Report status (default open) */
                status?: components["schemas"]["ReportStatus"];
                /** @description Reports strictly before this UTC timestamp */
                before?: string;
                /** @description Maximum report count (default 50) */
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
                    "application/json": components["schemas"]["ReportsResponse"];
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
    getReport: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                report_id: string;
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
                    "application/json": components["schemas"]["ReportDetail"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    getEvidence: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                report_id: string;
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
                    "application/json": components["schemas"]["EvidenceResponse"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    setMessageVisibility: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                report_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["VisibilityRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VisibilityResponse"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    resolveReport: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                report_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ResolveRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ReportDetail"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    getTermsSettings: {
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
                    "application/json": components["schemas"]["TermsSettingsItem"];
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
    putTermsSettings: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TermsSettingsRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TermsSettingsItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    cancelDmAttachment: {
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
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
    getDmAttachmentDownloadUrl: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmAttachmentUrlResponse"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    getDmAttachmentPreviewUrl: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmAttachmentUrlResponse"];
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
            /** @description Attachment type cannot be previewed */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
    listDmBlocks: {
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
                    "application/json": components["schemas"]["DmBlockListResponse"];
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
    blockDmUser: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmBlockResult"];
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    unblockDmUser: {
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
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmBlockResult"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    listDmConversations: {
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
                    "application/json": components["schemas"]["DmConversationListResponse"];
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
    createDmConversation: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmCreateConversationRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    updateDmConversation: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmUpdateConversationRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    reserveDmAttachment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmAttachmentReserveRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmMessageAttachmentItem"];
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
            /** @description Attachment exceeds the 10 MiB limit */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    listDmMessages: {
        parameters: {
            query?: {
                /** @description Maximum message count (default 50) */
                limit?: number;
                /** @description Messages strictly before this UTC timestamp. Use one pagination cursor. */
                before?: string;
                /** @description Messages with lower sequence numbers. Use one pagination cursor. */
                before_sequence?: number;
                /** @description Up to 50 comma-separated message IDs, without a pagination cursor. */
                ids?: string;
            };
            header?: never;
            path: {
                conversation_id: string;
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
                    "application/json": components["schemas"]["DmMessageListResponse"];
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
    sendDmMessage: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmSendMessageRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    deleteDmMessage: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
                message_id: string;
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
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    editDmMessage: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
                message_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmEditMessageRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    addReaction: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
                message_id: string;
                reaction: components["schemas"]["ReactionKind"];
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
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    removeReaction: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
                message_id: string;
                reaction: components["schemas"]["ReactionKind"];
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
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    muteDmConversation: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmMuteRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    addDmConversationParticipants: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmAddParticipantsRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    leaveDmConversation: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
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
            /** @description Requested resource was not found */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
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
    removeDmConversationParticipant: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
                user_id: string;
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
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    markDmConversationRead: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
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
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    getMessengerMe: {
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
                    "application/json": components["schemas"]["DmMe"];
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
    reportDm: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmReportRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmReportResult"];
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
            /** @description Current state conflicts with the request */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Request validation or domain policy rejected the operation */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorResponse"];
                };
            };
            /** @description Retry after the reporting quota resets */
            429: {
                headers: {
                    /** @description Seconds to wait */
                    "Retry-After"?: number;
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
    searchMessages: {
        parameters: {
            query: {
                /** @description Message text, 1 to 200 characters after trimming */
                q: string;
                /** @description Filter to a conversation the caller participates in */
                conversation_id?: string;
                /** @description Opaque next_cursor from a previous page */
                cursor?: string;
                /** @description Requested page size (default 20, capped at 50) */
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
                    "application/json": components["schemas"]["SearchResponse"];
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
    getDmTerms: {
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
                    "application/json": components["schemas"]["UgcTermsStatus"];
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
    acceptDmTerms: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UgcTermsAcceptRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UgcTermsAcceptResult"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    legacyListDmThreads: {
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
                    "application/json": components["schemas"]["DmConversationListResponse"];
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
    legacyCreateDmThread: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmCreateConversationRequest"];
            };
        };
        responses: {
            /** @description Created */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
            /** @description A required upstream service failed */
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
    legacyListDmThreadMessages: {
        parameters: {
            query?: {
                /** @description Maximum message count (default 50) */
                limit?: number;
                /** @description Messages strictly before this UTC timestamp. Use one pagination cursor. */
                before?: string;
                /** @description Messages with lower sequence numbers. Use one pagination cursor. */
                before_sequence?: number;
                /** @description Up to 50 comma-separated message IDs, without a pagination cursor. */
                ids?: string;
            };
            header?: never;
            path: {
                conversation_id: string;
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
                    "application/json": components["schemas"]["DmMessageListResponse"];
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Request validation or domain policy rejected the operation */
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
    legacySendDmThreadMessage: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DmSendMessageRequest"];
            };
        };
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmMessageItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    legacyMarkDmThreadRead: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                conversation_id: string;
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
                    "application/json": components["schemas"]["DmConversationItem"];
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
            /** @description Request validation or domain policy rejected the operation */
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
    searchDmUsers: {
        parameters: {
            query?: {
                /** @description Directory name search */
                q?: string;
                /** @description Maximum user count (default 30) */
                limit?: number;
                /** @description Include the signed caller (default false) */
                include_current?: boolean;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Directory matches */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DmUserItem"][];
                };
            };
            /** @description Malformed query parameters */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Invalid search parameters */
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
            /** @description Directory request failed */
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
}
