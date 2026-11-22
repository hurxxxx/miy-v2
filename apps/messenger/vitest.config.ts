import { defineConfig } from "vitest/config";
import { resolve } from "node:path";

// AI-DO runs its web tests in jsdom with globals and a setup that loads i18n
// (apps/web/vite.config.mts). The ported specs assume a Korean host clock.
process.env.TZ = "Asia/Seoul";

export default defineConfig({
  resolve: {
    alias: {
      "@miy/sdk": resolve("sdk/browser/index.ts"),
      "@miy/ui": resolve("sdk/ui/index.tsx"),
      "@miy/components": resolve("platform-components"),
    },
  },
  test: {
    watch: false,
    globals: true,
    environment: "jsdom",
    setupFiles: ["./web/src/test-setup.ts"],
    include: ["web/src/**/*.spec.{ts,tsx}"],
    css: false,
  },
});
