import { defineConfig } from "@playwright/test";
import base from "../scripts/runtime-regression/ui.config";

export default defineConfig({
  ...base,
  testDir: ".",
  testMatch: "surface-boundaries.spec.ts",
  outputDir: "../.local/evidence/surface-boundaries/browser",
  webServer: {
    ...base.webServer,
    command: "node scripts/runtime-regression/ui-server.mjs",
    cwd: "..",
    url: "http://127.0.0.1:18147",
    reuseExistingServer: false,
  },
});
