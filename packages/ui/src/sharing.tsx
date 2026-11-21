import { useEffect, useMemo, useRef, useState } from "react";
import {
  DirectoryPicker,
  directoryOptionKey,
  type DirectoryOption,
  type DirectoryProvider,
} from "./directory";
import { useConfirm } from "./internal/dialog";
import { Badge, Button, ContentState, Loading } from "./internal/primitives";
import { requestId as newRequestId } from "./utilities";

export interface ShareEntry {
  subject: DirectoryOption;
  role: string;
  /** Presentation restriction returned by the app; the app also enforces it on save. */
  locked?: boolean;
}
export interface ShareSnapshot {
  revision: string;
  entries: ShareEntry[];
  roles: Array<{ id: string; label: string }>;
  /**
   * Role preselected for a newly added subject. When it is absent or not one of
   * `roles`, the person must choose: the order of `roles` implies nothing.
   */
  defaultRole?: string | null;
  canManage: boolean;
}
export interface ShareProvider {
  load: (options: { signal: AbortSignal }) => Promise<ShareSnapshot>;
  replace: (
    input: {
      requestId: string;
      expectedRevision: string;
      entries: ShareEntry[];
    },
    options: { signal: AbortSignal },
  ) => Promise<ShareSnapshot>;
  /** null means no completed result is available yet; it never authorizes an automatic resend. */
  lookupResult: (
    requestId: string,
    options: { signal: AbortSignal },
  ) => Promise<ShareSnapshot | null>;
}

/** Providers may wrap a server error with one of these stable, non-sensitive codes. */
export class UiOperationError extends Error {
  constructor(
    public readonly code:
      | "forbidden"
      | "conflict"
      | "invalid_subject"
      | "invalid_role"
      | "invalid_input"
      | "not_found"
      | "too_large"
      | "expired"
      | "cancelled"
      | "unknown",
  ) {
    super(code);
    this.name = "UiOperationError";
  }
}
export function uiFailureCode(error: unknown): UiOperationError["code"] {
  const value =
    error && typeof error === "object" && "code" in error
      ? (error as { code: unknown }).code
      : error instanceof Error
        ? error.message
        : undefined;
  const aliases: Record<string, UiOperationError["code"]> = {
    acl_denied: "forbidden",
    acl_revision_conflict: "conflict",
    acl_request_conflict: "conflict",
    acl_invalid_request: "invalid_input",
    acl_invalid_role: "invalid_role",
    acl_invalid_subject: "invalid_subject",
    acl_owner_protected: "conflict",
    acl_last_manager: "conflict",
  };
  if (typeof value === "string" && Object.hasOwn(aliases, value))
    return aliases[value];
  const known = [
    "forbidden",
    "conflict",
    "invalid_subject",
    "invalid_role",
    "invalid_input",
    "not_found",
    "too_large",
    "expired",
    "cancelled",
  ];
  return typeof value === "string" && known.includes(value)
    ? (value as UiOperationError["code"])
    : "unknown";
}

function fingerprint(entries: readonly ShareEntry[]): string {
  return JSON.stringify(
    entries
      .map((entry) => [directoryOptionKey(entry.subject), entry.role])
      .sort(([a], [b]) => a.localeCompare(b)),
  );
}

export interface ShareEditorProps {
  provider: ShareProvider;
  directoryProvider: DirectoryProvider;
  currentUserId?: string;
  title?: string;
  /** Restore only for this resource and user. A pending request must be checked before another edit. */
  initialPendingRequestId?: string;
  onDirtyChange?: (dirty: boolean) => void;
  /** Persist this ID outside the editor if recovery after closing the app is needed. */
  onPendingRequestChange?: (requestId: string | null) => void;
}

export function ShareEditor({
  provider,
  directoryProvider,
  currentUserId,
  title = "공유 및 권한",
  initialPendingRequestId,
  onDirtyChange,
  onPendingRequestChange,
}: ShareEditorProps) {
  const [snapshot, setSnapshot] = useState<ShareSnapshot | null>(null);
  const [entries, setEntries] = useState<ShareEntry[]>([]);
  const [phase, setPhase] = useState<
    "loading" | "ready" | "saving" | "checking" | "unknown" | "load_error"
  >("loading");
  const [notice, setNotice] = useState<string | null>(null);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [blocked, setBlocked] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const generation = useRef(0);
  const callbacks = useRef({ onDirtyChange, onPendingRequestChange });
  const initialPending = useRef(initialPendingRequestId);
  initialPending.current = initialPendingRequestId;
  callbacks.current = { onDirtyChange, onPendingRequestChange };
  const { confirm, confirmDialog } = useConfirm();
  const busy =
    phase === "loading" || phase === "saving" || phase === "checking";
  const dirty =
    Boolean(
      snapshot && fingerprint(entries) !== fingerprint(snapshot.entries),
    ) ||
    phase === "saving" ||
    Boolean(requestId);
  // A subject without a role is never sent: nobody receives access by default.
  const incomplete = entries.some((entry) => !entry.role);
  const locked = useMemo(
    () =>
      new Set(
        entries
          .filter((entry) => entry.locked)
          .map((entry) => directoryOptionKey(entry.subject)),
      ),
    [entries],
  );
  const adopt = (next: ShareSnapshot, pendingId: string | null = null) => {
    if (
      typeof next.revision !== "string" ||
      !next.revision ||
      !Array.isArray(next.entries) ||
      !Array.isArray(next.roles) ||
      typeof next.canManage !== "boolean" ||
      (next.defaultRole != null && typeof next.defaultRole !== "string") ||
      next.entries.some(
        (entry) =>
          !entry ||
          !entry.subject ||
          !["user", "group"].includes(entry.subject.kind) ||
          !entry.subject.id ||
          typeof entry.subject.name !== "string" ||
          !entry.role,
      ) ||
      next.roles.some(
        (role) => !role || !role.id || typeof role.label !== "string",
      ) ||
      new Set(next.entries.map((entry) => directoryOptionKey(entry.subject)))
        .size !== next.entries.length ||
      new Set(next.roles.map((role) => role.id)).size !== next.roles.length
    )
      throw new Error("invalid_share_snapshot");
    setSnapshot(next);
    setEntries(
      next.entries.map((entry) => ({
        ...entry,
        subject: { ...entry.subject },
      })),
    );
    setRequestId(pendingId);
    setBlocked(false);
    setPhase(pendingId ? "unknown" : "ready");
    setNotice(pendingId ? "이전 저장 요청의 결과를 먼저 확인해 주세요." : null);
    callbacks.current.onPendingRequestChange?.(pendingId);
  };

  useEffect(() => {
    const current = ++generation.current;
    const pendingId = initialPending.current ?? null;
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    setSnapshot(null);
    setEntries([]);
    setRequestId(null);
    setPhase("loading");
    setNotice(null);
    setBlocked(false);
    void Promise.resolve()
      .then(() => provider.load({ signal: abort.signal }))
      .then((next) => {
        if (generation.current === current && !abort.signal.aborted)
          adopt(next, pendingId);
      })
      .catch(() => {
        if (generation.current === current && !abort.signal.aborted)
          setPhase("load_error");
      });
    return () => {
      generation.current += 1;
      abort.abort();
      controller.current?.abort();
    };
  }, [provider]);
  useEffect(() => {
    callbacks.current.onDirtyChange?.(dirty);
  }, [dirty]);
  useEffect(
    () => () => {
      callbacks.current.onDirtyChange?.(false);
    },
    [],
  );

  const reload = async () => {
    if (busy) return;
    if (
      (dirty || requestId) &&
      !(await confirm({
        title: "공유 설정 다시 불러오기",
        description: requestId
          ? "저장 결과가 아직 확인되지 않았습니다. 초안을 버리고 서버의 현재 설정을 불러올까요?"
          : "저장하지 않은 변경을 버리고 서버의 현재 설정을 불러올까요?",
        confirmLabel: "다시 불러오기",
      }))
    )
      return;
    const current = ++generation.current;
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    setPhase("loading");
    try {
      const next = await provider.load({ signal: abort.signal });
      if (generation.current === current && !abort.signal.aborted) adopt(next);
    } catch {
      if (generation.current === current && !abort.signal.aborted) {
        setPhase(snapshot ? (requestId ? "unknown" : "ready") : "load_error");
        setNotice("현재 공유 설정을 불러올 수 없습니다. 초안은 유지됩니다.");
      }
    }
  };
  const save = async () => {
    if (
      !snapshot ||
      !snapshot.canManage ||
      busy ||
      blocked ||
      requestId ||
      !dirty ||
      incomplete
    )
      return;
    const current = ++generation.current;
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    const id = newRequestId();
    setRequestId(id);
    callbacks.current.onPendingRequestChange?.(id);
    setPhase("saving");
    setNotice(null);
    try {
      const next = await provider.replace(
        {
          requestId: id,
          expectedRevision: snapshot.revision,
          entries: entries.map((entry) => ({
            ...entry,
            subject: { ...entry.subject },
          })),
        },
        { signal: abort.signal },
      );
      if (generation.current === current && !abort.signal.aborted) adopt(next);
    } catch (error) {
      if (generation.current !== current || abort.signal.aborted) return;
      const code = uiFailureCode(error);
      if (code === "unknown") {
        setPhase("unknown");
        setNotice(
          "저장 결과를 확인할 수 없습니다. 다시 전송하지 않고 결과를 조회해 주세요.",
        );
      } else {
        setRequestId(null);
        callbacks.current.onPendingRequestChange?.(null);
        setPhase("ready");
        setBlocked(code === "forbidden" || code === "conflict");
        setNotice(
          code === "conflict"
            ? "공유 설정이 다른 곳에서 변경되었습니다. 초안을 확인한 뒤 다시 불러와 주세요."
            : code === "forbidden"
              ? "현재 공유 설정을 변경할 권한이 없습니다. 초안은 유지됩니다."
              : "선택한 대상이나 역할을 저장할 수 없습니다. 초안을 확인해 주세요.",
        );
      }
    }
  };
  const lookup = async () => {
    if (!requestId || busy) return;
    const current = ++generation.current;
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    setPhase("checking");
    try {
      const next = await provider.lookupResult(requestId, {
        signal: abort.signal,
      });
      if (generation.current !== current || abort.signal.aborted) return;
      if (next) adopt(next);
      else {
        setPhase("unknown");
        setNotice(
          "완료된 저장 결과가 아직 없습니다. 잠시 후 다시 조회해 주세요.",
        );
      }
    } catch {
      if (generation.current === current && !abort.signal.aborted) {
        setPhase("unknown");
        setNotice(
          "저장 결과를 조회할 수 없습니다. 초안과 요청 기록을 유지합니다.",
        );
      }
    }
  };
  const editable = Boolean(
    snapshot?.canManage && !busy && !requestId && !blocked,
  );
  const defaultRole =
    snapshot?.defaultRole &&
    snapshot.roles.some((role) => role.id === snapshot.defaultRole)
      ? snapshot.defaultRole
      : "";
  return (
    <section
      className="miy-share"
      aria-label={title}
      aria-busy={busy || undefined}
    >
      <header className="miy-share__header">
        <h3>{title}</h3>
        {dirty ? <Badge tone="warning">저장하지 않은 변경</Badge> : null}
      </header>
      {!snapshot ? (
        phase === "load_error" ? (
          <ContentState
            kind="unavailable"
            title="공유 설정을 불러올 수 없습니다."
            action={{ label: "다시 시도", onClick: () => void reload() }}
          />
        ) : (
          <Loading />
        )
      ) : (
        <>
          {!snapshot.canManage ? (
            <p className="miy-share__notice">
              현재 공유 설정은 조회만 할 수 있습니다.
            </p>
          ) : null}
          <DirectoryPicker
            label="공유 대상"
            provider={directoryProvider}
            value={entries.map((entry) => entry.subject)}
            lockedKeys={locked}
            currentUserId={currentUserId}
            disabled={!editable || !snapshot.roles.length}
            onChange={(subjects) =>
              setEntries(
                subjects.map(
                  (subject) =>
                    entries.find(
                      (entry) =>
                        directoryOptionKey(entry.subject) ===
                        directoryOptionKey(subject),
                    ) ?? { subject, role: defaultRole },
                ),
              )
            }
          />
          {entries.length ? (
            <ul className="miy-share__entries">
              {entries.map((entry) => (
                <li key={directoryOptionKey(entry.subject)}>
                  <span>
                    <strong>{entry.subject.name}</strong>
                    <small>
                      {entry.subject.kind === "user" ? "사용자" : "그룹"}
                      {entry.locked ? " · 고정" : ""}
                    </small>
                  </span>
                  <select
                    className="miy-input"
                    aria-label={`${entry.subject.name} 역할`}
                    value={entry.role}
                    disabled={!editable || entry.locked}
                    onChange={(event) =>
                      setEntries((values) =>
                        values.map((value) =>
                          directoryOptionKey(value.subject) ===
                          directoryOptionKey(entry.subject)
                            ? { ...value, role: event.target.value }
                            : value,
                        ),
                      )
                    }
                  >
                    {!entry.role ? (
                      <option value="" disabled>
                        역할 선택
                      </option>
                    ) : !snapshot.roles.some(
                        (role) => role.id === entry.role,
                      ) ? (
                      <option value={entry.role}>현재 역할</option>
                    ) : null}
                    {snapshot.roles.map((role) => (
                      <option key={role.id} value={role.id}>
                        {role.label}
                      </option>
                    ))}
                  </select>
                  <Button
                    variant="ghost"
                    aria-label={`${entry.subject.name} 공유 해제`}
                    disabled={!editable || entry.locked}
                    onClick={() =>
                      setEntries((values) =>
                        values.filter(
                          (value) =>
                            directoryOptionKey(value.subject) !==
                            directoryOptionKey(entry.subject),
                        ),
                      )
                    }
                  >
                    해제
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="miy-share__notice">
              명시적으로 공유한 대상이 없습니다.
            </p>
          )}
        </>
      )}
      {incomplete ? (
        <p className="miy-share__notice">
          역할을 선택하지 않은 대상이 있습니다. 역할을 선택해야 저장할 수
          있습니다.
        </p>
      ) : null}
      {notice ? (
        <p className="miy-share__notice" role="status">
          {notice}
        </p>
      ) : null}
      {snapshot ? (
        <footer className="miy-share__actions">
          <Button onClick={() => void reload()} disabled={busy}>
            다시 불러오기
          </Button>
          {requestId ? (
            <Button
              busy={phase === "checking"}
              disabled={phase === "saving"}
              onClick={() => void lookup()}
            >
              저장 결과 조회
            </Button>
          ) : (
            <Button
              variant="primary"
              busy={phase === "saving"}
              disabled={!editable || !dirty || incomplete}
              onClick={() => void save()}
            >
              공유 설정 저장
            </Button>
          )}
        </footer>
      ) : null}
      {confirmDialog}
    </section>
  );
}
