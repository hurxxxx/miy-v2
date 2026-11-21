import { createRoot } from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import "./styles/index.css";
import { SessionProvider } from "./data/session";
import { applyDocumentSettings, DisplaySettingsProvider, readStoredSettings } from "./data/settings";
import { router } from "./router";

// Before the first paint, so a dark preference never flashes light.
applyDocumentSettings(readStoredSettings(), window.matchMedia("(prefers-color-scheme: dark)").matches);

createRoot(document.getElementById("root")!).render(
  <DisplaySettingsProvider>
    <SessionProvider>
      <RouterProvider router={router} />
    </SessionProvider>
  </DisplaySettingsProvider>,
);
