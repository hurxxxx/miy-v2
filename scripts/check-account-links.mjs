#!/usr/bin/env node
// Real-server acceptance of account links with ordinary browser sign-in: an
// administrator issues an invitation on the console, a new person creates their own
// sign-in account from the link and signs in to the portal, recovers the password
// through an administrator's reset link and then changes it on their own.
//
// This CHANGES the selected server: it registers one person named
// "계정 링크 점검 <id>" (sign-in name link-check-<id>). The account is disabled at
// the end unless --keep-account true is given; its records remain.
// Passwords, cookies and link secrets are never printed or stored.
//
// Usage: node scripts/check-account-links.mjs --origin https://PORTAL
//   --credentials-file /abs/credentials.json --output NEW_FILE.json
//   [--keep-account true] [--lockout true] [--browser-channel chrome]
// --lockout true also sends thirteen sign-in attempts to show the temporary lock.
// The credentials file is { "admin": "password" } for the platform administrator.
import { randomBytes } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { chromium } from "@playwright/test";

const error = code => Object.assign(new Error(code), { safeCode: code });
const safe = value => typeof value === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(value) ? value : "unexpected_failure";
function requireThat(condition, code) { if (!condition) throw error(code); }

const result = { schemaVersion: 1, harness: "account-links-v1", startedAt: new Date().toISOString(), finishedAt: null,
  status: "running", passes: [], cleanup: "not_needed" };
const passed = name => { result.passes.push(name); console.log("PASS " + name); };
let stage = "arguments", output, browser, admin, origins, accountId;

async function signIn(context, origin, username, password) {
  const page = await context.newPage();
  await page.goto(origin + "/auth/login");
  await page.locator("input[name=username]").fill(username);
  await page.locator("input[name=password]").fill(password);
  await page.locator("#kc-login").click();
  return page;
}

async function me(context, origin) {
  const response = await context.request.get(origin + "/api/v1/me");
  return { status: response.status(), body: response.status() === 200 ? await response.json() : null };
}

/** The address shown once after issuing; it stays in memory only. */
async function issuedLink(page, expectedPage) {
  const address = (await page.locator(".account-link-issued pre code").innerText()).trim();
  const url = new URL(address);
  requireThat(url.origin === origins.console && url.pathname === expectedPage
    && /^#[A-Za-z0-9_-]{43}$/.test(url.hash) && !url.search, "issued_link_shape");
  return address;
}

async function openLink(address) {
  const context = await browser.newContext({ locale: "ko-KR" });
  context.setDefaultTimeout(30000);
  const page = await context.newPage();
  const requested = [];
  page.on("request", request => requested.push(request.url()));
  await page.goto(address);
  return { context, page, requested };
}

try {
  const values = {};
  for (let index = 2; index < process.argv.length; index += 2) {
    const flag = process.argv[index];
    requireThat(["--origin", "--credentials-file", "--output", "--keep-account", "--lockout", "--browser-channel"].includes(flag)
      && process.argv[index + 1] !== undefined && !(flag in values), "invalid_arguments");
    values[flag] = process.argv[index + 1];
  }
  requireThat(["--origin", "--credentials-file", "--output"].every(key => values[key]), "required_arguments_missing");
  requireThat(isAbsolute(values["--credentials-file"]) && isAbsolute(values["--output"]), "absolute_paths_required");
  requireThat((await lstat(values["--credentials-file"])).isFile(), "regular_private_input_file_required");
  requireThat(await lstat(values["--output"]).then(() => false, () => true), "output_must_be_new");
  requireThat(["true", undefined].includes(values["--keep-account"]) && ["true", undefined].includes(values["--lockout"]), "invalid_arguments");
  output = values["--output"];
  const portal = new URL(values["--origin"]);
  requireThat(portal.origin === values["--origin"].replace(/\/$/, "") && /^https?:$/.test(portal.protocol), "invalid_origin");

  stage = "server";
  const options = await (await fetch(portal.origin + "/auth/options")).json();
  requireThat(options.surface === "portal" && options.portalOrigin === portal.origin, "not_the_portal_origin");
  origins = { portal: portal.origin, console: options.consoleOrigin };
  requireThat(options.accountLinks === true, "account_links_not_configured");
  const capabilities = await (await fetch(portal.origin + "/api/v1/capabilities")).json();
  requireThat(capabilities.features.includes("accounts.links.v1"), "account_links_not_advertised");
  // Every sign-in below is ordinary OIDC, also on a local profile that offers development accounts.
  Object.assign(result, { origin: origins.portal, consoleOrigin: origins.console, serverVersion: capabilities.serverVersion,
    developmentLogin: options.developmentLogin === true });
  passed("server advertises configured account links");

  stage = "administrator_sign_in";
  let credentials = JSON.parse(await readFile(values["--credentials-file"], "utf8"));
  requireThat(typeof credentials.admin === "string" && credentials.admin, "admin_credential_required");
  browser = await chromium.launch({ channel: values["--browser-channel"] || "chrome", headless: true, timeout: 30000 });
  admin = await browser.newContext({ locale: "ko-KR" });
  admin.setDefaultTimeout(30000);
  const console_ = await signIn(admin, origins.console, "admin", credentials.admin);
  credentials = null;
  await console_.waitForURL(url => url.origin === origins.console && url.pathname === "/");
  const administrator = await me(admin, origins.console);
  requireThat(administrator.body?.admin === true && administrator.body.browserPurpose === "console", "not_a_console_administrator");
  const headers = { origin: origins.console, "x-miy-csrf": administrator.body.csrf, "content-type": "application/json" };
  passed("administrator signs in to the console with ordinary OIDC");

  stage = "invitation";
  const id = randomBytes(4).toString("hex");
  const person = { username: "link-check-" + id, name: "계정 링크 점검 " + id, email: `link-check-${id}@accounts.invalid` };
  const firstPassword = randomBytes(12).toString("hex") + "-Aa1";
  const secondPassword = randomBytes(12).toString("hex") + "-Bb2";
  const thirdPassword = randomBytes(12).toString("hex") + "-Cc3";
  await console_.goto(origins.console + "/admin");
  const invitePanel = console_.locator("section.account-links");
  await invitePanel.locator("input[name=name]").fill(person.name);
  await invitePanel.getByRole("button", { name: "초대 링크 만들기" }).click();
  const invitation = await issuedLink(console_, "/join");
  const listed = await (await admin.request.get(origins.console + "/api/v1/account-links")).json();
  requireThat(listed.links.some(link => link.kind === "invitation" && link.name === person.name && link.state === "pending")
    && !JSON.stringify(listed).includes(new URL(invitation).hash.slice(1)), "invitation_not_listed_without_secret");
  passed("console issues an invitation whose secret is shown once and never listed");

  stage = "account_creation";
  const guest = await openLink(invitation);
  await guest.page.getByRole("heading", { name: "계정 만들기" }).waitFor();
  // The secret leaves the address bar and never travels in a request URL.
  requireThat(new URL(guest.page.url()).hash === "" && !guest.requested.some(url => url.includes(new URL(invitation).hash.slice(1))),
    "link_secret_left_in_an_address");
  requireThat(await guest.page.locator("input[name=name]").inputValue() === person.name, "invited_name_not_offered");
  await guest.page.locator("input[name=username]").fill(person.username);
  await guest.page.locator("input[name=email]").fill(person.email);
  await guest.page.locator("input[name=password]").fill(firstPassword);
  await guest.page.locator("input[name=confirm]").fill(firstPassword);
  await guest.page.getByRole("button", { name: "계정 만들기" }).click();
  await guest.page.getByRole("heading", { name: "계정을 만들었습니다" }).waitFor();
  requireThat(await guest.page.getByRole("link", { name: "로그인하러 가기" }).getAttribute("href") === origins.portal + "/",
    "login_link_not_the_portal");
  const users = await (await admin.request.get(origins.console + "/api/v1/users")).json();
  const account = users.find(user => user.name === person.name);
  requireThat(account && account.enabled === true && account.admin === false && account.canDevelop === false, "person_not_registered_as_invited");
  accountId = account.id;
  result.accountId = accountId;
  passed("invited person creates their own sign-in account and is registered without privileges");

  stage = "first_sign_in";
  await guest.page.getByRole("link", { name: "로그인하러 가기" }).click();
  await guest.page.locator("a.login-primary").click();
  await guest.page.locator("input[name=username]").waitFor();
  // A Korean browser gets the Korean sign-in page (the realm also offers English).
  result.signInPageLanguage = await guest.page.locator("html").getAttribute("lang");
  requireThat(result.signInPageLanguage === "ko", "sign_in_page_not_korean");
  await guest.page.locator("input[name=username]").fill(person.username);
  await guest.page.locator("input[name=password]").fill(firstPassword);
  await guest.page.locator("#kc-login").click();
  // No profile form may stand between the new account and the portal.
  await guest.page.waitForURL(url => url.origin === origins.portal && url.pathname === "/");
  const signedIn = await me(guest.context, origins.portal);
  requireThat(signedIn.body?.id === accountId && signedIn.body.name === person.name && signedIn.body.admin === false
    && signedIn.body.browserPurpose === "portal", "portal_identity_mismatch");
  await guest.page.getByRole("heading", { name: "운영 앱" }).waitFor();
  const installations = await guest.context.request.get(origins.portal + "/api/v1/installations");
  requireThat(installations.status() === 200 && (await installations.json()).length === 0, "new_person_received_an_app");
  passed("new person signs in to the portal directly and sees no app until one is granted");

  stage = "used_invitation";
  const reused = await openLink(invitation);
  await reused.page.getByRole("heading", { name: "사용할 수 없는 링크입니다" }).waitFor();
  await reused.context.close();
  passed("a used invitation is refused");

  stage = "password_reset";
  await console_.goto(origins.console + "/admin");
  const row = console_.locator("li.user-account", { hasText: accountId });
  await row.getByRole("button", { name: "비밀번호 재설정 링크" }).click();
  await console_.getByRole("button", { name: "링크 만들기", exact: true }).click();
  const reset = await issuedLink(console_, "/reset-password");
  const recovery = await openLink(reset);
  await recovery.page.getByRole("heading", { name: "비밀번호 재설정" }).waitFor();
  await recovery.page.locator("input[name=password]").fill(secondPassword);
  await recovery.page.locator("input[name=confirm]").fill(secondPassword);
  await recovery.page.getByRole("button", { name: "비밀번호 변경" }).click();
  await recovery.page.getByRole("heading", { name: "비밀번호를 변경했습니다" }).waitFor();
  await recovery.context.close();
  requireThat((await me(guest.context, origins.portal)).status === 401, "earlier_browser_session_survived_the_reset");
  // Keep the same browser's identity-provider cookies and start sign-in again.
  // A logged-out Keycloak session must ask for credentials instead of silently
  // creating a fresh portal session.
  await guest.page.goto(origins.portal + "/auth/login");
  await guest.page.locator("input[name=password]").waitFor();
  requireThat(new URL(guest.page.url()).origin === origins.console
    && (await me(guest.context, origins.portal)).status === 401, "earlier_identity_session_survived_the_reset");
  await guest.context.close();
  passed("an administrator's reset link sets a new password and ends earlier portal and Keycloak browser sessions");

  stage = "sign_in_after_reset";
  const stale = await browser.newContext({ locale: "ko-KR" });
  stale.setDefaultTimeout(30000);
  const refused = await signIn(stale, origins.portal, person.username, firstPassword);
  await refused.locator("#input-error, #input-error-username, .pf-m-error, .alert-error").first().waitFor();
  requireThat(new URL(refused.url()).origin !== origins.portal && (await me(stale, origins.portal)).status === 401, "old_password_still_signed_in");
  await stale.close();
  const fresh = await browser.newContext({ locale: "ko-KR" });
  fresh.setDefaultTimeout(30000);
  const returned = await signIn(fresh, origins.portal, person.username, secondPassword);
  await returned.waitForURL(url => url.origin === origins.portal && url.pathname === "/");
  requireThat((await me(fresh, origins.portal)).body?.id === accountId, "new_password_identity_mismatch");
  const recoveryAgain = await openLink(reset);
  await recoveryAgain.page.getByRole("heading", { name: "사용할 수 없는 링크입니다" }).waitFor();
  await recoveryAgain.context.close();
  passed("only the new password signs in and the reset link works once");

  stage = "own_password_change";
  // The header link opens the sign-in server's own form; the platform never sees the password.
  requireThat(await returned.locator("a.account-password").getAttribute("href") === origins.portal + "/auth/password",
    "password_change_not_offered");
  await returned.locator("a.account-password").click();
  await returned.locator("input[name='password-new']").fill(thirdPassword);
  await returned.locator("input[name='password-confirm']").fill(thirdPassword);
  await returned.locator("input[name='password-confirm']").press("Enter");
  await returned.waitForURL(url => url.origin === origins.portal && url.pathname === "/");
  requireThat((await me(fresh, origins.portal)).body?.id === accountId, "session_lost_after_password_change");
  await fresh.close();
  for (const [password, accepted] of [[secondPassword, false], [thirdPassword, true]]) {
    const context = await browser.newContext({ locale: "ko-KR" });
    context.setDefaultTimeout(30000);
    const page = await signIn(context, origins.portal, person.username, password);
    if (accepted) await page.waitForURL(url => url.origin === origins.portal && url.pathname === "/");
    else await page.locator("#input-error, #input-error-username, .pf-m-error, .alert-error").first().waitFor();
    requireThat(((await me(context, origins.portal)).status === 200) === accepted, "password_change_not_effective");
    await context.close();
  }
  passed("a signed-in person changes their own password on the sign-in server's form");

  if (values["--lockout"] === "true") {
    stage = "lockout";
    // Repeated wrong passwords lock the account for a while: the right one is refused too.
    const context = await browser.newContext({ locale: "ko-KR" });
    context.setDefaultTimeout(30000);
    const page = await context.newPage();
    await page.goto(origins.portal + "/auth/login");
    for (const password of [...Array.from({ length: 12 }, () => "wrong-" + randomBytes(8).toString("hex")), thirdPassword]) {
      await page.locator("input[name=username]").fill(person.username);
      await page.locator("input[name=password]").fill(password);
      await Promise.all([page.waitForEvent("load"), page.locator("#kc-login").click()]);
      requireThat(new URL(page.url()).origin === origins.console, "sign_in_left_the_identity_page_during_lockout");
    }
    requireThat((await me(context, origins.portal)).status === 401, "locked_account_signed_in");
    await context.close();
    passed("repeated wrong passwords temporarily lock the account");
  }

  stage = "cleanup";
  if (values["--keep-account"] === "true") {
    result.cleanup = "kept";
  } else {
    const disabled = await admin.request.post(`${origins.console}/api/v1/users/${accountId}/state`, { headers, data: { enabled: false } });
    requireThat(disabled.status() === 200, "account_not_disabled");
    result.cleanup = "disabled";
    passed("the acceptance account is disabled");
  }
  result.status = "PASS";
} catch (failure) {
  result.status = "FAIL";
  result.failedStage = stage;
  result.errorCode = safe(failure?.safeCode ?? failure?.name);
  if (accountId && admin && result.cleanup === "not_needed") {
    // Do not leave an enabled acceptance account behind a failed run.
    try {
      const administrator = await me(admin, origins.console);
      const disabled = await admin.request.post(`${origins.console}/api/v1/users/${accountId}/state`, {
        headers: { origin: origins.console, "x-miy-csrf": administrator.body.csrf, "content-type": "application/json" },
        data: { enabled: false },
      });
      result.cleanup = disabled.status() === 200 ? "disabled" : "failed";
    } catch {
      result.cleanup = "failed";
    }
  }
  // Isolated stacks only: a browser failure log can quote what was typed into a form.
  if (process.env.MIY_CHECK_DEBUG === "1") console.error(String(failure?.message).split("\n").slice(0, 12).join("\n"));
  console.log(`FAIL ${stage}: ${result.errorCode}; credentials and link secrets omitted`);
  process.exitCode = 1;
} finally {
  await browser?.close();
  result.finishedAt = new Date().toISOString();
  if (output) await writeFile(output, JSON.stringify(result, null, 2) + "\n", { mode: 0o600, flag: "wx" }).catch(() => {});
  console.log(JSON.stringify({ status: result.status, passes: result.passes.length, cleanup: result.cleanup }));
}
