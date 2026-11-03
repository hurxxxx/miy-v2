import { defineConfig } from "@playwright/test";
// Browser checks against the real core server (current source) and a disposable database.
export default defineConfig({
  testDir: "../../tests",
  testMatch: "server-ui.spec.ts",
  workers: 1,
  timeout: 60000,
  expect: { timeout: 10000 },
  outputDir: "../../.local/evidence/server-ui/results",
  reporter: [["list"], ["json", { outputFile: "../../.local/evidence/server-ui/results.json" }]],
  use: { baseURL: "http://127.0.0.1:18148", headless: true, screenshot: "only-on-failure" },
  webServer: {
    command: "node scripts/runtime-regression/server-ui.mjs",
    cwd: "../..",
    url: "http://127.0.0.1:18148/auth/options",
    reuseExistingServer: false,
    timeout: 240000,
    // SIGTERM lets the harness remove its disposable database container.
    gracefulShutdown: { signal: "SIGTERM", timeout: 15000 },
  },
});
