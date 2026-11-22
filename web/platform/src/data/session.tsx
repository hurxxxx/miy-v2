import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { DateFormat, Locale, ThemePreference } from "@miy/frame-protocol";
import { useDisplaySettings } from "./settings";

// Two business-app portals (prod and dev) and the management console are three
// origins. Dev app code never shares a JavaScript origin with prod apps.
export type BrowserSurface = "portal" | "dev_portal" | "console";
export type LoginOptions = {
  surface: BrowserSurface;
  portalOrigin: string;
  devPortalOrigin: string;
  consoleOrigin: string;
  /**
   * A server from before the dev portal split (no devPortalOrigin): its portal
   * still serves both environments, so this UI keeps that behaviour. The server
   * decides access either way; this only avoids breaking an older deployment.
   */
  legacyPortal?: boolean;
  developmentLogin: boolean;
  developmentOrigin: string;
  accounts: { account: string; label: string; description: string }[];
  oauthUrl: string;
  /** This origin's start of the identity provider's own password change form. */
  passwordUrl?: string;
  /** An administrator can issue invitation and password reset links on this server. */
  accountLinks?: boolean;
};
/** A person's own settings (GET /api/v1/me `preferences`). Presentation only. */
export type Preferences = {
  theme: ThemePreference;
  locale: Locale;
  timeZone: string;
  dateFormat: DateFormat;
  jobTitle: string | null;
  /** App IDs pinned to the launcher, in display order. */
  pinnedApps: string[];
  pinnedPanels: string[];
};
export type User = {
  id: string;
  name: string;
  preferences: Preferences;
  admin: boolean;
  csrf: string;
  browserPurpose: BrowserSurface | null;
  portalOrigin: string;
  devPortalOrigin: string;
  consoleOrigin: string;
  canManageConsole: boolean;
};

export function surfaceOrigin(
  options: Pick<LoginOptions, "portalOrigin" | "devPortalOrigin" | "consoleOrigin">,
  surface: BrowserSurface,
) {
  return {
    portal: options.portalOrigin,
    dev_portal: options.devPortalOrigin,
    console: options.consoleOrigin,
  }[surface];
}

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
  isCurrentSession: (csrf: string) => boolean;
  /** Merge only the fields written by this request, for the session that sent it. */
  updateUser: (csrf: string, saved: Pick<User, "name" | "preferences">, fields: (keyof Preferences | "name")[]) => void;
};

const SessionContext = createContext<SessionContextValue | null>(null);
const SESSION_CHANNEL = "miy:browser-session";

export function announceBrowserSessionChange() {
  if (typeof BroadcastChannel === "undefined") return;
  const channel = new BroadcastChannel(SESSION_CHANNEL);
  channel.postMessage("changed");
  channel.close();
}

function validateOptions(received: LoginOptions): LoginOptions {
  if (!["portal", "dev_portal", "console"].includes(received.surface))
    throw new Error("invalid_surface");
  const legacyPortal =
    received.devPortalOrigin === undefined && received.surface !== "dev_portal";
  const value = legacyPortal
    ? { ...received, devPortalOrigin: received.portalOrigin, legacyPortal }
    : received;
  const origins = legacyPortal
    ? [value.portalOrigin, value.consoleOrigin]
    : [value.portalOrigin, value.devPortalOrigin, value.consoleOrigin];
  for (const origin of origins) {
    const url = new URL(origin);
    if (!/^https?:$/.test(url.protocol) || url.origin !== origin)
      throw new Error("invalid_surface_origin");
  }
  if (
    new Set(origins).size !== origins.length ||
    window.location.origin !== surfaceOrigin(value, value.surface)
  )
    throw new Error("invalid_surface_origin");
  // The password change starts on this origin and nowhere else.
  const passwordUrl =
    value.passwordUrl === `${window.location.origin}/auth/password` ? value.passwordUrl : undefined;
  const checked = { ...value, passwordUrl, accountLinks: value.accountLinks === true };
  return checked.developmentLogin &&
    checked.developmentOrigin !== window.location.origin
    ? { ...checked, developmentLogin: false, accounts: [] }
    : checked;
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
    (!options.legacyPortal && user.devPortalOrigin !== options.devPortalOrigin) ||
    user.consoleOrigin !== options.consoleOrigin
  )
    throw new Error("session_surface_mismatch");
  return user;
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SessionState>({ status: "loading" });
  const display = useDisplaySettings();
  const adopt = display.update;
  const [options, setOptions] = useState<LoginOptions>();
  const [attempt, setAttempt] = useState(0);
  const controller = useRef<AbortController>(undefined);
  const currentOptions = useRef<LoginOptions>(undefined);
  const sessionCsrf = useRef<string>(undefined);
  const isCurrentSession = useCallback((csrf: string) => csrf === sessionCsrf.current, []);
  const retry = useCallback(() => {
    controller.current?.abort();
    sessionCsrf.current = undefined;
    setState({ status: "loading" });
    setAttempt((value) => value + 1);
  }, []);
  useEffect(() => {
    const channel = typeof BroadcastChannel === "undefined" ? undefined : new BroadcastChannel(SESSION_CHANNEL);
    channel?.addEventListener("message", (event) => {
      if (event.data === "changed") retry();
    });
    let restoreCheck: AbortController | undefined;
    const restore = (event: PageTransitionEvent) => {
      const options = currentOptions.current;
      const csrf = sessionCsrf.current;
      if (!event.persisted || !options) return;
      restoreCheck?.abort();
      const check = new AbortController();
      restoreCheck = check;
      void readSession(options, check.signal).then((user) => {
        if (!check.signal.aborted && sessionCsrf.current === csrf && user?.csrf !== csrf) retry();
      }).catch(() => undefined);
    };
    window.addEventListener("pageshow", restore);
    return () => {
      channel?.close();
      restoreCheck?.abort();
      window.removeEventListener("pageshow", restore);
    };
  }, [retry]);
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
        if (!current.signal.aborted) {
          currentOptions.current = next;
          setOptions(next);
        }
        return readSession(next, current.signal);
      })
      .then((user) => {
        if (current.signal.aborted) return;
        // The saved choice wins over this browser's copy from now on (a server
        // without saved preferences leaves this browser's copy in effect).
        sessionCsrf.current = user?.csrf;
        if (user?.preferences) adopt(displayOf(user.preferences));
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
  }, [attempt, adopt]);
  const updateUser = useCallback(
    (csrf: string, saved: Pick<User, "name" | "preferences">, fields: (keyof Preferences | "name")[]) => {
      if (csrf !== sessionCsrf.current) return;
      const preferences = Object.fromEntries(
        fields.filter((field) => field !== "name").map((field) => [field, saved.preferences[field]]),
      ) as Partial<Preferences>;
      adopt(preferences);
      setState((current) => {
        if (current.status !== "ready" || current.user.csrf !== csrf) return current;
        const user = current.user;
        return {
          status: "ready",
          user: {
            ...user,
            name: fields.includes("name") ? saved.name : user.name,
            preferences: { ...user.preferences, ...preferences },
          },
        };
      });
    },
    [adopt],
  );
  return (
    <SessionContext.Provider value={{ state, options, retry, isCurrentSession, updateUser }}>
      {children}
    </SessionContext.Provider>
  );
}

export function displayOf(preferences: Preferences) {
  return {
    theme: preferences.theme,
    locale: preferences.locale,
    timeZone: preferences.timeZone,
    dateFormat: preferences.dateFormat,
  };
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
