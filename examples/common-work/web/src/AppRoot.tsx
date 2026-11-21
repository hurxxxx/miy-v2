import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  readBlob,
  readJson,
  type AppProps,
  type AppRuntime,
} from "@miy/sdk";
import {
  Badge,
  Button,
  ContentState,
  FileTransferPanel,
  Input,
  Panel,
  ShareEditor,
  UiOperationError,
  formatByteSize,
  requestId,
  type ButtonProps,
  type DirectoryProvider,
  type FileMetadata,
  type FileService,
  type FileTransferState,
  type InputProps,
  type ShareProvider,
  type ShareSnapshot,
} from "@miy/ui";

type Client = Pick<AppRuntime, "api" | "fileApi" | "directory">;
type Project = { id: string; title: string };
type Task = { id: string; title: string; completed: boolean };
type Attachment = Pick<FileMetadata, "id" | "name" | "mime" | "size">;
type TaskRequest =
  | { kind: "create"; id: string; title: string }
  | { kind: "complete"; id: string; completed: boolean };
type ProjectRecovery = {
  shareRequest?: string;
  files?: FileTransferState[];
  taskRequest?: TaskRequest;
};
type Recovery = {
  projectRequest?: Project;
  projects: Record<string, ProjectRecovery>;
};
type WireSharing = Omit<ShareSnapshot, "revision" | "entries"> & {
  revision: number;
  entries: Array<{
    subject: { kind: "user" | "group"; id: string; name: string };
    role: string;
    protected: boolean;
  }>;
};
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const layout = { display: "grid", gap: 18 } as const;
const actions = {
  display: "flex",
  gap: 8,
  flexWrap: "wrap",
  alignItems: "center",
} as const;
/** A labelled text input: the common UI has Input, the label is the app's. */
function Field({ label, ...props }: InputProps & { label: string }) {
  return (
    <label style={{ display: "grid", gap: 6 }}>
      <span style={{ fontWeight: 600, fontSize: "var(--ui-text-caption)" }}>
        {label}
      </span>
      <Input {...props} />
    </label>
  );
}
/** While its request runs, an action is disabled and announced as busy. */
function BusyButton({
  busy,
  disabled,
  ...props
}: ButtonProps & { busy: boolean }) {
  return (
    <Button
      {...props}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
    />
  );
}
const Loading = () => (
  <ContentState kind="loading" size="compact" title="불러오는 중…" />
);
const isDenied = (error: unknown) =>
  error instanceof ApiError && (error.status === 401 || error.status === 403);
const rejected = (error: unknown) =>
  error instanceof ApiError &&
  error.kind === "http" &&
  [400, 401, 403, 422].includes(error.status ?? 0);
/**
 * Nothing exists under an ID that only this client generated, so the request
 * never completed. `acl_denied` counts only where the caller would own the
 * resource had it been created.
 */
const absent = (error: unknown, ownedIfCreated = false) =>
  error instanceof ApiError &&
  error.kind === "http" &&
  (error.status === 404 ||
    (ownedIfCreated && error.status === 403 && error.code === "acl_denied"));
const failure = (error: unknown) =>
  isDenied(error)
    ? "현재 이 작업을 수행할 권한이 없습니다."
    : "요청을 완료하지 못했습니다. 잠시 후 다시 조회해 주세요.";

function useControllers() {
  const requests = useRef(new Set<AbortController>());
  useEffect(
    () => () => {
      for (const item of requests.current) item.abort();
      requests.current.clear();
    },
    [],
  );
  return useCallback(() => {
    const controller = new AbortController();
    requests.current.add(controller);
    return {
      signal: controller.signal,
      abort: () => controller.abort(),
      done: () => requests.current.delete(controller),
    };
  }, []);
}
function decodeSharing(value: WireSharing): ShareSnapshot {
  if (!Number.isSafeInteger(value.revision) || value.revision < 0)
    throw new UiOperationError("unknown");
  return {
    ...value,
    revision: String(value.revision),
    entries: value.entries.map((entry) => ({
      subject: entry.subject,
      role: entry.role,
      locked: entry.protected,
    })),
  };
}
function fileError(error: unknown): never {
  if (error instanceof ApiError) {
    if (isDenied(error)) throw new UiOperationError("forbidden");
    if (error.status === 409) throw new UiOperationError("conflict");
    if (error.status === 400) throw new UiOperationError("invalid_input");
    if (error.status === 404) throw new UiOperationError("not_found");
    if (error.status === 410) throw new UiOperationError("expired");
    if (error.status === 413) throw new UiOperationError("too_large");
  }
  throw error;
}
function fileService(client: Client, projectId: string): FileService {
  const base = `/projects/${projectId}/files`;
  const json = async <T,>(
    path: string,
    signal: AbortSignal,
    init?: RequestInit,
  ) => {
    try {
      return await client.api<T>(base + path, { ...init, signal });
    } catch (error) {
      return fileError(error);
    }
  };
  return {
    create: (input, signal) =>
      json<FileMetadata>("", signal, {
        method: "POST",
        body: JSON.stringify(input),
      }),
    get: (id, signal) =>
      json<FileMetadata>(`/${encodeURIComponent(id)}`, signal),
    byRequest: (id, signal) =>
      json<FileMetadata | null>(
        `/by-request/${encodeURIComponent(id)}`,
        signal,
      ),
    attach: (id, input, signal) =>
      json<FileMetadata>(`/${encodeURIComponent(id)}/attach`, signal, {
        method: "POST",
        body: JSON.stringify(input),
      }),
    cancel: (id, signal) =>
      json<FileMetadata>(`/${encodeURIComponent(id)}`, signal, {
        method: "DELETE",
      }),
    upload: async (id, file, { signal, onProgress }) => {
      const init = {
        method: "PUT",
        body: file,
        signal,
        headers: { "content-type": file.type || "application/octet-stream" },
      };
      onProgress?.({ loaded: 0, total: file.size });
      try {
        const result = await readJson<FileMetadata>(
          await client.fileApi(
            `/projects/${projectId}/${encodeURIComponent(id)}/content`,
            init,
          ),
          init,
        );
        // Fetch does not report upload-byte progress. Only confirmed completion is reported.
        onProgress?.({ loaded: file.size, total: file.size });
        return result;
      } catch (error) {
        return fileError(error);
      }
    },
    download: async (id, signal) => {
      try {
        return await readBlob(
          await client.fileApi(
            `/projects/${projectId}/${encodeURIComponent(id)}/content`,
            { signal },
          ),
          { signal },
        );
      } catch (error) {
        return fileError(error);
      }
    },
  };
}

// Recovery state contains request IDs and presentation metadata, never credentials or File bytes.
// It is scoped to this installation and verified directory subject; it grants no server authority.
function loadRecovery(key: string): Recovery {
  try {
    const parsed: unknown = JSON.parse(sessionStorage.getItem(key) ?? "null");
    if (!parsed || typeof parsed !== "object") return { projects: {} };
    const value = parsed as Recovery;
    const projects: Record<string, ProjectRecovery> = {};
    if (value.projects && typeof value.projects === "object") {
      for (const [id, entry] of Object.entries(value.projects).slice(0, 100)) {
        if (!uuid.test(id) || !entry || typeof entry !== "object") continue;
        projects[id] = {
          shareRequest:
            typeof entry.shareRequest === "string" &&
            uuid.test(entry.shareRequest)
              ? entry.shareRequest
              : undefined,
          files: Array.isArray(entry.files)
            ? entry.files
                .filter(
                  (file) =>
                    file &&
                    uuid.test(file.clientId) &&
                    uuid.test(file.requestId) &&
                    uuid.test(file.attachRequestId) &&
                    typeof file.name === "string" &&
                    typeof file.mime === "string" &&
                    Number.isSafeInteger(file.size) &&
                    file.size >= 0 &&
                    file.size <= 10485760,
                )
                .slice(0, 32)
                .map((file) => ({
                  ...file,
                  status: "unknown",
                  metadata: undefined,
                  message: undefined,
                }))
            : undefined,
          taskRequest:
            entry.taskRequest &&
            uuid.test(entry.taskRequest.id) &&
            ((entry.taskRequest.kind === "create" &&
              typeof entry.taskRequest.title === "string") ||
              (entry.taskRequest.kind === "complete" &&
                typeof entry.taskRequest.completed === "boolean"))
              ? entry.taskRequest
              : undefined,
        };
      }
    }
    return {
      projects,
      projectRequest:
        value.projectRequest &&
        uuid.test(value.projectRequest.id) &&
        typeof value.projectRequest.title === "string"
          ? value.projectRequest
          : undefined,
    };
  } catch {
    return { projects: {} };
  }
}

export default function AppRoot({ runtime }: AppProps) {
  return (
    <Session
      key={`${runtime.installationId}:${runtime.appId}:${runtime.releaseId}`}
      runtime={runtime}
    />
  );
}
function Session({ runtime }: AppProps) {
  const current = useRef(runtime);
  current.current = runtime;
  const client = useMemo<Client>(
    () => ({
      api: (path, init) => current.current.api(path, init),
      fileApi: (path, init) => current.current.fileApi(path, init),
      directory: (query, options) => current.current.directory(query, options),
    }),
    [],
  );
  const [actor, setActor] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    setError(false);
    void client
      .directory(
        { kind: "memberships", limit: 1 },
        { signal: controller.signal },
      )
      .then((page) => {
        if (!controller.signal.aborted) setActor(page.subjectId);
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true);
      });
    return () => controller.abort();
  }, [client, attempt]);
  return (
    <main style={{ ...layout, maxWidth: 1120, margin: "0 auto", padding: 20 }}>
      {!actor ? (
        error ? (
          <ContentState
            kind="unavailable"
            title="사용자 정보를 확인할 수 없습니다."
            action={{
              label: "다시 조회",
              onClick: () => setAttempt((value) => value + 1),
            }}
          />
        ) : (
          <Loading />
        )
      ) : (
        <Work key={actor} runtime={runtime} client={client} actor={actor} />
      )}
    </main>
  );
}

function Work({
  runtime,
  client,
  actor,
}: AppProps & { client: Client; actor: string }) {
  const scope = `miy-common-work:${runtime.installationId}:${actor}`;
  const recovery = useRef<Recovery | null>(null);
  if (!recovery.current) recovery.current = loadRecovery(scope);
  const persist = useCallback(
    (change: (value: Recovery) => Recovery) => {
      recovery.current = change(recovery.current!);
      try {
        sessionStorage.setItem(scope, JSON.stringify(recovery.current));
      } catch {
        /* Memory recovery remains available when browser storage is disabled. */
      }
    },
    [scope],
  );
  const [projects, setProjects] = useState<Project[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [listError, setListError] = useState<string | null>(null);
  const [title, setTitle] = useState(
    recovery.current.projectRequest?.title ?? "",
  );
  const [pending, setPending] = useState<Project | null>(
    recovery.current.projectRequest ?? null,
  );
  // The server confirmed that the pending project was not created.
  const [notCreated, setNotCreated] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [detailDirty, setDetailDirty] = useState(false);
  const start = useControllers();
  const listVersion = useRef(0);
  const selectedId = /^\/projects\/([0-9a-f-]{36})\/?$/i.exec(
    runtime.location.pathname,
  )?.[1];
  const selected = selectedId && uuid.test(selectedId) ? selectedId : null;
  const dirty = Boolean(title.trim() || pending || busy || detailDirty);
  useEffect(() => {
    runtime.setNavigationState({
      dirty,
      message: "저장하지 않은 입력이나 결과를 확인할 요청이 있습니다.",
    });
  }, [runtime, dirty]);
  useEffect(
    () => () => {
      runtime.setNavigationState({ dirty: false });
    },
    [],
  );

  const loadProjects = useCallback(
    async (next?: string) => {
      const task = start(),
        version = ++listVersion.current;
      setLoading(true);
      setListError(null);
      try {
        const page = await client.api<{
          projects: Project[];
          nextCursor: string | null;
        }>(`/projects${next ? `?cursor=${encodeURIComponent(next)}` : ""}`, {
          signal: task.signal,
        });
        if (task.signal.aborted || version !== listVersion.current) return;
        setProjects((previous) => [
          ...new Map(
            [...(next ? previous : []), ...page.projects].map((item) => [
              item.id,
              item,
            ]),
          ).values(),
        ]);
        setCursor(page.nextCursor);
      } catch (error) {
        if (!task.signal.aborted && version === listVersion.current) {
          setListError(failure(error));
          if (!next) setProjects([]);
        }
      } finally {
        task.done();
        if (!task.signal.aborted && version === listVersion.current)
          setLoading(false);
      }
    },
    [client, start],
  );
  useEffect(() => {
    void loadProjects();
  }, [loadProjects]);
  const rememberProject = (value: Project | null) => {
    setPending(value);
    setNotCreated(false);
    persist((previous) => ({
      ...previous,
      projectRequest: value ?? undefined,
    }));
  };
  const acceptProject = (value: Project) => {
    listVersion.current += 1;
    setLoading(false);
    setProjects((previous) => [
      ...new Map([...previous, value].map((item) => [item.id, item])).values(),
    ]);
    rememberProject(null);
    setTitle("");
    setNotice("프로젝트가 확인되었습니다. 목록에서 열어 주세요.");
  };
  // `again` sends the confirmed-absent request once more under its own ID, so
  // a late arrival of the first attempt answers 409 instead of a second project.
  const createProject = async (again?: Project) => {
    if (busy || (again ? !notCreated : pending || !title.trim())) return;
    const input = again ?? { id: requestId(), title: title.trim() },
      task = start();
    rememberProject(input);
    setBusy(true);
    setNotice(null);
    try {
      const value = await client.api<Project>("/projects", {
        method: "POST",
        body: JSON.stringify(input),
        signal: task.signal,
      });
      if (!task.signal.aborted) acceptProject(value);
    } catch (error) {
      if (!task.signal.aborted) {
        if (rejected(error)) rememberProject(null);
        setNotice(
          rejected(error)
            ? failure(error)
            : "생성 결과를 확인할 수 없습니다. 다시 생성하지 않고 결과를 조회해 주세요.",
        );
      }
    } finally {
      task.done();
      if (!task.signal.aborted) setBusy(false);
    }
  };
  const lookupProject = async () => {
    if (!pending || busy) return;
    const task = start();
    setBusy(true);
    try {
      const value = await client.api<Project>(`/projects/${pending.id}`, {
        signal: task.signal,
      });
      if (!task.signal.aborted) acceptProject(value);
    } catch (error) {
      if (task.signal.aborted) return;
      setNotCreated(absent(error, true));
      setNotice(
        absent(error, true)
          ? "서버에 만들어진 프로젝트가 없습니다. 같은 요청을 다시 보내거나 요청을 취소할 수 있습니다."
          : "현재 생성 결과를 확인할 수 없습니다. 요청 기록을 유지하고 다시 조회할 수 있습니다.",
      );
    } finally {
      task.done();
      if (!task.signal.aborted) setBusy(false);
    }
  };
  const updateRecovery = useCallback(
    (id: string, patch: Partial<ProjectRecovery>) =>
      persist((value) => ({
        ...value,
        projects: {
          ...value.projects,
          [id]: { ...value.projects[id], ...patch },
        },
      })),
    [persist],
  );
  return (
    <>
      <Panel
        title="프로젝트와 업무"
        actions={<Badge tone="accent">공통 기능 예제</Badge>}
      >
        <p>프로젝트를 만들고 사용자·그룹과 업무 및 파일을 공유하세요.</p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void createProject();
          }}
          style={layout}
        >
          <Field
            label="새 프로젝트 이름"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            maxLength={200}
            disabled={busy || Boolean(pending)}
          />
          <div style={actions}>
            {pending && notCreated ? (
              <>
                <BusyButton
                  variant="primary"
                  busy={busy}
                  onClick={() => void createProject(pending)}
                >
                  같은 요청 다시 보내기
                </BusyButton>
                <Button
                  disabled={busy}
                  onClick={() => {
                    rememberProject(null);
                    setNotice(null);
                  }}
                >
                  요청 취소
                </Button>
              </>
            ) : pending ? (
              <BusyButton busy={busy} onClick={() => void lookupProject()}>
                프로젝트 생성 결과 조회
              </BusyButton>
            ) : (
              <BusyButton
                type="submit"
                variant="primary"
                busy={busy}
                disabled={!title.trim()}
              >
                프로젝트 만들기
              </BusyButton>
            )}
          </div>
        </form>
        {notice ? <p role="status">{notice}</p> : null}
      </Panel>
      <Panel
        title="내 프로젝트"
        actions={
          <Button disabled={loading} onClick={() => void loadProjects()}>
            목록 새로고침
          </Button>
        }
      >
        {listError ? (
          <ContentState kind="unavailable" title={listError} />
        ) : null}
        {!projects.length && !loading && !listError ? (
          <ContentState
            kind="empty"
            title="표시할 프로젝트가 없습니다."
            description="프로젝트를 만들거나 공유받으면 여기에 표시됩니다."
          />
        ) : null}
        <div style={actions}>
          {projects.map((project) => (
            <Button
              key={project.id}
              variant={selected === project.id ? "primary" : "secondary"}
              onClick={() => runtime.navigate(`/projects/${project.id}`)}
            >
              {project.title}
            </Button>
          ))}
        </div>
        {loading ? (
          <Loading />
        ) : cursor ? (
          <Button onClick={() => void loadProjects(cursor)}>
            프로젝트 더 보기
          </Button>
        ) : null}
      </Panel>
      {selected ? (
        <ProjectDetail
          key={selected}
          id={selected}
          client={client}
          actor={actor}
          initial={recovery.current.projects[selected] ?? {}}
          remember={updateRecovery}
          onDirtyChange={setDetailDirty}
        />
      ) : (
        <ContentState kind="empty" title="프로젝트를 선택해 주세요." />
      )}
    </>
  );
}

function ProjectDetail({
  id,
  client,
  actor,
  initial,
  remember,
  onDirtyChange,
}: {
  id: string;
  client: Client;
  actor: string;
  initial: ProjectRecovery;
  remember: (id: string, patch: Partial<ProjectRecovery>) => void;
  onDirtyChange: (dirty: boolean) => void;
}) {
  const [project, setProject] = useState<Project | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [fileReload, setFileReload] = useState(0);
  const [fileErrorNotice, setFileErrorNotice] = useState<string | null>(null);
  const [taskTitle, setTaskTitle] = useState(
    initial.taskRequest?.kind === "create" ? initial.taskRequest.title : "",
  );
  const [pending, setPending] = useState<TaskRequest | null>(
    initial.taskRequest ?? null,
  );
  // The server confirmed that the pending new task was not created.
  const [notCreated, setNotCreated] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [shareDirty, setShareDirty] = useState(false);
  const [shareRequest, setShareRequest] = useState(initial.shareRequest);
  const [fileDirty, setFileDirty] = useState(false);
  const [pendingFiles, setPendingFiles] = useState(
    Boolean(initial.files?.length),
  );
  const [sharingDenied, setSharingDenied] = useState(false);
  const start = useControllers();
  const completedFiles = useRef(
    new Set(
      initial.files
        ?.filter((file) => file.status === "complete")
        .map((file) => file.clientId),
    ),
  );
  const urls = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const initialState = useRef(initial);
  const dirty = Boolean(
    taskTitle.trim() ||
      pending ||
      busy ||
      shareDirty ||
      shareRequest ||
      fileDirty ||
      pendingFiles,
  );
  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(
    () => () => {
      onDirtyChange(false);
      for (const [url, timer] of urls.current) {
        clearTimeout(timer);
        URL.revokeObjectURL(url);
      }
    },
    [onDirtyChange],
  );
  useEffect(() => {
    const task = start();
    setLoading(true);
    setError(null);
    void Promise.all([
      client.api<Project>(`/projects/${id}`, { signal: task.signal }),
      client.api<Task[]>(`/projects/${id}/tasks`, { signal: task.signal }),
    ])
      .then(([value, rows]) => {
        if (!task.signal.aborted) {
          setProject(value);
          setTasks(rows);
        }
      })
      .catch((reason) => {
        if (!task.signal.aborted) {
          setProject(null);
          setTasks([]);
          setError(failure(reason));
        }
      })
      .finally(() => {
        task.done();
        if (!task.signal.aborted) setLoading(false);
      });
    return () => task.abort();
  }, [client, id, reload, start]);
  useEffect(() => {
    const controller = new AbortController();
    setFileErrorNotice(null);
    void client
      .api<Attachment[]>(`/projects/${id}/files`, { signal: controller.signal })
      .then((rows) => {
        if (!controller.signal.aborted) setAttachments(rows);
      })
      .catch((reason) => {
        if (!controller.signal.aborted) {
          setAttachments([]);
          setFileErrorNotice(failure(reason));
        }
      });
    return () => controller.abort();
  }, [client, id, fileReload]);
  const directory = useMemo<DirectoryProvider>(
    () => ({
      load: async ({ kind, query, cursor, limit, signal }) => {
        const page = await client.directory(
          {
            kind: kind === "user" ? "users" : "groups",
            q: query,
            cursor,
            limit,
          },
          { signal },
        );
        return {
          items:
            kind === "user"
              ? page.users.map((user) => ({
                  kind: "user" as const,
                  id: user.id,
                  name: user.name,
                  description: user.primaryGroup?.name,
                }))
              : page.groups.map((group) => ({
                  kind: "group" as const,
                  id: group.id,
                  name: group.name,
                })),
          nextCursor: page.nextCursor,
        };
      },
    }),
    [client],
  );
  const sharing = useMemo<ShareProvider>(
    () => ({
      load: async ({ signal }) => {
        try {
          return decodeSharing(
            await client.api<WireSharing>(`/projects/${id}/sharing`, {
              signal,
            }),
          );
        } catch (reason) {
          if (!signal.aborted && isDenied(reason)) setSharingDenied(true);
          throw reason;
        }
      },
      replace: async (input, { signal }) => {
        const revision = Number(input.expectedRevision);
        if (!Number.isSafeInteger(revision) || revision < 0)
          throw new UiOperationError("invalid_input");
        return decodeSharing(
          await client.api<WireSharing>(`/projects/${id}/sharing`, {
            method: "PUT",
            signal,
            body: JSON.stringify({
              expectedRevision: revision,
              requestId: input.requestId,
              entries: input.entries.map((entry) => ({
                subject: { kind: entry.subject.kind, id: entry.subject.id },
                role: entry.role,
              })),
            }),
          }),
        );
      },
      lookupResult: async (request, { signal }) => {
        const value = await client.api<WireSharing | null>(
          `/projects/${id}/sharing/results/${encodeURIComponent(request)}`,
          { signal },
        );
        return value ? decodeSharing(value) : null;
      },
    }),
    [client, id],
  );
  const files = useMemo(() => fileService(client, id), [client, id]);
  const rememberTask = (value: TaskRequest | null) => {
    setPending(value);
    setNotCreated(false);
    remember(id, { taskRequest: value ?? undefined });
  };
  const acceptTask = (value: Task, request: TaskRequest) => {
    setTasks((previous) => [
      ...new Map([...previous, value].map((item) => [item.id, item])).values(),
    ]);
    rememberTask(null);
    if (request.kind === "create") setTaskTitle("");
    setNotice("서버의 현재 업무 상태를 확인했습니다.");
  };
  const mutateTask = async (request: TaskRequest, again = false) => {
    if (busy || loading || (again ? !notCreated : pending)) return;
    const task = start();
    rememberTask(request);
    setBusy(true);
    setNotice(null);
    try {
      const value = await client.api<Task>(
        `/projects/${id}/tasks${request.kind === "complete" ? `/${request.id}` : ""}`,
        {
          signal: task.signal,
          method: request.kind === "create" ? "POST" : "PATCH",
          body: JSON.stringify(
            request.kind === "create"
              ? { id: request.id, title: request.title }
              : { completed: request.completed },
          ),
        },
      );
      if (!task.signal.aborted) acceptTask(value, request);
    } catch (reason) {
      if (!task.signal.aborted) {
        if (rejected(reason)) rememberTask(null);
        setNotice(
          rejected(reason)
            ? failure(reason)
            : "업무 변경 결과를 확인할 수 없습니다. 다시 쓰지 않고 결과를 조회해 주세요.",
        );
      }
    } finally {
      task.done();
      if (!task.signal.aborted) setBusy(false);
    }
  };
  const lookupTask = async () => {
    if (!pending || busy) return;
    const task = start();
    setBusy(true);
    try {
      const value = await client.api<Task>(
        `/projects/${id}/tasks/${pending.id}`,
        { signal: task.signal },
      );
      if (!task.signal.aborted) acceptTask(value, pending);
    } catch (error) {
      if (task.signal.aborted) return;
      const missing = absent(error) && pending.kind === "create";
      setNotCreated(missing);
      setNotice(
        missing
          ? "서버에 추가된 업무가 없습니다. 같은 요청을 다시 보내거나 요청을 취소할 수 있습니다."
          : "현재 업무 결과를 확인할 수 없습니다. 요청 기록을 유지합니다.",
      );
    } finally {
      task.done();
      if (!task.signal.aborted) setBusy(false);
    }
  };
  const download = async (file: Attachment) => {
    const task = start();
    setFileErrorNotice(null);
    try {
      const blob = await files.download(file.id, task.signal);
      if (task.signal.aborted) return;
      if (blob.size !== file.size || blob.size > 10485760)
        throw new Error("invalid_file_size");
      const url = URL.createObjectURL(blob),
        link = document.createElement("a");
      link.href = url;
      link.download =
        file.name
          .split(/[\\/]/)
          .at(-1)
          ?.replace(/[\u0000-\u001f\u007f]/g, "")
          .trim() || "download";
      document.body.append(link);
      link.click();
      link.remove();
      urls.current.set(
        url,
        setTimeout(() => {
          URL.revokeObjectURL(url);
          urls.current.delete(url);
        }, 1000),
      );
    } catch {
      if (!task.signal.aborted)
        setFileErrorNotice(
          "파일을 다운로드할 수 없습니다. 현재 접근 권한을 확인해 주세요.",
        );
    } finally {
      task.done();
    }
  };
  if (loading && !project) return <Loading />;
  if (!project)
    return (
      <ContentState
        kind="unavailable"
        title={error ?? "프로젝트를 표시할 수 없습니다."}
        action={{
          label: "다시 조회",
          onClick: () => setReload((value) => value + 1),
        }}
      />
    );
  return (
    <section style={layout} aria-label={project.title}>
      <Panel
        title={project.title}
        actions={
          <Button
            disabled={loading || busy}
            onClick={() => setReload((value) => value + 1)}
          >
            업무 새로고침
          </Button>
        }
      >
        <form
          style={layout}
          onSubmit={(event) => {
            event.preventDefault();
            if (taskTitle.trim())
              void mutateTask({
                kind: "create",
                id: requestId(),
                title: taskTitle.trim(),
              });
          }}
        >
          <Field
            label="새 업무"
            value={taskTitle}
            maxLength={200}
            disabled={busy || loading || Boolean(pending)}
            onChange={(event) => setTaskTitle(event.target.value)}
          />
          <div style={actions}>
            {pending && notCreated ? (
              <>
                <BusyButton
                  variant="primary"
                  busy={busy}
                  disabled={loading}
                  onClick={() => void mutateTask(pending, true)}
                >
                  같은 요청 다시 보내기
                </BusyButton>
                <Button
                  disabled={busy}
                  onClick={() => {
                    rememberTask(null);
                    setNotice(null);
                  }}
                >
                  요청 취소
                </Button>
              </>
            ) : pending ? (
              <BusyButton
                busy={busy}
                disabled={loading}
                onClick={() => void lookupTask()}
              >
                업무 변경 결과 조회
              </BusyButton>
            ) : (
              <BusyButton
                type="submit"
                variant="primary"
                busy={busy}
                disabled={loading || !taskTitle.trim()}
              >
                업무 추가
              </BusyButton>
            )}
          </div>
        </form>
        {notice ? <p role="status">{notice}</p> : null}
        {tasks.length ? (
          <ul style={{ padding: 0, listStyle: "none" }}>
            {tasks.map((task) => (
              <li
                key={task.id}
                style={{
                  ...actions,
                  padding: "10px 0",
                  borderBottom: "1px solid var(--ui-color-border)",
                }}
              >
                <label
                  style={{
                    display: "flex",
                    gap: 8,
                    alignItems: "center",
                    flex: 1,
                  }}
                >
                  <input
                    type="checkbox"
                    checked={task.completed}
                    disabled={busy || loading || Boolean(pending)}
                    onChange={(event) =>
                      void mutateTask({
                        kind: "complete",
                        id: task.id,
                        completed: event.target.checked,
                      })
                    }
                  />
                  <span
                    style={{
                      textDecoration: task.completed
                        ? "line-through"
                        : undefined,
                      overflowWrap: "anywhere",
                    }}
                  >
                    {task.title}
                  </span>
                </label>
                {task.completed ? <Badge tone="success">완료</Badge> : null}
              </li>
            ))}
          </ul>
        ) : (
          <ContentState kind="empty" title="등록된 업무가 없습니다." />
        )}
      </Panel>
      <Panel title="프로젝트 공유">
        {sharingDenied ? (
          <ContentState
            kind="unavailable"
            title="공유 설정은 프로젝트 소유자만 관리할 수 있습니다."
            action={{
              label: "권한 다시 확인",
              onClick: () => setSharingDenied(false),
            }}
          />
        ) : (
          <ShareEditor
            provider={sharing}
            directoryProvider={directory}
            currentUserId={actor}
            initialPendingRequestId={shareRequest}
            onDirtyChange={setShareDirty}
            onPendingRequestChange={(value) => {
              setShareRequest(value ?? undefined);
              remember(id, { shareRequest: value ?? undefined });
            }}
          />
        )}
      </Panel>
      <Panel title="파일과 첨부">
        <FileTransferPanel
          service={files}
          reference={id}
          initialTransfers={initialState.current.files}
          onDirtyChange={setFileDirty}
          onStateChange={(values) => {
            const pendingValues = values
              .filter(
                (value) =>
                  value.status !== "complete" && value.status !== "cancelled",
              )
              .map((value) => ({ ...value }));
            initialState.current = {
              ...initialState.current,
              files: pendingValues,
            };
            setPendingFiles(pendingValues.length > 0);
            remember(id, { files: pendingValues });
            const added = values.filter(
              (value) =>
                value.status === "complete" &&
                !completedFiles.current.has(value.clientId),
            );
            for (const value of added)
              completedFiles.current.add(value.clientId);
            if (added.length) setFileReload((value) => value + 1);
          }}
        />
        <h3>연결된 파일</h3>
        <Button onClick={() => setFileReload((value) => value + 1)}>
          첨부 목록 새로고침
        </Button>
        {fileErrorNotice ? <p role="status">{fileErrorNotice}</p> : null}
        {attachments.length ? (
          <ul>
            {attachments.map((file) => (
              <li key={file.id} style={{ ...actions, padding: "8px 0" }}>
                <span style={{ flex: 1, overflowWrap: "anywhere" }}>
                  {file.name} · {formatByteSize(file.size)}
                </span>
                <Button
                  aria-label={`${file.name} 다운로드`}
                  onClick={() => void download(file)}
                >
                  다운로드
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <p>연결된 파일이 없습니다.</p>
        )}
      </Panel>
    </section>
  );
}
