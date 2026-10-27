import { defineConfig } from "@playwright/test";
import { networkInterfaces } from "node:os";

const host = process.env.MIY_HTTP_TEST_HOST ?? Object.values(networkInterfaces())
  .flat().find((entry) => entry && entry.family === "IPv4" && !entry.internal)?.address;
if (!host) throw new Error("HTTP IP verification requires a non-loopback network interface");
const baseURL = `http://${host}:18149`;

export default defineConfig({
  testDir: "../../tests",
  testMatch: "http-ip.spec.ts",
  workers: 1,
  timeout: 60000,
  expect: { timeout: 10000 },
  outputDir: "../../.local/evidence/http-ip-ui/results",
  reporter: [["list"]],
  use: { baseURL, headless: true },
  webServer: {
    command: "node scripts/runtime-regression/http-ui-server.mjs",
    cwd: "../..",
    url: baseURL,
    reuseExistingServer: false,
  },
});
