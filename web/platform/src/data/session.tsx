import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

export type BrowserSurface = "portal" | "console";
export type LoginOptions = {
  surface: BrowserSurface;
  portalOrigin: string;
  consoleOrigin: string;
  developmentLogin: boolean;
  developmentOrigin: string;
  accounts: { account: string; label: string; description: string }[];
  oauthUrl: string;
};
export type User = {
  id: string;
  name: string;
  admin: boolean;
  csrf: string;
  browserPurpose: BrowserSurface | null;
  portalOrigin: string;
  consoleOrigin: string;
  canManageConsole: boolean;
};

// A failed read is never shown as "signed out": only HTTP 401 means anonymous.
export type SessionState =
  | { status: "loading" }
  | { status: "anonymous" }
  | { status: "ready"; user: User }
  | { status: "error"; code: string };

type SessionContextValue = {
  state: SessionState;
  options?: LoginOptions;
  retry: () => void;
};

const SessionContext = createContext<SessionContextValue | null>(null);

function validateOptions(value: LoginOptions): LoginOptions {
  if (value.surface !== "portal" && value.surface !== "console")
    throw new Error("invalid_surface");
  for (const origin of [value.portalOrigin, value.consoleOrigin]) {
    const url = new URL(origin);
    if (!/^https?:$/.test(url.protocol) || url.origin !== origin)
      throw new Error("invalid_surface_origin");
  }
  if (
    value.portalOrigin === value.consoleOrigin ||
    window.location.origin !==
      (value.surface === "portal" ? value.portalOrigin : value.consoleOrigin)
  )
    throw new Error("invalid_surface_origin");
  return value.developmentLogin &&
    value.developmentOrigin !== window.location.origin
    ? { ...value, developmentLogin: false, accounts: [] }
    : value;
}

async function readSession(
  options: LoginOptions,
  signal: AbortSignal,
): Promise<User | null> {
  const response = await fetch("/api/v1/me", {
    credentials: "same-origin",
    signal,
  });
  if (response.status === 401) return null;
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const user = (await response.json()) as User;
  if (
    user.browserPurpose !== options.surface ||
    user.portalOrigin !== options.portalOrigin ||
    user.consoleOrigin !== options.consoleOrigin
  )
    throw new Error("session_surface_mismatch");
  return user;
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SessionState>({ status: "loading" });
  const [options, setOptions] = useState<LoginOptions>();
  const [attempt, setAttempt] = useState(0);
  const controller = useRef<AbortController>(undefined);
  useEffect(() => {
    controller.current?.abort();
    const current = new AbortController();
    controller.current = current;
    setState({ status: "loading" });
    setOptions(undefined);
    fetch("/auth/options", {
      credentials: "same-origin",
      signal: current.signal,
    })
      .then(async (response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const next = validateOptions((await response.json()) as LoginOptions);
        if (!current.signal.aborted) setOptions(next);
        return readSession(next, current.signal);
      })
      .then((user) => {
        if (!current.signal.aborted)
          setState(user ? { status: "ready", user } : { status: "anonymous" });
      })
      .catch((error: unknown) => {
        if (!current.signal.aborted)
          setState({
            status: "error",
            code: error instanceof Error ? error.message : "network_error",
          });
      });
    return () => current.abort();
  }, [attempt]);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);
  return (
    <SessionContext.Provider value={{ state, options, retry }}>
      {children}
    </SessionContext.Provider>
  );
}

export function useSession() {
  const value = useContext(SessionContext);
  if (!value) throw new Error("useSession outside SessionProvider");
  return value;
}

/** The signed-in user; only valid below the shell's ready state. */
export function useUser(): User {
  const { state } = useSession();
  if (state.status !== "ready") throw new Error("useUser before sign-in");
  return state.user;
}

/** Server-selected surface; URL paths and query parameters never select it. */
export function useSurface(): LoginOptions {
  const { options } = useSession();
  if (!options) throw new Error("useSurface before configuration");
  return options;
}
