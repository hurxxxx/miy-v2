/** Shared response semantics; credentials remain the responsibility of each host. */
export type ApiErrorKind = "http" | "network" | "parse" | "abort";

export class ApiError extends Error {
  readonly name = "ApiError";
  constructor(
    readonly kind: ApiErrorKind,
    readonly code: string,
    readonly status: number | null = null,
    readonly requestId: string | null = null,
    /** A read may be offered again. This never schedules an automatic retry. */
    readonly retryable = false,
  ) {
    super(code);
  }
}

const isRead = (method?: string) => /^(GET|HEAD)$/i.test(method || "GET");
const safeCode = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z][a-z0-9_]{0,127}$/.test(value);
const safeRequestId = (value: unknown): value is string =>
  typeof value === "string" && /^[a-zA-Z0-9._:-]{1,128}$/.test(value);

/** Preserve Headers/tuple forms and let the browser supply multipart boundaries. */
export function requestHeaders(init: RequestInit = {}): Headers {
  const headers = new Headers(init.headers);
  if (typeof init.body === "string" && !headers.has("Content-Type"))
    headers.set("Content-Type", "application/json");
  return headers;
}

function transferError(error: unknown, init: RequestInit = {}): ApiError {
  if (error instanceof ApiError) return error;
  if (init.signal?.aborted || (error instanceof Error && error.name === "AbortError"))
    return new ApiError("abort", "request_aborted");
  return new ApiError("network", "network_unavailable", null, null, isRead(init.method));
}

/** Wait for shared work without cancelling it when this caller leaves. */
export async function withAbort<T>(operation: Promise<T>, signal?: AbortSignal | null): Promise<T> {
  if (!signal) return operation;
  if (signal.aborted) {
    void operation.catch(() => {});
    throw new ApiError("abort", "request_aborted");
  }
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(new ApiError("abort", "request_aborted"));
    signal.addEventListener("abort", abort, { once: true });
    operation.then(resolve, reject).finally(() => signal.removeEventListener("abort", abort));
  });
}

/** One fetch, with no raw URL, server body or browser exception in the error. */
export async function fetchResponse(path: string, init: RequestInit = {}): Promise<Response> {
  if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
  try {
    return await fetch(path, init);
  } catch (error) {
    throw transferError(error, init);
  }
}

async function httpError(response: Response, init: RequestInit): Promise<ApiError> {
  let payload: { error?: { code?: unknown; requestId?: unknown } } | null = null;
  try {
    payload = await response.json();
  } catch (error) {
    if (init.signal?.aborted) throw transferError(error, init);
    // Proxy HTML and empty errors remain HTTP errors; never echo their content.
  }
  const code = safeCode(payload?.error?.code) ? payload.error.code : `http_${response.status}`;
  const headerId = response.headers.get("x-request-id");
  const requestId = safeRequestId(headerId)
    ? headerId
    : safeRequestId(payload?.error?.requestId) ? payload.error.requestId : null;
  const transient = response.status === 408 || response.status === 429 || response.status >= 500;
  return new ApiError("http", code, response.status, requestId, isRead(init.method) && transient);
}

/** Preserve binary headers/streaming while applying the shared HTTP error contract. */
export async function checkedResponse(response: Response, init: RequestInit = {}): Promise<Response> {
  if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
  if (!response.ok) throw await httpError(response, init);
  return response;
}

export async function readJson<T>(response: Response, init: RequestInit = {}): Promise<T> {
  if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
  if (!response.ok) throw await httpError(response, init);
  let text: string;
  try {
    text = await response.text();
  } catch (error) {
    throw transferError(error, init);
  }
  if (response.status === 204 || response.status === 205 || text.trim() === "")
    return undefined as T;
  try {
    return JSON.parse(text) as T;
  } catch {
    const requestId = response.headers.get("x-request-id");
    throw new ApiError("parse", "invalid_json_response", response.status,
      safeRequestId(requestId) ? requestId : null);
  }
}

export async function readBlob(response: Response, init: RequestInit = {}): Promise<Blob> {
  if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
  if (!response.ok) throw await httpError(response, init);
  try {
    return await response.blob();
  } catch (error) {
    throw transferError(error, init);
  }
}

export async function request<T>(path: string, csrf?: string, init: RequestInit = {}): Promise<T> {
  const headers = requestHeaders(init);
  if (csrf) headers.set("x-miy-csrf", csrf);
  return readJson<T>(await fetchResponse(path, { ...init, credentials: "same-origin", headers }), init);
}

export async function requestBlob(path: string, csrf?: string, init: RequestInit = {}): Promise<Blob> {
  const headers = requestHeaders(init);
  if (csrf) headers.set("x-miy-csrf", csrf);
  return readBlob(await fetchResponse(path, { ...init, credentials: "same-origin", headers }), init);
}

export async function requestResponse(path: string, csrf?: string, init: RequestInit = {}): Promise<Response> {
  const headers = requestHeaders(init);
  if (csrf) headers.set("x-miy-csrf", csrf);
  return checkedResponse(await fetchResponse(path, { ...init, credentials: "same-origin", headers }), init);
}
