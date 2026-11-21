import { StrictMode, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import "./app.css";
import "../../tokens/tokens.css";
import {
  directoryQuery,
  request,
  requestBlob,
  requestResponse,
  type AppRuntime,
  type NavigationState,
} from "@miy/sdk";
import AppRoot from "../../../examples/common-work/web/src/AppRoot";

declare global {
  interface Window {
    commonWork: { dirty: boolean; states: NavigationState[]; path: string };
  }
}
const initialPath = new URL(location.href).searchParams.get("path") ?? "/";
window.commonWork = { dirty: false, states: [], path: initialPath };
function Fixture() {
  const [path, setPath] = useState(initialPath);
  const runtime = useMemo<AppRuntime>(
    () => ({
      installationId: "11111111-1111-4111-8111-111111111111",
      appId: "common-work",
      releaseId: "fixture-release",
      basePath: "/apps/common-work",
      location: { pathname: path, search: "", hash: "" },
      navigate: (relative) => {
        // Host behavior only; application source never reads or changes parent history.
        const url = new URL(location.href);
        url.searchParams.set("path", relative);
        history.replaceState(null, "", url);
        window.commonWork.path = relative;
        setPath(relative);
      },
      api: (relative, init) => request(`/work-api${relative}`, undefined, init),
      apiBlob: (relative, init) =>
        requestBlob(`/work-api${relative}`, undefined, init),
      fileApi: (relative, init) =>
        requestResponse(`/work-files${relative}`, undefined, init),
      directory: (query, options) =>
        request(`/work-directory${directoryQuery(query)}`, undefined, options),
      platformApi: (_resource, query, options) =>
        request(`/work-directory${directoryQuery(query)}`, undefined, options),
      appApi: () =>
        Promise.reject(new Error("fixture_cross_app_not_supported")),
      setNavigationState: (state) => {
        window.commonWork.dirty = state.dirty;
        window.commonWork.states.push(state);
      },
      theme: "light",
      settings: {
        theme: "light",
        themePreference: "light",
        locale: "ko-KR",
        timeZone: "Asia/Seoul",
        dateFormat: "korean",
      },
    }),
    [path],
  );
  return <AppRoot runtime={runtime} />;
}
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Fixture />
  </StrictMode>,
);
