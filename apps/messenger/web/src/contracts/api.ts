import type { components } from '../api.generated';

type WireSchema = components['schemas'];
type OptionalFields<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

// UI normalizers also accept projections with fields omitted by older releases.
type DmUserItem = OptionalFields<WireSchema['DmUserItem'],
  'display_name' | 'job_title' | 'primary_org_unit_name'>;
type DmConversationParticipantItem = OptionalFields<
  Omit<WireSchema['DmConversationParticipantItem'], 'user'>,
  'left_at' | 'muted_at' | 'last_read_message_id'
> & { user: DmUserItem };
type DmMessageAttachmentItem = OptionalFields<WireSchema['DmMessageAttachmentItem'],
  'message_id' | 'preview_url'>;
type DmMessageReplyToItem = OptionalFields<WireSchema['DmMessageReplyToItem'],
  'hidden' | 'deleted'>;
type DmReactionItem = Omit<WireSchema['DmReactionItem'], 'reaction' | 'users'> & {
  reaction: string;
  users: DmUserItem[];
};
type DmMessageItem = OptionalFields<
  Omit<WireSchema['DmMessageItem'], 'reply_to' | 'attachments' | 'reactions'>,
  'hidden' | 'edited_at' | 'deleted' | 'mentions'
> & {
  reply_to?: DmMessageReplyToItem | null;
  attachments?: DmMessageAttachmentItem[];
  reactions?: DmReactionItem[];
};
type UgcContact = OptionalFields<WireSchema['UgcContact'], 'support_email' | 'support_url'>;
type UgcTermsStatus = OptionalFields<
  Omit<WireSchema['UgcTermsStatus'], 'contact'>,
  'accepted_at' | 'document_url'
> & { contact?: UgcContact | null };
type DmSafetyCapability = OptionalFields<
  Omit<WireSchema['DmSafetyCapability'], 'terms'>,
  'manager' | 'manager_claimable'
> & { terms?: UgcTermsStatus | null };
type DmConversationItem = OptionalFields<
  Omit<WireSchema['DmConversationItem'], 'other_user' | 'participants' | 'last_message'>,
  'title' | 'last_read_message_id' | 'muted_at' | 'peer_blocked'
> & {
  other_user?: DmUserItem | null;
  participants: DmConversationParticipantItem[];
  last_message?: DmMessageItem | null;
};
type DmBlockItem = Omit<WireSchema['DmBlockItem'], 'user'> & { user: DmUserItem };
type ModelOverrides = {
  DmUserItem: DmUserItem;
  DmConversationParticipantItem: DmConversationParticipantItem;
  DmMessageAttachmentItem: DmMessageAttachmentItem;
  DmMessageReplyToItem: DmMessageReplyToItem;
  DmMessageItem: DmMessageItem;
  DmReactionItem: DmReactionItem;
  DmConversationItem: DmConversationItem;
  DmConversationListResponse: Omit<WireSchema['DmConversationListResponse'], 'items' | 'safety'> & {
    items: DmConversationItem[];
    safety?: DmSafetyCapability | null;
  };
  DmMessageListResponse: Omit<WireSchema['DmMessageListResponse'], 'items'> & {
    items: DmMessageItem[];
  };
  DmSendMessageRequest: Omit<WireSchema['DmSendMessageRequest'], 'body' | 'attachment_ids'> & {
    body: NonNullable<WireSchema['DmSendMessageRequest']['body']>;
    attachment_ids: NonNullable<WireSchema['DmSendMessageRequest']['attachment_ids']>;
  };
  DmSafetyCapability: DmSafetyCapability;
  DmBlockItem: DmBlockItem;
  DmBlockListResponse: Omit<WireSchema['DmBlockListResponse'], 'items'> & { items: DmBlockItem[] };
  UgcTermsStatus: UgcTermsStatus;
};

type Schemas = Omit<WireSchema, keyof ModelOverrides> & ModelOverrides;

export type ApiSchema<Name extends keyof Schemas> = Schemas[Name];
