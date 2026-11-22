//! Platform notifications. While serving a user request, the backend publishes
//! notifications to people who may use this app, retracts a group when its
//! source changes or someone loses access, and marks the calling person's group
//! read when they read it in the app. Needs the approved
//! `platform:notifications:write`. Publishing never undoes the business write:
//! on an uncertain result, look the request ID up instead of sending again.
use crate::{CallError, Identity, Method, call_error, valid_uuid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    /// A new UUID per notification; the same ID returns the first result.
    pub request_id: String,
    /// miy user IDs (1-500). The platform leaves out people who may not use
    /// this app, disabled accounts and people who turned this kind off.
    pub recipients: Vec<String>,
    /// For example `message` or `mention`.
    pub kind: String,
    /// One line per person and key (for example a conversation ID): a newer
    /// notification replaces the older one.
    pub group_key: String,
    /// Short display text without the message body, e.g. "Alice · 2 new messages".
    pub title: String,
    /// App-relative destination, opened in this app's frame.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    pub request_id: String,
    /// How many people received it. Who was left out is not disclosed.
    pub accepted: u32,
}

#[derive(Deserialize)]
struct Retracted {
    retracted: u64,
}

impl Identity {
    pub async fn notify(&self, notification: &Notification) -> Result<Published, CallError> {
        if !valid_uuid(&notification.request_id) || notification.recipients.is_empty() {
            return Err(call_error("invalid_notification"));
        }
        let body =
            serde_json::to_value(notification).map_err(|_| call_error("invalid_notification"))?;
        let value = self
            .call("platform/notifications", Method::POST, Some(&body))
            .await?;
        serde_json::from_value(value).map_err(|_| call_error("notification_result_unknown"))
    }

    /// The result of an earlier [`Identity::notify`] by its request ID.
    pub async fn notification_by_request(&self, request_id: &str) -> Result<Published, CallError> {
        if !valid_uuid(request_id) {
            return Err(call_error("invalid_notification"));
        }
        let value = self
            .call(
                &format!("platform/notifications/by-request/{request_id}"),
                Method::GET,
                None,
            )
            .await?;
        serde_json::from_value(value).map_err(|_| call_error("notification_result_unknown"))
    }

    /// Remove a group's notifications, for `recipients` only or for everyone.
    pub async fn retract_notifications(
        &self,
        group_key: &str,
        recipients: Option<&[String]>,
    ) -> Result<u64, CallError> {
        let mut body = serde_json::json!({ "groupKey": group_key });
        if let Some(recipients) = recipients {
            body["recipients"] = serde_json::json!(recipients);
        }
        let value = self
            .call("platform/notifications/retract", Method::POST, Some(&body))
            .await?;
        serde_json::from_value::<Retracted>(value)
            .map(|value| value.retracted)
            .map_err(|_| call_error("notification_result_unknown"))
    }

    /// Mark the calling person's notification of `group_key` read.
    pub async fn mark_notifications_read(&self, group_key: &str) -> Result<(), CallError> {
        let body = serde_json::json!({ "groupKey": group_key });
        self.call("platform/notifications/read", Method::POST, Some(&body))
            .await
            .map(|_| ())
    }
}
