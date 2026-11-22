import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// The portal no longer hosts app modules: business apps run in their own frame
// origins (web/frame-runtime, docs/APP-FRAMES.md). Tailwind CSS 4 styles the
// screens ported from AI-DO, the same way AI-DO's web app builds them.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: { target: "esnext" },
});
