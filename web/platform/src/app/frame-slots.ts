import { request } from "@miy/sdk";

const CLOSED_DOCUMENT_FRAMES = "miy:closed-document-frames";

function pendingFrames(): string[] {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(CLOSED_DOCUMENT_FRAMES) ?? "[]");
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

export function rememberClosedDocumentFrame(id: string) {
  try {
    const pending = new Set([...pendingFrames(), id]);
    sessionStorage.setItem(CLOSED_DOCUMENT_FRAMES, JSON.stringify([...pending].slice(-16)));
  } catch {
    // This browser still receives the keepalive request.
  }
}

export function closeFrame(id: string, csrf: string) {
  return request<void>(`/api/v1/app-frames/${encodeURIComponent(id)}`, csrf, {
    method: "DELETE",
    keepalive: true,
  });
}

/** Release the previous document's slots before opening another frame. */
export async function releaseClosedDocumentFrames(csrf: string) {
  const ids = pendingFrames();
  if (ids.length === 0) return;
  const settled = await Promise.allSettled(ids.map((id) => closeFrame(id, csrf)));
  const released = new Set(ids.filter((_, index) => settled[index].status === "fulfilled"));
  try {
    sessionStorage.setItem(CLOSED_DOCUMENT_FRAMES, JSON.stringify(pendingFrames().filter((id) => !released.has(id))));
  } catch {
    // A repeated close is safe for this portal session.
  }
}
