import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import type { AppProps } from "@miy/sdk";
import { Button } from "@miy/components/ui";
import styles from "./AppRoot.module.css";

type Todo = {
  id: string;
  title: string;
  completed: boolean;
  createdAt: string;
  updatedAt: string;
};
type View = "all" | "active" | "completed";

const views: { id: View; label: string }[] = [
  { id: "all", label: "전체" },
  { id: "active", label: "할 일" },
  { id: "completed", label: "완료" },
];
const dateFormat = new Intl.DateTimeFormat("ko-KR", { month: "long", day: "numeric" });
const primaryButton = { background: "#176451", borderColor: "#176451", color: "white" };

function errorMessage(error: unknown): string {
  const code = error instanceof Error ? error.message : "";
  const messages: Record<string, string> = {
    invalid_title: "할 일을 1~200자로 입력해 주세요. 줄바꿈과 제어 문자는 사용할 수 없습니다.",
    todo_not_found: "이미 삭제되었거나 접근할 수 없는 할 일입니다. 목록을 새로고침해 주세요.",
    database_error: "저장소에 연결하지 못했습니다. 잠시 후 목록을 새로고침해 주세요.",
    invalid_json: "요청 형식이 올바르지 않습니다. 입력 내용을 확인해 주세요.",
    invalid_id: "할 일 정보가 올바르지 않습니다. 목록을 새로고침해 주세요.",
  };
  return messages[code] ?? "요청을 확인하지 못했습니다. 목록을 새로고침해 저장 여부를 확인한 뒤 다시 시도해 주세요.";
}

export default function AppRoot({ runtime }: AppProps) {
  const [items, setItems] = useState<Todo[]>([]);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [fieldError, setFieldError] = useState("");
  const request = useRef<AbortController | null>(null);
  const mutationLock = useRef(false);
  const draftRef = useRef("");
  const inputRef = useRef<HTMLInputElement>(null);

  const requestedView = new URLSearchParams(runtime.location.search).get("view");
  const view: View = requestedView === "active" || requestedView === "completed" ? requestedView : "all";
  const completed = items.filter((item) => item.completed).length;
  const remaining = items.length - completed;
  const counts = { all: items.length, active: remaining, completed };
  const visible = items.filter((item) => view === "all" || item.completed === (view === "completed"));
  const busy = loading || pending !== null;
  const titleLength = Array.from(draft.trim()).length;

  useEffect(() => runtime.registerNavigationGuard(() => {
    if (mutationLock.current) return "저장 중인 작업이 있습니다. 페이지를 이동하시겠습니까?";
    if (draftRef.current.length > 0) return "작성 중인 할 일이 있습니다. 페이지를 이동하시겠습니까?";
    return undefined;
  }), [runtime.registerNavigationGuard]);

  const load = useCallback(async () => {
    if (mutationLock.current) return;
    request.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    setLoading(true);
    setError("");
    try {
      const data = await runtime.api<{ items: Todo[] }>("/todos", { signal: controller.signal });
      if (!controller.signal.aborted) {
        setItems(data.items);
        setLoaded(true);
      }
    } catch (cause) {
      if (!controller.signal.aborted) setError(errorMessage(cause));
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, [runtime.api]);

  useEffect(() => {
    void load();
    return () => request.current?.abort();
  }, [load]);

  async function mutate(key: string, action: () => Promise<void>) {
    if (mutationLock.current || loading || !loaded) return;
    mutationLock.current = true;
    setPending(key);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      mutationLock.current = false;
      setPending(null);
    }
  }

  function addTodo(event: FormEvent) {
    event.preventDefault();
    const title = draft.trim();
    if (!title || titleLength > 200 || /[\u0000-\u001f\u007f-\u009f]/u.test(draft)) {
      setFieldError("할 일을 1~200자로 입력해 주세요. 제어 문자는 사용할 수 없습니다.");
      inputRef.current?.focus();
      return;
    }
    setFieldError("");
    void mutate("create", async () => {
      const item = await runtime.api<Todo>("/todos", { method: "POST", body: JSON.stringify({ title }) });
      setItems((current) => [item, ...current]);
      draftRef.current = "";
      setDraft("");
      setNotice(view === "completed" ? "할 일을 추가했습니다. ‘할 일’ 목록에서 확인해 주세요." : "할 일을 추가했습니다.");
      inputRef.current?.focus();
    });
  }

  function toggleTodo(item: Todo) {
    void mutate(item.id, async () => {
      const saved = await runtime.api<Todo>(`/todos/${item.id}`, {
        method: "PATCH", body: JSON.stringify({ completed: !item.completed }),
      });
      setItems((current) => current.map((todo) => todo.id === saved.id ? saved : todo));
      setNotice(saved.completed ? "할 일을 완료했습니다." : "완료를 취소했습니다.");
    });
  }

  function deleteTodo(item: Todo) {
    void mutate(item.id, async () => {
      await runtime.api<{ deleted: boolean; id: string }>(`/todos/${item.id}`, { method: "DELETE" });
      setItems((current) => current.filter((todo) => todo.id !== item.id));
      setConfirmDelete(null);
      setNotice("할 일을 삭제했습니다.");
    });
  }

  return (
    <main className={styles.page}>
      <div className={styles.container}>
        <header className={styles.header}>
          <div>
            <p className={styles.eyebrow}><span className={styles.logo} aria-hidden="true">✓</span> 나의 작은 계획</p>
            <h1>오늘의 할 일</h1>
            <p className={styles.subtitle}>하나씩 마무리하며, 가벼운 하루를 만들어 보세요.</p>
          </div>
          <span className={styles.privateBadge}>나만의 목록</span>
        </header>

        <section className={styles.summary} aria-label="할 일 현황">
          <div><span>남은 할 일</span><strong>{loaded ? remaining : "—"}<small>개</small></strong></div>
          <div><span>완료한 일</span><strong>{loaded ? completed : "—"}<small>개</small></strong></div>
          <div className={styles.progressBlock}>
            <span>{loaded && items.length > 0 ? `${Math.round(completed / items.length * 100)}% 완료` : "작은 시작이 만드는 변화"}</span>
            <progress max={Math.max(items.length, 1)} value={completed} aria-label="전체 할 일 완료율" />
          </div>
        </section>

        <section className={styles.card} aria-labelledby="new-todo-heading">
          <h2 id="new-todo-heading">새로운 할 일</h2>
          <form onSubmit={addTodo}>
            <label className={styles.srOnly} htmlFor="todo-title">할 일 내용</label>
            <div className={styles.inputRow}>
              <input
                id="todo-title" ref={inputRef} value={draft} readOnly={pending === "create"}
                placeholder="무엇을 해볼까요?" autoComplete="off"
                aria-describedby="todo-title-help" aria-invalid={Boolean(fieldError)}
                onChange={(event) => {
                  draftRef.current = event.target.value;
                  setDraft(event.target.value);
                  setFieldError("");
                }}
              />
              <Button type="submit" className={styles.addButton} style={primaryButton} disabled={busy || !loaded}>
                {pending === "create" ? "추가 중…" : "+ 추가하기"}
              </Button>
            </div>
            <div className={styles.inputHelp} id="todo-title-help">
              <span className={fieldError ? styles.fieldError : undefined}>{fieldError || "짧고 구체적으로 적어 보세요. 최대 200자까지 입력할 수 있어요."}</span>
              <span className={titleLength > 200 ? styles.fieldError : undefined}>{titleLength}/200</span>
            </div>
          </form>
        </section>

        {error && <div className={styles.error} role="alert"><strong>확인이 필요해요</strong><span>{error}</span></div>}
        <p className={styles.notice} role="status" aria-live="polite">{pending ? "변경 사항을 저장하고 있습니다…" : notice}</p>

        <section className={styles.listCard} aria-label="내 할 일 목록" aria-busy={busy}>
          <div className={styles.toolbar}>
            <nav className={styles.filters} aria-label="목록 필터">
              {views.map((filter) => (
                <button key={filter.id} type="button" className={view === filter.id ? styles.selectedFilter : styles.filter}
                  aria-current={view === filter.id ? "page" : undefined}
                  onClick={() => {
                    if (view !== filter.id) runtime.navigate(filter.id === "all" ? "/" : `/?view=${filter.id}`);
                  }}>
                  {filter.label}<span>{loaded ? counts[filter.id] : "—"}</span>
                </button>
              ))}
            </nav>
            <Button type="button" className={styles.refresh} disabled={busy} onClick={() => { setNotice(""); void load(); }}>
              {loading ? "불러오는 중…" : "새로고침"}
            </Button>
          </div>

          {!loaded ? (
            <div className={styles.empty}><span className={styles.emptyIcon} aria-hidden="true">☰</span>
              <h2>{loading ? "할 일을 불러오고 있어요" : "목록을 불러오지 못했어요"}</h2>
              <p>{loading ? "잠시만 기다려 주세요." : "위의 안내를 확인하고 새로고침해 주세요."}</p>
            </div>
          ) : visible.length === 0 ? (
            <div className={styles.empty}><span className={styles.emptyIcon} aria-hidden="true">✓</span>
              <h2>{view === "completed" ? "아직 완료한 일이 없어요" : view === "active" && items.length > 0 ? "할 일을 모두 마쳤어요!" : "새로운 시작을 적어 보세요"}</h2>
              <p>{view === "completed" ? "마무리한 일의 체크 상자를 눌러 보세요." : "위에서 할 일을 추가하면 이곳에 차곡차곡 모여요."}</p>
            </div>
          ) : (
            <ul className={styles.list}>
              {visible.map((item) => (
                <li key={item.id} className={`${styles.item} ${item.completed ? styles.completed : ""}`} data-todo-id={item.id}>
                  <input className={styles.checkbox} type="checkbox" checked={item.completed} disabled={busy}
                    aria-label={`${item.title} ${item.completed ? "완료 취소" : "완료"}`}
                    onChange={() => toggleTodo(item)} />
                  <div className={styles.itemContent}>
                    <span className={styles.title}>{item.title}</span>
                    <span className={styles.itemMeta}><time dateTime={item.createdAt} title={new Date(item.createdAt).toLocaleString("ko-KR")}>{dateFormat.format(new Date(item.createdAt))} 추가</time>
                      {item.completed && <span className={styles.doneBadge}>완료</span>}
                    </span>
                    {confirmDelete === item.id && <div className={styles.deleteConfirm}>
                      <span>이 할 일을 삭제할까요?</span>
                      <button type="button" disabled={busy} onClick={() => deleteTodo(item)}>삭제하기</button>
                      <button type="button" disabled={busy} onClick={() => setConfirmDelete(null)}>취소</button>
                    </div>}
                  </div>
                  <button type="button" className={styles.deleteButton} disabled={busy}
                    aria-label={`${item.title} 삭제`} aria-expanded={confirmDelete === item.id}
                    onClick={() => setConfirmDelete(confirmDelete === item.id ? null : item.id)}>
                    {pending === item.id ? "저장 중…" : "삭제"}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <footer className={styles.listFooter}>내가 작성한 할 일만 볼 수 있어요.<span>{loaded ? `전체 ${items.length}개` : ""}</span></footer>
        </section>
      </div>
    </main>
  );
}
