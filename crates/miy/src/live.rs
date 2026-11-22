//! Live connections (M4, docs/APP-FRAMES.md "실시간 연결"). A frame opens a
//! WebSocket on its own origin, proves itself with its frame token in the first
//! message, and the Core relays messages to the app's `/api/live` with the
//! person's signed identity. The Core keeps rechecking the frame session and
//! the person's use of the app and closes the connection when either ends.
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite;

/// Close codes the frame runtime understands.
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub mod close {
    /// The first message was not a valid auth message in time.
    pub const BAD_AUTH: u16 = 4400;
    /// The frame session ended (sign-out, expiry, revoked); relaunch the frame.
    pub const SESSION_ENDED: u16 = 4401;
    /// The person may no longer use the app.
    pub const ACCESS_REVOKED: u16 = 4403;
    /// No message in either direction for the idle limit.
    pub const IDLE: u16 = 4408;
    /// The release changed; the frame reloads.
    pub const RELEASE_CHANGED: u16 = 4409;
    /// Too many live connections for the person, the installation or the server.
    pub const BUDGET: u16 = 4429;
    /// The app did not accept or dropped the connection.
    pub const APP_UNAVAILABLE: u16 = 4502;
}

pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const RECHECK: Duration = Duration::from_secs(30);
pub const PING: Duration = Duration::from_secs(30);
pub const IDLE_LIMIT: Duration = Duration::from_secs(300);

/// Connection budgets and the first-message deadline: the Core's frame listener only.
#[cfg(feature = "server")]
mod budget {
    use std::{
        collections::HashMap,
        sync::{LazyLock, Mutex},
        time::Duration,
    };
    use uuid::Uuid;

    pub const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
    pub(super) const PER_USER: usize = 10;
    const PER_INSTALLATION: usize = 500;
    const PER_SERVER: usize = 2000;

    #[derive(Default)]
    struct Counts {
        users: HashMap<String, usize>,
        installations: HashMap<Uuid, usize>,
        total: usize,
    }
    static COUNTS: LazyLock<Mutex<Counts>> = LazyLock::new(Mutex::default);

    /// One open connection's share of the budgets, returned when dropped.
    pub struct Slot {
        user: String,
        installation: Uuid,
    }

    pub fn acquire(user: &str, installation: Uuid) -> Option<Slot> {
        let mut counts = COUNTS.lock().ok()?;
        let user_count = counts.users.get(user).copied().unwrap_or(0);
        let installation_count = counts
            .installations
            .get(&installation)
            .copied()
            .unwrap_or(0);
        if user_count >= PER_USER
            || installation_count >= PER_INSTALLATION
            || counts.total >= PER_SERVER
        {
            return None;
        }
        *counts.users.entry(user.to_owned()).or_default() += 1;
        *counts.installations.entry(installation).or_default() += 1;
        counts.total += 1;
        Some(Slot {
            user: user.to_owned(),
            installation,
        })
    }

    impl Drop for Slot {
        fn drop(&mut self) {
            if let Ok(mut counts) = COUNTS.lock() {
                if let Some(count) = counts.users.get_mut(&self.user) {
                    *count -= 1;
                    if *count == 0 {
                        counts.users.remove(&self.user);
                    }
                }
                if let Some(count) = counts.installations.get_mut(&self.installation) {
                    *count -= 1;
                    if *count == 0 {
                        counts.installations.remove(&self.installation);
                    }
                }
                counts.total = counts.total.saturating_sub(1);
            }
        }
    }
}
#[cfg(feature = "server")]
pub use budget::{AUTH_TIMEOUT, acquire};

pub async fn close_with(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}

/// Why a recheck ended the connection.
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub enum Ended {
    Session,
    Access,
    Release,
}

/// Relay between the frame and the app until one side closes, the idle limit
/// passes or `recheck` ends the connection.
pub async fn relay<F, Fut>(
    mut client: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    recheck: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), Ended>>,
{
    let (mut app_tx, mut app_rx) = upstream.split();
    let mut recheck_tick = tokio::time::interval(RECHECK);
    recheck_tick.tick().await;
    let mut ping_tick = tokio::time::interval(PING);
    ping_tick.tick().await;
    let mut last = Instant::now();
    loop {
        tokio::select! {
            message = client.recv() => {
                let Some(Ok(message)) = message else {
                    let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                    return;
                };
                let forwarded = match message {
                    Message::Text(text) => tungstenite::Message::Text(text.as_str().into()),
                    Message::Binary(bytes) => tungstenite::Message::Binary(bytes),
                    Message::Close(_) => {
                        let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                        return;
                    }
                    Message::Ping(_) | Message::Pong(_) => continue,
                };
                last = Instant::now();
                if app_tx.send(forwarded).await.is_err() {
                    close_with(&mut client, close::APP_UNAVAILABLE, "app_unavailable").await;
                    return;
                }
            }
            message = app_rx.next() => {
                let forwarded = match message {
                    Some(Ok(tungstenite::Message::Text(text))) => Message::Text(text.as_str().into()),
                    Some(Ok(tungstenite::Message::Binary(bytes))) => Message::Binary(bytes),
                    Some(Ok(tungstenite::Message::Ping(_) | tungstenite::Message::Pong(_) | tungstenite::Message::Frame(_))) => continue,
                    Some(Ok(tungstenite::Message::Close(_))) => {
                        close_with(&mut client, 1000, "app_closed").await;
                        return;
                    }
                    Some(Err(_)) | None => {
                        close_with(&mut client, close::APP_UNAVAILABLE, "app_unavailable").await;
                        return;
                    }
                };
                last = Instant::now();
                if client.send(forwarded).await.is_err() {
                    let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                    return;
                }
            }
            _ = recheck_tick.tick() => {
                if let Err(ended) = recheck().await {
                    let (code, reason) = match ended {
                        Ended::Session => (close::SESSION_ENDED, "session_ended"),
                        Ended::Access => (close::ACCESS_REVOKED, "access_revoked"),
                        Ended::Release => (close::RELEASE_CHANGED, "release_changed"),
                    };
                    close_with(&mut client, code, reason).await;
                    let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                    return;
                }
                if last.elapsed() >= IDLE_LIMIT {
                    close_with(&mut client, close::IDLE, "idle").await;
                    let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                    return;
                }
            }
            _ = ping_tick.tick() => {
                if client.send(Message::Ping(Vec::new().into())).await.is_err() {
                    let _ = app_tx.send(tungstenite::Message::Close(None)).await;
                    return;
                }
            }
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::budget::*;
    use uuid::Uuid;

    #[test]
    fn budgets_count_per_person_and_return_slots() {
        let installation = Uuid::new_v4();
        let user = format!("live-test-{}", Uuid::new_v4());
        let slots: Vec<Slot> = (0..PER_USER)
            .filter_map(|_| acquire(&user, installation))
            .collect();
        assert_eq!(slots.len(), PER_USER);
        assert!(acquire(&user, installation).is_none());
        assert!(acquire(&format!("{user}-other"), installation).is_some());
        drop(slots);
        assert!(acquire(&user, installation).is_some());
    }
}
