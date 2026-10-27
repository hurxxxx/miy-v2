import { test, expect, type Page } from "@playwright/test";

const serverUrl = "https://configured-platform.example:8443";
const guide = {
  schemaVersion: 1,
  serverUrl,
  cliVersion: "0.6.0",
  commands: {
    unix: `bash ./install-cli.sh --server '${serverUrl}'`,
    windows: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\\install-cli.ps1 -ArchivePath $Archive -Sha256 $ExpectedSha256 -Server '${serverUrl}'`,
    login: "miy auth login",
  },
};

async function setup(page: Page, signedIn = false, unavailable = false) {
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/me") {
      await route.fulfill({
        status: signedIn ? 200 : 401,
        json: signedIn ? { id: "fixture", name: "설치 사용자", admin: false, csrf: "fixture" } : {},
      });
    } else if (path === "/api/v1/cli/install") {
      await route.fulfill({ status: unavailable ? 503 : 200, json: unavailable ? {} : guide });
    } else {
      await route.fulfill({ json: [] });
    }
  });
  await page.route("**/auth/options", (route) => route.fulfill({
    json: { developmentLogin: false, developmentOrigin: "", accounts: [], oauthUrl: "/auth/login" },
  }));
}

test("public installation guide uses server configuration and copies exact commands", async ({ page }, testInfo) => {
  await setup(page);
  await page.addInitScript(() => {
    const state = window as typeof window & { copiedCommand?: string };
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text: string) => { state.copiedCommand = text; } },
    });
  });
  await page.goto("/");
  await page.getByRole("link", { name: "CLI 설치 안내" }).click();
  await expect(page).toHaveURL(/\/cli\/install$/);
  await expect(page.getByRole("heading", { name: "CLI 설치", exact: true })).toBeVisible();
  await expect(page.getByTestId("cli-install-server")).toHaveText(serverUrl);
  expect(new URL(page.url()).origin).not.toBe(serverUrl);
  await expect(page.getByLabel("Linux·macOS 설치 명령", { exact: true })).toHaveText(guide.commands.unix);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveText(guide.commands.windows);
  await expect(page.getByLabel("CLI 로그인 명령", { exact: true })).toHaveText("miy auth login");
  await expect(page.getByText("CLI에는 기본 IP나 도메인이 없습니다.", { exact: false })).toBeVisible();
  await expect(page.getByText("설치 파일을 준비 중입니다.", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "Windows 설치 명령 복사", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(guide.commands.windows);
  await page.getByRole("button", { name: "CLI 로그인 명령 복사", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(guide.commands.login);
  await page.screenshot({ path: testInfo.outputPath("cli-install-desktop.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath("cli-install-mobile.png"), fullPage: true });
  await page.getByRole("link", { name: "플랫폼으로 돌아가기" }).click();
  await expect(page.getByRole("link", { name: "로그인", exact: true })).toBeVisible();
});

test("signed-in users can open installation guidance from the platform menu", async ({ page }) => {
  await setup(page, true);
  await page.goto("/environments/dev");
  await page.getByRole("link", { name: "CLI 설치", exact: true }).click();
  await expect(page.getByTestId("cli-install-server")).toHaveText(serverUrl);
  await expect(page.locator("header")).toContainText("설치 사용자");
});

test("unavailable server guidance never falls back to a browser-derived installation command", async ({ page }) => {
  await setup(page, false, true);
  await page.goto("/cli/install");
  await expect(page.getByRole("alert")).toContainText("CLI 설치 정보를 불러오지 못했습니다");
  await expect(page.getByTestId("cli-install-server")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /복사/ })).toHaveCount(0);
});

test("published candidate exposes immutable archive and checksum links with its install command", async ({ page }) => {
  await setup(page);
  const commit = "a".repeat(40);
  const file = "miy-0.6.0-x86_64-pc-windows-msvc.zip";
  const url = `/cli/downloads/${commit}/${file}`;
  const command = `$Archive = Join-Path $env:TEMP '${file}'\nInvoke-WebRequest -UseBasicParsing -Uri '${serverUrl}${url}' -OutFile $Archive\n# verified candidate installation\npowershell.exe -File .\\install-cli.ps1 -Server '${serverUrl}' -AddToPath`;
  await page.route("**/api/v1/cli/install", (route) => route.fulfill({ json: {
    ...guide, sourceCommit: commit,
    commands: { ...guide.commands, windows: command },
    downloads: [{ target: "x86_64-pc-windows-msvc", file, sha256: "b".repeat(64), url, checksumUrl: `${url}.sha256` }],
  } }));
  await page.goto("/cli/install");
  await expect(page.getByRole("link", { name: file, exact: true })).toHaveAttribute("href", url);
  await expect(page.getByRole("link", { name: "SHA-256", exact: true })).toHaveAttribute("href", `${url}.sha256`);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveText(command);
  await expect(page.getByText(commit, { exact: true })).toBeVisible();
  await expect(page.getByText("설치 파일을 준비 중입니다.", { exact: false })).toHaveCount(0);
});
