import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "*.spec.ts",
  workers: 1,
  fullyParallel: false,
  timeout: 120000,
  expect: { timeout: 15000 },
  reporter: [
    ["list"],
    ["json", { outputFile: ".local/evidence/playwright.json" }],
  ],
  use: {
    baseURL: "https://localhost:18443",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});
