// Generated from docs/openapi.json. Run pnpm api:generate.

export interface paths {
    "/v1/calendar/events": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** List the caller's own calendar events. */
        get: operations["listCalendarEvents"];
        put?: never;
        post?: never;
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
        /** Platform callback for the portal home card. */
        post: operations["getPlannerHomeCard"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/personal-widgets/memo": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getPersonalMemo"];
        put: operations["savePersonalMemo"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/personal-widgets/todos": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listPersonalTodos"];
        put?: never;
        post: operations["createPersonalTodo"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/personal-widgets/todos/{todo_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["deletePersonalTodo"];
        options?: never;
        head?: never;
        patch: operations["updatePersonalTodo"];
        trace?: never;
    };
    "/v1/planner/events": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["listPlannerEvents"];
        put?: never;
        post: operations["createPlannerEvent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/v1/planner/events/{event_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["getPlannerEvent"];
        put?: never;
        post?: never;
        delete: operations["deletePlannerEvent"];
        options?: never;
        head?: never;
        patch: operations["updatePlannerEvent"];
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
        /** Platform callback for integrated search. */
        post: operations["searchPlanner"];
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
        ApiErrorDetails: {
            code: string;
            message: string;
        };
        ApiErrorResponse: {
            error: components["schemas"]["ApiErrorDetails"];
        };
        CalendarEventMetadata: {
            assigneeIds: string[] | null;
            /** Format: int64 */
            attendeeCount: number | null;
            externalCalendarId: string | null;
            location: string | null;
            meetingId: string | null;
            ownerId: string | null;
            ownerName: string | null;
            plannerAllDay: boolean | null;
            plannerEndHasTime: boolean | null;
            plannerEventId: string | null;
            plannerStartHasTime: boolean | null;
            plannerTimeZone: string | null;
            status: string | null;
            taskListId: string | null;
            taskListKey: string | null;
            /** Format: int64 */
            taskNumber: number | null;
        };
        CalendarEventOut: {
            allDay: boolean;
            color: string;
            /** @description Calendar end, a date for untimed events or an ISO UTC datetime. */
            end: string;
            id: string;
            metadata: components["schemas"]["CalendarEventMetadata"];
            sourceId: string;
            sourceType: components["schemas"]["CalendarSource"];
            /** @description Calendar start, a date for untimed events or an ISO UTC datetime. */
            start: string;
            title: string;
            workspace: components["schemas"]["CalendarWorkspaceRef"] | null;
        };
        CalendarEventsResponse: {
            items: components["schemas"]["CalendarEventOut"][];
        };
        /** @enum {string} */
        CalendarSource: "planner_event";
        CalendarWorkspaceRef: {
            id: string;
            name: string;
            slug: string;
        };
        EventSearchHit: {
            /** Format: date-time */
            at: string | null;
            kind: string;
            path: string;
            snippet: string | null;
            title: string;
        };
        GatewayErrorDetails: {
            code: string;
        };
        /** @description The SDK gateway returns an error code without a message for invalid identity. */
        GatewayErrorResponse: {
            error: components["schemas"]["GatewayErrorDetails"];
        };
        HomeCardItem: {
            /** Format: date-time */
            at: string | null;
            path: string;
            subtitle: string | null;
            title: string;
        };
        HomeCardRequest: {
            /** @description en-US selects English display text. Other values use Korean. */
            locale?: string | null;
            /** @description IANA time zone for the viewer's local day. */
            timeZone?: string | null;
        };
        HomeCardResponse: {
            /** @description Count before truncating the card to five items. */
            count: number;
            items: components["schemas"]["HomeCardItem"][];
            title: string;
        };
        PersonalMemoOut: {
            body: string;
            createdAt: string | null;
            id: string | null;
            updatedAt: string | null;
        };
        PersonalMemoUpdateRequest: {
            body: string;
            /** @description Pass updatedAt from the last GET unchanged. For a first save, pass null. */
            expectedUpdatedAt: string | null;
        };
        PersonalTodoCreateRequest: {
            /** @description Leading and trailing whitespace is stripped. The result must not be blank. */
            title: string;
        };
        PersonalTodoItemOut: {
            completed: boolean;
            completedAt: string | null;
            createdAt: string;
            id: string;
            /** Format: int32 */
            sortOrder: number;
            title: string;
            updatedAt: string;
        };
        PersonalTodoListResponse: {
            items: components["schemas"]["PersonalTodoItemOut"][];
        };
        /** @description Omitted or null fields leave the corresponding values unchanged. */
        PersonalTodoUpdateRequest: {
            completed?: boolean | null;
            title?: string | null;
        };
        /** @description schemas.PlannerEventCreateRequest (camelCase, snake_case accepted). */
        PlannerEventCreateRequest: {
            /**
             * @description The legacy spelling `all_day` is also accepted.
             * @default false
             */
            allDay?: boolean;
            /** @default  */
            description?: string;
            /**
             * @description End boundary, strictly after the start. An all-day end date is exclusive.
             * @example 2026-11-22T10:00:00+09:00
             */
            end: string;
            /** @default  */
            location?: string;
            /**
             * @description Date only, or an ISO datetime with a T separator for timed events.
             * @example 2026-11-22T09:00:00+09:00
             */
            start: string;
            title: string;
        };
        /** @description schemas.PlannerEventOut. */
        PlannerEventOut: {
            allDay: boolean;
            createdAt: string;
            description: string;
            /** @description Date-only value when the event has no explicit end time, otherwise an ISO datetime. */
            end: string;
            endHasTime: boolean;
            id: string;
            location: string;
            ownerId: string;
            ownerName: string;
            /** @description Date-only value when the event has no explicit start time, otherwise an ISO datetime. */
            start: string;
            startHasTime: boolean;
            timeZone: string;
            title: string;
            updatedAt: string;
        };
        /**
         * @description Omitted or null fields keep their current values. A single supplied time
         *     boundary uses the stored value for the other boundary. When also changing
         *     allDay, supply both boundaries together or omit both.
         */
        PlannerEventUpdateRequest: {
            /** @description The legacy spelling `all_day` is also accepted. */
            allDay?: boolean | null;
            description?: string | null;
            end?: string | null;
            location?: string | null;
            start?: string | null;
            title?: string | null;
        };
        PlannerEventsResponse: {
            items: components["schemas"]["PlannerEventOut"][];
        };
        SearchHit: components["schemas"]["EventSearchHit"] | components["schemas"]["TodoSearchHit"];
        SearchRequest: {
            /**
             * Format: int64
             * @description Defaults to 5. Other values are clamped to the range 1 to 10.
             */
            limit?: number | null;
            locale?: string | null;
            /**
             * @description Leading and trailing whitespace is ignored. Blank queries or queries
             *     longer than 200 characters return no items.
             */
            query: string;
        };
        SearchResponse: {
            items: components["schemas"]["SearchHit"][];
        };
        TodoSearchHit: {
            kind: string;
            path: string;
            snippet: string | null;
            title: string;
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
    listCalendarEvents: {
        parameters: {
            query: {
                /** @description Inclusive lower bound. ISO date or datetime, with or without offset. */
                from: string;
                /** @description Exclusive upper bound, at most 366 days after from. */
                to: string;
                /**
                 * @description Comma-separated sources. This app supports planner_event, the default.
                 *     An empty value selects no sources.
                 * @example planner_event
                 */
                sources?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Calendar events owned by the current user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CalendarEventsResponse"];
                };
            };
            /** @description Invalid date range or sources */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Missing from or to */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not read the calendar */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    getPlannerHomeCard: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description Omit the body to use default display settings */
        requestBody?: {
            content: {
                "application/json": components["schemas"]["HomeCardRequest"];
            };
        };
        responses: {
            /** @description Today's events for the signed-in user */
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
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Platform calls only */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Request body too large */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
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
            /** @description Invalid home card request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Could not read the events */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    getPersonalMemo: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The current user's memo */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PersonalMemoOut"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Could not read the memo */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    savePersonalMemo: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PersonalMemoUpdateRequest"];
            };
        };
        responses: {
            /** @description Saved memo */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PersonalMemoOut"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description The memo revision has changed */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Invalid memo request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not save the memo */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    listPersonalTodos: {
        parameters: {
            query?: {
                /**
                 * @description Defaults to true. Accepted values are true/false, 1/0, yes/no, on/off, t/f and y/n.
                 * @example false
                 */
                include_completed?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The current user's to-dos */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PersonalTodoListResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Invalid include_completed filter */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not read the to-dos */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    createPersonalTodo: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PersonalTodoCreateRequest"];
            };
        };
        responses: {
            /** @description Created to-do */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PersonalTodoItemOut"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Invalid to-do request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not create the to-do */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    deletePersonalTodo: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description To-do ID */
                todo_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description To-do deleted */
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
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description To-do unavailable to the current user */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not delete the to-do */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    updatePersonalTodo: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description To-do ID */
                todo_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PersonalTodoUpdateRequest"];
            };
        };
        responses: {
            /** @description Updated to-do */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PersonalTodoItemOut"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description To-do unavailable to the current user */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Invalid to-do request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not update the to-do */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    listPlannerEvents: {
        parameters: {
            query?: {
                /** @description Inclusive lower bound. ISO date or datetime, with or without offset. */
                from?: string;
                /** @description Exclusive upper bound, at most 366 days after from. Supply both bounds or neither. */
                to?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Events owned by the current user */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlannerEventsResponse"];
                };
            };
            /** @description Invalid date range */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Could not read the events */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    createPlannerEvent: {
        parameters: {
            query?: {
                /**
                 * @description IANA time zone for the event. Missing or unknown values use Asia/Seoul.
                 * @example Asia/Seoul
                 */
                timeZone?: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PlannerEventCreateRequest"];
            };
        };
        responses: {
            /** @description Created event */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlannerEventOut"];
                };
            };
            /** @description Invalid event dates or times */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Invalid event request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not create the event */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    getPlannerEvent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Planner event ID */
                event_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Planner event */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlannerEventOut"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Event unavailable to the current user */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not read the event */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    deletePlannerEvent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Planner event ID */
                event_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Event deleted */
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
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Event unavailable to the current user */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not delete the event */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    updatePlannerEvent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Planner event ID */
                event_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PlannerEventUpdateRequest"];
            };
        };
        responses: {
            /** @description Updated event */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlannerEventOut"];
                };
            };
            /** @description Invalid event dates or times */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Platform identity could not be verified */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Event unavailable to the current user */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Invalid event request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Could not update the event */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
    searchPlanner: {
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
            /** @description Matching events and to-dos for the signed-in user */
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
                    "application/json": components["schemas"]["GatewayErrorResponse"];
                };
            };
            /** @description Platform calls only */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
            /** @description Request body too large */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Unsupported or missing content type */
            415: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Invalid search request */
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
            /** @description Could not search the events and to-dos */
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiErrorResponse"];
                };
            };
        };
    };
}
