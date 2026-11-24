import * as Y from "yjs";
import { Awareness, applyAwarenessUpdate, removeAwarenessStates } from "y-protocols/awareness";
import * as encoding from "lib0/encoding";
import type { AppRuntime } from "../browser";
import type { BinaryConnection } from "../browser/live-binary";

type Phase = "connecting" | "reconnecting" | "saving" | "saved" | "failed" | "revoked" | "replaced" | "rejected";
export type RoomStatus = { phase: Phase; ready: boolean; dirty: boolean; revision: number };
type Session = { roomId: string; generation: string; sessionId: string; revision: number; name: string };
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const REMOTE = Symbol("miy remote");
function packet(tag: number, body: Uint8Array = new Uint8Array()) {
  const data = new Uint8Array(body.length + 1);
  data[0] = tag;
  data.set(body, 1);
  return data;
}

/** One browser editor. Keep this object during transient transport drops. */
export class EditingRoom {
  readonly doc = new Y.Doc();
  readonly awareness = new Awareness(this.doc);
  private status: RoomStatus = { phase: "connecting", ready: false, dirty: false, revision: 0 };
  private generation?: string;
  private session?: Session;
  private connection?: BinaryConnection;
  private sequence = 0;
  private joined = false;
  private closed = false;
  private renewal?: ReturnType<typeof setInterval>;
  private attempt = 0;
  private retry?: ReturnType<typeof setTimeout>;
  private delay = 500;
  private remoteVector?: Uint8Array;
  private clocks = new Map<number, number>();
  private finish?: { resolve: () => void; reject: () => void };

  constructor(private runtime: AppRuntime, private path: string, private changed: (status: RoomStatus) => void) {
    this.doc.on("update", (update: Uint8Array, origin: unknown) => {
      if (origin === REMOTE) return;
      this.publish({ dirty: true, phase: this.joined ? "saving" : "reconnecting" });
      if (this.joined) {
        try { this.sendUpdate(update); } catch { this.reconnect(); }
      }
    });
    this.awareness.on("update", (_change: unknown, origin: unknown) => {
      if (origin !== REMOTE && this.joined) this.sendPresence();
    });
  }

  private publish(next: Partial<RoomStatus>) {
    if (this.closed) return;
    this.status = { ...this.status, ...next };
    this.changed(this.status);
  }
  private sendUpdate(update: Uint8Array) {
    const message = new Uint8Array(5 + update.length);
    message[0] = 2;
    new DataView(message.buffer).setUint32(1, ++this.sequence);
    message.set(update, 5);
    this.connection?.send(message);
  }
  private sendPresence() {
    try {
      this.connection?.send(packet(4, encoder.encode(JSON.stringify(this.awareness.getLocalState() ?? {}))));
    } catch { this.reconnect(); }
  }
  private request(session?: string) {
    return this.runtime.api<Session>(this.path, { method: "POST",
      headers: { "Content-Type": "application/json" }, body: JSON.stringify({ session }) });
  }
  private disconnect() {
    ++this.attempt;
    clearInterval(this.renewal);
    clearTimeout(this.retry);
    this.connection?.close();
    this.connection = undefined;
    this.joined = false;
    this.finish?.reject();
    this.finish = undefined;
    removeAwarenessStates(this.awareness, [...this.awareness.getStates().keys()].filter(id => id !== this.doc.clientID), REMOTE);
  }
  private stop(phase: Phase) {
    this.disconnect();
    this.publish({ phase });
  }
  private reconnect() {
    if (this.closed) return;
    this.stop("reconnecting");
    this.retry = setTimeout(() => this.start(), this.delay);
    this.delay = Math.min(10000, this.delay * 2);
  }
  start() {
    const attempt = ++this.attempt;
    const stale = () => this.closed || this.attempt !== attempt;
    void this.request().then(session => {
      if (stale()) return;
      if (this.generation && session.generation !== this.generation) { this.stop("replaced"); return; }
      this.generation = session.generation;
      this.session = session;
      this.publish({ revision: session.revision });
      this.awareness.setLocalStateField("user", { name: session.name, color: "#" + (this.doc.clientID % 0x1000000).toString(16).padStart(6, "0") });
      this.connection = this.runtime.liveBinary({
        onOpen: () => {
          if (stale()) return;
          this.connection?.send(packet(0, encoder.encode(JSON.stringify({ resource: session.roomId,
            generation: session.generation, session: session.sessionId, client: this.doc.clientID }))));
        },
        onMessage: data => { if (!stale()) this.receive(data); },
        onClose: ({ code, retrying }) => {
          if (stale()) return;
          if (code === 4410 || code === 4403) this.stop("revoked");
          else if (code === 4411) this.stop("replaced");
          else if (code === 4412) this.stop("rejected");
          else if (retrying) this.reconnect();
          else this.stop("failed");
        },
      });
      this.renewal = setInterval(() => {
        void this.request(session.sessionId).catch(e => {
          if (stale()) return;
          if (e?.status === 403 || e?.status === 404) this.stop("revoked");
          else this.reconnect();
        });
      }, 8000);
    }).catch(e => {
      if (stale()) return;
      if (e?.status === 403 || e?.status === 404) this.stop("revoked");
      else this.reconnect();
    });
  }
  private receive(data: Uint8Array) {
    const body = data.subarray(1);
    switch (data[0]) {
      case 3: {
        this.connection?.send(packet(1, Y.encodeStateVector(this.doc)));
        this.remoteVector = body;
        break;
      }
      case 1:
        Y.applyUpdate(this.doc, body, REMOTE);
        if (this.remoteVector && this.status.dirty) {
          this.sendUpdate(Y.encodeStateAsUpdate(this.doc, this.remoteVector));
        }
        this.remoteVector = undefined;
        this.joined = true;
        this.delay = 500;
        this.publish({ ready: true, phase: this.status.dirty ? "saving" : "saved" });
        this.sendPresence();
        break;
      case 2:
        Y.applyUpdate(this.doc, body, REMOTE);
        break;
      case 4: {
        const {client, state} = JSON.parse(decoder.decode(body));
        if (client === this.doc.clientID) break;
        const clock = Math.max(this.clocks.get(client) ?? 0, this.awareness.meta.get(client)?.clock ?? 0) + 1;
        this.clocks.set(client, clock);
        const out = encoding.createEncoder();
        encoding.writeVarUint(out, 1); encoding.writeVarUint(out, client); encoding.writeVarUint(out, clock);
        encoding.writeVarString(out, JSON.stringify(state));
        applyAwarenessUpdate(this.awareness, encoding.toUint8Array(out), REMOTE);
        break;
      }
      case 7: {
        const ack = JSON.parse(decoder.decode(body));
        const confirmed = this.session && ack.sequences[this.session.sessionId] === this.sequence;
        this.publish({ revision: ack.revision, ...(confirmed ? { dirty: false, phase: "saved" } : {}) });
        break;
      }
      case 8:
        this.publish({ dirty: false, phase: "saved" });
        this.finish?.resolve(); this.finish = undefined;
        this.disconnect();
        break;
      case 10:
        this.publish({ phase: "failed" }); break;
    }
  }

  async end() {
    if (!this.joined || this.finish) throw new Error("room_not_ready");
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => { this.finish = undefined; reject(new Error("save_unconfirmed")); }, 15000);
      this.finish = { resolve: () => { clearTimeout(timer); resolve(); }, reject: () => { clearTimeout(timer); reject(new Error("save_unconfirmed")); } };
      try { this.connection?.send(packet(8)); } catch { this.finish?.reject(); this.finish = undefined; }
    });
  }
  destroy() {
    this.closed = true; this.joined = false;
    this.disconnect();
    this.awareness.destroy(); this.doc.destroy();
  }
}
