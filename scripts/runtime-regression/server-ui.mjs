// Real core server + built platform UI on loopback, against a disposable database.
// Never reads .local/runtime.env, never touches the running platform, its DB or networks.
import { execFileSync, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createReadStream, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer, request } from "node:http";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const evidence = resolve(root, ".local/evidence/server-ui");
const lock = `${evidence}.lock`;
const site = join(evidence, "site");
const UI_PORT = 18148;
const API_PORT = 18149;
const ISSUER_PORT = 18150;
const origin = `http://127.0.0.1:${UI_PORT}`;
const issuer = `http://127.0.0.1:${ISSUER_PORT}/identity/realms/miy`;
const owner = `${process.pid}-${randomBytes(8).toString("hex")}`;
const label = `miy.server-ui=${owner}`;
const database = `miy-server-ui-${owner}`;
const binary = resolve(root, "target/debug/miy");
const children = [];
let ownsDatabase = false;
let ownsLock = false;

function cleanup() {
  for (const child of children) child.kill("SIGTERM");
  try {
    if (!ownsDatabase) return;
    const actualOwner = execFileSync("docker", ["inspect", "-f", '{{ index .Config.Labels "miy.server-ui" }}', database], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
    if (actualOwner === owner) {
      execFileSync("docker", ["rm", "-f", "-v", database], { stdio: "ignore" });
      ownsDatabase = false;
    }
  } catch {} finally {
    if (ownsLock) {
      rmSync(lock, { recursive: true, force: true });
      ownsLock = false;
    }
  }
}
for (const signal of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(signal, () => (cleanup(), process.exit(0)));
process.on("exit", cleanup);

if (!existsSync(binary)) throw new Error("build the core first: cargo build");
mkdirSync(dirname(evidence), { recursive: true });
// Ports and evidence paths are fixed; fail before touching another run's files.
mkdirSync(lock);
ownsLock = true;
// Other invocations may still be running. Cleanup is limited to this execution's
// successfully created container; interrupted runs require explicit inspection.
rmSync(evidence, { recursive: true, force: true });
mkdirSync(join(evidence, "artifacts"), { recursive: true });

// 1. Disposable PostgreSQL: random loopback port, tmpfs storage, labelled for cleanup.
const password = randomBytes(18).toString("hex");
execFileSync("docker", [
  "run", "-d", "--name", database, "--label", label, "--memory", "512m", "--pids-limit", "128",
  "-p", "127.0.0.1::5432", "-e", `POSTGRES_PASSWORD=${password}`, "-e", "POSTGRES_DB=miy_ui_core",
  "--tmpfs", "/var/lib/postgresql", "postgres:18.3",
], { stdio: "ignore" });
ownsDatabase = true;
for (let i = 0; ; i++) {
  try {
    execFileSync("docker", ["exec", database, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"], { stdio: "ignore" });
    break;
  } catch {
    if (i > 120) throw new Error("isolated postgres not ready");
    await new Promise((r) => setTimeout(r, 250));
  }
}
const port = execFileSync("docker", ["port", database, "5432/tcp"]).toString().trim().split(":").at(-1);

// 2. Fixture identity metadata (listening before the core starts): discovery and an empty JWKS. Only the explicit
// loopback development login is exercised; no OIDC token is ever issued.
const identity = createServer((req, res) => {
  const base = `http://127.0.0.1:${ISSUER_PORT}/identity/realms/miy`;
  const body = req.url.endsWith("/.well-known/openid-configuration")
    ? {
        issuer: base,
        authorization_endpoint: `${base}/protocol/openid-connect/auth`,
        token_endpoint: `${base}/protocol/openid-connect/token`,
        introspection_endpoint: `${base}/protocol/openid-connect/token/introspect`,
        jwks_uri: `${base}/protocol/openid-connect/certs`,
        end_session_endpoint: `${base}/protocol/openid-connect/logout`,
      }
    : { keys: [] };
  res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(body));
});

const env = {
  ...process.env,
  DATABASE_URL: `postgres://postgres:${password}@127.0.0.1:${port}/miy_ui_core`,
  PROVISION_DATABASE_URL: `postgres://postgres:${password}@127.0.0.1:${port}/postgres`,
  MIY_ORIGIN: origin,
  MIY_ISSUER: issuer,
  MIY_ALLOW_HTTP: "1",
  MIY_DEV_LOGIN: "1",
  MIY_DEV_ORIGIN: origin,
  MIY_BIND_ADDR: `127.0.0.1:${API_PORT}`,
  MIY_WEB_SECRET: randomBytes(32).toString("hex"),
  MIY_RESOURCE_SECRET: randomBytes(32).toString("hex"),
  MIY_ENCRYPTION_KEY: randomBytes(32).toString("hex"),
  MIY_ARTIFACTS: join(evidence, "artifacts"),
  RUST_LOG: "miy=info",
};
writeFileSync(join(evidence, "environment.json"), JSON.stringify({ origin, issuer, apiPort: API_PORT, database, databasePort: port }, null, 2));
execFileSync(binary, ["migrate"], { env, cwd: root, stdio: "inherit" });
execFileSync(binary, ["seed"], { env, cwd: root, stdio: "inherit" });

// 3. Platform build in its own output directory; the running platform build is untouched.
// Synchronous steps finish before any server must answer requests.
execFileSync("pnpm", ["--filter", "@miy/platform", "exec", "vite", "build", "--mode", "production", "--outDir", site, "--emptyOutDir"], {
  cwd: root,
  stdio: "inherit",
});
await new Promise((ready) => identity.listen(ISSUER_PORT, "127.0.0.1", ready));
const server = spawn(binary, ["serve"], { env, cwd: root, stdio: ["ignore", "inherit", "inherit"] });
children.push(server);
server.on("exit", (code) => {
  console.error(`core server exited (${code})`);
  process.exit(1);
});

// 4. One origin for the browser: static platform files, /api and /auth proxied to the core.
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2", ".json": "application/json", ".svg": "image/svg+xml" };
createServer((req, res) => {
  const path = new URL(req.url, origin).pathname;
  if (/^\/(api|auth|artifacts)\//.test(path)) {
    const upstream = request(
      { host: "127.0.0.1", port: API_PORT, method: req.method, path: req.url, headers: req.headers },
      (reply) => {
        res.writeHead(reply.statusCode, reply.headers);
        reply.pipe(res);
      },
    );
    upstream.on("error", () => res.writeHead(502).end());
    req.pipe(upstream);
    return;
  }
  let file = resolve(site, "." + decodeURIComponent(path));
  if (!file.startsWith(site) || !existsSync(file) || statSync(file).isDirectory()) file = join(site, "index.html");
  res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(res);
}).listen(UI_PORT, "127.0.0.1", () => console.log(`server-ui ready on ${origin}`));
