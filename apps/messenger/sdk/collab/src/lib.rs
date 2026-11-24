//! One app process hosts each active room. The adapter owns resource authorization,
//! JSON projection, history and all database transactions.
pub mod db;
pub mod wire;

use anyhow::{Result, anyhow, ensure};
use async_trait::async_trait;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, broadcast};
use tokio::time::Instant;
use uuid::Uuid;
use wire::packet;
pub use yrs;
use yrs::{
    Doc, ReadTxn, StateVector, Transact, Update,
    updates::{decoder::Decode, encoder::Encode},
};

pub struct Loaded {
    pub generation: Uuid,
    pub bytes: Vec<u8>,
    pub revision: i64,
}
pub struct Save<'a> {
    pub resource: &'a str,
    pub generation: Uuid,
    pub owner: Uuid,
    pub state: Option<&'a [u8]>,
    pub checkpoint: bool,
    pub release: bool,
    pub author: &'a Identity,
}
#[async_trait]
pub trait Store: Clone + Send + Sync + 'static {
    async fn authorize(&self, key: &str, who: &Identity) -> Result<()>;
    async fn name(&self, who: &Identity) -> String {
        let query = miy_app_sdk::DirectoryQuery {
            kind: miy_app_sdk::DirectoryKind::Users,
            ids: Some(vec![who.user_id.clone()]),
            limit: Some(1),
            ..Default::default()
        };
        match tokio::time::timeout(Duration::from_secs(2), who.directory(&query)).await {
            Ok(Ok(page)) => page
                .users
                .into_iter()
                .find(|u| u.id == who.user_id && !u.name.is_empty())
                .map(|u| u.name)
                .unwrap_or_else(|| who.user_id.clone()),
            _ => who.user_id.clone(),
        }
    }
    async fn validate(&self, key: &str, candidate: &[u8]) -> Result<()>;
    async fn load(&self, key: &str, who: &Identity, owner: Uuid) -> Result<Loaded>;
    async fn persist(&self, save: Save<'_>) -> Result<i64>;
    async fn generation(&self, key: &str) -> Result<Option<Uuid>>;
}

pub fn document(bytes: &[u8]) -> Result<Doc> {
    let doc = Doc::new();
    doc.transact_mut().apply_update(Update::decode_v1(bytes)?)?;
    Ok(doc)
}
pub fn snapshot(doc: &Doc) -> Vec<u8> {
    doc.transact()
        .encode_state_as_update_v1(&StateVector::default())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub room_id: String,
    pub generation: Uuid,
    pub revision: i64,
    pub session_id: Uuid,
    pub name: String,
}
struct Permit {
    user: Identity,
    name: String,
    renewed: Instant,
    connected: bool,
}
type Events = broadcast::Sender<Arc<Vec<u8>>>;
struct Room {
    doc: Doc,
    generation: Uuid,
    owner: Uuid,
    revision: i64,
    writer: Identity,
    events: Events,
    permits: HashMap<Uuid, Permit>,
    presence: HashMap<u32, (Uuid, Value)>,
    dirty: bool,
    last_save: Instant,
    last_version: Instant,
    empty_since: Option<Instant>,
    closed: bool,
    // Sequence acknowledged after a successful transaction, including delete-only updates.
    accepted: HashMap<Uuid, u32>,
}
type Slot = Arc<Mutex<Option<Room>>>;
type Rooms = HashMap<String, Slot>;
#[derive(Clone, Default)]
pub struct Hub {
    rooms: Arc<Mutex<Rooms>>,
    draining: Arc<std::sync::atomic::AtomicBool>,
}

impl Hub {
    pub async fn session<S: Store>(
        &self,
        store: S,
        key: String,
        who: Identity,
        resume: Option<Uuid>,
    ) -> Result<Session> {
        store.authorize(&key, &who).await?;
        let name = if resume.is_none() {
            Some(store.name(&who).await)
        } else {
            None
        };
        let mut rooms = self.rooms.lock().await;
        ensure!(
            !self.draining.load(std::sync::atomic::Ordering::Acquire),
            "server_restarting"
        );
        let slot = rooms.entry(key.clone()).or_default().clone();
        // Loading a large resource must not hold the global room map.
        drop(rooms);
        let mut guard = slot.lock().await;
        ensure!(
            !self.draining.load(std::sync::atomic::Ordering::Acquire),
            "server_restarting"
        );
        if guard.is_none() {
            let owner = Uuid::new_v4();
            let loaded = match store.load(&key, &who, owner).await {
                Ok(loaded) => loaded,
                Err(e) => {
                    drop(guard);
                    let mut rooms = self.rooms.lock().await;
                    if Arc::strong_count(&slot) == 2
                        && rooms.get(&key).is_some_and(|s| Arc::ptr_eq(s, &slot))
                    {
                        rooms.remove(&key);
                    }
                    return Err(e);
                }
            };
            let doc = document(&loaded.bytes)?;
            *guard = Some(Room {
                doc,
                generation: loaded.generation,
                owner,
                revision: loaded.revision,
                writer: who.clone(),
                events: broadcast::channel(64).0,
                permits: HashMap::new(),
                presence: HashMap::new(),
                dirty: false,
                last_save: Instant::now(),
                last_version: Instant::now(),
                empty_since: Some(Instant::now()),
                closed: false,
                accepted: HashMap::new(),
            });
            tokio::spawn(
                self.clone()
                    .maintain(store.clone(), key.clone(), slot.clone()),
            );
        }
        let r = guard.as_mut().unwrap();
        ensure!(
            !r.closed && store.generation(&key).await? == Some(r.generation),
            "generation_changed"
        );
        let (id, name) = if let Some(id) = resume {
            let permit = r
                .permits
                .get_mut(&id)
                .ok_or_else(|| anyhow!("session_expired"))?;
            ensure!(permit.user.user_id == who.user_id, "access_revoked");
            permit.renewed = Instant::now();
            permit.user = who;
            (id, permit.name.clone())
        } else {
            let id = Uuid::new_v4();
            let name = name.unwrap_or_else(|| who.user_id.clone());
            r.permits.insert(
                id,
                Permit {
                    user: who,
                    name: name.clone(),
                    renewed: Instant::now(),
                    connected: false,
                },
            );
            (id, name)
        };
        Ok(Session {
            room_id: key,
            generation: r.generation,
            revision: r.revision,
            session_id: id,
            name,
        })
    }

    async fn maintain<S: Store>(self, store: S, key: String, slot: Slot) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut guard = slot.lock().await;
            let Some(r) = guard.as_mut() else { break };
            r.permits
                .retain(|_, p| p.connected || p.renewed.elapsed() < Duration::from_secs(15));
            let release = r
                .empty_since
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(120));
            let checkpoint = r.last_version.elapsed() >= Duration::from_secs(600) || release;
            if !r.closed
                && (r.last_save.elapsed() >= Duration::from_secs(if r.dirty { 2 } else { 10 })
                    || release)
            {
                if let Err(e) = flush(&store, &key, r, checkpoint, release).await {
                    if e.to_string() == "generation_changed" {
                        r.closed = true;
                    } else {
                        let _ = r.events.send(Arc::new(packet(10, b"save_failed")));
                    }
                } else if release {
                    r.closed = true;
                }
            }
            if r.closed {
                let _ = r.events.send(Arc::new(packet(11, &[])));
                break;
            }
        }
        let mut rooms = self.rooms.lock().await;
        if rooms
            .get(&key)
            .is_some_and(|entry| Arc::ptr_eq(entry, &slot))
        {
            rooms.remove(&key);
        }
    }

    pub async fn shutdown<S: Store>(&self, store: &S) -> Result<()> {
        self.draining
            .store(true, std::sync::atomic::Ordering::Release);
        let slots = self.rooms.lock().await.clone();
        let mut result = Ok(());
        for (key, slot) in slots {
            if let Some(r) = slot.lock().await.as_mut()
                && !r.closed
            {
                if let Err(error) = flush(store, &key, r, true, true).await
                    && error.to_string() != "generation_changed"
                {
                    result = Err(error);
                }
                r.closed = true;
                let _ = r.events.send(Arc::new(packet(12, &[])));
            }
        }
        result
    }

    pub async fn serve<S: Store>(&self, store: S, who: Identity, mut ws: WebSocket) {
        let mut reader = wire::Reader::default();
        let Ok(Ok(join)) =
            tokio::time::timeout(Duration::from_secs(5), read_join(&mut ws, &mut reader)).await
        else {
            return;
        };
        let Some(slot) = self.rooms.lock().await.get(&join.resource).cloned() else {
            close(&mut ws, 1011).await;
            return;
        };
        let outcome = participate(&store, &who, &join, &slot, &mut ws, &mut reader).await;
        let code = match outcome {
            Ok(()) => 1000,
            Err(e) if e.to_string() == "access_revoked" => 4410,
            Err(e) if e.to_string() == "generation_changed" => 4411,
            Err(e) if e.to_string() == "invalid_update" => 4412,
            Err(_) => 1011,
        };
        close(&mut ws, code).await;
    }
}
async fn close(ws: &mut WebSocket, code: u16) {
    let _ = ws
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: "".into(),
        })))
        .await;
}

async fn flush<S: Store>(
    store: &S,
    key: &str,
    r: &mut Room,
    checkpoint: bool,
    release: bool,
) -> Result<()> {
    let state = r.dirty.then(|| snapshot(&r.doc));
    let revision = store
        .persist(Save {
            resource: key,
            generation: r.generation,
            owner: r.owner,
            state: state.as_deref(),
            checkpoint,
            release,
            author: &r.writer,
        })
        .await?;
    r.last_save = Instant::now();
    if checkpoint {
        r.last_version = r.last_save;
    }
    r.revision = revision;
    if r.dirty || !r.accepted.is_empty() {
        let ack = serde_json::to_vec(&json!({"revision":revision,"sequences":r.accepted}))?;
        let _ = r.events.send(Arc::new(packet(7, &ack)));
        r.accepted.clear();
        r.dirty = false;
    }
    Ok(())
}

#[derive(Deserialize)]
struct Join {
    resource: String,
    generation: Uuid,
    session: Uuid,
    client: u32,
}
async fn read_join(ws: &mut WebSocket, reader: &mut wire::Reader) -> Result<Join> {
    if let Some(Ok(Message::Binary(frame))) = ws.recv().await {
        ensure!(frame.len() < 2048, "join too large");
        let data = reader
            .push(&frame)?
            .ok_or_else(|| anyhow!("incomplete join"))?;
        ensure!(data.first() == Some(&0), "join required");
        return Ok(serde_json::from_slice(&data[1..])?);
    }
    Err(anyhow!("join required"))
}

async fn participate<S: Store>(
    store: &S,
    who: &Identity,
    join: &Join,
    slot: &Slot,
    ws: &mut WebSocket,
    reader: &mut wire::Reader,
) -> Result<()> {
    store.authorize(&join.resource, who).await?;
    let mut guard = slot.lock().await;
    let r = guard
        .as_mut()
        .ok_or_else(|| anyhow!("generation_changed"))?;
    ensure!(
        r.generation == join.generation
            && store.generation(&join.resource).await? == Some(r.generation),
        "generation_changed"
    );
    ensure!(!r.closed, "session_expired");
    ensure!(!r.presence.contains_key(&join.client), "duplicate client");
    let permit = r
        .permits
        .get_mut(&join.session)
        .ok_or_else(|| anyhow!("session_expired"))?;
    ensure!(
        !permit.connected
            && permit.user.user_id == who.user_id
            && permit.renewed.elapsed() < Duration::from_secs(15),
        "session_expired"
    );
    permit.connected = true;
    r.empty_since = None;
    let mut rx = r.events.subscribe();
    let vector = r.doc.transact().state_vector().encode_v1();
    let peers: Vec<_> = r.presence.values().map(|(_, v)| v.clone()).collect();
    r.presence.insert(
        join.client,
        (join.session, json!({"client":join.client,"state":null})),
    );
    drop(guard);
    let mut explicit = false;
    let result:Result<()>=async {
        wire::send(ws,&packet(3,&vector)).await?;
        for peer in peers {wire::send(ws,&packet(4,&serde_json::to_vec(&peer)?)).await?;}
        let mut tick=tokio::time::interval(Duration::from_secs(1));
        let mut last_check=Instant::now();
        loop {
            tokio::select! {
                result=rx.recv()=>{
                    let message=result?;
                    if message.first()==Some(&12){return Err(anyhow!("server_restarting"));}
                    if message.first()==Some(&11){return Err(anyhow!("generation_changed"));}
                    wire::send(ws,&message).await?;
                }
                _=tick.tick()=>{
                    let guard=slot.lock().await;
                    let room=guard.as_ref().ok_or_else(||anyhow!("generation_changed"))?;
                    ensure!(!room.closed,"generation_changed");
                    let permit=room.permits.get(&join.session).ok_or_else(||anyhow!("access_revoked"))?;
                    ensure!(permit.renewed.elapsed()<Duration::from_secs(15),"session_expired");
                    if last_check.elapsed()>=Duration::from_secs(15){
                        store.authorize(&join.resource,&permit.user).await?;
                        ensure!(store.generation(&join.resource).await?==Some(join.generation),"generation_changed");
                        last_check=Instant::now();
                    }
                }
                raw=ws.recv()=>{
                    let frame=match raw {Some(Ok(Message::Binary(f)))=>f,Some(Ok(Message::Ping(_) | Message::Pong(_)))=>continue,_=>break};
                    let Some(message)=reader.push(&frame)? else {continue};
                    let mut guard=slot.lock().await;
                    let room=guard.as_mut().ok_or_else(||anyhow!("generation_changed"))?;
                    ensure!(!room.closed,"generation_changed");
                    let permit=room.permits.get(&join.session).ok_or_else(||anyhow!("access_revoked"))?;
                    ensure!(permit.renewed.elapsed()<Duration::from_secs(15),"session_expired");
                    match message[0] {
                        1=>{
                            let vector=StateVector::decode_v1(&message[1..])?;
                            let diff=room.doc.transact().encode_state_as_update_v1(&vector);
                            drop(guard);wire::send(ws,&packet(1,&diff)).await?;
                        }
                        2=>{
                            ensure!(message.len()>=6,"short update");
                            store.authorize(&join.resource,&permit.user).await?;
                            ensure!(store.generation(&join.resource).await?==Some(join.generation),"generation_changed");
                            let author=permit.user.clone();
                            let seq=u32::from_be_bytes(message[1..5].try_into()?);
                            let candidate=document(&snapshot(&room.doc))?;
                            candidate.transact_mut().apply_update(Update::decode_v1(&message[5..])?)?;
                            let bytes=snapshot(&candidate);
                            ensure!(bytes.len()+1<=wire::LIMIT,"invalid_update");
                            store.validate(&join.resource,&bytes).await?;
                            room.doc=candidate;
                            room.writer=author;room.dirty=true;room.accepted.insert(join.session,seq);
                            let _=room.events.send(Arc::new(packet(2,&message[5..])));
                        }
                        4=>{
                            ensure!(message.len()<8192,"presence too large");
                            let mut state:Value=serde_json::from_slice(&message[1..])?;
                            ensure!(state.is_object(),"invalid presence");
                            state["user"]=json!({"id":who.user_id,"name":permit.name,"color":format!("#{:06x}",join.client%0x1000000)});
                            let presence=json!({"client":join.client,"state":state});
                            room.presence.insert(join.client,(join.session,presence.clone()));
                            let _=room.events.send(Arc::new(packet(4,&serde_json::to_vec(&presence)?)));
                        }
                        8=>{explicit=true;break;}
                        _=>return Err(anyhow!("invalid message")),
                    }
                }
            }
        }
        Ok(())
    }.await;
    let mut guard = slot.lock().await;
    if let Some(r) = guard.as_mut() {
        r.permits.remove(&join.session);
        r.presence.remove(&join.client);
        let _ = r.events.send(Arc::new(packet(
            4,
            &serde_json::to_vec(&json!({"client":join.client,"state":null}))?,
        )));
        let last = r.presence.is_empty();
        if last {
            r.empty_since = Some(Instant::now());
        }
        if !r.closed && (last || explicit) {
            let checkpoint = explicit && last;
            let release = checkpoint && r.permits.is_empty();
            flush(store, &join.resource, r, checkpoint, release).await?;
            if release {
                r.closed = true;
            }
            if explicit {
                drop(guard);
                wire::send(ws, &packet(8, &[])).await?;
            }
        }
    }
    result
}
