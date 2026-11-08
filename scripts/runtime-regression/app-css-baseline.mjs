// Compare how a remote app renders inside the shell before and after a change.
// Usage: node scripts/runtime-regression/app-css-baseline.mjs <base-commit>
// Builds the base platform in a temporary git worktree and the current one, loads the
// same reference Federation app (it uses the published ui Panel/Button and a CSS module)
// with mocked APIs, and records computed styles and screenshots of the app subtree.
import { execFileSync } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const base = process.argv[2];
if (!base) throw new Error("usage: app-css-baseline.mjs <base-commit>");
const out = resolve(root, ".local/evidence/app-css-baseline");
const worktree = join(out, "base-worktree");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit" });
const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root }).toString().trim();

run("git", ["worktree", "add", "--detach", worktree, base]);
try {
  run("pnpm", ["install", "--offline", "--frozen-lockfile"], worktree);
  run("pnpm", ["--filter", "@miy/platform", "exec", "vite", "build", "--outDir", join(out, "base-site"), "--emptyOutDir"], worktree);
} finally {
  run("git", ["worktree", "remove", "--force", worktree]);
}
run("pnpm", ["--filter", "@miy/platform", "exec", "vite", "build", "--outDir", join(out, "head-site"), "--emptyOutDir"]);
run("pnpm", ["--filter", "@miy/fixtures", "exec", "vite", "build", "--mode", "reference", "--outDir", join(out, "remote"), "--emptyOutDir"]);

function serve(dir, port) {
  return new Promise((ready) => {
    const server = createServer((req, res) => {
      let file = resolve(dir, "." + decodeURIComponent(new URL(req.url, "http://x").pathname));
      if (!file.startsWith(dir) || !existsSync(file) || statSync(file).isDirectory()) file = join(dir, "index.html");
      const type = { ".js": "text/javascript", ".css": "text/css", ".html": "text/html", ".woff2": "font/woff2", ".json": "application/json" }[extname(file)];
      res.writeHead(200, { "content-type": type ?? "application/octet-stream" });
      createReadStream(file).pipe(res);
    }).listen(port, "127.0.0.1", () => ready(server));
  });
}

const app = {
  id: "60000000-0000-4000-8000-000000000001", appId: "reference", name: "기준정보", releaseId: "70000000-0000-4000-8000-000000000001",
  environment: "dev", version: "1.0.0", dbStatus: "ready", runtimeStatus: "running", desiredState: "running", kind: "validation_fixture",
  ui: { entry: "/artifacts/reference/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};
const PROPS = ["font-family", "font-size", "font-weight", "line-height", "letter-spacing", "color", "background-color", "margin", "padding", "border", "border-radius", "width"];

async function measure(site, port) {
  const server = await serve(join(out, site), port);
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const portalOrigin = `http://127.0.0.1:${port}`;
  const consoleOrigin = "http://127.0.0.1:18153";
  await page.route("**/auth/options", (r) => r.fulfill({ json: {
    surface: "portal", portalOrigin, consoleOrigin, developmentLogin: false,
    developmentOrigin: portalOrigin, accounts: [], oauthUrl: `${portalOrigin}/auth/login`,
  } }));
  await page.route("**/artifacts/reference/**", (r) =>
    r.fulfill({ path: join(out, "remote", new URL(r.request().url()).pathname.slice("/artifacts/reference/".length)) }),
  );
  await page.route("**/api/**", (r) => {
    const path = new URL(r.request().url()).pathname;
    return r.fulfill({ json: path === "/api/v1/me" ? {
      id: "u", name: "기준선", admin: false, csrf: "c", browserPurpose: "portal",
      portalOrigin, consoleOrigin, canManageConsole: false,
    } : path === "/api/v1/installations" ? [app] : {} });
  });
  await page.goto(`${portalOrigin}/environments/dev/apps/reference`);
  const rootNode = page.getByTestId("app-root");
  await rootNode.waitFor();
  await page.evaluate(() => document.fonts.ready);
  const styles = await rootNode.evaluate((node, props) => {
    const pick = (el) => Object.fromEntries(props.map((p) => [p, getComputedStyle(el).getPropertyValue(p)]));
    // Skip only the per-second counter paragraph; its text changes between runs.
    const list = [node, ...node.querySelectorAll("*")].filter((el) => !(el.tagName === "P" && el.textContent?.startsWith("화면 실행")));
    return list.map((el, i) => ({ index: i, tag: el.tagName.toLowerCase(), text: (el.textContent ?? "").trim().slice(0, 24), style: pick(el) }));
  }, PROPS);
  await rootNode.screenshot({ path: join(out, `${site}.png`), mask: [page.getByText(/화면 실행/)] });
  await browser.close();
  server.close();
  return styles;
}

const before = await measure("base-site", 18151);
const after = await measure("head-site", 18152);
const differences = [];
for (const node of after) {
  const prior = before[node.index];
  if (!prior || prior.tag !== node.tag) {
    differences.push({ index: node.index, tag: node.tag, change: "structure" });
    continue;
  }
  for (const prop of PROPS)
    if (prior.style[prop] !== node.style[prop])
      differences.push({ index: node.index, tag: node.tag, text: node.text, prop, before: prior.style[prop], after: node.style[prop] });
}
const result = { base, head, nodes: after.length, differences };
writeFileSync(join(out, "result.json"), JSON.stringify(result, null, 2));
console.log(JSON.stringify({ base, head, nodes: after.length, differenceCount: differences.length }, null, 2));
for (const d of differences) console.log(JSON.stringify(d));
