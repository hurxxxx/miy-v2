import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { Button, ContentState, Loading, classNames } from "./primitives";

export type DirectoryKind = "user" | "group";
export interface DirectoryOption {
  kind: DirectoryKind;
  id: string;
  name: string;
  description?: string;
}
export interface DirectoryPage {
  items: DirectoryOption[];
  nextCursor: string | null;
}
export interface DirectoryProvider {
  load: (request: {
    kind: DirectoryKind;
    query: string;
    cursor?: string;
    limit: number;
    signal: AbortSignal;
  }) => Promise<DirectoryPage>;
}
export const directoryOptionKey = (
  option: Pick<DirectoryOption, "kind" | "id">,
) => `${option.kind}:${option.id}`;

/** Keeps the server page order, with the current user first when present. */
export function mergeDirectoryOptions(
  previous: readonly DirectoryOption[],
  incoming: readonly DirectoryOption[],
  currentUserId?: string,
): DirectoryOption[] {
  const byKey = new Map(
    previous.map((item) => [directoryOptionKey(item), item]),
  );
  for (const item of incoming) byKey.set(directoryOptionKey(item), item);
  const items = [...byKey.values()];
  const own = items.findIndex(
    (item) => item.kind === "user" && item.id === currentUserId,
  );
  if (own > 0) items.unshift(...items.splice(own, 1));
  return items;
}

export interface DirectoryPickerLabels {
  users: string;
  groups: string;
  searchPlaceholder: string;
  loading: string;
  noMatches: string;
  unavailable: string;
  retry: string;
  loadMore: string;
  currentUser: string;
  locked: string;
  remove: (name: string) => string;
}
// One directory read per pause in typing, not per keystroke: the server
// authorizes and audits every read.
const SEARCH_DELAY_MS = 250;
const defaultLabels: DirectoryPickerLabels = {
  users: "사용자",
  groups: "그룹",
  searchPlaceholder: "이름으로 검색",
  loading: "검색 중…",
  noMatches: "검색 결과가 없습니다.",
  unavailable: "목록을 불러올 수 없습니다. 접근 권한을 확인해 주세요.",
  retry: "다시 시도",
  loadMore: "더 보기",
  currentUser: "본인",
  locked: "고정",
  remove: (name) => `${name} 제거`,
};

export interface DirectoryPickerProps {
  label: string;
  provider: DirectoryProvider;
  value: readonly DirectoryOption[];
  onChange: (value: DirectoryOption[]) => void;
  currentUserId?: string;
  lockedKeys?: ReadonlySet<string>;
  kinds?: readonly DirectoryKind[];
  labels?: Partial<DirectoryPickerLabels>;
  disabled?: boolean;
  pageSize?: number;
  className?: string;
}

/** Selection is presentation state. The receiving app must recheck every subject on save. */
export function DirectoryPicker({
  label,
  provider,
  value,
  onChange,
  currentUserId,
  lockedKeys,
  kinds = ["user", "group"],
  labels: suppliedLabels,
  disabled = false,
  pageSize = 20,
  className,
}: DirectoryPickerProps) {
  const labels = { ...defaultLabels, ...suppliedLabels };
  const allowedKinds = [...new Set(kinds)];
  const [chosenKind, setChosenKind] = useState<DirectoryKind>(
    allowedKinds[0] ?? "user",
  );
  const kind = allowedKinds.includes(chosenKind)
    ? chosenKind
    : (allowedKinds[0] ?? "user");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [items, setItems] = useState<DirectoryOption[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [active, setActive] = useState(-1);
  const epoch = useRef(0);
  const request = useRef<AbortController | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const pointerActive = useRef(false);
  const id = useId();
  const [search, setSearch] = useState("");
  const typed = query.trim();
  useEffect(() => {
    if (typed === search) return;
    const timer = setTimeout(() => setSearch(typed), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  }, [typed, search]);
  const normalizedQuery = search;
  const limit = Number.isFinite(pageSize)
    ? Math.max(1, Math.min(100, Math.trunc(pageSize)))
    : 20;
  const enabled = expanded && !disabled && allowedKinds.length > 0;

  useEffect(() => {
    const owner = input.current!.ownerDocument;
    let releaseTimer: ReturnType<typeof setTimeout> | undefined;
    const down = () => {
      pointerActive.current = true;
    };
    const release = () => {
      clearTimeout(releaseTimer);
      releaseTimer = setTimeout(() => {
        pointerActive.current = false;
      }, 0);
    };
    const click = (event: MouseEvent) => {
      // Let the clicked control run before collapsing an inline list moves it.
      pointerActive.current = false;
      if (!container.current?.contains(event.target as Node))
        setExpanded(false);
    };
    owner.addEventListener("pointerdown", down, true);
    owner.addEventListener("pointerup", release, true);
    owner.addEventListener("pointercancel", release, true);
    owner.addEventListener("click", click);
    return () => {
      clearTimeout(releaseTimer);
      owner.removeEventListener("pointerdown", down, true);
      owner.removeEventListener("pointerup", release, true);
      owner.removeEventListener("pointercancel", release, true);
      owner.removeEventListener("click", click);
    };
  }, []);

  const accept = (
    page: DirectoryPage,
    previous: readonly DirectoryOption[],
  ) => {
    if (
      !Array.isArray(page.items) ||
      page.items.some(
        (item) =>
          item.kind !== kind || !item.id || typeof item.name !== "string",
      ) ||
      (page.nextCursor !== null && typeof page.nextCursor !== "string")
    )
      throw new Error("invalid_directory_page");
    setItems(mergeDirectoryOptions(previous, page.items, currentUserId));
    setCursor(page.nextCursor);
  };

  useEffect(() => {
    const version = ++epoch.current;
    request.current?.abort();
    setItems([]);
    setCursor(null);
    setError(false);
    setLoading(false);
    setActive(-1);
    if (!enabled) return;
    const controller = new AbortController();
    request.current = controller;
    setLoading(true);
    void Promise.resolve()
      .then(() =>
        provider.load({
          kind,
          query: normalizedQuery,
          limit,
          signal: controller.signal,
        }),
      )
      .then((page) => {
        if (version !== epoch.current || controller.signal.aborted) return;
        accept(page, []);
      })
      .catch(() => {
        if (version !== epoch.current || controller.signal.aborted) return;
        setItems([]);
        setCursor(null);
        setError(true);
      })
      .finally(() => {
        if (version === epoch.current && !controller.signal.aborted)
          setLoading(false);
      });
    return () => {
      epoch.current += 1;
      controller.abort();
      request.current?.abort();
    };
    // Every provider change clears former-app data; callers should memoize their adapter.
  }, [provider, kind, normalizedQuery, limit, enabled, attempt, currentUserId]);

  const more = () => {
    if (loading || !cursor || !enabled) return;
    input.current?.focus();
    const version = ++epoch.current;
    request.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    const previous = items;
    setLoading(true);
    setError(false);
    void Promise.resolve()
      .then(() =>
        provider.load({
          kind,
          query: normalizedQuery,
          cursor,
          limit,
          signal: controller.signal,
        }),
      )
      .then((page) => {
        if (version !== epoch.current || controller.signal.aborted) return;
        if (page.nextCursor === cursor)
          throw new Error("repeated_directory_cursor");
        accept(page, previous);
      })
      .catch(() => {
        if (version !== epoch.current || controller.signal.aborted) return;
        // Do not leave cached candidates selectable after an access failure.
        setItems([]);
        setCursor(null);
        setError(true);
      })
      .finally(() => {
        if (version === epoch.current && !controller.signal.aborted)
          setLoading(false);
      });
  };
  const toggle = (option: DirectoryOption) => {
    if (disabled) return;
    const key = directoryOptionKey(option);
    const selected = value.some((item) => directoryOptionKey(item) === key);
    if (selected && lockedKeys?.has(key)) return;
    input.current?.focus();
    onChange(
      selected
        ? value.filter((item) => directoryOptionKey(item) !== key)
        : [...value, option],
    );
  };
  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Escape") {
      if (expanded) {
        event.preventDefault();
        event.stopPropagation();
        setExpanded(false);
      }
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setExpanded(true);
      if (items.length)
        setActive((index) =>
          event.key === "ArrowDown"
            ? (index + 1) % items.length
            : index <= 0
              ? items.length - 1
              : index - 1,
        );
    } else if (event.key === "Enter" && expanded && items[active]) {
      event.preventDefault();
      toggle(items[active]);
    }
  };
  const selectedKeys = new Set(value.map(directoryOptionKey));
  const activeId =
    expanded && active >= 0 && active < items.length
      ? `${id}-option-${active}`
      : undefined;
  return (
    <div
      ref={container}
      className={classNames("miy-directory", className)}
      onBlur={(event) => {
        if (
          !pointerActive.current &&
          !event.currentTarget.contains(event.relatedTarget as Node | null)
        )
          setExpanded(false);
      }}
    >
      <label className="miy-directory__label" htmlFor={id}>
        {label}
      </label>
      {value.length ? (
        <div className="miy-directory__chips" aria-label={`${label} 선택됨`}>
          {value.map((option) => {
            const key = directoryOptionKey(option),
              locked = lockedKeys?.has(key);
            return (
              <span key={key} className="miy-directory__chip">
                <span className="miy-avatar" aria-hidden="true">
                  {option.kind === "group"
                    ? "#"
                    : (Array.from(option.name.trim())[0] ?? "?")}
                </span>
                <span>{option.name}</span>
                {option.description ? (
                  <small>{option.description}</small>
                ) : null}
                {locked ? (
                  <small>{labels.locked}</small>
                ) : (
                  <button
                    type="button"
                    disabled={disabled}
                    aria-label={labels.remove(option.name)}
                    onClick={() => toggle(option)}
                  >
                    ×
                  </button>
                )}
              </span>
            );
          })}
        </div>
      ) : null}
      {allowedKinds.length > 1 ? (
        <div
          className="miy-directory__kinds"
          role="group"
          aria-label={`${label} 유형`}
        >
          {allowedKinds.map((item) => (
            <Button
              key={item}
              variant={kind === item ? "primary" : "ghost"}
              aria-pressed={kind === item}
              disabled={disabled}
              onClick={() => {
                setChosenKind(item);
                setExpanded(true);
                input.current?.focus();
              }}
            >
              {item === "user" ? labels.users : labels.groups}
            </Button>
          ))}
        </div>
      ) : null}
      <input
        ref={input}
        id={id}
        className="miy-input"
        type="search"
        role="combobox"
        autoComplete="off"
        value={query}
        placeholder={labels.searchPlaceholder}
        disabled={disabled || allowedKinds.length === 0}
        aria-expanded={enabled}
        aria-controls={enabled ? `${id}-results` : undefined}
        aria-autocomplete="list"
        aria-activedescendant={activeId}
        onFocus={() => setExpanded(true)}
        onChange={(event) => {
          setQuery(event.target.value);
          setExpanded(true);
        }}
        onKeyDown={keyDown}
      />
      {enabled ? (
        <div className="miy-directory__results">
          {error ? (
            <ContentState
              kind="error"
              title={labels.unavailable}
              action={{
                label: labels.retry,
                onClick: () => {
                  input.current?.focus();
                  setAttempt((value) => value + 1);
                },
              }}
            />
          ) : null}
          <ul
            id={`${id}-results`}
            role="listbox"
            aria-label={`${label} 검색 결과`}
            aria-multiselectable="true"
            aria-busy={loading}
          >
            {items.map((option, index) => {
              const key = directoryOptionKey(option),
                selected = selectedKeys.has(key),
                locked = selected && lockedKeys?.has(key);
              return (
                <li key={key} role="none">
                  <button
                    type="button"
                    id={`${id}-option-${index}`}
                    role="option"
                    tabIndex={-1}
                    aria-selected={selected}
                    aria-disabled={locked || undefined}
                    className={classNames(
                      "miy-directory__option",
                      active === index && "miy-directory__option--active",
                    )}
                    onMouseDown={(event) => event.preventDefault()}
                    onMouseEnter={() => setActive(index)}
                    onClick={() => toggle(option)}
                  >
                    <span className="miy-avatar" aria-hidden="true">
                      {option.kind === "group"
                        ? "#"
                        : (Array.from(option.name.trim())[0] ?? "?")}
                    </span>
                    <span className="miy-directory__name">
                      <span>
                        {option.name}
                        {option.kind === "user" &&
                        option.id === currentUserId ? (
                          <small> · {labels.currentUser}</small>
                        ) : null}
                      </span>
                      {option.description ? (
                        <small>{option.description}</small>
                      ) : null}
                    </span>
                    {selected ? <span aria-hidden="true">✓</span> : null}
                  </button>
                </li>
              );
            })}
          </ul>
          {loading ? (
            <Loading label={labels.loading} />
          ) : !error && !items.length ? (
            <p className="miy-directory__empty" role="status">
              {labels.noMatches}
            </p>
          ) : null}
          {!error && cursor ? (
            <Button
              className="miy-directory__more"
              busy={loading}
              onClick={more}
            >
              {labels.loadMore}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
