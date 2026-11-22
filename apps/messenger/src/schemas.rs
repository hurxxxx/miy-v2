//! Response and request shapes of AI-DO domains/dm/schemas.py and
//! domains/ugc_policy/schemas.py, field for field. Datetimes are the naive
//! UTC ISO strings Pydantic produced. miy additions are marked.
use serde::{Deserialize, Serialize};

// Schema enums describe string values validated by the messenger domain.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationType {
    Direct,
    Group,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum ParticipantRole {
    Owner,
    Admin,
    Member,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportCategory {
    Harassment,
    Hate,
    Sexual,
    Violence,
    Spam,
    Privacy,
    Other,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum ReportReceiptStatus {
    Received,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct DmUserItem {
    pub id: String,
    /// miy: the directory has no e-mail; always "".
    pub email: String,
    pub full_name: String,
    #[schema(required)]
    pub display_name: Option<String>,
    #[schema(required)]
    pub job_title: Option<String>,
    #[schema(required)]
    pub primary_org_unit_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmConversationParticipantItem {
    pub user: DmUserItem,
    #[schema(value_type = ParticipantRole)]
    pub role: String,
    pub joined_at: String,
    #[schema(required = true)]
    pub left_at: Option<String>,
    #[schema(required = true)]
    pub muted_at: Option<String>,
    #[schema(required = true)]
    pub last_read_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmMessageAttachmentItem {
    pub id: String,
    pub conversation_id: String,
    #[schema(required = true)]
    pub message_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub is_image: bool,
    /// miy: an app file path for `runtime.fileApi` (AI-DO: a signed URL).
    pub download_url: String,
    #[schema(required = true)]
    pub preview_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmAttachmentUrlResponse {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct DmMessageReadStateItem {
    pub unread_count: i64,
    pub read_by_all: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmMessageReplyToItem {
    pub id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub body_preview: String,
    pub attachment_count: i64,
    pub created_at: String,
    pub hidden: bool,
    /// miy: the quoted message was deleted by its author.
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmMessageItem {
    pub id: String,
    pub conversation_id: String,
    pub thread_id: String,
    pub sequence: i32,
    pub sender_id: String,
    pub sender_name: String,
    pub read_state: DmMessageReadStateItem,
    #[schema(required = true)]
    pub reply_to: Option<DmMessageReplyToItem>,
    pub body: String,
    pub attachments: Vec<DmMessageAttachmentItem>,
    pub created_at: String,
    pub hidden: bool,
    /// miy: when the author last edited the body ("수정됨").
    #[schema(required = true)]
    pub edited_at: Option<String>,
    /// miy: deleted by the author; body "" and no attachments.
    pub deleted: bool,
    /// miy: mention ranges (UTF-16 code units of `body`).
    pub mentions: Vec<crate::mentions::Mention>,
    /// miy: reactions in the fixed order, with who reacted.
    pub reactions: Vec<DmReactionItem>,
}

/// miy: one reaction of a message.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmReactionItem {
    #[schema(value_type = crate::reactions::ReactionKind)]
    pub reaction: String,
    pub count: i64,
    /// The caller is among the people who reacted.
    pub reacted: bool,
    pub users: Vec<DmUserItem>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmConversationItem {
    pub id: String,
    #[schema(value_type = ConversationType)]
    pub conversation_type: String,
    #[schema(value_type = ConversationType)]
    pub thread_type: String,
    #[schema(required = true)]
    pub title: Option<String>,
    pub display_name: String,
    #[schema(required = true)]
    pub other_user: Option<DmUserItem>,
    pub participants: Vec<DmConversationParticipantItem>,
    pub participant_count: i64,
    #[schema(required = true)]
    pub last_message: Option<DmMessageItem>,
    pub unread_count: i64,
    #[schema(required = true)]
    pub last_read_message_id: Option<String>,
    #[schema(required = true)]
    pub muted_at: Option<String>,
    pub created_by_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub peer_blocked: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UgcContact {
    #[schema(required = true)]
    pub support_email: Option<String>,
    #[schema(required = true)]
    pub support_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UgcTermsStatus {
    pub version: String,
    pub required: bool,
    pub accepted: bool,
    #[schema(required = true)]
    pub accepted_at: Option<String>,
    #[schema(required = true)]
    pub document_url: Option<String>,
    #[schema(required = true)]
    pub contact: Option<UgcContact>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmSafetyCapability {
    pub version: i32,
    pub report: bool,
    pub block: bool,
    pub report_categories: Vec<&'static str>,
    pub report_note_max_length: usize,
    #[schema(required = true)]
    pub terms: Option<UgcTermsStatus>,
    /// miy: the caller holds the messenger manager role (shows the report
    /// review screen link). Display only; the server rechecks every call.
    pub manager: bool,
    /// Nobody holds the role yet. The claim endpoint still checks atomically.
    pub manager_claimable: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmConversationListResponse {
    pub items: Vec<DmConversationItem>,
    #[schema(required = true)]
    pub safety: Option<DmSafetyCapability>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmMessageListResponse {
    pub items: Vec<DmMessageItem>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmReportResult {
    pub report_id: String,
    #[schema(value_type = ReportReceiptStatus)]
    pub status: &'static str,
    pub duplicate: bool,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmBlockItem {
    pub user: DmUserItem,
    pub blocked_at: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmBlockListResponse {
    pub items: Vec<DmBlockItem>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DmBlockResult {
    pub user_id: String,
    pub blocked: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UgcTermsAcceptResult {
    pub version: String,
    pub accepted_at: String,
    pub changed: bool,
}

// --- requests -------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DmCreateConversationRequest {
    #[serde(default)]
    pub recipient_user_id: Option<String>,
    #[serde(default)]
    pub participant_user_ids: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DmUpdateConversationRequest {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DmAddParticipantsRequest {
    #[serde(default)]
    #[schema(required = true, value_type = Vec<String>, min_items = 1, max_items = 50)]
    pub user_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DmSendMessageRequest {
    /// Provide a nonblank body, an attachment, or both.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    #[schema(max_items = 10)]
    pub attachment_ids: Option<Vec<String>>,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
    /// miy: mention ranges of the trimmed body.
    #[serde(default)]
    pub mentions: Option<Vec<crate::mentions::Mention>>,
}

/// miy: author edit (`PATCH .../messages/{id}`).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DmEditMessageRequest {
    #[serde(default)]
    pub body: Option<String>,
    /// Mention ranges of the new trimmed body (replace the old ones).
    #[serde(default)]
    pub mentions: Option<Vec<crate::mentions::Mention>>,
}

/// miy: mute switch (`PUT .../mute`).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DmMuteRequest {
    pub muted: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DmReportRequest {
    pub client_report_id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub reported_user_id: Option<String>,
    #[schema(value_type = ReportCategory)]
    pub category: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub block: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UgcTermsAcceptRequest {
    pub version: String,
}

/// miy: attachment reservation. AI-DO received the multipart file here; miy
/// reserves platform storage first and the bytes follow through the file
/// gateway (`runtime.fileApi`).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DmAttachmentReserveRequest {
    #[schema(format = "uuid")]
    pub request_id: String,
    pub filename: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[schema(minimum = 1, maximum = 10485760)]
    pub size_bytes: i64,
}
