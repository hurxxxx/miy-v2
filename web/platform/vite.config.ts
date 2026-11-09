import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// The portal no longer hosts app modules: business apps run in their own frame
// origins (web/frame-runtime, docs/APP-FRAMES.md).
export default defineConfig({
  plugins: [react()],
  build: { target: "esnext" },
});
