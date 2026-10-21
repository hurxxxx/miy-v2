// Real Codex OAuth + MCP call. Credentials remain in memory/the client's store.
import { chromium } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
const credentials = JSON.parse(readFileSync(".local/credentials.json", "utf8"));
const origin = "https://localhost:18443";
const roots = [
  readFileSync("/etc/ssl/cert.pem", "utf8"),
  readFileSync(".local/caddy/caddy/pki/authorities/local/root.crt", "utf8"),
];
for (const keychain of [
  "/System/Library/Keychains/SystemRootCertificates.keychain",
  "/Library/Keychains/System.keychain",
]) {
  try {
    roots.push(
      execFileSync("security", ["find-certificate", "-a", "-p", keychain], {
        encoding: "utf8",
      }),
    );
  } catch {}
}
writeFileSync(".local/combined-ca.pem", roots.join("\n"));
const env = {
  ...process.env,
  SSL_CERT_FILE: resolve(".local/combined-ca.pem"),
  NODE_EXTRA_CA_CERTS: resolve(
    ".local/caddy/caddy/pki/authorities/local/root.crt",
  ),
};
const overrides = [
  "-c",
  "mcp_servers={}",
  "-c",
  `mcp_servers.miy_validation.url="${origin}/mcp"`,
  "-c",
  'mcp_servers.miy_validation.oauth.client_id="miy-codex"',
  "-c",
  'mcp_servers.miy_validation.oauth.callback_url="http://127.0.0.1:18555/callback"',
  "-c",
  "mcp_servers.miy_validation.oauth.callback_port=18555",
];
overrides.push(
  "-c",
  'mcp_servers.miy_validation.enabled_tools=["get_contract"]',
  "-c",
  'mcp_servers.miy_validation.tools.get_contract.approval_mode="approve"',
);
overrides.push(
  "-c",
  'mcp_servers.miy_validation.scopes=["miy.read","miy.dev","offline_access"]',
);
const login = spawn(
  "codex",
  [
    ...overrides,
    "mcp",
    "login",
    "miy_validation",
    "--scopes",
    "miy.read,miy.dev,offline_access",
    "--no-browser",
  ],
  { env, stdio: ["pipe", "pipe", "pipe"] },
);
let output = "";
let resolveUrl, rejectUrl;
const urlPromise = new Promise((r, j) => {
  resolveUrl = r;
  rejectUrl = j;
});
const done = new Promise((r) => login.on("exit", r));
function capture(b) {
  output += b.toString();
  const url = output.match(/https:\/\/localhost:18443\/identity\/[^\s]+/);
  if (url) resolveUrl(url[0]);
}
login.stdout.on("data", capture);
login.stderr.on("data", capture);
login.on("error", rejectUrl);
const timer = setTimeout(() => {
  login.kill();
  rejectUrl(new Error("Codex OAuth timed out"));
}, 45000);
let browser;
try {
  const url = await urlPromise;
  browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.route("http://127.0.0.1:18555/**", async (route) => {
    // --no-browser accepts the completed callback on stdin; no auth code is logged.
    login.stdin.write(route.request().url() + "\n");
    await route.fulfill({
      status: 200,
      contentType: "text/plain",
      body: "OAuth callback delivered to Codex.",
    });
  });
  await page.goto(url);
  if (await page.locator("#username").count()) {
    await page.locator("#username").fill("developer");
    await page.locator("#password").fill(credentials.developer);
    await page.locator("#kc-login").click();
  }
  const code = await done;
  clearTimeout(timer);
  await context.close();
  if (code !== 0)
    throw new Error("Codex OAuth login failed (credentials omitted)");
  console.log("Codex OAuth login passed.");
  mkdirSync(".local/evidence", { recursive: true });
  async function execute(label) {
    const child = spawn(
      "codex",
      [
        ...overrides,
        "exec",
        "--ephemeral",
        "--sandbox",
        "read-only",
        "--skip-git-repo-check",
        "--json",
        "Use the miy_validation MCP server get_contract tool exactly once. Reply with only the returned contract version. Do not inspect local files or call other tools.",
      ],
      { env, stdio: ["ignore", "pipe", "pipe"] },
    );
    let events = "",
      err = "";
    child.stdout.on("data", (b) => (events += b));
    child.stderr.on("data", (b) => (err += b));
    const timeout = setTimeout(() => child.kill(), 90000);
    const code = await new Promise((r) => child.on("exit", r));
    clearTimeout(timeout);
    writeFileSync(`.local/evidence/codex-${label}.jsonl`, events);
    const parsed = events
      .split("\n")
      .filter(Boolean)
      .flatMap((l) => {
        try {
          return [JSON.parse(l)];
        } catch {
          return [];
        }
      });
    const calls = parsed.filter(
      (e) =>
        e.item?.type === "mcp_tool_call" && e.item?.tool === "get_contract",
    );
    if (
      code !== 0 ||
      !calls.some(
        (e) =>
          e.item.status === "completed" &&
          !e.item.error &&
          JSON.stringify(e.item.result).includes("miy.app/v1alpha1"),
      )
    ) {
      writeFileSync(
        ".local/evidence/codex-diagnostic.txt",
        err.replace(/https?:\/\/[^\s]+/g, "[URL]"),
      );
      throw new Error(
        "Codex get_contract call did not complete; inspect local evidence.",
      );
    }
    return { exitCode: code, tool: "get_contract", completed: true };
  }
  const initial = await execute("initial");
  console.log("Codex MCP get_contract passed.");
  await new Promise((r) => setTimeout(r, 47000));
  const refreshed = await execute("refreshed");
  console.log("Codex MCP after access-token expiry passed.");
  writeFileSync(
    ".local/evidence/codex.json",
    JSON.stringify(
      {
        client: "codex-cli 0.163.0",
        oauth: "authorization_code_pkce",
        registration: "pre_registered",
        initial,
        refreshed,
      },
      null,
      2,
    ),
  );
} finally {
  clearTimeout(timer);
  login.kill();
  if (browser) await browser.close();
}
