import { fixtureEnv, workflowArgs, workflowData, fixtureCredential } from "./lib/cli-workflow.mjs";
// Independent CLI acceptance. All registration/grant writes use the public CLI.
import assert from "node:assert/strict";
import { CLI_EXIT, cliResult, cliErrorText } from "./lib/cli-result.mjs";
import { spawn } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  appendFileSync,
  rmSync,
} from "node:fs";
import { resolve, join } from "node:path";
const root = process.cwd(),
  cli = join(root, "target/debug/miy");
const stamp = Date.now().toString(36),
  evidence = join(root, ".local/evidence/workflow-" + stamp);
mkdirSync(evidence, { recursive: true });
const cliEnv = fixtureEnv(evidence,{PATH:process.env.HOME+"/.cargo/bin:"+process.env.PATH});
const passed = [];
function pass(name) {
  passed.push(name);
  console.log("PASS " + name);
}
async function command(cwd, args, expected = 0, env = {}) {
  const child = spawn(cli, workflowArgs(args,cwd,{...cliEnv,...env},cli), {
    cwd,
    env: {
      ...cliEnv,
      ...env,
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
    join(evidence, "commands.log"),
    JSON.stringify({ args, cwd, code }) + "\n" + out + err + "\n",
    { mode: 0o600 },
  );
  const response = cliResult(out, { exitCode: code, expectedExit: expected });
  if (!response.ok) return cliErrorText(response.error);
  const data = workflowData(response.data, args);
  if (Array.isArray(data) && response.data.nextCursor) {
    const next = [...args];
    const previous = next.indexOf("--cursor");
    if (previous >= 0) next.splice(previous, 2);
    next.push("--cursor", response.data.nextCursor);
    return [...data, ...await command(cwd, next, expected, env)];
  }
  return data;
}
// Check three untouched scaffolds; caches are shared but every source directory is new.
for (let i = 1; i <= 3; i++) {
  const dir = join(root, ".local/scaffold-checks", stamp + "-" + i);
  await command(root, [
    "init",
    dir,
    "--id",
    "scaffold-" + stamp + "-" + i,
    "--name",
    'Template "escape" 검증',
  ]);
  const checked = await command(dir, ["check"], 0, {
    CARGO_TARGET_DIR: join(root, ".local/scaffold-target"),
  });
  assert.equal(checked.bundleVersion, JSON.parse(readFileSync(join(dir, "miy.bundle.json"))).bundleVersion);
  assert.ok(checked.checks.includes("federation-assets"));
  pass("fresh-scaffold-" + i + "-format-rust-typescript-federation-build");
  if (i === 1) {
    for (const [path, change, code] of [
      [
        "sdk/rust/src/lib.rs",
        (s) => s + "\n// modified\n",
        "protected_bundle_modified",
      ],
      ["Dockerfile", (s) => s + "\n# modified\n", "protected_bundle_modified"],
      [
        "miy.bundle.json",
        (s) => JSON.stringify({ ...JSON.parse(s), bundleVersion: "99.0.0" }),
        "bundle_version_mismatch",
      ],
      [
        "manifest.json",
        (s) => s.replace("mf-manifest.json", "remoteEntry.js"),
        "invalid_manifest",
      ],
    ]) {
      const p = join(dir, path),
        original = readFileSync(p, "utf8");
      try {
        writeFileSync(p, change(original));
        assert.ok((await command(dir, ["check"], CLI_EXIT.environment)).includes(code));
        pass("reject-" + code + "-" + path);
      } finally {
        writeFileSync(p, original);
      }
    }
    for (const [name, sql, code] of [
      [
        "0002_wrapped.sql",
        "/* comment */ BeGiN; CREATE TABLE app.x(id int); COMMIT;",
        "explicit_migration_transaction",
      ],
      ["1_duplicate.sql", "SELECT 1;", "duplicate_migration_version"],
      [
        "0002_nontransactional.sql",
        "-- no-transaction\nSELECT 1;",
        "nontransactional_migration_unsupported",
      ],
    ]) {
      const p = join(dir, "migrations", name);
      try {
        writeFileSync(p, sql);
        assert.ok((await command(dir, ["check"], CLI_EXIT.environment)).includes(code));
        pass("reject-" + code);
      } finally {
        rmSync(p);
      }
    }
  }
}
const appId = "workflow-notes-" + stamp,
  app = resolve(root, "../miy-apps", appId);
assert.ok(!existsSync(app));
await command(root, [
  "init",
  app,
  "--id",
  appId,
  "--name",
  "개발·배포 검증 메모",
]);
await command(app, ["auth","login", "--account", "alice"]);
assert.match(await command(app, ["register"], CLI_EXIT.forbidden), /forbidden/);
pass("ordinary-user-cannot-request-registration");
await command(app, ["auth","login", "--account", "developer"]);
const requested = await command(app, ["register"]);
assert.equal(requested.status, "pending");
assert.equal((await command(app, ["register"])).id, requested.id);
pass("developer-registration-request-idempotent");
assert.match(await command(app, ["start"], CLI_EXIT.forbidden), /forbidden/);
pass("pending-request-has-no-deploy-access");
assert.match(await command(app, ["registration","approve", requested.id], CLI_EXIT.forbidden), /forbidden/);
pass("developer-cannot-self-approve");
assert.match(await command(app, ["user","list"], CLI_EXIT.forbidden), /forbidden/);
pass("developer-cannot-list-all-users");
await command(app, ["auth","login", "--account", "admin"]);
assert.ok(
  (await command(app, ["registration","list"])).some((r) => r.id === requested.id),
);
assert.equal(
  (await command(app, ["registration","approve", requested.id])).status,
  "approved",
);
assert.equal(
  (await command(app, ["registration","approve", requested.id])).status,
  "approved",
);
pass("admin-approval-atomic-and-idempotent");
assert.match(
  await command(app, ["registration","reject", requested.id], CLI_EXIT.conflict),
  /registration_already_decided/,
);
pass("approved-request-cannot-be-reversed-as-rejection");
const users = await command(app, ["user","list"]);
for (const name of ["alice", "bob"])
  await command(app, [
    "grant", "set", "--permission", "use", "--environment", "dev",
    "--user",
    users.find((u) => u.name === name).id,
  ]);
pass("admin-grants-via-cli");
await command(app, ["auth","login", "--account", "developer"]);
assert.match(
  await command(app, ["start", "--environment", "prod"], CLI_EXIT.forbidden),
  /forbidden/,
);
pass("approval-grants-dev-only");
const session = fixtureCredential(app,cliEnv);
const contract = await (
  await fetch(session.origin + "/api/v1/contracts", {
    headers: { authorization: "Bearer " + session.token },
  })
).json();
assert.equal(contract.bundleVersion, JSON.parse(readFileSync(join(app, "miy.bundle.json"))).bundleVersion);
assert.equal(
  contract.guidance,
  readFileSync(join(app, "docs/CONTRACT.md"), "utf8"),
);
assert.equal(
  contract.agentGuidance,
  readFileSync(join(app, "AGENTS.md"), "utf8"),
);
assert.deepEqual(
  contract.protectedFiles,
  JSON.parse(readFileSync(join(app, "miy.bundle.json"))).protectedFiles,
);
pass("cli-and-live-mcp-contract-source-identical");
writeFileSync(
  join(root, ".local/workflow-app.json"),
  JSON.stringify(
    { app, appId, evidence, configDir:cliEnv.MIY_CONFIG_DIR, registrationId: requested.id, stamp },
    null,
    2,
  ),
);
writeFileSync(
  join(evidence, "baseline.json"),
  JSON.stringify(
    { date: new Date().toISOString(), app, appId, passed },
    null,
    2,
  ),
);
console.log(JSON.stringify({ app, appId, evidence, checks: passed.length }));
