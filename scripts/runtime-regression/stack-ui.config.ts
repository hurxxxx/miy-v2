import { defineConfig } from "@playwright/test";
// Browser checks against the disposable full stack started by stack-ui.sh.
export default defineConfig({
  testDir: "../../tests",
  testMatch: "stack-ui.spec.ts",
  workers: 1,
  timeout: 600000,
  expect: { timeout: 20000 },
  outputDir: "../../.local/evidence/stack-ui/results",
  reporter: [["list"], ["json", { outputFile: "../../.local/evidence/stack-ui/results.json" }]],
  use: { baseURL: "http://localhost:18300", headless: true, trace: "off", screenshot: "only-on-failure" },
});
