import { fixtureEnv, workflowArgs, workflowData, journalEntries } from "./lib/cli-workflow.mjs";
// Run after the real Codex has completed v1 or v2. The evaluator only commits,
// builds, deploys and injects the explicitly named failed-migration experiment.
import assert from "node:assert/strict";
import { CLI_EXIT, cliResult, cliErrorText } from "./lib/cli-result.mjs";
import { spawn, execFileSync } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  readdirSync,
  existsSync,
} from "node:fs";
import { join, relative } from "node:path";
import { createHash } from "node:crypto";
import { chromium } from "@playwright/test";
const root = process.cwd(),
  statePath = join(root, ".local/workflow-app.json");
const state = JSON.parse(readFileSync(statePath)),
  { app, appId, evidence } = state;
const cliEnv=fixtureEnv(evidence,{PATH:process.env.HOME+"/.cargo/bin:"+process.env.PATH});
if(state.configDir) cliEnv.MIY_CONFIG_DIR=state.configDir;
const phase = process.argv[2];
assert.ok(["v1", "v2", "failure"].includes(phase));
const passed = [],
  origin = "http://localhost:18000";
function pass(name) {
  passed.push(name);
  console.log("PASS " + name);
}
async function cli(args, expected = 0) {
  const child = spawn(join(app, ".miy-tools/miy"), workflowArgs(args,app,cliEnv,join(app,".miy-tools/miy")), {
    cwd: app,
    env: {
      ...cliEnv,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let out = "",
    err = "";
  child.stdout.on("data", (b) => (out += b));
  child.stderr.on("data", (b) => (err += b));
  const code = await new Promise((r, j) => {
    child.on("error", j);
    child.on("close", r);
  });
  appendFileSync(
    join(evidence, phase + "-commands.log"),
    JSON.stringify({ args, code }) + "\n" + out + err + "\n",
    { mode: 0o600 },
  );
  const response = cliResult(out, { exitCode: code, expectedExit: expected });
  return response.ok ? workflowData(response.data,args) : cliErrorText(response.error);
}
function git(...args) {
  return execFileSync("git", args, { cwd: app, encoding: "utf8" }).trim();
}
function commit(message) {
  git("add", ".");
  git("commit", "-m", message);
  return git("rev-parse", "HEAD");
}
function service(action) {
  execFileSync("python3", ["scripts/services.py", action, "worker"], {
    cwd: root,
  });
}
function treeHash(path, skip = []) {
  const files = [];
  function walk(p) {
    for (const f of readdirSync(p, { withFileTypes: true })) {
      if (skip.includes(f.name)) continue;
      const q = join(p, f.name);
      if (f.isDirectory()) walk(q);
      else if (f.isFile()) files.push(q);
    }
  }
  walk(path);
  files.sort();
  return createHash("sha256")
    .update(
      files
        .map(
          (p) =>
            relative(path, p) +
            "\0" +
            createHash("sha256").update(readFileSync(p)).digest("hex"),
        )
        .join("\n"),
    )
    .digest("hex");
}
function frozen() {
  assert.equal(
    treeHash(join(root, "templates/rust-react"), [
      "node_modules",
      "target",
      "dist",
      ".git",
    ]),
    state.frozenTemplateHash,
  );
  assert.equal(treeHash(join(root, "web/platform/dist")), state.hostBuildHash);
  pass("frozen-template-and-platform-ui-unchanged");
}
async function installation() {
  return (await cli(["status"])).find((i) => i.environment === "dev");
}
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
const contexts = [];
async function login(account) {
  const context = await browser.newContext();
  contexts.push(context);
  assert.equal(
    (
      await context.request.post(origin + "/auth/development", {
        headers: { origin },
        data: { account },
      })
    ).status(),
    200,
  );
  const me = await (await context.request.get(origin + "/api/v1/me")).json();
  return { context, me };
}
async function api(user, inst, path, method = "GET", data) {
  return user.context.request.fetch(
    origin + `/api/v1/installations/${inst.id}/api` + path,
    {
      method,
      data,
      headers: {
        origin,
        "x-miy-csrf": user.me.csrf,
        "x-miy-release": inst.releaseId,
      },
    },
  );
}
function sql(db, text) {
  return execFileSync(
    "docker",
    [
      "exec",
      "miy-v2-validation-postgres-1",
      "psql",
      "-U",
      "postgres",
      "-d",
      db,
      "-Atc",
      text,
    ],
    { encoding: "utf8" },
  ).trim();
}
try {
  await cli(["auth","login", "--account", "developer"]);
  const alice = await login("alice"),
    bob = await login("bob");
  if (phase === "v1") {
    frozen();
    state.v1Commit = commit(
      "feat: implement notes through miy bundle and MCP contract",
    );
    state.v1Image = await cli(["build"]);
    pass("cli-build-rust-and-react-image-after-full-check");
    const readme = join(app, "README.md"),
      original = readFileSync(readme);
    try {
      appendFileSync(readme, "\nUncommitted acceptance probe\n");
      assert.match(await cli(["deploy"], CLI_EXIT.conflict), /stale_build/);
      pass("stale-build-deploy-rejected");
    } finally {
      writeFileSync(readme, original);
    }
    // Upload import needs a running worker. Complete deployment first, then
    // simulate a lost lifecycle receipt with an immutable saved request.
    assert.equal((await cli(["deploy"])).status, "succeeded");
    pass("cli-deploy-completes-upload-and-runtime-job");
    service("stop");
    let first, second, requestId;
    try {
      first = await cli(["stop", "--no-wait"]);
      assert.equal(first.status, "queued");
      const saved = journalEntries(cliEnv, appId).find(entry => entry.jobId === first.id);
      assert(saved, "Queued request must have a private global journal entry");
      const { path, ...entry } = saved;
      requestId = entry.requestId;
      entry.jobId = null;
      entry.submissionStatus = "sending";
      writeFileSync(path, JSON.stringify(entry), { mode: 0o600 });
      second = await cli(["job", "resume", requestId, "--environment", "dev", "--no-wait"]);
      assert.equal(first.id, second.id);
      assert.equal(journalEntries(cliEnv, appId).find(entry => entry.requestId === requestId).jobId, first.id);
      pass("lost-response-resume-recovers-one-server-job");
    } finally {
      service("start");
    }
    const done = await cli(["job", "resume", requestId, "--environment", "dev"]);
    assert.equal(done.id, first.id);
    assert.equal(done.status, "succeeded");
    assert.equal((await cli(["start"])).status, "succeeded");
    pass("cli-job-resume-completes-original-stop-and-start-restores-app");
    const inst = await installation();
    assert.equal(inst.runtimeStatus, "running");
    assert.equal(inst.kind, "docker_app");
    assert.equal(inst.imageId, state.v1Image.imageId);
    state.installationId = inst.id;
    pass("platform-created-database-and-container");
    const note = await api(alice, inst, "/notes", "POST", {
      title: "배포 전 데이터 " + state.stamp,
      body: "스키마 변경 후에도 보존할 데이터",
    });
    assert.ok(note.ok());
    state.originalNote = await note.json();
    assert.equal(state.originalNote.user_id, alice.me.id);
    const denied = await api(bob, inst, "/notes/" + state.originalNote.id);
    assert.equal(denied.status(), 404);
    pass("sdk-identity-and-real-app-db-ownership");
    const page = await alice.context.newPage(),
      errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(origin + "/apps/" + appId);
    await page
      .getByRole("heading", { name: "검증 메모", exact: true })
      .waitFor();
    await page
      .getByLabel("제목", { exact: true })
      .fill("브라우저 메모 " + state.stamp);
    await page
      .getByLabel("내용", { exact: true })
      .fill("라우터 직접 링크 확인");
    const creation = page.waitForResponse(
      (r) => r.request().method() === "POST" && r.url().endsWith("/api/notes"),
    );
    await page.getByRole("button", { name: "저장", exact: true }).click();
    assert.equal((await creation).status(), 201);
    await page.goto(origin + "/apps/" + appId);
    await page
      .getByRole("link", { name: "브라우저 메모 " + state.stamp, exact: true })
      .click();
    await page.waitForURL(/\/notes\/[a-f0-9-]+$/);
    await page.reload();
    await page.getByText("라우터 직접 링크 확인", { exact: true }).waitFor();
    assert.equal(await page.locator("iframe").count(), 0);
    assert.deepEqual(errors, []);
    await page.screenshot({ path: join(evidence, "v1.png"), fullPage: true });
    pass("browser-module-router-create-detail-and-deep-link-reload");
    await cli(["stop"]);
    assert.equal((await api(alice, inst, "/notes")).status(), 503);
    await cli(["start"]);
    assert.deepEqual(
      await (
        await api(
          alice,
          await installation(),
          "/notes/" + state.originalNote.id,
        )
      ).json(),
      state.originalNote,
    );
    pass("cli-stop-start-preserves-data");
    frozen();
  } else if (phase === "v2") {
    frozen();
    for (const [name, hash] of Object.entries(state.v1Migrations)) {
      assert.equal(
        createHash("sha256")
          .update(readFileSync(join(app, "migrations", name)))
          .digest("hex"),
        hash,
      );
    }
    pass("published-v1-migrations-unchanged");
    state.v2Commit = commit(
      "feat: add notes category through an additive migration",
    );
    state.v2Image = await cli(["build"]);
    assert.notEqual(state.v1Image.imageId, state.v2Image.imageId);
    await cli(["deploy"]);
    const inst = await installation();
    assert.equal(inst.imageId, state.v2Image.imageId);
    const note = await (
      await api(alice, inst, "/notes/" + state.originalNote.id)
    ).json();
    assert.deepEqual(
      { ...note, tag: undefined },
      { ...state.originalNote, tag: undefined },
    );
    assert.equal(note.tag, "general");
    pass("schema-changing-v2-deployment-preserves-existing-row");
    const updated = await api(alice, inst, "/notes/" + note.id, "PATCH", {
      tag: "verified",
    });
    assert.ok(updated.ok());
    assert.equal(
      (await (await api(alice, inst, "/notes/" + note.id)).json()).tag,
      "verified",
    );
    pass("v2-backend-uses-new-column-with-runtime-role");
    const page = await alice.context.newPage(),
      errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(origin + "/apps/" + appId + "/notes/" + note.id);
    await page.getByText(note.body, { exact: true }).waitFor();
    await page.getByText("verified", { exact: true }).waitFor();
    assert.deepEqual(errors, []);
    assert.equal(await page.locator("iframe").count(), 0);
    await page.screenshot({ path: join(evidence, "v2.png"), fullPage: true });
    pass("v2-ui-loaded-by-existing-platform-router");
    frozen();
  } else {
    const mPath = join(app, "manifest.json"),
      migration = join(app, "migrations/0004_recovery.sql");
    assert.ok(!existsSync(migration));
    const manifest = JSON.parse(readFileSync(mPath));
    manifest.version = "0.1.2";
    writeFileSync(mPath, JSON.stringify(manifest, null, 2) + "\n");
    writeFileSync(
      migration,
      "ALTER TABLE app.notes ADD COLUMN recovery_marker text;\nSELECT 1/0;\n",
    );
    commit("test: inject transactional migration failure");
    const failedBuild = await cli(["build"]);
    assert.match(await cli(["deploy"], CLI_EXIT.remoteFailure), /remote_job_failed/);
    const failed = journalEntries(cliEnv, appId).find(entry => entry.intent.action === "deploy" && entry.intent.imageId === failedBuild.imageId && entry.jobId);
    assert(failed, "Failed deployment must retain its request journal");
    assert.equal((await cli(["job", "get", failed.jobId])).errorCode, "migration_failed");
    const inst = await installation();
    assert.equal(inst.runtimeStatus, "failed");
    assert.equal(inst.dbStatus, "failed");
    assert.equal((await api(alice, inst, "/notes")).status(), 503);
    pass("failed-migration-reports-failed-db-and-stopped-app");
    const db = "miy_app_" + inst.id.replaceAll("-", "");
    assert.equal(
      sql(
        db,
        "SELECT count(*) FROM information_schema.columns WHERE table_schema='app' AND table_name='notes' AND column_name='recovery_marker'",
      ),
      "0",
    );
    assert.equal(
      sql(db, "SELECT count(*) FROM app._sqlx_migrations WHERE version=4"),
      "0",
    );
    assert.equal(
      sql(db, `SELECT tag FROM app.notes WHERE id='${state.originalNote.id}'`),
      "verified",
    );
    pass("failed-migration-rolls-back-ddl-and-preserves-data");
    assert.match(await cli(["deploy"], CLI_EXIT.remoteFailure), /remote_job_failed/);
    const freshFailure = journalEntries(cliEnv, appId).find(entry => entry.intent.action === "deploy" && entry.intent.imageId === failedBuild.imageId && entry.requestId !== failed.requestId && entry.jobId);
    assert(freshFailure, "An explicit new deploy must retain a distinct checked request");
    assert.notEqual(freshFailure.jobId, failed.jobId);
    assert.equal(
      journalEntries(cliEnv, appId).find(entry => entry.requestId === failed.requestId).jobId,
      failed.jobId,
    );
    assert.match(await cli(["job", "resume", failed.requestId, "--environment", "dev"], CLI_EXIT.remoteFailure), /remote_job_failed/);
    assert.equal((await cli(["job", "get", failed.jobId])).errorCode, "migration_failed");
    pass("resume-preserves-failed-job-and-explicit-new-deploy-gets-a-new-request");
    // No version 4 migration was applied; correct it in a NEW immutable release.
    manifest.version = "0.1.3";
    writeFileSync(mPath, JSON.stringify(manifest, null, 2) + "\n");
    writeFileSync(
      migration,
      "ALTER TABLE app.notes ADD COLUMN recovery_marker text NOT NULL DEFAULT 'ready';\n",
    );
    state.recoveryCommit = commit(
      "fix: publish corrected unexecuted migration as a new release",
    );
    state.recoveryImage = await cli(["build"]);
    await cli(["deploy"]);
    const recovered = await installation();
    assert.equal(recovered.runtimeStatus, "running");
    assert.equal(recovered.dbStatus, "ready");
    const note = await (
      await api(alice, recovered, "/notes/" + state.originalNote.id)
    ).json();
    assert.equal(note.tag, "verified");
    assert.equal(note.body, state.originalNote.body);
    assert.equal(
      sql(
        db,
        `SELECT recovery_marker FROM app.notes WHERE id='${state.originalNote.id}'`,
      ),
      "ready",
    );
    pass("forward-recovery-new-release-preserves-data-and-applies-migration");
    frozen();
  }
  writeFileSync(statePath, JSON.stringify(state, null, 2));
  writeFileSync(
    join(evidence, phase + ".json"),
    JSON.stringify({ date: new Date().toISOString(), appId, passed }, null, 2),
  );
  console.log(JSON.stringify({ phase, checks: passed.length, evidence }));
} finally {
  for (const c of contexts) await c.close();
  await browser.close();
}
