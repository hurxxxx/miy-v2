//! Response and request shapes of AI-DO domains/dm/schemas.py and
//! domains/ugc_policy/schemas.py, field for field. Datetimes are the naive
//! UTC ISO strings Pydantic produced. miy additions are marked.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DmUserItem {
    pub id: String,
    /// miy: the directory has no e-mail; always "".
    pub email: String,
    pub full_name: String,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub primary_org_unit_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmConversationParticipantItem {
    pub user: DmUserItem,
    pub role: String,
    pub joined_at: String,
    pub left_at: Option<String>,
    pub muted_at: Option<String>,
    pub last_read_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmMessageAttachmentItem {
    pub id: String,
    pub conversation_id: String,
    pub message_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub is_image: bool,
    /// miy: an app file path for `runtime.fileApi` (AI-DO: a signed URL).
    pub download_url: String,
    pub preview_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmAttachmentUrlResponse {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DmMessageReadStateItem {
    pub unread_count: i64,
    pub read_by_all: bool,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct DmMessageItem {
    pub id: String,
    pub conversation_id: String,
    pub thread_id: String,
    pub sequence: i32,
    pub sender_id: String,
    pub sender_name: String,
    pub read_state: DmMessageReadStateItem,
    pub reply_to: Option<DmMessageReplyToItem>,
    pub body: String,
    pub attachments: Vec<DmMessageAttachmentItem>,
    pub created_at: String,
    pub hidden: bool,
    /// miy: when the author last edited the body ("수정됨").
    pub edited_at: Option<String>,
    /// miy: deleted by the author; body "" and no attachments.
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmConversationItem {
    pub id: String,
    pub conversation_type: String,
    pub thread_type: String,
    pub title: Option<String>,
    pub display_name: String,
    pub other_user: Option<DmUserItem>,
    pub participants: Vec<DmConversationParticipantItem>,
    pub participant_count: i64,
    pub last_message: Option<DmMessageItem>,
    pub unread_count: i64,
    pub last_read_message_id: Option<String>,
    pub muted_at: Option<String>,
    pub created_by_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub peer_blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UgcContact {
    pub support_email: Option<String>,
    pub support_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UgcTermsStatus {
    pub version: String,
    pub required: bool,
    pub accepted: bool,
    pub accepted_at: Option<String>,
    pub document_url: Option<String>,
    pub contact: Option<UgcContact>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmSafetyCapability {
    pub version: i32,
    pub report: bool,
    pub block: bool,
    pub report_categories: Vec<&'static str>,
    pub report_note_max_length: usize,
    pub terms: Option<UgcTermsStatus>,
    /// miy: the caller holds the messenger manager role (shows the report
    /// review screen link). Display only; the server rechecks every call.
    pub manager: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmConversationListResponse {
    pub items: Vec<DmConversationItem>,
    pub safety: Option<DmSafetyCapability>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmMessageListResponse {
    pub items: Vec<DmMessageItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmReportResult {
    pub report_id: String,
    pub status: &'static str,
    pub duplicate: bool,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmBlockItem {
    pub user: DmUserItem,
    pub blocked_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmBlockListResponse {
    pub items: Vec<DmBlockItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DmBlockResult {
    pub user_id: String,
    pub blocked: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UgcTermsAcceptResult {
    pub version: String,
    pub accepted_at: String,
    pub changed: bool,
}

// --- requests -------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DmCreateConversationRequest {
    #[serde(default)]
    pub recipient_user_id: Option<String>,
    #[serde(default)]
    pub participant_user_ids: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DmUpdateConversationRequest {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DmAddParticipantsRequest {
    #[serde(default)]
    pub user_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DmSendMessageRequest {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub attachment_ids: Option<Vec<String>>,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

/// miy: author edit (`PATCH .../messages/{id}`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmEditMessageRequest {
    #[serde(default)]
    pub body: Option<String>,
}

/// miy: mute switch (`PUT .../mute`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmMuteRequest {
    pub muted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmReportRequest {
    pub client_report_id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub reported_user_id: Option<String>,
    pub category: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub block: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UgcTermsAcceptRequest {
    pub version: String,
}

/// miy: attachment reservation. AI-DO received the multipart file here; miy
/// reserves platform storage first and the bytes follow through the file
/// gateway (`runtime.fileApi`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DmAttachmentReserveRequest {
    pub request_id: String,
    pub filename: String,
    #[serde(default)]
    pub content_type: Option<String>,
    pub size_bytes: i64,
}
