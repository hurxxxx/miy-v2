import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
export default defineConfig({
  root: "sdk/preview", base: "/",
  resolve: { alias: { "@miy/sdk": resolve("sdk/browser/index.ts"), "@miy/ui": resolve("sdk/ui/index.tsx"), "@miy/components": resolve("platform-components") } },
  plugins: [react()], build: {outDir: resolve("dist/preview"),emptyOutDir:true,target:"esnext"},
});
