import { defineConfig, devices } from "@playwright/test";
// Business app frame checks against the disposable full stack started by
// stack-ui.sh (`up`). Browsers: MIY_FRAME_BROWSERS=chromium,firefox,webkit.
const browsers = (process.env.MIY_FRAME_BROWSERS || "chromium,firefox").split(",");
const device = { chromium: devices["Desktop Chrome"], firefox: devices["Desktop Firefox"], webkit: devices["Desktop Safari"] };
export default defineConfig({
  testDir: "../../tests",
  testMatch: "app-frames.spec.ts",
  workers: 1,
  timeout: 300000,
  expect: { timeout: 30000 },
  outputDir: "../../.local/evidence/app-frames/results",
  reporter: [["list"], ["json", { outputFile: "../../.local/evidence/app-frames/results.json" }]],
  projects: browsers.map((name) => ({
    name,
    use: { ...device[name as keyof typeof device], headless: true, trace: "off", screenshot: "only-on-failure" },
  })),
});
