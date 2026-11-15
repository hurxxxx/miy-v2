import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: [
    "common-ui.spec.ts",
    "sharing.spec.ts",
    "files.spec.ts",
    "common-work.spec.ts",
  ],
  workers: 1,
  timeout: 30000,
  expect: { timeout: 5000 },
  outputDir: "../../../.local/evidence/common-ui/results",
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:19871", headless: true },
  webServer: {
    command: "node packages/ui/tests/server.mjs",
    cwd: "../../..",
    url: "http://127.0.0.1:19871",
    reuseExistingServer: false,
  },
});
