import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: ["cli-experience.spec.ts", "cli-install.spec.ts"],
  outputDir: "../.local/evidence/cli-experience/browser",
  workers: 1,
  timeout: 30000,
  reporter: [
    ["list"],
    [
      "json",
      { outputFile: ".local/evidence/cli-experience/browser-results.json" },
    ],
  ],
  use: {
    baseURL: "http://127.0.0.1:18763",
    channel: process.platform === "win32" ? "chrome" : undefined,
    screenshot: "only-on-failure",
  },
  webServer: {
    command:
      "pnpm --filter @miy/platform dev --host 127.0.0.1 --port 18763 --strictPort",
    url: "http://127.0.0.1:18763",
    reuseExistingServer: false,
    timeout: 60000,
  },
});
