// Structural acceptance tests use only new synthetic apps. Secrets stay in .local/.miy.
import assert from "node:assert/strict";
import { CLI_EXIT, cliData, assertCliFailure } from "./lib/cli-result.mjs";
import { execFileSync, spawn } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  statSync,
  openSync,
  closeSync,
} from "node:fs";
import { resolve, join } from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { chromium } from "@playwright/test";
const root = resolve(import.meta.dirname, "..");
const evidence = join(root, ".local/evidence/structural-audit");
mkdirSync(evidence, { recursive: true });
const statePath = join(evidence, "apps.json");
const env = {
  ...process.env,
  PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
  CI: "true",
  CARGO_TARGET_DIR: join(root, ".local/architecture-cargo"),
};
const origin = "http://localhost:18000";
const stage = process.argv[2] || "prepare";
const passed = [];
const ok = (name) => {
  passed.push(name);
  console.log("PASS " + name);
};
function command(program, args, cwd = root, label = "command") {
  const file = openSync(join(evidence, label + ".log"), "w");
  try {
    return execFileSync(program, args, {
      cwd,
      env,
      encoding: "utf8",
      stdio: ["ignore", "pipe", file],
      maxBuffer: 4 * 1024 * 1024,
    });
  } finally {
    closeSync(file);
  }
}
const cli = (app, args, label = args[0]) =>
  cliData(
    command(
      join(app.path, ".miy-tools/miy"),
      ["--output", "json", "app", ...args],
      app.path,
      app.id + "-" + label,
    ),
  );
async function login(account) {
  const r = await fetch(origin + "/auth/development", {
    method: "POST",
    headers: { origin, "content-type": "application/json" },
    body: JSON.stringify({ account }),
  });
  assert.equal(r.status, 200);
  const cookie = r.headers.get("set-cookie").split(";")[0];
  const me = await (
    await fetch(origin + "/api/v1/me", { headers: { cookie } })
  ).json();
  return { cookie, csrf: me.csrf, id: me.id };
}
async function api(
  user,
  path,
  body,
  method = body ? "POST" : "GET",
  expect = 200,
) {
  const r = await fetch(origin + path, {
    method,
    headers: {
      cookie: user.cookie,
      origin,
      "x-miy-csrf": user.csrf,
      "content-type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const v = await r.json();
  assert.equal(r.status, expect, `${path}: ${r.status} ${v.error?.code}`);
  return v;
}
function baseline() {
  return {
    pid: readFileSync(join(root, ".local/serve.pid"), "utf8").trim(),
    images: command(
      "docker",
      ["ps", "--filter", "name=miy-app-", "--format", "{{.Names}} {{.Image}}"],
      root,
      "baseline",
    )
      .trim()
      .split("\n")
      .sort(),
  };
}
function backend() {
  return `use axum::{Extension,Json,Router,extract::State,routing::get};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value,json};
use sqlx::{PgPool,Row};
#[derive(Deserialize)] struct NewRecord { title:String }
async fn list(State(db):State<PgPool>,Extension(who):Extension<Identity>)->Json<Value>{
let rows=sqlx::query("SELECT id,title,owner FROM app.records WHERE owner=$1 ORDER BY id").bind(&who.user_id).fetch_all(&db).await.unwrap();
Json(json!({"userId":who.user_id,"records":rows.iter().map(|r|json!({"id":r.get::<i32,_>("id"),"title":r.get::<String,_>("title"),"owner":r.get::<String,_>("owner")})).collect::<Vec<_>>()}))}
async fn create(State(db):State<PgPool>,Extension(who):Extension<Identity>,Json(v):Json<NewRecord>)->Result<Json<Value>,axum::http::StatusCode>{
if v.title.trim().is_empty()||v.title.len()>120{return Err(axum::http::StatusCode::BAD_REQUEST)}
let id:i32=sqlx::query_scalar("INSERT INTO app.records(title,owner) VALUES($1,$2) RETURNING id").bind(v.title.trim()).bind(&who.user_id).fetch_one(&db).await.map_err(|_|axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
Ok(Json(json!({"id":id,"owner":who.user_id})))}
#[tokio::main] async fn main()->anyhow::Result<()>{let pool=miy_app_sdk::database().await?;miy_app_sdk::serve(Router::new().route("/records",get(list).post(create)).with_state(pool)).await}
`;
}
function ui(target) {
  return `import {useState,useEffect} from "react";
import type {AppProps} from "@miy/sdk";
export default function AppRoot({runtime}:AppProps){
const [data,setData]=useState<unknown>();const [connection,setConnection]=useState<unknown>();const [directory,setDirectory]=useState<unknown>();const [error,setError]=useState("");
const load=()=>runtime.api("/records").then(setData).catch(e=>setError(String(e)));
useEffect(()=>{void load()},[runtime.installationId]);
return <section><h1>Architecture probe</h1><p data-testid="route">{runtime.location.pathname}</p><button onClick={()=>{void runtime.api("/records",{method:"POST",body:JSON.stringify({title:"browser-record"})}).then(load).catch(e=>setError(String(e)))}}>Create record</button><button onClick={()=>runtime.navigate("/detail/1")}>Detail</button><button onClick={()=>{void runtime.platformApi("directory").then(setDirectory).catch(e=>setError(String(e)))}}>Directory</button><button onClick={()=>{void runtime.appApi("${target}","/records").then(setConnection).catch(e=>setError(String(e)))}}>Connected data</button><pre data-testid="records">{JSON.stringify(data)}</pre><pre data-testid="directory">{JSON.stringify(directory)}</pre><pre data-testid="connection">{JSON.stringify(connection)}</pre><p role="alert">{error}</p></section>}
`;
}
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
try {
  if (stage === "prepare") {
    assert(
      !existsSync(statePath),
      "Existing synthetic apps; use later stages or archive the prior state intentionally.",
    );
    command(
      "bash",
      [
        "scripts/install-cli.sh",
        "--from",
        "target/debug/miy",
        "--prefix",
        join(evidence, "bin"),
      ],
      root,
      "install",
    );
    const id = "arch-" + Date.now().toString(36);
    const apps = [id + "-data", id + "-ui"].map((id) => ({
      id,
      path: resolve(root, "../miy-apps", id),
    }));
    const admin = await login("admin");
    const developer = await login("developer");
    for (const app of apps) {
      cliData(
        command(
          join(evidence, "bin/miy"),
          ["--output", "json", "app", "init", app.path, "--id", app.id, "--name", app.id],
          root,
          app.id + "-init",
        ),
      );
      const m = JSON.parse(readFileSync(join(app.path, "manifest.json")));
      m.requestedPermissions = [
        "database",
        "platform:directory:read",
        `app:${apps[0].id}:read`,
      ];
      writeFileSync(
        join(app.path, "manifest.json"),
        JSON.stringify(m, null, 2),
      );
      writeFileSync(join(app.path, "src/main.rs"), backend());
      writeFileSync(join(app.path, "web/src/AppRoot.tsx"), ui(apps[0].id));
      writeFileSync(
        join(app.path, "migrations/0002_records.sql"),
        "CREATE TABLE app.records(id serial PRIMARY KEY,title text NOT NULL,owner text NOT NULL);\n",
      );
      command(
        "cargo",
        ["fmt", "--package", "miy-app"],
        app.path,
        app.id + "-fmt",
      );
      cli(app, ["login", "--account", "developer"]);
      cli(app, ["components", "sync"]);
      const request = cli(app, ["register"]);
      await api(admin, `/api/v1/app-registrations/${request.id}/decision`, {
        approve: true,
      });
      command("git", ["init", "-b", "main"], app.path, app.id + "-git-init");
      command("git", ["add", "."], app.path, app.id + "-git-add");
      command(
        "git",
        ["commit", "-m", "test: independent structural acceptance app"],
        app.path,
        app.id + "-git-commit",
      );
      const doctor = cli(app, ["doctor"]);
      assert.equal(doctor.mcp.transport, "stdio");
      cli(app, ["check"]);
      ok(app.id + ": clean independent bundle checks");
    }
    writeFileSync(
      statePath,
      JSON.stringify(
        { apps, baseline: baseline(), developer: developer.id },
        null,
        2,
      ),
    );
    ok("user-local installer and independent CLI scaffolding");
  }
  if (stage === "auth") {
    const { apps } = JSON.parse(readFileSync(statePath));
    const app = apps[1];
    const context = await browser.newContext();
    const page = await context.newPage();
    let stderr = "";
    let stdout = "";
    const child = spawn(
      join(app.path, ".miy-tools/miy"),
      ["--output", "json", "app", "login", "--no-open"],
      { cwd: app.path, env },
    );
    child.stderr.on("data", (b) => (stderr += b));
    child.stdout.on("data", (b) => (stdout += b));
    const done = new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("close", (c) =>
        c === 0 ? resolve() : reject(Error("CLI browser login failed")),
      );
    });
    for (let i = 0; i < 100 && !stderr.includes("/cli/authorize/"); i++)
      await new Promise((r) => setTimeout(r, 100));
    const url = stderr.match(
      /http:\/\/localhost:18000\/cli\/authorize\/[a-f0-9-]+/,
    )?.[0];
    assert(url);
    await page.goto(url);
    await page.getByRole("button", { name: "개발자", exact: false }).click();
    await page
      .getByRole("button", { name: "이 CLI 연결 승인", exact: true })
      .click();
    await done;
    assert.equal(cliData(stdout, "app login").signedIn, "developer");
    const session = JSON.parse(
      readFileSync(join(app.path, ".miy/session.json")),
    );
    assert.equal(
      statSync(join(app.path, ".miy/session.json")).mode & 0o777,
      0o600,
    );
    const response = await fetch(origin + "/api/v1/me", {
      headers: { authorization: "Bearer " + session.token },
    });
    assert.equal(response.status, 200);
    const csrf = await fetch(origin + "/api/v1/me", {
      headers: {
        authorization: "Bearer " + session.token,
        origin: "https://wrong.invalid",
      },
    });
    assert.equal(csrf.status, 403);
    ok(
      "actual CLI browser approval and bearer authentication; no browser-origin bearer bypass",
    );
    const mcp = spawn(join(app.path, ".miy-tools/miy"), ["app", "mcp"], {
      cwd: app.path,
      env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let buffer = "";
    const messages = [];
    mcp.stdout.on("data", (b) => {
      buffer += b;
      let n;
      while ((n = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, n);
        buffer = buffer.slice(n + 1);
        messages.push(JSON.parse(line));
      }
    });
    const send = (x) => mcp.stdin.write(JSON.stringify(x) + "\n");
    const receive = async (id) => {
      for (let i = 0; i < 200; i++) {
        const msg = messages.find((m) => m.id === id);
        if (msg) return msg;
        await new Promise((r) => setTimeout(r, 25));
      }
      throw Error("MCP timeout");
    };
    send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "miy-structural-check", version: "1" },
      },
    });
    assert((await receive(1)).result);
    send({ jsonrpc: "2.0", method: "notifications/initialized" });
    send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
    assert(
      (await receive(2)).result.tools.some((t) => t.name === "get_cli_help"),
    );
    for (const [id, name, args] of [
      [3, "get_contract", {}],
      [4, "get_cli_help", {}],
      [5, "search_api", { query: "connected_app" }],
    ]) {
      send({
        jsonrpc: "2.0",
        id,
        method: "tools/call",
        params: { name, arguments: args },
      });
      const res = await receive(id);
      assert(!res.result.isError);
      const data = JSON.parse(res.result.content[0].text);
      if (id === 3) {
        assert.equal(data.bundleVersion, JSON.parse(readFileSync(join(app.path, "miy.bundle.json"))).bundleVersion);
        assert.deepEqual(
          data.protectedFiles,
          JSON.parse(readFileSync(join(app.path, "miy.bundle.json")))
            .protectedFiles,
        );
      }
      if (id === 4)
        assert(data.commands.some((v) => v.command === "miy app dev"));
      if (id === 5) assert(data.length >= 4);
    }
    mcp.stdin.end();
    mcp.kill();
    ok(
      "standard stdio MCP: live contracts, complete CLI reference and connected API discovery",
    );
    const codex = JSON.parse(
      command(
        "codex",
        ["mcp", "get", "miy_app", "--json"],
        app.path,
        "codex-project-config",
      ),
    );
    assert(codex.transport.command.endsWith(".miy-tools/miy"));
    const claude = command(
      "claude",
      ["mcp", "get", "miy_app"],
      app.path,
      "claude-project-config",
    );
    assert(
      claude.includes("app mcp") ||
        claude.includes("app, mcp") ||
        claude.includes("Args: app mcp"),
    );
    ok("plain Codex and Claude Code read project MCP configuration");
    await context.close();
  }
  if (stage === "deploy") {
    const state = JSON.parse(readFileSync(statePath));
    const admin = await login("admin");
    const developer = await login("developer");
    for (const app of state.apps) {
      app.image = cli(app, ["build"]);
      const deployed = cli(app, ["deploy"]);
      assert.equal(deployed.status, "succeeded");
      const active = cli(app, ["status"])[0];
      assert.equal(active.localBuild.matchesRunningImage, true);
      assert.equal(active.environment, "dev");
      app.installation = active;
      const headers = {
        cookie: developer.cookie,
        origin,
        "x-miy-csrf": developer.csrf,
        "content-type": "application/json",
        "x-miy-release": active.releaseId,
      };
      const r = await fetch(
        `${origin}/api/v1/installations/${active.id}/api/records`,
        {
          method: "POST",
          headers,
          body: JSON.stringify({
            title:
              app === state.apps[0] ? "server-target-data" : "dev-only-record",
          }),
        },
      );
      assert.equal(r.status, 200);
      const meta = JSON.parse(
        command(
          "docker",
          [
            "inspect",
            "--format",
            "{{json .NetworkSettings.Networks}}",
            "miy-app-" + active.id.replaceAll("-", ""),
          ],
          root,
          app.id + "-network",
        ),
      );
      assert.equal(Object.keys(meta).length, 1);
      assert(Object.keys(meta)[0].startsWith("miy-runtime-"));
      ok(
        app.id +
          ": CLI upload, DB creation, runtime deploy, private app network",
      );
    }
    assert.notEqual(
      state.apps[0].installation.id,
      state.apps[1].installation.id,
    );
    writeFileSync(statePath, JSON.stringify(state, null, 2));
    const app = state.apps[1];
    const ctx = await browser.newContext();
    await ctx.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account: "developer" },
    });
    const page = await ctx.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(app.installation.url);
    await page.getByRole("heading", { name: "Architecture probe" }).waitFor();
    await page
      .getByTestId("records")
      .filter({ hasText: "dev-only-record" })
      .waitFor();
    await page.getByRole("button", { name: "Directory", exact: true }).click();
    await page
      .getByTestId("directory")
      .filter({ hasText: "developer" })
      .waitFor();
    await page
      .getByRole("button", { name: "Connected data", exact: true })
      .click();
    await page
      .getByTestId("connection")
      .filter({ hasText: "server-target-data" })
      .waitFor();
    await page.getByRole("button", { name: "Detail", exact: true }).click();
    assert(page.url().endsWith("/detail/1"));
    await page.reload();
    await page.getByTestId("route").filter({ hasText: "/detail/1" }).waitFor();
    assert.equal(await page.locator("iframe").count(), 0);
    assert.deepEqual(errors, []);
    await page.screenshot({
      path: join(evidence, "dev-platform.png"),
      fullPage: true,
    });
    await ctx.close();
    ok(
      "runtime router, authenticated common data and another app's API, deep-link reload without iframe",
    );
    await api(
      admin,
      "/api/v1/grants",
      {
        userId: developer.id,
        appId: state.apps[0].id,
        environment: "dev",
        permission: "develop",
        enabled: false,
      },
      "PUT",
    );
    await api(
      developer,
      `/api/v1/installations/${app.installation.id}/connections/${state.apps[0].id}/api/records`,
      null,
      "GET",
      403,
    );
    await api(
      admin,
      "/api/v1/grants",
      {
        userId: developer.id,
        appId: state.apps[0].id,
        environment: "dev",
        permission: "develop",
        enabled: true,
      },
      "PUT",
    );
    ok("target permission revocation blocks connected API immediately");
    assert.equal(baseline().pid, state.baseline.pid);
    ok("deployment did not restart platform core");
  }
  if (stage === "preview") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const output = openSync(join(evidence, "preview.log"), "w");
    const child = spawn(
      join(app.path, ".miy-tools/miy"),
      ["app", "dev", "--port", "18100", "--no-open"],
      { cwd: app.path, env, stdio: ["ignore", output, output] },
    );
    let code;
    child.on("exit", (c) => (code = c));
    try {
      const info = join(app.path, ".miy/preview.json");
      for (let i = 0; i < 1200 && !existsSync(info) && code === undefined; i++)
        await new Promise((r) => setTimeout(r, 500));
      assert.equal(
        code,
        undefined,
        "preview process failed; inspect private preview.log",
      );
      assert(existsSync(info));
      const local = JSON.parse(readFileSync(info));
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      // Docker Desktop browser uses host.docker.internal; the loopback gateway is tested in a host browser below.
      const hostBrowser = await chromium.launch({
        channel: "chrome",
        headless: true,
      });
      const hostContext = await hostBrowser.newContext();
      const localPage = await hostContext.newPage();
      const errors = [];
      localPage.on("pageerror", (e) => errors.push(e.message));
      await localPage.goto(local.url);
      await localPage
        .getByRole("heading", { name: "Architecture probe" })
        .waitFor();
      await localPage.waitForFunction(()=>Boolean(document.querySelector('[data-testid="records"]')?.textContent));
      const beforeLocal=JSON.parse(await localPage.getByTestId("records").textContent());
      assert.equal(beforeLocal.userId,state.developer);
      assert(!beforeLocal.records.some(r=>r.title==="dev-only-record"));
      let duplicateFailure;
      try {execFileSync(join(app.path,".miy-tools/miy"),["--output","json","app","dev","--port","18101","--no-open"],{cwd:app.path,env,timeout:5000,stdio:["ignore","pipe","pipe"]});}
      catch(e){duplicateFailure=e;}
      assertCliFailure(duplicateFailure,CLI_EXIT.conflict,/preview_already_running_for_this_project/,"app dev");
      await localPage
        .getByRole("button", { name: "Create record", exact: true })
        .click();
      await localPage.waitForFunction(n=>{const text=document.querySelector('[data-testid="records"]')?.textContent;return text&&JSON.parse(text).records.length===n;},beforeLocal.records.length+1);
      await localPage
        .getByRole("button", { name: "Directory", exact: true })
        .click();
      await localPage
        .getByTestId("directory")
        .filter({ hasText: "developer" })
        .waitFor();
      await localPage
        .getByRole("button", { name: "Connected data", exact: true })
        .click();
      await localPage
        .getByTestId("connection")
        .filter({ hasText: "server-target-data" })
        .waitFor();
      await localPage
        .getByRole("button", { name: "Detail", exact: true })
        .click();
      await localPage.reload();
      await localPage
        .getByTestId("route")
        .filter({ hasText: "/detail/1" })
        .waitFor();
      assert.equal(await localPage.locator("iframe").count(), 0);
      assert.deepEqual(errors, []);
      await localPage.screenshot({
        path: join(evidence, "local-preview.png"),
        fullPage: true,
      });
      await hostBrowser.close();
      await ctx.close();
      const dev = await login("developer");
      const remote = await api(
        dev,
        `/api/v1/installations/${app.installation.id}/api/records`,
      );
      assert(remote.records.some((v) => v.title === "dev-only-record"));
      assert(!remote.records.some((v) => v.title === "browser-record"));
      const bad = await fetch("http://127.0.0.1:18100/__miy/context");
      assert.equal(bad.status, 401);
      ok(
        "local working-tree backend/UI and independent DB; real identity and remote dev APIs; no platform DB contamination",
      );
    } finally {
      child.kill("SIGINT");
      await new Promise((r) => child.once("close", r));
      closeSync(output);
    }
  }
  if (stage === "promote") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const admin = await login("admin");
    const developer = await login("developer");
    const release = app.installation.releaseId;
    const req = (action, extra = {}) => ({
      action,
      environment: "prod",
      idempotencyKey: randomUUID(),
      ...extra,
    });
    await api(
      admin,
      `/api/v1/apps/${app.id}/runtime`,
      req("promote", { releaseId: release }),
      "POST",
      409,
    );
    await api(
      developer,
      `/api/v1/apps/${app.id}/runtime`,
      req("promote", { releaseId: release }),
      "POST",
      403,
    );
    await api(
      admin,
      "/api/v1/grants",
      {
        userId: developer.id,
        appId: app.id,
        environment: "prod",
        permission: "develop",
        enabled: true,
      },
      "PUT",
    );
    await api(
      developer,
      `/api/v1/apps/${app.id}/runtime`,
      req("promote", { releaseId: release }),
      "POST",
      403,
    );
    ok(
      "unaccepted release denied; prod/develop does not grant production deployment",
    );
    const accepted = cli(app, [
      "accept",
      "--note",
      "Browser router, own CRUD, shared directory, connected dev API and local DB isolation verified by check-architecture.mjs",
    ]);
    assert.equal(accepted.releaseId, release);
    await api(
      admin,
      "/api/v1/grants",
      {
        userId: developer.id,
        appId: app.id,
        environment: "prod",
        permission: "deploy",
        enabled: true,
      },
      "PUT",
    );
    await api(
      developer,
      `/api/v1/apps/${app.id}/runtime`,
      req("deploy", { imageId: app.image.imageId }),
      "POST",
      400,
    );
    const job = cli(app, ["promote", "--release", release]);
    assert.equal(job.status, "succeeded");
    const all = cli(app, ["status"]);
    const dev = all.find((v) => v.environment === "dev");
    const prod = all.find((v) => v.environment === "prod");
    assert(prod);
    assert.notEqual(prod.id, dev.id);
    assert.equal(prod.imageId, dev.imageId);
    assert.equal(prod.releaseId, dev.releaseId);
    const data = await api(
      developer,
      `/api/v1/installations/${prod.id}/api/records`,
    );
    assert.deepEqual(data.records, []);
    const ctx = await browser.newContext();
    await ctx.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account: "developer" },
    });
    const page = await ctx.newPage();
    await page.goto(prod.url);
    await page.getByRole("heading", { name: "Architecture probe" }).waitFor();
    await page
      .getByTestId("records")
      .filter({ hasText: '"records":[]' })
      .waitFor();
    assert.equal(await page.locator("iframe").count(), 0);
    await page.screenshot({
      path: join(evidence, "prod-platform.png"),
      fullPage: true,
    });
    await ctx.close();
    ok(
      "exact-image CLI production promotion, separate prod DB and explicit production router",
    );
    await api(
      developer,
      `/api/v1/installations/${prod.id}/connections/${state.apps[0].id}/api/records`,
      null,
      "GET",
      403,
    );
    ok("production connection never falls back to development app data");
    cli(app, ["control", "stop", "--environment", "prod"]);
    await api(
      developer,
      `/api/v1/installations/${prod.id}/api/records`,
      null,
      "GET",
      503,
    );
    assert.equal(
      (await api(developer, `/api/v1/installations/${dev.id}/api/records`))
        .records[0].title,
      "dev-only-record",
    );
    cli(app, ["control", "start", "--environment", "prod"]);
    ok(
      "production lifecycle affects only its installation, development stays available",
    );
    await api(
      admin,
      "/api/v1/grants",
      {
        userId: developer.id,
        appId: app.id,
        environment: "prod",
        permission: "deploy",
        enabled: false,
      },
      "PUT",
    );
    await api(
      developer,
      `/api/v1/apps/${app.id}/runtime`,
      req("stop"),
      "POST",
      403,
    );
    ok("production deploy permission revocation blocks lifecycle changes");
    assert.equal(baseline().pid, state.baseline.pid);
    const running = baseline().images;
    for (const image of state.baseline.images) assert(running.includes(image));
    ok(
      "existing deployed app images and core process preserved across the full workflow",
    );
    state.prod = prod;
    writeFileSync(statePath, JSON.stringify(state, null, 2));
  }
  if (stage === "update") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const developer = await login("developer");
    const ctx = await browser.newContext();
    await ctx.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account: "developer" },
    });
    const page = await ctx.newPage();
    await page.goto(app.installation.url);
    await page
      .getByRole("heading", { name: "Architecture probe", exact: true })
      .waitFor();
    const manifestPath = join(app.path, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath));
    manifest.version = "0.1.1";
    writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
    const uiPath = join(app.path, "web/src/AppRoot.tsx");
    writeFileSync(
      uiPath,
      readFileSync(uiPath, "utf8").replace(
        "Architecture probe</h1>",
        "Architecture probe v2</h1>",
      ),
    );
    writeFileSync(
      join(app.path, "migrations/0003_category.sql"),
      "ALTER TABLE app.records ADD COLUMN category text NOT NULL DEFAULT 'retained';\n",
    );
    command("git", ["add", "."], app.path, "update-add");
    command(
      "git",
      [
        "commit",
        "-m",
        "test: verify independent dev update and retained production release",
      ],
      app.path,
      "update-commit",
    );
    const build = cli(app, ["build"], "update-build");
    // Deliberately remove only the NEW, unused synthetic image after Docker save.
    // Successful deployment must load the transferred archive instead of relying on a shared cache.
    const docker = command("which", ["docker"], root, "docker-path").trim();
    const bin = join(evidence, "client-bin");
    mkdirSync(bin, { recursive: true });
    const marker = join(evidence, "archive-only-transfer.txt");
    writeFileSync(
      join(bin, "docker"),
      `#!/bin/sh\nreal='${docker}'\nif [ "$1" = save ]; then\n  "$real" "$@" || exit $?\n  for arg in "$@"; do last="$arg"; done\n  if [ "$last" = "$MIY_EXPECTED_TRANSFER_IMAGE" ]; then\n    "$real" image rm -f "$last" >/dev/null || exit $?\n    if "$real" image inspect "$last" >/dev/null 2>&1; then exit 9; fi\n    echo 'new image absent before HTTP upload' > '${marker}'\n  fi\nelse\n  exec "$real" "$@"\nfi\n`,
      { mode: 0o755 },
    );
    const priorPath = env.PATH;
    env.PATH = bin + ":" + priorPath;
    env.MIY_EXPECTED_TRANSFER_IMAGE = build.imageId;
    try {
      assert.equal(cli(app, ["deploy"], "update-deploy").status, "succeeded");
    } finally {
      env.PATH = priorPath;
      delete env.MIY_EXPECTED_TRANSFER_IMAGE;
    }
    assert(existsSync(marker));
    await page
      .getByRole("heading", { name: "앱 상태가 변경되었습니다" })
      .waitFor();
    await page.getByRole("button", { name: "새로고침", exact: true }).click();
    await page
      .getByRole("heading", { name: "Architecture probe v2", exact: true })
      .waitFor();
    await page
      .getByTestId("records")
      .filter({ hasText: "dev-only-record" })
      .waitFor();
    const all = cli(app, ["status"]);
    const dev = all.find((i) => i.environment === "dev");
    const prod = all.find((i) => i.environment === "prod");
    assert.equal(dev.version, "0.1.1");
    assert.equal(prod.version, "0.1.0");
    assert.notEqual(dev.imageId, prod.imageId);
    await page.goto(prod.url);
    await page
      .getByRole("heading", { name: "Architecture probe", exact: true })
      .waitFor();
    await page
      .getByTestId("records")
      .filter({ hasText: '"records":[]' })
      .waitFor();
    assert.equal(baseline().pid, state.baseline.pid);
    const platformHash = createHash("sha256");
    const { readdirSync } = await import("node:fs");
    function files(dir) {
      return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory() ? files(join(dir, e.name)) : [join(dir, e.name)],
      );
    }
    for (const f of files(join(root, "web/platform/dist")).sort())
      platformHash.update(readFileSync(f));
    assert.equal(platformHash.digest("hex"), state.baseline.platformHash);
    state.apps[1].installation = dev;
    state.apps[1].image = build;
    writeFileSync(statePath, JSON.stringify(state, null, 2));
    await ctx.close();
    ok("HTTP archive transfer succeeds with no server-side image cache");
    ok(
      "runtime dev UI and schema update preserve records and leave prod release unchanged",
    );
    ok("platform core PID and UI build stay unchanged during app update");
  }
  if (stage === "boundaries") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const dataApp = state.apps[0];
    const admin = await login("admin");
    const developer = await login("developer");
    const alice = await login("alice");
    const grant = {
      userId: developer.id,
      appId: app.id,
      environment: "prod",
      permission: "deploy",
    };
    await api(admin, "/api/v1/grants", { ...grant, enabled: true }, "PUT");
    command(
      "python3",
      ["scripts/services.py", "stop", "worker"],
      root,
      "queued-stop-worker",
    );
    let job;
    try {
      job = await api(developer, `/api/v1/apps/${app.id}/runtime`, {
        action: "stop",
        environment: "prod",
        idempotencyKey: randomUUID(),
      });
      await api(admin, "/api/v1/grants", { ...grant, enabled: false }, "PUT");
    } finally {
      command(
        "python3",
        ["scripts/services.py", "start", "worker"],
        root,
        "queued-start-worker",
      );
    }
    let result;
    for (let i = 0; i < 100; i++) {
      result = await api(admin, `/api/v1/runtime/jobs/${job.id}`);
      if (["failed", "succeeded"].includes(result.status)) break;
      await new Promise((r) => setTimeout(r, 200));
    }
    assert.equal(result.status, "failed");
    assert.equal(result.errorCode, "permission_revoked");
    assert.equal(
      (await api(admin, "/api/v1/installations")).find(
        (v) => v.id === state.prod.id,
      ).runtimeStatus,
      "running",
    );
    ok("queued production job rechecks revoked permission before execution");
    const targetName = "miy-app-" + dataApp.installation.id.replaceAll("-", "");
    command("docker", ["stop", targetName], root, "external-container-stop");
    let stopped;
    for (let i = 0; i < 70; i++) {
      stopped = (await api(admin, "/api/v1/installations")).find(
        (v) => v.id === dataApp.installation.id,
      );
      if (stopped.runtimeStatus === "stopped") break;
      await new Promise((r) => setTimeout(r, 200));
    }
    assert.equal(stopped.runtimeStatus, "stopped");
    cli(dataApp, ["control", "start"], "reconciled-start");
    ok(
      "external Docker stop reconciles into platform state; CLI restores only that app",
    );
    await api(
      admin,
      "/api/v1/installations",
      {
        appId: app.id,
        environment: "prod",
        releaseId: app.installation.releaseId,
      },
      "POST",
      400,
    );
    ok(
      "legacy fixture installation endpoint cannot bypass production promotion",
    );
    const active = (await api(admin, "/api/v1/installations")).find(
      (v) => v.id === app.installation.id,
    );
    const entry = active.ui.entry;
    assert.equal((await fetch(origin + entry)).status, 401);
    assert.equal(
      (await fetch(origin + entry, { headers: { cookie: alice.cookie } }))
        .status,
      403,
    );
    assert.equal(
      (
        await fetch(
          origin + entry.replace(/ui\/.*$/, "migrations/0001_initial.sql"),
          { headers: { cookie: admin.cookie } },
        )
      ).status,
      404,
    );
    ok(
      "UI assets require app access; migration files are not public artifacts",
    );
    const challenge = createHash("sha256").update("x".repeat(40)).digest("hex");
    const flow = await (
      await fetch(origin + "/auth/cli/start", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ challenge }),
      })
    ).json();
    await api(developer, `/api/v1/cli-authorizations/${flow.id}/approve`, {});
    const exchange = async (verifier) =>
      fetch(origin + "/auth/cli/exchange", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ id: flow.id, verifier }),
      });
    assert.equal((await exchange("wrong".repeat(10))).status, 401);
    const issued = await exchange("x".repeat(40));
    assert.equal(issued.status, 200);
    const { token } = await issued.json();
    assert.equal((await exchange("x".repeat(40))).status, 401);
    const revoked = await fetch(origin + "/api/v1/cli/logout", {
      method: "POST",
      headers: { authorization: "Bearer " + token },
    });
    assert.equal(revoked.status, 200);
    assert.equal(
      (
        await fetch(origin + "/api/v1/me", {
          headers: { authorization: "Bearer " + token },
        })
      ).status,
      401,
    );
    ok("CLI verifier binding, single-use exchange and credential revocation");
    const unauthorized = await fetch(`${origin}/api/v1/apps/${app.id}/images`, {
      method: "POST",
      headers: {
        cookie: alice.cookie,
        origin,
        "x-miy-csrf": alice.csrf,
        "content-type": "application/x-tar",
        "x-miy-image": app.image.imageId,
        "x-miy-archive-sha256": "0".repeat(64),
      },
      body: "bad",
    });
    assert.equal(unauthorized.status, 403);
    ok("unauthorized image upload is rejected before archive import");
    const context = await browser.newContext();
    await context.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account: "admin" },
    });
    const page = await context.newPage();
    await page.goto(origin + "/admin");
    await page
      .getByRole("heading", { name: "플랫폼 관리", exact: true })
      .waitFor();
    const card = page.locator("section").filter({
      has: page.getByRole("heading", {
        name: `${dataApp.id} · dev`,
        exact: true,
      }),
    });
    await card.getByRole("button", { name: "중지", exact: true }).click();
    await card.getByRole("button", { name: "시작", exact: true }).waitFor();
    await card.getByRole("button", { name: "시작", exact: true }).click();
    await card.getByRole("button", { name: "중지", exact: true }).waitFor();
    await page.screenshot({
      path: join(evidence, "admin.png"),
      fullPage: true,
    });
    await context.close();
    ok(
      "administrator web controls manage an individual app without platform restart",
    );
  }
  if (stage === "routes") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const admin = await login("admin");
    const context = await browser.newContext();
    const [name, ...value] = admin.cookie.split("=");
    await context.addCookies([{ name, value: value.join("="), url: origin }]);
    const page = await context.newPage();
    try {
      for (const environment of ["development", "production", "unknown"]) {
        await page.goto(`${origin}/environments/${environment}/apps/${app.id}`);
        await page.getByRole("heading", { name: "존재하지 않는 실행 환경입니다" }).waitFor();
        assert.equal(await page.getByRole("heading", { name: /Architecture probe/ }).count(), 0);
      }
      for (const environment of ["dev", "prod"]) {
        await page.goto(`${origin}/environments/${environment}/apps/${app.id}/detail/1`);
        await page.getByRole("heading", { name: /Architecture probe/ }).waitFor();
        assert.equal(await page.getByTestId("route").textContent(), "/detail/1");
        assert.equal(await page.locator("iframe").count(), 0);
        await page.reload();
        await page.getByRole("heading", { name: /Architecture probe/ }).waitFor();
      }
      ok("unknown environment routes never fall back to dev/prod; exact environments support direct links and reloads");
    } finally {
      await context.close();
    }
  }
  if (stage === "auth-https") {
    const state = JSON.parse(readFileSync(statePath));
    const app = state.apps[1];
    const savedSession = readFileSync(join(app.path, ".miy/session.json"));
    const savedPlatform = readFileSync(join(app.path, ".miy/platform.json"));
    const secrets = JSON.parse(
      readFileSync(join(root, ".local/credentials.json")),
    );
    const context = await browser.newContext();
    const page = await context.newPage();
    let stderr = "",
      stdout = "";
    const child = spawn(
      join(app.path, ".miy-tools/miy"),
      [
        "--output",
        "json",
        "app",
        "login",
        "--url",
        "https://localhost:18443",
        "--ca-file",
        join(root, ".local/caddy/caddy/pki/authorities/local/root.crt"),
        "--no-open",
      ],
      { cwd: app.path, env },
    );
    child.stderr.on("data", (b) => (stderr += b));
    child.stdout.on("data", (b) => (stdout += b));
    const done = new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("close", (c) =>
        c === 0 ? resolve() : reject(Error("HTTPS CLI login failed")),
      );
    });
    try {
      for (let i = 0; i < 200 && !stderr.includes("/cli/authorize/"); i++)
        await new Promise((r) => setTimeout(r, 50));
      const url = stderr.match(
        /https:\/\/localhost:18443\/cli\/authorize\/[a-f0-9-]+/,
      )?.[0];
      assert(url);
      await page.goto(url);
      await page.getByRole("link", { name: "로그인", exact: true }).click();
      await page.locator("#username").fill("developer");
      await page.locator("#password").fill(secrets.developer);
      await page.locator("#kc-login").click();
      await page
        .getByRole("button", { name: "이 CLI 연결 승인", exact: true })
        .waitFor();
      await page
        .getByRole("button", { name: "이 CLI 연결 승인", exact: true })
        .click();
      await done;
      assert.equal(cliData(stdout, "app login").signedIn, "developer");
      const session = JSON.parse(
        readFileSync(join(app.path, ".miy/session.json")),
      );
      assert.equal(session.origin, "https://localhost:18443");
      assert(cli(app, ["doctor"]).platform.connected);
      cli(app, ["logout"]);
      ok(
        "actual Keycloak OIDC browser login -> CLI approval over trusted HTTPS -> authenticated API -> logout",
      );
    } finally {
      child.kill();
      writeFileSync(join(app.path, ".miy/session.json"), savedSession, {
        mode: 0o600,
      });
      writeFileSync(join(app.path, ".miy/platform.json"), savedPlatform, {
        mode: 0o600,
      });
      await context.close();
    }
  }
  writeFileSync(
    join(evidence, stage + ".json"),
    JSON.stringify(
      { stage, passed, checkedAt: new Date().toISOString() },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
