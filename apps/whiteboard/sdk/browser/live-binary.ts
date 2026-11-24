/** Ordered binary frames on the authenticated miy live connection. */
const FRAME = 64 * 1024;
const HEADER = 8;
const MAX = 16 * 1024 * 1024;

export interface BinaryHandlers {
  onOpen?: () => void;
  onMessage: (data: Uint8Array) => void;
  onClose?: (event: { code: number; retrying: boolean }) => void;
}
export interface BinaryConnection {
  send(data: Uint8Array): void;
  close(): void;
}

export function* binaryFrames(data: Uint8Array) {
  if (data.length > MAX) throw new Error("live_message_too_large");
  let offset = 0;
  do {
    const payload = data.subarray(offset, offset + FRAME - HEADER);
    const frame = new Uint8Array(HEADER + payload.length);
    const header = new DataView(frame.buffer);
    header.setUint32(0, data.length);
    header.setUint32(4, offset);
    frame.set(payload, HEADER);
    yield frame;
    offset += payload.length;
  } while (offset < data.length);
}

export class BinaryReader {
  private chunks: Uint8Array[] = [];
  private received = 0;
  private total = 0;

  read(frame: ArrayBuffer): Uint8Array | undefined {
    if (frame.byteLength < HEADER || frame.byteLength > FRAME) throw new Error("invalid_live_frame");
    const header = new DataView(frame);
    const total = header.getUint32(0);
    const offset = header.getUint32(4);
    const chunk = new Uint8Array(frame, HEADER);
    if (total > MAX || offset !== this.received || (offset && total !== this.total) ||
        chunk.length !== Math.min(total - offset, FRAME - HEADER)) throw new Error("invalid_live_chunk");
    this.total = total;
    this.chunks.push(chunk);
    this.received += chunk.length;
    if (this.received < total) return;
    const result = new Uint8Array(total);
    let pos = 0;
    for (const part of this.chunks) { result.set(part, pos); pos += part.length; }
    this.chunks = [];
    this.received = 0;
    return result;
  }
}

export function connectBinary(
  handlers: BinaryHandlers,
  prepare: () => Promise<{ url: string; auth?: string }>,
  isStopped: () => boolean,
  frameClosed: (code: number) => void,
): BinaryConnection {
  let socket: WebSocket | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;
  let ready = false;
  let delay = 1000;
  const ended = () => closed || isStopped();
  function retry() {
    if (ended()) return;
    timer = setTimeout(() => void open(), delay);
    delay = Math.min(30_000, delay * 2);
  }
  async function open() {
    try {
      const { url, auth } = await prepare();
      if (ended()) return;
      const current = new WebSocket(url);
      const reader = new BinaryReader();
      socket = current;
      ready = false;
      current.binaryType = "arraybuffer";
      current.onopen = () => { if (auth) current.send(auth); };
      current.onmessage = ({ data }) => {
        if (ended() || current !== socket) return;
        if (!ready && data === '{"type":"miy.ready"}') {
          ready = true;
          delay = 1000;
          handlers.onOpen?.();
          return;
        }
        try {
          if (!ready || !(data instanceof ArrayBuffer)) throw new Error("invalid_live_frame");
          const message = reader.read(data);
          if (message) handlers.onMessage(message);
        } catch {
          current.close(4412, "invalid_live_message");
        }
      };
      current.onclose = ({ code }) => {
        ready = false;
        socket = undefined;
        frameClosed(code);
        const retrying = !ended() && ![4400, 4401, 4403, 4409, 4410, 4411, 4412].includes(code);
        handlers.onClose?.({ code, retrying });
        if (retrying) retry();
      };
    } catch {
      if (!ended()) { handlers.onClose?.({ code: 4502, retrying: true }); retry(); }
    }
  }
  void open();
  return {
    send(data) {
      if (!ready || !socket || socket.readyState !== WebSocket.OPEN) throw new Error("live_not_open");
      if (socket.bufferedAmount > MAX) throw new Error("live_busy");
      for (const chunk of binaryFrames(data)) socket.send(chunk);
    },
    close() { closed = true; ready = false; clearTimeout(timer); socket?.close(); },
  };
}
