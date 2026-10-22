// Invoke the real Codex CLI in an independent app directory. Never log OAuth codes or credentials.
import { spawn, execFileSync } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  createWriteStream,
} from "node:fs";
import { resolve, join } from "node:path";
import { chromium } from "@playwright/test";
const app = resolve(process.argv[2]);
const prompt = readFileSync(process.argv[3], "utf8");
const label = process.argv[4] || "leave-codex";
if (!/^[a-z0-9-]+$/.test(label)) throw Error("Invalid evidence label");
const root = process.cwd(),
  local = join(root, ".local");
const credentials = JSON.parse(
  readFileSync(join(local, "credentials.json"), "utf8"),
);
const ca = join(app, ".miy-tools/ca.pem");
const roots = [
  readFileSync("/etc/ssl/cert.pem", "utf8"),
  readFileSync(
    join(local, "caddy/caddy/pki/authorities/local/root.crt"),
    "utf8",
  ),
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
mkdirSync(join(app, ".miy-tools"), { recursive: true });
writeFileSync(ca, roots.join("\n"));
const env = {
  PATH: process.env.HOME + "/.cargo/bin:" + process.env.PATH,
  HOME: process.env.HOME,
  TMPDIR: process.env.TMPDIR,
  LANG: "en_US.UTF-8",
  SSL_CERT_FILE: ca,
  NODE_EXTRA_CA_CERTS: ca,
};
const overrides = [
  "-c",
  "mcp_servers={}",
  "-c",
  'mcp_servers.miy_app.url="https://localhost:18443/mcp"',
  "-c",
  "mcp_servers.miy_app.required=true",
  "-c",
  'mcp_servers.miy_app.oauth.client_id="miy-codex"',
  "-c",
  'mcp_servers.miy_app.oauth.callback_url="http://127.0.0.1:18555/callback"',
  "-c",
  "mcp_servers.miy_app.oauth.callback_port=18555",
  "-c",
  'mcp_servers.miy_app.scopes=["miy.read","miy.dev","offline_access"]',
  "-c",
  'mcp_servers.miy_app.enabled_tools=["get_contract","search_api","get_api","validate_manifest"]',
];
for (const tool of [
  "get_contract",
  "search_api",
  "get_api",
  "validate_manifest",
])
  overrides.push(
    "-c",
    `mcp_servers.miy_app.tools.${tool}.approval_mode="approve"`,
  );
const login = spawn(
  "codex",
  [
    ...overrides,
    "mcp",
    "login",
    "miy_app",
    "--scopes",
    "miy.read,miy.dev,offline_access",
    "--no-browser",
  ],
  { env, stdio: ["pipe", "pipe", "pipe"] },
);
let captured = "";
let resolveUrl;
const urlPromise = new Promise((r) => (resolveUrl = r));
for (const stream of [login.stdout, login.stderr])
  stream.on("data", (b) => {
    captured += b;
    const match = captured.match(/https:\/\/localhost:18443\/identity\/[^\s]+/);
    if (match) resolveUrl(match[0]);
  });
const loginDone = new Promise((r) =>
  login.on("exit", (code) => {
    r(code);
    if (!captured.match(/https:\/\/localhost:18443\/identity\/[^\s]+/))
      resolveUrl(null);
  }),
);
const timeout = setTimeout(() => {
  login.kill();
  resolveUrl(null);
}, 45000);
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
try {
  const url = await urlPromise;
  if (!url)
    throw Error(
      "OAuth login failed: " + captured.replace(/https?:\/\/[^\s]+/g, "[URL]"),
    );
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.route("http://127.0.0.1:18555/**", async (route) => {
    login.stdin.write(route.request().url() + "\n");
    await route.fulfill({ status: 200, body: "Callback delivered." });
  });
  await page.goto(url);
  if (await page.locator("#username").count()) {
    await page.locator("#username").fill("developer");
    await page.locator("#password").fill(credentials.developer);
    await page.locator("#kc-login").click();
  }
  if ((await loginDone) !== 0) throw Error("OAuth login failed");
  clearTimeout(timeout);
  await context.close();
} finally {
  clearTimeout(timeout);
  login.kill();
  await browser.close();
}
mkdirSync(join(local, "evidence"), { recursive: true });
// macOS policy independently denies the platform repository and Docker socket.
const allowed = [
  app,
  join(process.env.HOME, ".cargo"),
  join(process.env.HOME, ".codex"),
  "/private/var/folders",
  "/private/tmp",
  "/dev",
];
const policy = `(version 1)(allow default)(deny file-read* file-write* (subpath "${root}"))(deny file-read* file-write* (subpath "${join(process.env.HOME, "projects/miy")}"))(deny file-read* file-write* (subpath "${join(process.env.HOME, ".docker/run")}"))(deny file-write* (require-all ${allowed.map((p) => `(require-not (subpath "${p}"))`).join(" ")}))`;
// Codex's nested macOS sandbox cannot initialize inside sandbox-exec. The outer
// OS policy enforces the above file boundaries for Codex and all child tools.
const args = [
  "--no-daemon",
  ...overrides,
  "-a",
  "never",
  "exec",
  "--ignore-user-config",
  "--ignore-rules",
  "--ephemeral",
  "--sandbox",
  "danger-full-access",
  "-C",
  app,
  "--json",
  "-",
];
const child = spawn("sandbox-exec", ["-p", policy, "codex", ...args], {
  cwd: app,
  env,
  stdio: ["pipe", "pipe", "pipe"],
});
const events = createWriteStream(join(local, `evidence/${label}.jsonl`), {
  mode: 0o600,
});
const errors = createWriteStream(join(local, `evidence/${label}.stderr`), {
  mode: 0o600,
});
child.stdout.pipe(events);
child.stderr.pipe(errors);
child.stdin.end(prompt);
console.log(
  "Real Codex started in " +
    app +
    "; MCP OAuth authenticated as developer; platform source and Docker socket denied.",
);
const timer = setTimeout(() => child.kill(), 30 * 60 * 1000);
const code = await new Promise((r) => child.on("exit", r));
clearTimeout(timer);
events.end();
errors.end();
console.log("Codex exit code: " + code);
process.exitCode = code ?? 1;
