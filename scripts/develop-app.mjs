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
const label = process.argv[4] || "app-codex";
if (!/^[a-z0-9-]+$/.test(label)) throw Error("Invalid evidence label");
const cli = join(app, ".miy-tools", process.platform === "win32" ? "miy.exe" : "miy");
if (!existsSync(cli))
  throw Error("Install miy and run miy app setup in the app directory first.");
const doctor = cliData(
  execFileSync(cli, ["--output", "json", "app", "doctor"], { cwd: app, encoding: "utf8" }),
  "app doctor",
);
if (!doctor.mcp.configured || !doctor.platform?.connected)
  throw Error(
    "Run miy app setup and miy app login before invoking the coding agent.",
  );
const args = ["app", "codex", "--", "exec", "--json", "-"];
if (process.argv.includes("--dry-run")) {
  console.log(
    JSON.stringify({
      executable: cli,
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
  const child = spawn(cli, args, {
    cwd: app,
    env: process.env,
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
