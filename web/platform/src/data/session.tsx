import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

export type User = { id: string; name: string; admin: boolean; csrf: string };

// A failed read is never shown as "signed out": only HTTP 401 means anonymous.
export type SessionState =
  | { status: "loading" }
  | { status: "anonymous" }
  | { status: "ready"; user: User }
  | { status: "error"; code: string };

type SessionContextValue = { state: SessionState; retry: () => void };

const SessionContext = createContext<SessionContextValue | null>(null);

async function readSession(signal: AbortSignal): Promise<User | null> {
  const response = await fetch("/api/v1/me", { credentials: "same-origin", signal });
  if (response.status === 401) return null;
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return (await response.json()) as User;
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SessionState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const controller = useRef<AbortController>(undefined);
  useEffect(() => {
    controller.current?.abort();
    const current = new AbortController();
    controller.current = current;
    setState({ status: "loading" });
    readSession(current.signal)
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
  return <SessionContext.Provider value={{ state, retry }}>{children}</SessionContext.Provider>;
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
