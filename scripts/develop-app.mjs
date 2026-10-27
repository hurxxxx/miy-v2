// Optional audit recorder around the user's real Codex CLI. No model, auth, trust,
// approval, sandbox or global configuration overrides. Normal CLI approval still applies.
import { spawn, execFileSync } from "node:child_process";
import { cliData } from "./lib/cli-result.mjs";
import {
  readFileSync,
  existsSync,
  mkdirSync,
  createWriteStream,
} from "node:fs";
import { resolve, join } from "node:path";
const app = resolve(process.argv[2] || ".");
const promptPath = process.argv[3];
if (!promptPath)
  throw Error(
    "Usage: node scripts/develop-app.mjs APP_PATH PROMPT_FILE [EVIDENCE_LABEL] [--dry-run]",
  );
const prompt = readFileSync(promptPath, "utf8");
const label = process.argv[4] && !process.argv[4].startsWith("--") ? process.argv[4] : "app-codex";
if (!/^[a-z0-9-]+$/.test(label)) throw Error("Invalid evidence label");
const cli = join(app, ".miy-tools", process.platform === "win32" ? "miy.exe" : "miy");
if (!existsSync(cli))
  throw Error("Install miy and run miy app setup in the app directory first.");
const env = { ...process.env };
const fixtureState = resolve(import.meta.dirname, "../.local/workflow-app.json");
if (existsSync(fixtureState)) {
  const state = JSON.parse(readFileSync(fixtureState));
  if (state.app && resolve(state.app) === app && state.configDir) {
    // Only this exact generated workflow app inherits its isolated MIY vault.
    // Codex's own login/model/approval configuration is untouched.
    env.MIY_CONFIG_DIR = state.configDir;
    env.MIY_CREDENTIAL_BACKEND = "file";
    delete env.MIY_CONTEXT;
  }
}
const doctor = cliData(
  execFileSync(cli, ["--output", "json", "doctor", "--for", "develop"], { cwd: app, env, encoding: "utf8" }),
  "doctor",
);
if (!doctor.ready || !["projectGuidance", "authentication"].every(id => doctor.checks.some(check => check.id === id && check.status === "pass")))
  throw Error(
    "Run miy app setup and miy auth login before invoking the coding agent.",
  );
const executable = "codex";
const args = ["exec", "--json", "-"];
if (process.argv.includes("--dry-run")) {
  console.log(
    JSON.stringify({
      executable,
      args,
      cwd: app,
      ready: true,
      modelInvoked: false,
    }),
  );
} else {
  const output = resolve(".local/evidence");
  mkdirSync(output, { recursive: true });
  const events = createWriteStream(join(output, label + ".jsonl"), {
    mode: 0o600,
  });
  const errors = createWriteStream(join(output, label + ".stderr"), {
    mode: 0o600,
  });
  const child = spawn(executable, args, {
    cwd: app,
    env,
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stdout.pipe(events);
  child.stderr.pipe(errors);
  child.stdin.end(prompt);
  console.log(
    "Codex started with the app's project MCP and the user's normal configuration.",
  );
  const code = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", resolve);
  });
  events.end();
  errors.end();
  process.exitCode = code ?? 1;
}
