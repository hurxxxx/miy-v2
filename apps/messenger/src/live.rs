//! Instant updates (COMMON-FEATURES-PLAN M4 "메신저를 즉시 갱신으로 전환").
//!
//! The platform relays the frame's live connection (`runtime.live`) to
//! `GET /api/live` with the person's verified identity. Messenger keeps every
//! read and write on `runtime.api`: the connection only carries small hints.
//!
//! - After a committed write (send, edit, delete, read, membership change,
//!   moderation hide) the server sends `{"type":"conversation.changed",
//!   "conversationId"}` to the conversation's current active participants.
//!   The person whose membership just ended also gets that one hint, so their
//!   list drops the conversation; it names nothing they did not already know.
//! - A client reports the conversation it shows (`{"type":"open",
//!   "conversationId"}`) and that its person is typing (`{"type":"typing",
//!   "conversationId","active"}`). Participation is checked against the
//!   database when the client opens a conversation and again for every typing
//!   hint delivered; typing goes only to other current participants that have
//!   the conversation open.
//!
//! The hub is in-process and keyed by user ID. One app process is assumed: a
//! hint is lost when the process restarts or a client's queue is full, and the
//! client's reconnect refetch and its slow safety refresh cover it.
use crate::{error::ApiError, request::is_dm_route_id};
use axum::{
    Extension,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// Hints waiting for one connection; more are dropped (the client refetches).
const QUEUE: usize = 64;
/// The platform closes a connection silent for 5 minutes.
const HEARTBEAT: Duration = Duration::from_secs(120);
/// One typing hint per conversation and connection at most this often.
const TYPING_INTERVAL: Duration = Duration::from_secs(1);
/// Client messages are tiny JSON objects.
const MAX_CLIENT_MESSAGE: usize = 4 * 1024;

struct Connection {
    tx: mpsc::Sender<String>,
    open: Option<String>,
}

/// Open live connections by user ID.
#[derive(Default)]
pub struct Hub {
    users: Mutex<HashMap<String, HashMap<u64, Connection>>>,
    next: AtomicU64,
}

impl Hub {
    pub fn register(&self, user: &str) -> (u64, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(QUEUE);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut users) = self.users.lock() {
            users
                .entry(user.to_string())
                .or_default()
                .insert(id, Connection { tx, open: None });
        }
        (id, rx)
    }

    pub fn unregister(&self, user: &str, id: u64) {
        if let Ok(mut users) = self.users.lock()
            && let Some(connections) = users.get_mut(user)
        {
            connections.remove(&id);
            if connections.is_empty() {
                users.remove(user);
            }
        }
    }

    fn set_open(&self, user: &str, id: u64, conversation: Option<String>) {
        if let Ok(mut users) = self.users.lock()
            && let Some(connection) = users.get_mut(user).and_then(|c| c.get_mut(&id))
        {
            connection.open = conversation;
        }
    }

    fn deliver(&self, users: &[String], open: Option<&str>, message: &str) -> usize {
        let Ok(map) = self.users.lock() else {
            return 0;
        };
        let mut sent = 0;
        for user in users {
            for connection in map.get(user).into_iter().flat_map(HashMap::values) {
                if open.is_some_and(|id| connection.open.as_deref() != Some(id)) {
                    continue;
                }
                if connection.tx.try_send(message.to_string()).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }

    /// Every connection of `users`.
    pub fn send_users(&self, users: &[String], message: &str) -> usize {
        self.deliver(users, None, message)
    }

    /// Connections of `users` that show `conversation`.
    pub fn send_open(&self, users: &[String], conversation: &str, message: &str) -> usize {
        self.deliver(users, Some(conversation), message)
    }

    #[cfg(test)]
    pub fn connections(&self, user: &str) -> usize {
        self.users
            .lock()
            .map(|users| users.get(user).map_or(0, HashMap::len))
            .unwrap_or(0)
    }
}

static HUB: LazyLock<Hub> = LazyLock::new(Hub::default);

pub fn hub() -> &'static Hub {
    &HUB
}

async fn active_participants(pool: &PgPool, conversation_id: &str) -> Option<Vec<String>> {
    sqlx::query_scalar(
        "SELECT user_id FROM dm_conversation_participants \
         WHERE conversation_id = $1 AND left_at IS NULL",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|_| eprintln!("live_participants_query_failed"))
    .ok()
}

pub fn changed_hint(conversation_id: &str) -> String {
    json!({"type": "conversation.changed", "conversationId": conversation_id}).to_string()
}

/// After a committed write: hint the conversation's current active
/// participants (read now, after the commit) and `departed`, whose
/// membership this write ended.
pub async fn conversation_changed(pool: &PgPool, conversation_id: &str, departed: &[String]) {
    let Some(mut users) = active_participants(pool, conversation_id).await else {
        return;
    };
    users.extend(departed.iter().cloned());
    users.sort();
    users.dedup();
    hub().send_users(&users, &changed_hint(conversation_id));
}

/// A hint for `user`'s own connections only (for example a mute switch made
/// in another frame of the same person).
pub fn user_changed(user: &str, conversation_id: &str) {
    hub().send_users(&[user.to_string()], &changed_hint(conversation_id));
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "open")]
    Open {
        #[serde(rename = "conversationId", default)]
        conversation_id: Option<String>,
    },
    #[serde(rename = "typing")]
    Typing {
        #[serde(rename = "conversationId")]
        conversation_id: String,
        #[serde(default = "default_active")]
        active: bool,
    },
}

fn default_active() -> bool {
    true
}

/// `GET /live`: the WebSocket the platform relays from `runtime.live`.
pub async fn live(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ws: WebSocketUpgrade,
) -> Response {
    // The platform's read-only recheck calls never open live connections.
    if identity.context.platform {
        return ApiError::forbidden(
            "platform_call_required",
            "Platform calls cannot open live connections.",
        )
        .into_response();
    }
    let user = identity.user_id.clone();
    ws.max_message_size(MAX_CLIENT_MESSAGE)
        .on_upgrade(move |socket| serve(socket, pool, user))
}

async fn serve(socket: WebSocket, pool: PgPool, user: String) {
    let (id, mut hints) = hub().register(&user);
    let (mut sink, mut stream) = socket.split();
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    let mut last_typing: HashMap<String, Instant> = HashMap::new();
    loop {
        tokio::select! {
            hint = hints.recv() => {
                let Some(hint) = hint else { break };
                if sink.send(Message::Text(hint.into())).await.is_err() {
                    break;
                }
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    client_message(&pool, &user, id, text.as_str(), &mut last_typing).await;
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            _ = heartbeat.tick() => {
                if sink.send(Message::Text(r#"{"type":"heartbeat"}"#.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    hub().unregister(&user, id);
}

async fn client_message(
    pool: &PgPool,
    user: &str,
    connection: u64,
    text: &str,
    last_typing: &mut HashMap<String, Instant>,
) {
    let Ok(message) = serde_json::from_str::<ClientMessage>(text) else {
        return;
    };
    match message {
        ClientMessage::Open { conversation_id } => {
            // Checked when subscribing: a conversation the person is not in
            // shows nothing.
            let open = match conversation_id.filter(|id| is_dm_route_id(id)) {
                Some(id) => active_participants(pool, &id)
                    .await
                    .filter(|users| users.iter().any(|u| u == user))
                    .map(|_| id),
                None => None,
            };
            hub().set_open(user, connection, open);
        }
        ClientMessage::Typing {
            conversation_id,
            active,
        } => {
            if !is_dm_route_id(&conversation_id) {
                return;
            }
            // Repeated "typing" hints are limited; the end of typing always
            // passes and lets the next start through at once.
            if !typing_allowed(last_typing, &conversation_id, active, Instant::now()) {
                return;
            }
            // Checked again when delivering: the typist and the receivers
            // must be current active participants.
            let Some(users) = active_participants(pool, &conversation_id).await else {
                return;
            };
            if !users.iter().any(|u| u == user) {
                return;
            }
            let others: Vec<String> = users.into_iter().filter(|u| u != user).collect();
            let hint = json!({
                "type": "typing",
                "conversationId": conversation_id,
                "userId": user,
                "active": active,
            })
            .to_string();
            hub().send_open(&others, &conversation_id, &hint);
        }
    }
}

fn typing_allowed(
    last_typing: &mut HashMap<String, Instant>,
    conversation_id: &str,
    active: bool,
    now: Instant,
) -> bool {
    if !active {
        last_typing.remove(conversation_id);
        return true;
    }
    if last_typing
        .get(conversation_id)
        .is_some_and(|at| now.duration_since(*at) < TYPING_INTERVAL)
    {
        return false;
    }
    if last_typing.len() > 64 {
        last_typing.clear();
    }
    last_typing.insert(conversation_id.to_string(), now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_starts_are_limited_but_ends_always_pass() {
        let mut last = HashMap::new();
        let start = Instant::now();
        assert!(typing_allowed(&mut last, "c1", true, start));
        assert!(!typing_allowed(
            &mut last,
            "c1",
            true,
            start + Duration::from_millis(500)
        ));
        assert!(typing_allowed(
            &mut last,
            "c2",
            true,
            start + Duration::from_millis(500)
        ));
        assert!(typing_allowed(
            &mut last,
            "c1",
            false,
            start + Duration::from_millis(600)
        ));
        assert!(typing_allowed(
            &mut last,
            "c1",
            true,
            start + Duration::from_millis(700)
        ));
        assert!(typing_allowed(
            &mut last,
            "c1",
            true,
            start + Duration::from_millis(1800)
        ));
    }

    #[test]
    fn hints_reach_every_connection_of_the_listed_people_only() {
        let hub = Hub::default();
        let (a1, mut rx_a1) = hub.register("a");
        let (_a2, mut rx_a2) = hub.register("a");
        let (_b, mut rx_b) = hub.register("b");
        assert_eq!(hub.send_users(&["a".into()], "x"), 2);
        assert_eq!(rx_a1.try_recv().unwrap(), "x");
        assert_eq!(rx_a2.try_recv().unwrap(), "x");
        assert!(rx_b.try_recv().is_err());
        hub.unregister("a", a1);
        assert_eq!(hub.connections("a"), 1);
    }

    #[test]
    fn open_conversation_filters_typing_hints() {
        let hub = Hub::default();
        let (a, mut rx_a) = hub.register("a");
        let (_b, mut rx_b) = hub.register("b");
        hub.set_open("a", a, Some("c1".into()));
        assert_eq!(hub.send_open(&["a".into(), "b".into()], "c1", "t"), 1);
        assert_eq!(rx_a.try_recv().unwrap(), "t");
        assert!(rx_b.try_recv().is_err());
        hub.set_open("a", a, None);
        assert_eq!(hub.send_open(&["a".into()], "c1", "t"), 0);
    }

    #[test]
    fn a_full_queue_drops_hints_instead_of_blocking() {
        let hub = Hub::default();
        let (_a, _rx) = hub.register("a");
        for _ in 0..QUEUE {
            assert_eq!(hub.send_users(&["a".into()], "x"), 1);
        }
        assert_eq!(hub.send_users(&["a".into()], "x"), 0);
    }

    #[test]
    fn client_messages_parse_with_defaults() {
        assert!(matches!(
            serde_json::from_str::<ClientMessage>(r#"{"type":"typing","conversationId":"c1"}"#),
            Ok(ClientMessage::Typing { active: true, .. })
        ));
        assert!(matches!(
            serde_json::from_str::<ClientMessage>(r#"{"type":"open","conversationId":null}"#),
            Ok(ClientMessage::Open {
                conversation_id: None
            })
        ));
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"other"}"#).is_err());
        assert_eq!(
            changed_hint("c1"),
            r#"{"conversationId":"c1","type":"conversation.changed"}"#
        );
    }
}
