import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import { registerRemotes, loadRemote } from "@module-federation/enhanced/runtime";
import { UI_CONTRACT, type AppProps, type Installation } from "@miy/sdk";

const modules = new Map<string, LazyExoticComponent<ComponentType<AppProps>>>();

/** Load the release's same-origin `./AppRoot`; one module per release. */
export function remote(app: Installation) {
  if (app.ui.contractVersion !== UI_CONTRACT) throw new Error("unsupported_ui_contract");
  const entry = new URL(app.ui.entry, window.location.origin);
  if (entry.origin !== window.location.origin || !entry.pathname.startsWith("/artifacts/"))
    throw new Error("untrusted_ui_origin");
  const key = "miy_" + app.releaseId.replaceAll("-", "");
  if (!modules.has(key)) {
    registerRemotes([{ name: key, entry: entry.href }]);
    modules.set(
      key,
      lazy(async () => {
        const module = await loadRemote<{ default: ComponentType<AppProps> }>(key + "/AppRoot");
        if (!module?.default) throw new Error("missing_AppRoot");
        return module;
      }),
    );
  }
  return modules.get(key)!;
}
