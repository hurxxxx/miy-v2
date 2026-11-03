import { createRoot } from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import "./style.css";
import { SessionProvider } from "./data/session";
import { router } from "./router";

createRoot(document.getElementById("root")!).render(
  <SessionProvider>
    <RouterProvider router={router} />
  </SessionProvider>,
);
