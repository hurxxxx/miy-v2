// Current CLI helpers for explicitly invoked regression fixtures. Never select
// the operator's config/keychain; historical binaries retain their own calls.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { cliData } from "./cli-result.mjs";

const roots = new Set(["context", "auth", "doctor", "describe", "completion", "component", "job", "registration", "user", "grant", "mcp", "app"]);
const selected = new Map();
const digest = value => createHash("sha256").update(value).digest("hex");
export function fixtureEnv(evidence, extra = {}) {
  const directory = join(resolve(evidence), "cli-config");
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  chmodSync(directory, 0o700);
  const env = { ...process.env, ...extra, MIY_CONFIG_DIR: directory, MIY_CREDENTIAL_BACKEND: "file" };
  delete env.MIY_CONTEXT;
  return env;
}
function config(env) {
  const path = join(env.MIY_CONFIG_DIR, "config.json");
  return existsSync(path) ? JSON.parse(readFileSync(path)) : { contexts: {} };
}
export function contextName(cwd, env, origin = "http://localhost:18000") {
  return selected.get(`${env.MIY_CONFIG_DIR}\0${resolve(cwd)}`) ?? `fixture-${digest(`${resolve(cwd)}\0${origin}`).slice(0, 16)}`;
}
export function selectContext(cwd, env, name) {
  selected.set(`${env.MIY_CONFIG_DIR}\0${resolve(cwd)}`, name);
  if (existsSync(join(cwd, "manifest.json"))) {
    mkdirSync(join(cwd, ".miy"), { recursive: true, mode: 0o700 });
    writeFileSync(join(cwd, ".miy/context.json"), JSON.stringify({ context: name }), { mode: 0o600 });
  }
}
export function ensureContext(binary, cwd, env, origin = "http://localhost:18000", caFile) {
  const name = `fixture-${digest(`${resolve(cwd)}\0${origin}`).slice(0, 16)}`;
  if (!config(env).contexts[name]) {
    cliData(execFileSync(binary, ["--output", "json", "context", "add", name, "--url", origin,
      ...(caFile ? ["--ca-file", caFile] : [])], { cwd, env: { ...env, MIY_CONTEXT: name }, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
  }
  selectContext(cwd, env, name);
  return name;
}
export function workflowArgs(args, cwd, env, binary) {
  args = [...args];
  if (!roots.has(args[0])) args.unshift("app");
  if (args[0] === "auth" && args[1] === "login") {
    const take = flag => { const i = args.indexOf(flag); return i < 0 ? undefined : args.splice(i, 2)[1]; };
    // URL/CA choose a context; they are never forwarded as removed login flags.
    ensureContext(binary, cwd, env, take("--url") ?? "http://localhost:18000", take("--ca-file"));
  }
  const name = contextName(cwd, env);
  const prefix = ["--output", "json", ...(config(env).contexts[name] ? ["--context", name] : [])];
  const dangerous = args[0] === "app" && (args[1] === "remove" || args[1] === "promote" || args.includes("prod"));
  if (dangerous) {
    // These scripts explicitly operate their named fixtures. Preserve that
    // approval in the new CLI's mandatory context/app/environment arguments.
    assert(config(env).contexts[name], "Fixture context required for a confirmed operation");
    if (!args.includes("--app")) prefix.push("--app", JSON.parse(readFileSync(join(cwd, "manifest.json"))).id);
    if (!args.includes("--environment")) args.push("--environment", args[1] === "promote" ? "prod" : "dev");
    if (!args.includes("--dry-run") && !args.includes("--yes")) prefix.push("--yes");
  }
  return [...prefix, ...args];
}
export function workflowData(data, args) {
  if (args[0] === "status" || (args[0] === "app" && args[1] === "status")) return data.installations;
  for (const [group, key] of [["registration", "registrations"], ["user", "users"], ["grant", "grants"], ["job", "jobs"]]) {
    if (args[0] === group && args[1] === "list") return data[key];
  }
  return data;
}
export function journalEntries(env, appId) {
  const directory = join(env.MIY_CONFIG_DIR, "journal");
  if (!existsSync(directory)) return [];
  return readdirSync(directory).filter(name => /^[a-f0-9-]{36}\.json$/.test(name)).map(name => ({ path: join(directory, name), ...JSON.parse(readFileSync(join(directory, name))) }))
    .filter(entry => !appId || entry.appId === appId).sort((a, b) => b.createdAt - a.createdAt || b.requestId.localeCompare(a.requestId));
}
export function fixtureCredential(cwd, env) {
  assert.equal(process.platform === "win32", false, "This Linux fixture reader must not bypass Windows DPAPI");
  const context = config(env).contexts[contextName(cwd, env)];
  assert(context?.userId, "Fixture authentication required");
  const key = digest(`session\0${context.origin}\0${context.userId}`);
  const path = join(env.MIY_CONFIG_DIR, "credentials", `${key}.json`);
  const record = JSON.parse(readFileSync(path));
  assert.equal(record.backend, "unix_file", "Fixture credentials must use the isolated protected file backend");
  const credential = JSON.parse(record.value);
  assert.equal(credential.origin, context.origin);
  assert.equal(credential.userId, context.userId);
  return { path, origin: context.origin, token: credential.accessToken, userId: credential.userId };
}
export async function collectApiPage(value, path, fetchNext) {
  const key = Object.keys(value ?? {}).find(key => ["apps", "installations", "registrations", "users", "grants", "jobs", "sessions"].includes(key) && Array.isArray(value[key]));
  if (!key || !Object.hasOwn(value, "nextCursor")) return value;
  const rows = [...value[key]];
  while (value.nextCursor) {
    const next = new URL(path, "http://fixture.invalid");
    next.searchParams.set("cursor", value.nextCursor);
    value = await fetchNext(next.pathname + next.search);
    assert(Array.isArray(value[key]), "Page response shape changed");
    rows.push(...value[key]);
  }
  return rows;
}
export async function checkedRuntimeBody(api, path, body, planner) {
  const appId = path.match(/^\/api\/v1\/apps\/([^/]+)\/runtime$/)?.[1];
  if (!appId || !body || body.expectedState) return body;
  // A permitted planner may be supplied for tests that intentionally submit
  // with a revoked or insufficient actor; the actual submission stays denied.
  const input = { target: { appId, environment: body.environment }, action: body.action,
    ...(body.imageId ? { imageId: body.imageId } : {}), ...(body.releaseId ? { releaseId: body.releaseId } : {}) };
  const plan = await (planner ?? api)(`${path}/preflight`, input);
  assert(plan.expectedState && typeof plan.expectedState === "object", "Preflight must return the expected state snapshot");
  return { ...body, expectedState: plan.expectedState, confirmed: body.environment === "prod" || body.action === "remove" };
}
