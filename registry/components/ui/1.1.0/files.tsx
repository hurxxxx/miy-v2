import { useEffect, useMemo, useRef, useState } from "react";
import { Dialog } from "./dialog";
import { Badge, Button, ContentState, Loading } from "./primitives";
import { uiFailureCode } from "./sharing";
import { formatByteSize, requestId } from "./utilities";

export interface FileMetadata {
  id: string;
  version: string | number;
  name: string;
  mime: string;
  size: number;
  sha256: string | null;
  status: "pending" | "ready" | "attached" | "cancelled" | "expired";
}
export interface FileService {
  create: (
    input: { requestId: string; name: string; mime: string; size: number },
    signal: AbortSignal,
  ) => Promise<FileMetadata>;
  upload: (
    id: string,
    file: File,
    options: {
      signal: AbortSignal;
      onProgress?: (progress: { loaded: number; total: number }) => void;
    },
  ) => Promise<FileMetadata>;
  byRequest: (
    requestId: string,
    signal: AbortSignal,
  ) => Promise<FileMetadata | null>;
  get: (id: string, signal: AbortSignal) => Promise<FileMetadata>;
  attach: (
    id: string,
    input: { reference: string; requestId: string },
    signal: AbortSignal,
  ) => Promise<FileMetadata>;
  cancel: (id: string, signal: AbortSignal) => Promise<FileMetadata>;
  download: (id: string, signal: AbortSignal) => Promise<Blob>;
}
export type FileTransferStatus =
  | "queued"
  | "creating"
  | "pending"
  | "uploading"
  | "ready"
  | "attaching"
  | "cancelling"
  | "complete"
  | "cancelled"
  | "error"
  | "unknown";
/** Serializable progress only: no File objects, credentials or download URLs. */
export interface FileTransferState {
  clientId: string;
  requestId: string;
  attachRequestId: string;
  name: string;
  mime: string;
  size: number;
  status: FileTransferStatus;
  progress: number;
  metadata?: FileMetadata;
  message?: string;
}
export interface FileTransferPanelProps {
  service: FileService;
  /** App-owned resource identifier; the service must recheck its current ACL. */
  reference: string;
  title?: string;
  maxFileSize?: number;
  disabled?: boolean;
  initialTransfers?: readonly FileTransferState[];
  onStateChange?: (transfers: readonly FileTransferState[]) => void;
  onDirtyChange?: (dirty: boolean) => void;
}
const FILE_LIMIT = 10 * 1024 * 1024;
const activeStates = new Set<FileTransferStatus>([
  "creating",
  "uploading",
  "attaching",
  "cancelling",
]);
const statusLabels: Record<FileTransferStatus, string> = {
  queued: "대기",
  creating: "파일 준비 중",
  pending: "전송 대기",
  uploading: "전송 중",
  ready: "전송 완료 · 연결 대기",
  attaching: "업무 항목에 연결 중",
  complete: "첨부 완료",
  cancelling: "취소 확인 중",
  cancelled: "취소됨",
  error: "확인 필요",
  unknown: "결과 확인 필요",
};
const rasterTypes = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);
const rasterMime = (mime: string) => mime.toLowerCase().split(";", 1)[0].trim();

function checkMetadata(value: FileMetadata, expectedId?: string): FileMetadata {
  if (
    !value ||
    typeof value.id !== "string" ||
    !value.id ||
    (expectedId && value.id !== expectedId) ||
    typeof value.name !== "string" ||
    typeof value.mime !== "string" ||
    !Number.isSafeInteger(value.size) ||
    value.size < 0 ||
    !["pending", "ready", "attached", "cancelled", "expired"].includes(
      value.status,
    )
  )
    throw new Error("invalid_file_metadata");
  return value;
}
function restored(values: readonly FileTransferState[]): FileTransferState[] {
  return values.map((value) => ({
    ...value,
    status:
      activeStates.has(value.status) || value.status === "queued"
        ? "unknown"
        : value.status,
    message:
      activeStates.has(value.status) || value.status === "queued"
        ? "이전 요청의 결과를 먼저 확인해 주세요."
        : value.message,
  }));
}

/** Small attachments only. The supplied service talks to the owning app, never directly to Core storage. */
export function FileTransferPanel({
  service,
  reference,
  title = "첨부 파일",
  maxFileSize = FILE_LIMIT,
  disabled = false,
  initialTransfers = [],
  onStateChange,
  onDirtyChange,
}: FileTransferPanelProps) {
  const [transfers, setTransfers] = useState<FileTransferState[]>(() =>
    restored(initialTransfers),
  );
  const state = useRef(transfers);
  const files = useRef(new Map<string, File>());
  const controllers = useRef(new Map<string, AbortController>());
  const versions = useRef(new Map<string, number>());
  const mounted = useRef(true);
  const [running, setRunning] = useState(false);
  const runningRef = useRef(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [preview, setPreview] = useState<FileMetadata | null>(null);
  const urls = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const callbacks = useRef({ onStateChange, onDirtyChange });
  callbacks.current = { onStateChange, onDirtyChange };
  const scope = useMemo(() => ({}), [service, reference]);
  const currentScope = useRef(scope);
  currentScope.current = scope;
  const initial = useRef(initialTransfers);
  initial.current = initialTransfers;
  const limit = Number.isFinite(maxFileSize)
    ? Math.min(FILE_LIMIT, Math.max(1, Math.trunc(maxFileSize)))
    : FILE_LIMIT;
  const dirty = transfers.some(
    (value) => !["complete", "cancelled"].includes(value.status),
  );
  const publish = (next: FileTransferState[]) => {
    if (!mounted.current || currentScope.current !== scope) return;
    state.current = next;
    setTransfers(next);
    callbacks.current.onStateChange?.(next.map((item) => ({ ...item })));
  };
  const update = (id: string, patch: Partial<FileTransferState>) =>
    publish(
      state.current.map((item) =>
        item.clientId === id ? { ...item, ...patch } : item,
      ),
    );
  useEffect(() => {
    mounted.current = true;
    const next = restored(initial.current);
    state.current = next;
    setTransfers(next);
    setRunning(false);
    runningRef.current = false;
    setPreview(null);
    setNotice(null);
    return () => {
      mounted.current = false;
      for (const value of controllers.current.values()) value.abort();
      controllers.current.clear();
      files.current.clear();
      versions.current.clear();
      for (const [url, timer] of urls.current) {
        clearTimeout(timer);
        URL.revokeObjectURL(url);
      }
      urls.current.clear();
    };
  }, [scope]);
  useEffect(() => {
    callbacks.current.onDirtyChange?.(dirty);
  }, [dirty]);
  useEffect(
    () => () => {
      callbacks.current.onDirtyChange?.(false);
    },
    [],
  );

  const operation = (id: string) => {
    controllers.current.get(id)?.abort();
    const controller = new AbortController();
    controllers.current.set(id, controller);
    const version = (versions.current.get(id) ?? 0) + 1;
    versions.current.set(id, version);
    return {
      signal: controller.signal,
      current: () =>
        mounted.current &&
        currentScope.current === scope &&
        versions.current.get(id) === version &&
        !controller.signal.aborted,
      end: () => {
        if (controllers.current.get(id) === controller)
          controllers.current.delete(id);
      },
    };
  };
  const showMetadata = (entry: FileTransferState, metadata: FileMetadata) => {
    checkMetadata(metadata, entry.metadata?.id);
    const status: FileTransferStatus =
      metadata.status === "attached"
        ? "complete"
        : metadata.status === "ready"
          ? "ready"
          : metadata.status === "pending"
            ? "pending"
            : metadata.status === "cancelled"
              ? "cancelled"
              : "error";
    update(entry.clientId, {
      metadata,
      status,
      progress: ["ready", "attached"].includes(metadata.status)
        ? 100
        : entry.progress,
      message:
        metadata.status === "expired"
          ? "요청이 만료되었습니다. 새 파일 요청을 만들어 주세요."
          : metadata.status === "pending" && !files.current.has(entry.clientId)
            ? "계속하려면 원래 파일을 다시 선택해 주세요."
            : undefined,
    });
    if (status === "complete" || status === "cancelled")
      files.current.delete(entry.clientId);
  };
  const lookup = async (entry: FileTransferState) => {
    const task = operation(entry.clientId);
    try {
      const metadata = entry.metadata
        ? await service.get(entry.metadata.id, task.signal)
        : await service.byRequest(entry.requestId, task.signal);
      if (!task.current()) return;
      if (metadata) showMetadata(entry, metadata);
      else
        update(entry.clientId, {
          status: "error",
          message:
            "등록된 요청이 아직 없습니다. 명시적으로 다시 시작하거나 나중에 조회할 수 있습니다.",
        });
    } catch {
      if (task.current())
        update(entry.clientId, {
          status: "unknown",
          message: "현재 결과를 조회할 수 없습니다. 요청 기록을 유지합니다.",
        });
    } finally {
      task.end();
    }
  };

  const send = async (entry: FileTransferState) => {
    const task = operation(entry.clientId);
    try {
      let metadata: FileMetadata;
      if (entry.metadata)
        metadata = checkMetadata(
          await service.get(entry.metadata.id, task.signal),
          entry.metadata.id,
        );
      else {
        update(entry.clientId, { status: "creating", message: undefined });
        metadata = checkMetadata(
          await service.create(
            {
              requestId: entry.requestId,
              name: entry.name,
              mime: entry.mime,
              size: entry.size,
            },
            task.signal,
          ),
        );
      }
      if (!task.current()) return;
      update(entry.clientId, { metadata });
      if (
        metadata.status === "cancelled" ||
        metadata.status === "expired" ||
        metadata.status === "attached"
      ) {
        showMetadata(entry, metadata);
        return;
      }
      if (metadata.status === "pending") {
        const file = files.current.get(entry.clientId);
        if (!file) {
          update(entry.clientId, {
            status: "pending",
            message: "계속하려면 원래 파일을 다시 선택해 주세요.",
          });
          return;
        }
        update(entry.clientId, {
          status: "uploading",
          progress: 0,
          message: undefined,
        });
        metadata = checkMetadata(
          await service.upload(metadata.id, file, {
            signal: task.signal,
            onProgress: ({ loaded, total }) => {
              if (task.current() && Number.isFinite(loaded) && total > 0)
                update(entry.clientId, {
                  progress: Math.max(
                    0,
                    Math.min(100, Math.floor((loaded / total) * 100)),
                  ),
                });
            },
          }),
          metadata.id,
        );
        if (!task.current()) return;
        update(entry.clientId, { metadata, progress: 100 });
      }
      if (metadata.status !== "ready") {
        showMetadata(entry, metadata);
        return;
      }
      update(entry.clientId, { status: "attaching", progress: 100, metadata });
      const attached = checkMetadata(
        await service.attach(
          metadata.id,
          { reference, requestId: entry.attachRequestId },
          task.signal,
        ),
        metadata.id,
      );
      if (task.current()) showMetadata(entry, attached);
    } catch (error) {
      if (!task.current()) return;
      const code = uiFailureCode(error);
      update(entry.clientId, {
        status: code === "unknown" ? "unknown" : "error",
        message:
          code === "unknown"
            ? "처리 결과를 확인할 수 없습니다. 자동으로 다시 전송하지 않으니 결과를 조회해 주세요."
            : code === "forbidden"
              ? "현재 파일 작업 권한이 없습니다."
              : code === "conflict"
                ? "파일 상태가 바뀌었습니다. 결과를 조회해 주세요."
                : "파일 처리를 완료하지 못했습니다. 결과를 조회한 뒤 다시 진행해 주세요.",
      });
    } finally {
      task.end();
    }
  };
  const run = async (ids: string[]) => {
    if (runningRef.current || disabled) return;
    runningRef.current = true;
    setRunning(true);
    try {
      for (const id of ids) {
        if (!mounted.current || currentScope.current !== scope) break;
        const entry = state.current.find((value) => value.clientId === id);
        if (entry && !["cancelled", "complete"].includes(entry.status))
          await send(entry);
      }
    } finally {
      if (mounted.current && currentScope.current === scope) {
        runningRef.current = false;
        setRunning(false);
      }
    }
  };
  const cancel = async (entry: FileTransferState) => {
    const task = operation(entry.clientId);
    if (entry.status === "queued" && !entry.metadata) {
      update(entry.clientId, { status: "cancelled", message: undefined });
      files.current.delete(entry.clientId);
      task.end();
      return;
    }
    update(entry.clientId, { status: "cancelling", message: undefined });
    try {
      const metadata =
        entry.metadata ??
        (await service.byRequest(entry.requestId, task.signal));
      if (!task.current()) return;
      if (!metadata) {
        update(entry.clientId, {
          status: "unknown",
          message:
            "아직 취소 결과를 확인할 수 없습니다. 요청 결과를 다시 조회해 주세요.",
        });
        return;
      }
      const cancelled = checkMetadata(
        await service.cancel(metadata.id, task.signal),
        metadata.id,
      );
      if (task.current()) showMetadata({ ...entry, metadata }, cancelled);
    } catch {
      if (task.current())
        update(entry.clientId, {
          status: "unknown",
          message:
            "취소 결과를 확인할 수 없습니다. 파일을 다시 전송하거나 삭제하지 않고 결과를 조회해 주세요.",
        });
    } finally {
      task.end();
    }
  };
  const download = async (entry: FileTransferState) => {
    if (!entry.metadata) return;
    const task = operation(entry.clientId);
    try {
      const blob = await service.download(entry.metadata.id, task.signal);
      if (!task.current()) return;
      if (blob.size > FILE_LIMIT || blob.size !== entry.metadata.size)
        throw new Error("unexpected_file_size");
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download =
        entry.metadata.name
          .split(/[\\/]/)
          .at(-1)
          ?.replace(/[\u0000-\u001f\u007f]/g, "")
          .trim() || "download";
      document.body.append(link);
      link.click();
      link.remove();
      const timer = setTimeout(() => {
        URL.revokeObjectURL(url);
        urls.current.delete(url);
      }, 1000);
      urls.current.set(url, timer);
    } catch {
      if (task.current())
        setNotice(
          "파일을 다운로드할 수 없습니다. 현재 접근 권한을 확인해 주세요.",
        );
    } finally {
      task.end();
    }
  };
  const addFiles = (selected: FileList | null) => {
    if (!selected || disabled) return;
    const added: FileTransferState[] = [];
    let rejected = false;
    for (const file of Array.from(selected)) {
      if (file.size > limit || state.current.length + added.length >= 32) {
        rejected = true;
        continue;
      }
      const clientId = requestId();
      files.current.set(clientId, file);
      added.push({
        clientId,
        requestId: requestId(),
        attachRequestId: requestId(),
        name: file.name,
        mime: file.type || "application/octet-stream",
        size: file.size,
        status: "queued",
        progress: 0,
      });
    }
    publish([...state.current, ...added]);
    setNotice(
      rejected
        ? `파일 하나당 ${formatByteSize(limit)} 이하, 한 화면에서 최대 32개까지 선택할 수 있습니다.`
        : null,
    );
  };
  return (
    <section className="miy-files" aria-label={title}>
      <header className="miy-files__header">
        <h3>{title}</h3>
        <span>파일당 최대 {formatByteSize(limit)}</span>
      </header>
      <div className="miy-files__actions">
        <label className="miy-files__pick">
          파일 선택
          <input
            type="file"
            multiple
            disabled={disabled || transfers.length >= 32}
            onChange={(event) => {
              addFiles(event.target.files);
              event.target.value = "";
            }}
          />
        </label>
        <Button
          variant="primary"
          busy={running}
          disabled={
            disabled || !transfers.some((value) => value.status === "queued")
          }
          onClick={() =>
            void run(
              state.current
                .filter((value) => value.status === "queued")
                .map((value) => value.clientId),
            )
          }
        >
          업로드 시작
        </Button>
      </div>
      {notice ? <p role="status">{notice}</p> : null}
      <ul className="miy-files__list">
        {transfers.map((entry) => (
          <li
            key={entry.clientId}
            className="miy-files__item"
            aria-label={entry.name}
          >
            <div className="miy-files__name">
              <strong>{entry.name}</strong>
              <span>{formatByteSize(entry.size)}</span>
              <Badge
                tone={
                  entry.status === "complete"
                    ? "success"
                    : entry.status === "error" || entry.status === "unknown"
                      ? "warning"
                      : "neutral"
                }
              >
                {statusLabels[entry.status]}
              </Badge>
            </div>
            {entry.status === "uploading" || entry.status === "attaching" ? (
              <progress
                max={100}
                value={entry.progress}
                aria-label={`${entry.name} 전송률`}
              >
                {entry.progress}%
              </progress>
            ) : null}
            {entry.message ? (
              <p className="miy-files__message" role="status">
                {entry.message}
              </p>
            ) : null}
            {entry.status === "pending" &&
            !files.current.has(entry.clientId) ? (
              <label className="miy-files__pick">
                원래 파일 다시 선택
                <input
                  type="file"
                  aria-label={`${entry.name} 다시 선택`}
                  disabled={disabled || running}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (
                      file &&
                      file.name === entry.name &&
                      file.size === entry.size
                    ) {
                      files.current.set(entry.clientId, file);
                      update(entry.clientId, { message: undefined });
                    } else
                      update(entry.clientId, {
                        message:
                          "이름과 크기가 같은 원래 파일을 선택해 주세요.",
                      });
                    event.target.value = "";
                  }}
                />
              </label>
            ) : null}
            <div className="miy-files__actions">
              {!["queued", "complete", "cancelled"].includes(entry.status) &&
              !activeStates.has(entry.status) ? (
                <Button
                  disabled={disabled || running}
                  onClick={() => void lookup(entry)}
                >
                  결과 조회
                </Button>
              ) : null}
              {["pending", "ready", "error"].includes(entry.status) &&
              entry.metadata?.status !== "expired" ? (
                <Button
                  disabled={
                    disabled ||
                    running ||
                    (entry.status === "pending" &&
                      !files.current.has(entry.clientId))
                  }
                  onClick={() => void run([entry.clientId])}
                >
                  {entry.status === "ready"
                    ? "업무 항목에 연결"
                    : "명시적으로 다시 진행"}
                </Button>
              ) : null}
              {!["complete", "cancelled"].includes(entry.status) ? (
                <Button
                  disabled={disabled || entry.status === "cancelling"}
                  onClick={() => void cancel(entry)}
                >
                  취소
                </Button>
              ) : null}
              {entry.metadata &&
              ["ready", "attached"].includes(entry.metadata.status) ? (
                <>
                  <Button
                    disabled={disabled || activeStates.has(entry.status)}
                    onClick={() => void download(entry)}
                  >
                    다운로드
                  </Button>
                  {rasterTypes.has(rasterMime(entry.metadata.mime)) ? (
                    <Button
                      disabled={disabled || activeStates.has(entry.status)}
                      onClick={() => setPreview(entry.metadata!)}
                    >
                      이미지 보기
                    </Button>
                  ) : null}
                </>
              ) : null}
            </div>
          </li>
        ))}
      </ul>
      {!transfers.length ? (
        <p className="miy-files__empty">첨부할 파일을 선택해 주세요.</p>
      ) : null}
      {preview ? (
        <FileImagePreview
          service={service}
          file={preview}
          onClose={() => setPreview(null)}
        />
      ) : null}
    </section>
  );
}

async function validRaster(blob: Blob, mime: string): Promise<boolean> {
  if (!rasterTypes.has(mime) || rasterMime(blob.type) !== mime) return false;
  const head = new Uint8Array(await blob.slice(0, 12).arrayBuffer());
  if (mime === "image/png")
    return (
      head[0] === 137 &&
      head[1] === 80 &&
      head[2] === 78 &&
      head[3] === 71 &&
      head[4] === 13 &&
      head[5] === 10 &&
      head[6] === 26 &&
      head[7] === 10
    );
  if (mime === "image/jpeg")
    return head[0] === 255 && head[1] === 216 && head[2] === 255;
  const text = String.fromCharCode(...head);
  return mime === "image/gif"
    ? text.startsWith("GIF87a") || text.startsWith("GIF89a")
    : text.startsWith("RIFF") && text.slice(8, 12) === "WEBP";
}
export function FileImagePreview({
  service,
  file,
  onClose,
}: {
  service: FileService;
  file: FileMetadata;
  onClose: () => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const [zoom, setZoom] = useState(1);
  useEffect(() => {
    const controller = new AbortController();
    let objectUrl: string | null = null;
    setUrl(null);
    setError(false);
    setZoom(1);
    void Promise.resolve()
      .then(() => service.download(file.id, controller.signal))
      .then(async (blob) => {
        if (
          blob.size > FILE_LIMIT ||
          blob.size !== file.size ||
          !(await validRaster(blob, rasterMime(file.mime)))
        )
          throw new Error("unsupported_preview");
        if (controller.signal.aborted) return;
        objectUrl = URL.createObjectURL(blob);
        setUrl(objectUrl);
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true);
      });
    return () => {
      controller.abort();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [service, file.id, file.version, file.mime, file.size]);
  return (
    <Dialog
      open
      title={file.name}
      size="large"
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      actions={
        <>
          <Button
            disabled={!url}
            aria-label="이미지 축소"
            onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))}
          >
            −
          </Button>
          <Button disabled={!url} onClick={() => setZoom(1)}>
            원래 크기
          </Button>
          <Button
            disabled={!url}
            aria-label="이미지 확대"
            onClick={() => setZoom((value) => Math.min(4, value + 0.25))}
          >
            +
          </Button>
        </>
      }
    >
      {error ? (
        <ContentState
          kind="unavailable"
          title="이미지 미리보기를 제공할 수 없습니다."
          description="PNG, JPEG, GIF, WebP 이미지와 현재 파일 권한을 확인해 주세요."
        />
      ) : url ? (
        <div className="miy-file-preview">
          <img
            src={url}
            alt={file.name}
            style={{ width: `${zoom * 100}%`, maxWidth: "none" }}
            onError={() => setError(true)}
          />
        </div>
      ) : (
        <Loading />
      )}
    </Dialog>
  );
}
