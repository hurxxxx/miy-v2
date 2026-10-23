export { createNavigationGuards, type NavigationGuard } from "./navigation";
export const UI_CONTRACT = "miy.ui/v1alpha1" as const;
export interface Installation {
  id: string;
  appId: string;
  name: string;
  environment: "dev" | "prod";
  releaseId: string;
  version?: string;
  canPromote?: boolean;
  dbStatus: string;
  runtimeStatus: string;
  desiredState: string;
  kind: "validation_fixture" | "docker_app";
  canManage?: boolean;
  imageId?: string;
  sourceCommit?: string;
  ui: { entry: string; export: "./AppRoot"; contractVersion: string };
}
export interface AppRuntime {
  installationId: string;
  appId: string;
  releaseId: string;
  basePath: string;
  location: { pathname: string; search: string; hash: string };
  navigate: (relative: string, options?: { replace?: boolean }) => void;
  api: <T>(path: string, init?: RequestInit) => Promise<T>;
  platformApi: <T>(resource: "directory") => Promise<T>;
  appApi: <T>(appId: string, path: string, init?: RequestInit) => Promise<T>;
  /** Register in an effect and return its disposer. Hosts guard app/shell/history/unload. */
  registerNavigationGuard: (guard: () => string | undefined) => () => void;
  theme: "light";
}
export interface AppProps {
  runtime: AppRuntime;
}
export function appTarget(basePath: string, relative: string): string {
  if (!relative.startsWith("/") || relative.startsWith("//"))
    throw new Error("Use an app-relative path");
  const u = new URL(basePath + relative, "https://miy.invalid");
  if (
    u.origin !== "https://miy.invalid" ||
    !u.pathname.startsWith(basePath + "/")
  )
    throw new Error("Navigation outside this app is denied");
  return u.pathname + u.search + u.hash;
}
export async function request<T>(
  path: string,
  csrf?: string,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...(csrf ? { "x-miy-csrf": csrf } : {}),
      ...init.headers,
    },
  });
  const data = await response.json();
  if (!response.ok)
    throw new Error(data.error?.code || `HTTP ${response.status}`);
  return data;
}
