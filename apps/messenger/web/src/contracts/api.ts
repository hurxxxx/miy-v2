// AI-DO packages/contracts/src/api.ts derived these types from its generated
// OpenAPI schema (openapi.generated.d.ts). The messenger app has no such
// generator, so the DM schemas are written out here from the same
// components['schemas'] entries of AI-DO 0002af062. miy additions are marked.

type DmUserItem = {
  id: string;
  /** miy: the platform directory has no e-mail; always "". */
  email: string;
  full_name: string;
  display_name?: string | null;
  job_title?: string | null;
  primary_org_unit_name?: string | null;
};

type DmConversationParticipantItem = {
  user: DmUserItem;
  role: 'owner' | 'admin' | 'member';
  joined_at: string;
  left_at?: string | null;
  muted_at?: string | null;
  last_read_message_id?: string | null;
};

type DmMessageAttachmentItem = {
  id: string;
  conversation_id: string;
  message_id?: string | null;
  filename: string;
  content_type: string;
  size_bytes: number;
  is_image: boolean;
  /** miy: an app file path for `runtime.fileApi` (AI-DO: a signed URL). */
  download_url: string;
  preview_url?: string | null;
  created_at: string;
};

type DmMessageReadStateItem = {
  unread_count: number;
  read_by_all: boolean;
};

type DmMessageReplyToItem = {
  id: string;
  sender_id: string;
  sender_name: string;
  body_preview: string;
  attachment_count: number;
  created_at: string;
  hidden?: boolean;
  /** miy: the quoted message was deleted by its author. */
  deleted?: boolean;
};

type DmMessageItem = {
  id: string;
  conversation_id: string;
  thread_id: string;
  sequence: number;
  sender_id: string;
  sender_name: string;
  read_state: DmMessageReadStateItem;
  reply_to?: DmMessageReplyToItem | null;
  body: string;
  attachments?: DmMessageAttachmentItem[];
  created_at: string;
  hidden?: boolean;
  /** miy: last author edit ("수정됨"). */
  edited_at?: string | null;
  /** miy: deleted by the author (placeholder, no body or attachments). */
  deleted?: boolean;
  /** miy: mention ranges (UTF-16 code units of `body`). */
  mentions?: DmMentionItem[];
  /** miy: reactions in the fixed order. */
  reactions?: DmReactionItem[];
};

/** miy: one mention range. */
type DmMentionItem = {
  user_id: string;
  start: number;
  length: number;
};

/** miy: one reaction of a message with who reacted. */
type DmReactionItem = {
  reaction: string;
  count: number;
  reacted: boolean;
  users: DmUserItem[];
};

type UgcContact = {
  support_email?: string | null;
  support_url?: string | null;
};

type UgcTermsStatus = {
  version: string;
  required: boolean;
  accepted: boolean;
  accepted_at?: string | null;
  document_url?: string | null;
  contact?: UgcContact | null;
};

type DmSafetyCapability = {
  version: number;
  report: boolean;
  block: boolean;
  report_categories: string[];
  report_note_max_length: number;
  terms?: UgcTermsStatus | null;
  /** miy: the caller holds the messenger manager role (display only). */
  manager?: boolean;
  manager_claimable?: boolean;
};

type DmConversationItem = {
  id: string;
  conversation_type: 'direct' | 'group';
  thread_type: 'direct' | 'group';
  title?: string | null;
  display_name: string;
  other_user?: DmUserItem | null;
  participants: DmConversationParticipantItem[];
  participant_count: number;
  last_message?: DmMessageItem | null;
  unread_count: number;
  last_read_message_id?: string | null;
  muted_at?: string | null;
  created_by_id: string;
  created_at: string;
  updated_at: string;
  peer_blocked?: boolean;
};

type DmReportCategoryValue =
  | 'harassment'
  | 'hate'
  | 'sexual'
  | 'violence'
  | 'spam'
  | 'privacy'
  | 'other';

type Schemas = {
  DmUserItem: DmUserItem;
  DmConversationParticipantItem: DmConversationParticipantItem;
  DmMessageAttachmentItem: DmMessageAttachmentItem;
  DmMessageReadStateItem: DmMessageReadStateItem;
  DmMessageReplyToItem: DmMessageReplyToItem;
  DmMessageItem: DmMessageItem;
  DmMentionItem: DmMentionItem;
  DmReactionItem: DmReactionItem;
  DmConversationItem: DmConversationItem;
  DmConversationListResponse: {
    items: DmConversationItem[];
    safety?: DmSafetyCapability | null;
  };
  DmMessageListResponse: { items: DmMessageItem[] };
  DmAttachmentUrlResponse: { url: string };
  DmCreateConversationRequest: {
    recipient_user_id?: string | null;
    participant_user_ids?: string[];
    title?: string | null;
  };
  DmUpdateConversationRequest: { title?: string | null };
  DmAddParticipantsRequest: { user_ids: string[] };
  DmSendMessageRequest: {
    body: string;
    attachment_ids: string[];
    reply_to_message_id?: string | null;
  };
  DmSafetyCapability: DmSafetyCapability;
  DmReportRequest: {
    client_report_id: string;
    conversation_id: string;
    message_id?: string | null;
    reported_user_id?: string | null;
    category: DmReportCategoryValue;
    note?: string | null;
    block?: boolean;
  };
  DmReportResult: {
    report_id: string;
    status: 'received';
    duplicate: boolean;
    blocked: boolean;
  };
  DmBlockItem: { user: DmUserItem; blocked_at: string };
  DmBlockListResponse: { items: { user: DmUserItem; blocked_at: string }[] };
  DmBlockResult: { user_id: string; blocked: boolean; changed: boolean };
  UgcTermsStatus: UgcTermsStatus;
  UgcTermsAcceptResult: {
    version: string;
    accepted_at: string;
    changed: boolean;
  };
};

export type ApiSchema<Name extends keyof Schemas> = Schemas[Name];
