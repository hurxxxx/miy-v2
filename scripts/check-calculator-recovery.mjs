// Verify the user's deployed calculator after the immutable-version recovery.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
const origin = "http://localhost:18000";
const appId = "my-local-test";
const passed = [];
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
const context = await browser.newContext();
try {
  assert.equal(
    (
      await context.request.post(origin + "/auth/development", {
        headers: { origin },
        data: { account: "developer" },
      })
    ).status(),
    200,
  );
  const apps = await (
    await context.request.get(origin + "/api/v1/installations")
  ).json();
  const app = apps.find((a) => a.appId === appId && a.environment === "dev");
  assert.equal(app.runtimeStatus, "running");
  assert.equal(app.dbStatus, "ready");
  assert.equal(app.version, "0.1.1");
  const built = JSON.parse(
    readFileSync("../miy-apps/my-local-test/dist/image.json"),
  );
  assert.equal(app.imageId, built.imageId);
  assert.equal(app.sourceCommit, built.sourceCommit);
  const actual = execFileSync(
    "docker",
    [
      "inspect",
      "--format",
      "{{.Image}}",
      "miy-app-" + app.id.replaceAll("-", ""),
    ],
    { encoding: "utf8" },
  ).trim();
  assert.equal(actual, built.imageId);
  passed.push("active-release-and-docker-match-calculator-build");
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(origin + "/apps/" + appId);
  await page.getByRole("heading", { name: "계산기", exact: true }).waitFor();
  const press = async (name) =>
    page.getByRole("button", { name, exact: true }).click();
  const result = page.getByLabel("계산 결과", { exact: true });
  for (const key of ["1", "2", "더하기", "7", "계산하기"]) await press(key);
  assert.equal((await result.textContent()).trim(), "19");
  passed.push("router-loaded-calculator-and-12-plus-7-is-19");
  await press("전체 초기화");
  await page.keyboard.type("0.1+0.2");
  await page.keyboard.press("Enter");
  assert.equal((await result.textContent()).trim(), "0.3");
  passed.push("keyboard-decimal-calculation");
  await press("전체 초기화");
  for (const key of ["8", "나누기", "0", "계산하기"]) await press(key);
  await page.getByRole("alert").waitFor();
  await press("3");
  assert.equal((await result.textContent()).trim(), "3");
  passed.push("divide-by-zero-and-input-recovery");
  await page.reload();
  await page.getByRole("heading", { name: "계산기", exact: true }).waitFor();
  assert.equal((await result.textContent()).trim(), "0");
  assert.equal(await page.locator("iframe").count(), 0);
  assert.deepEqual(errors, []);
  passed.push("direct-url-reload-no-iframe-no-js-errors");
  await page.screenshot({
    path: ".local/evidence/calculator-platform.png",
    fullPage: true,
  });
  writeFileSync(
    ".local/evidence/calculator-platform.json",
    JSON.stringify(
      {
        date: new Date().toISOString(),
        appId,
        url: origin + "/apps/" + appId,
        version: app.version,
        imageId: app.imageId,
        sourceCommit: app.sourceCommit,
        registeredName: app.name,
        passed,
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ appId, version: app.version, passed }));
} finally {
  await context.close();
  await browser.close();
}
