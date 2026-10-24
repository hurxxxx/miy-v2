import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "../../tests",
  testMatch: "runtime-health.spec.ts",
  workers: 1,
  timeout: 60000,
  expect: { timeout: 10000 },
  outputDir: "../../.local/evidence/runtime-health-ui/results",
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:18147", headless: true },
  webServer: {
    command: "node scripts/runtime-regression/ui-server.mjs",
    cwd: "../..",
    url: "http://127.0.0.1:18147",
    reuseExistingServer: false,
  },
});
