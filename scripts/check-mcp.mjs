// Independent OAuth + MCP acceptance checks, using real Keycloak and Rust API.
// Run sequentially with web OAuth tests: this script revokes all developer sessions.
import { chromium } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { randomBytes, createHash } from "node:crypto";
const origin = "https://localhost:18443",
  resource = origin + "/mcp";
const secrets = JSON.parse(readFileSync(".local/credentials.json", "utf8"));
const evidence = JSON.parse(
  readFileSync(".local/evidence/runtime.json", "utf8"),
);
function check(condition, message) {
  if (!condition) throw new Error(message);
}
const metadata = await (
  await fetch(origin + "/.well-known/oauth-protected-resource/mcp")
).json();
check(metadata.resource === resource, "resource metadata mismatch");
const discovery = await (
  await fetch(
    metadata.authorization_servers[0] + "/.well-known/openid-configuration",
  )
).json();
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
async function authorize(target = resource, web = false, offline = false) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const verifier = randomBytes(32).toString("base64url"),
    state = randomBytes(24).toString("hex");
  let complete;
  const callback = new Promise((r) => (complete = r));
  const redirect = web
    ? origin + "/auth/callback"
    : "http://127.0.0.1:18555/callback";
  const client = web ? "miy-web" : "miy-codex";
  console.log(
    "OAuth check:",
    client,
    offline ? "offline" : "online",
    target === resource ? "mcp" : target ? "other-resource" : "web",
  );
  // Same-origin redirect chains need not trigger a new Fetch interception in
  // Chromium. Observe the callback request as well as routing it where possible.
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.origin + url.pathname === redirect) complete(url);
  });
  await page.route(
    (url) => url.origin + url.pathname === redirect,
    async (route) => {
      complete(new URL(route.request().url()));
      await route.fulfill({ status: 200, body: "OAuth completed" });
    },
  );
  const parameters = {
    client_id: client,
    response_type: "code",
    redirect_uri: redirect,
    scope: "miy.read miy.dev" + (offline ? " offline_access" : ""),
    state,
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    code_challenge_method: "S256",
  };
  if (target) parameters.resource = target;
  const url = new URL(discovery.authorization_endpoint);
  url.search = new URLSearchParams(parameters).toString();
  await page.goto(url.href);
  if (await page.locator("#username").count()) {
    await page.locator("#username").fill("developer");
    await page.locator("#password").fill(secrets.developer);
    await page.locator("#kc-login").click();
  }
  let result;
  try {
    result = await Promise.race([
      callback,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("OAuth callback timeout")), 20000),
      ),
    ]);
  } catch (error) {
    const u = new URL(page.url());
    console.error(
      "OAuth callback missing at",
      u.origin + u.pathname,
      (await page.locator("body").innerText()).slice(0, 700),
    );
    await page.screenshot({ path: ".local/evidence/oauth-failure.png" });
    throw error;
  }
  check(result.searchParams.get("state") === state, "OAuth state mismatch");
  await context.close();
  if (result.searchParams.has("error"))
    return { error: result.searchParams.get("error") };
  check(
    result.searchParams.get("iss") === discovery.issuer,
    "OAuth issuer mismatch",
  );
  const form = {
    client_id: client,
    grant_type: "authorization_code",
    code: result.searchParams.get("code"),
    redirect_uri: redirect,
    code_verifier: verifier,
  };
  if (target) form.resource = target;
  if (web) form.client_secret = secrets.web_client;
  const response = await fetch(discovery.token_endpoint, {
    method: "POST",
    body: new URLSearchParams(form),
  });
  return await response.json();
}
let rpcId = 0;
async function rpc(token, method, params) {
  const r = await fetch(resource, {
    method: "POST",
    headers: {
      Authorization: "Bearer " + token,
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      "MCP-Protocol-Version": "2025-11-25",
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: ++rpcId, method, params }),
  });
  const text = await r.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = {};
  }
  return { status: r.status, body };
}
const passed = [];
try {
  const tokens = await authorize();
  check(
    tokens.access_token && tokens.refresh_token,
    "authorization code exchange failed",
  );
  const init = await rpc(tokens.access_token, "initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "miy-independent-check", version: "0.1.0" },
  });
  check(init.status === 200 && init.body.result, "MCP initialize failed");
  const tools = await rpc(tokens.access_token, "tools/list", {});
  check(
    tools.body.result?.tools?.some((t) => t.name === "get_contract"),
    "tools discovery failed",
  );
  const contract = await rpc(tokens.access_token, "tools/call", {
    name: "get_contract",
    arguments: {},
  });
  check(
    contract.status === 200 && !contract.body.result?.isError,
    "contract call failed",
  );
  const components = await rpc(tokens.access_token, "tools/call", {
    name: "list_components", arguments: { query: "Basic miy" },
  });
  check(!components.body.result?.isError && JSON.parse(components.body.result.content[0].text).some(v=>v.name==="ui"&&v.version==="1.0.0"), "live component catalog failed");
  const component = await rpc(tokens.access_token, "tools/call", {
    name: "get_component", arguments: { name: "ui", version: "1.0.0" },
  });
  check(!component.body.result?.isError && JSON.parse(component.body.result.content[0].text).usage.includes("Panel"), "component guidance failed");
  passed.push("live-component-catalog", "component-guidance");
  passed.push(
    "metadata-discovery",
    "oauth-pkce-resource-binding",
    "mcp-initialize",
    "tools-discovery",
    "contract-call",
  );
  const resourceResult = await rpc(tokens.access_token, "resources/read", {
    uri: "miy://contracts/v1alpha1",
  });
  check(resourceResult.body.result?.contents?.length, "resource read failed");
  passed.push("resources-read");
  const denied = await rpc(tokens.access_token, "tools/call", {
    name: "prepare_database",
    arguments: {
      installationId: evidence.prod.id,
      releaseId: evidence.installations[0].releaseId,
      idempotencyKey: "deny-" + Date.now(),
    },
  });
  check(
    denied.body.result?.isError &&
      denied.body.result.content.some((c) => c.text?.includes("forbidden")),
    "production permission was not denied",
  );
  passed.push("production-denied");
  const malformed = await rpc(tokens.access_token + "x", "tools/list", {});
  check(malformed.status === 401, "invalid token signature accepted");
  passed.push("invalid-signature-denied");
  const webToken = await authorize(null, true);
  check(webToken.access_token, "web control token failed");
  check(
    (await rpc(webToken.access_token, "tools/list", {})).status === 401,
    "web audience token accepted by MCP",
  );
  passed.push("wrong-audience-denied");
  const bad = await authorize(origin + "/wrong");
  check(bad.error === "invalid_target", "unknown OAuth resource was accepted");
  passed.push("wrong-resource-denied");
  await new Promise((r) => setTimeout(r, 47000));
  check(
    (await rpc(tokens.access_token, "tools/list", {})).status === 401,
    "expired token accepted",
  );
  const refreshed = await (
    await fetch(discovery.token_endpoint, {
      method: "POST",
      body: new URLSearchParams({
        client_id: "miy-codex",
        grant_type: "refresh_token",
        refresh_token: tokens.refresh_token,
        resource,
      }),
    })
  ).json();
  check(refreshed.access_token, "OAuth refresh failed");
  check(
    (await rpc(refreshed.access_token, "tools/list", {})).status === 200,
    "refreshed MCP token rejected",
  );
  passed.push("expired-token-denied", "refresh-accepted");
  const admin = await (
    await fetch(
      origin + "/identity/realms/master/protocol/openid-connect/token",
      {
        method: "POST",
        body: new URLSearchParams({
          client_id: "admin-cli",
          grant_type: "password",
          username: "bootstrap",
          password: secrets.identity_admin,
        }),
      },
    )
  ).json();
  const revoked = await fetch(
    origin +
      "/identity/admin/realms/miy/users/00000000-0000-4000-8000-000000000002/logout",
    {
      method: "POST",
      headers: { Authorization: "Bearer " + admin.access_token },
    },
  );
  check(revoked.ok, "session revocation failed");
  check(
    (await rpc(refreshed.access_token, "tools/list", {})).status === 401,
    "revoked session token accepted",
  );
  const refreshAfterRevocation = await fetch(discovery.token_endpoint, {
    method: "POST",
    body: new URLSearchParams({
      client_id: "miy-codex",
      grant_type: "refresh_token",
      refresh_token: refreshed.refresh_token,
      resource,
    }),
  });
  check(!refreshAfterRevocation.ok, "revoked refresh token accepted");
  passed.push("session-revocation-denied", "revoked-refresh-denied");
  const offline = await authorize(resource, false, true);
  check(offline.access_token, "offline grant failed");
  check(
    (await rpc(offline.access_token, "tools/list", {})).status === 200,
    "offline access token rejected",
  );
  const consent = await fetch(
    origin +
      "/identity/admin/realms/miy/users/00000000-0000-4000-8000-000000000002/consents/miy-codex",
    {
      method: "DELETE",
      headers: { Authorization: "Bearer " + admin.access_token },
    },
  );
  check(consent.ok, "offline client grant revocation failed");
  check(
    (await rpc(offline.access_token, "tools/list", {})).status === 401,
    "revoked offline grant accepted",
  );
  const offlineRefresh = await fetch(discovery.token_endpoint, {
    method: "POST",
    body: new URLSearchParams({
      client_id: "miy-codex",
      grant_type: "refresh_token",
      refresh_token: offline.refresh_token,
      resource,
    }),
  });
  check(!offlineRefresh.ok, "revoked offline refresh token accepted");
  passed.push("offline-grant-revocation-denied");
  writeFileSync(
    ".local/evidence/mcp.json",
    JSON.stringify({ protocolVersion: "2025-11-25", passed }, null, 2),
  );
  console.log("Independent OAuth/MCP checks passed:", passed.length);
} finally {
  await browser.close();
}
