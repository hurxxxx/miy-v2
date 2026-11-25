// Invoked by the isolated development DB regression when MIY_PREVIEW_TEST_WEB
// names a built portal. Both test servers must be on loopback.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";

const [local, portal] = process.argv.slice(2);
for (const origin of [local, portal]) assert.equal(new URL(origin).hostname, "127.0.0.1");
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const users = [];
  for (const [account, id] of [["developer", "developer"], ["alice", "other"]]) {
    const profile = await browser.newContext({ locale: "ko-KR" });
    const page = await profile.newPage();
    await page.goto(local);
    await page.waitForURL(url => url.origin === portal && url.pathname.startsWith("/preview/login/"));
    assert.equal((await profile.request.get(local + "/__miy/context")).status(), 401);
    if (account === "developer") {
      // An anonymous browser sees the platform's existing sign-in page.
      await page.getByRole("button", { name: /개발자/ }).click();
    } else {
      // Establish the other browser's dev portal session independently, then
      // visit local again: SSO should return without another sign-in click.
      const signIn = await profile.request.post(portal + "/auth/development", {
        headers: { origin: portal }, data: { account },
      });
      assert.equal(signIn.status(), 200);
      await page.goto(local);
    }
    await page.waitForURL(url => url.origin === local && url.pathname.startsWith("/apps/"));
    const context = await (await profile.request.get(local + "/__miy/context")).json();
    assert.equal(context.user.id, id);
    const who = await (await profile.request.get(local + "/__miy/api/who")).json();
    assert.deepEqual(who, { id, admin: false, directoryUser: id });
    users.push({ profile, context });
  }
  const [first, second] = users;
  assert.notEqual(first.context.csrf, second.context.csrf);
  assert.equal((await first.profile.request.post(local + "/__miy/api/who", {
    headers: { origin: local, "x-miy-csrf": second.context.csrf },
  })).status(), 403);
  const me = await (await second.profile.request.get(portal + "/api/v1/me")).json();
  const logout = await second.profile.request.post(portal + "/auth/logout", {
    headers: { origin: portal, "x-miy-csrf": me.csrf },
  });
  assert.equal(logout.status(), 200);
  assert.equal((await second.profile.request.get(local + "/__miy/context")).status(), 401);
  assert.equal((await first.profile.request.get(local + "/__miy/context")).status(), 200);
  console.log("PASS two browser profiles, common login, existing dev session, signed API and backend directory, CSRF isolation and logout");
} finally {
  await browser.close();
}
