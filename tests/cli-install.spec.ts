import { browserOptions, browserUser, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";

const serverUrl = "https://configured-platform.example:8443";
const guide = {
  schemaVersion: 1,
  serverUrl,
  cliVersion: "0.11.0",
  downloads: ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"].map((target) => ({
    target, file: `miy-${target}`, sha256: "b".repeat(64), url: `/cli/downloads/fixture/miy-${target}`, checksumUrl: `/cli/downloads/fixture/miy-${target}.sha256`,
  })),
  commands: {
    unix: `curl -fsSL '${serverUrl}/api/v1/cli/install.sh' | bash`,
    windows: `& ([scriptblock]::Create((Invoke-RestMethod -Uri '${serverUrl}/api/v1/cli/install.ps1')))`,
    login: "miy auth login",
  },
};

async function setup(page: Page, signedIn = false, unavailable = false) {
  await mockSurface(page);
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/me") {
      await route.fulfill({
        status: signedIn ? 200 : 401,
        json: signedIn ? browserUser(route, { id: "fixture", name: "설치 사용자", admin: false, csrf: "fixture" }) : {},
      });
    } else if (path === "/api/v1/cli/install") {
      await route.fulfill({ status: unavailable ? 503 : 200, json: unavailable ? {} : guide });
    } else {
      await route.fulfill({ json: [] });
    }
  });
  await page.route("**/auth/options", (route) => route.fulfill({
    json: { ...browserOptions(route), developmentLogin: false, developmentOrigin: "", accounts: [], oauthUrl: "/auth/login" },
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
  await page.getByRole("link", { name: "통합 설치" }).click();
  await expect(page).toHaveURL(/\/cli\/install$/);
  await expect(page.getByRole("heading", { name: "통합 설치", exact: true })).toBeVisible();
  await expect(page.getByTestId("cli-install-server")).toHaveText(serverUrl);
  expect(new URL(page.url()).origin).not.toBe(serverUrl);
  const contents = page.getByRole("navigation", { name: "통합 설치 문서 목차" });
  await expect(contents.getByRole("link").first()).toHaveText("코딩 에이전트 연결");
  await expect(contents.locator('a[aria-current="page"]')).toHaveText("코딩 에이전트 연결");
  await expect(page.getByRole("article")).toHaveAccessibleName("코딩 에이전트 연결");
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveCount(0);
  const agentUrl = serverUrl + "/api/v1/cli/agent-setup";
  await expect(page.getByRole("link", { name: "에이전트용 원본 안내문 열기" })).toHaveAttribute("href", agentUrl);
  for (const agent of ["Codex", "Claude Code"]) {
    await page.getByRole("button", { name: agent, exact: true }).click();
    const label = agent + "에 보낼 요청문";
    const prompt = await page.getByLabel(label, { exact: true }).innerText();
    expect(prompt).toContain(agentUrl);
    expect(prompt).toContain("get_contract");
    await page.getByRole("button", { name: label + " 복사" }).click();
    await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(prompt);
  }
  await contents.getByRole("link", { name: "CLI 설치와 로그인" }).click();
  await expect(page).toHaveURL(/#cli-setup$/);
  await expect(page.getByLabel("Linux·macOS 설치 명령", { exact: true })).toHaveText(guide.commands.unix);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveText(guide.commands.windows);
  await expect(page.getByLabel("CLI 로그인 명령", { exact: true })).toHaveText("miy auth login");
  await expect(page.getByText("CLI에는 기본 IP나 도메인이 없습니다.", { exact: false })).toBeVisible();
  await expect(page.getByRole("heading", { name: "1. 명령 한 줄로 설치", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Windows 설치 명령 복사", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(guide.commands.windows);
  await page.getByRole("button", { name: "CLI 로그인 명령 복사", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(guide.commands.login);
  await expect(page.getByRole("link", { name: "회원가입 신청" })).toHaveAttribute("href", serverUrl + "/signup");
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
  await page.getByRole("link", { name: "통합 설치", exact: true }).click();
  await expect(page.getByTestId("cli-install-server")).toHaveText(serverUrl);
  await expect(page.locator("header")).toContainText("설치 사용자");
  await page.getByRole("navigation", { name: "통합 설치 문서 목차" }).getByRole("link", { name: "CLI 명령 문서" }).click();
  await expect(page.getByRole("article")).toContainText("docs/CLI-REFERENCE.md");
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
  await page.goto("/cli/install#cli-setup");
  await page.getByText("설치 파일 직접 다운로드", { exact: true }).click();
  await expect(page.getByRole("link", { name: file, exact: true })).toHaveAttribute("href", url);
  await expect(page.getByRole("link", { name: "SHA-256", exact: true })).toHaveAttribute("href", `${url}.sha256`);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveText(command);
  await expect(page.getByText(commit, { exact: true })).toBeVisible();
  await expect(page.getByText("설치 파일을 준비 중입니다.", { exact: false })).toHaveCount(0);
});

test("one-line commands stay primary when a Windows GUI installer is also published", async ({ page }, testInfo) => {
  await setup(page);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text: string) => {
        (window as typeof window & { copiedCommand?: string }).copiedCommand = text;
      } },
    });
  });
  const installerCommit = "d".repeat(40);
  const file = "MiySetup-0.6.0-windows-x64.exe";
  const url = `/cli/installers/${installerCommit}/${file}`;
  await page.route("**/api/v1/cli/install", (route) => route.fulfill({ json: {
    ...guide,
    windowsInstaller: { file, url, checksumUrl: `${url}.sha256`, sha256: "e".repeat(64) },
  } }));
  await page.goto("/cli/install#cli-setup");
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toBeVisible();
  await page.getByText("설치 파일 직접 다운로드", { exact: true }).click();
  const download = page.getByRole("link", { name: "Windows 설치 파일 다운로드", exact: true });
  await expect(download).toHaveAttribute("href", url);
  await expect(download).toHaveAttribute("download", file);
  await expect(page.getByRole("link", { name: "설치 파일 SHA-256", exact: true })).toHaveAttribute("href", `${url}.sha256`);
  await expect(page.getByText("처음 설치하는 PC에서는 서버 주소 입력란이 비어 있습니다.", { exact: false })).toBeVisible();
  await expect(page.getByLabel("설치에 입력할 서버 주소", { exact: true })).toHaveText(serverUrl);
  await page.getByRole("button", { name: "설치에 입력할 서버 주소 복사", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as typeof window & { copiedCommand?: string }).copiedCommand)).toBe(serverUrl);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("windows-installer-desktop.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath("windows-installer-mobile.png"), fullPage: true });
  await page.getByText("설치 파일 직접 다운로드", { exact: true }).click();
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveText(guide.commands.windows);
});

test("Windows setup guidance supports company HTTP IP servers without substituting a domain", async ({ page }) => {
  await setup(page);
  const companyUrl = "http://192.168.20.10:8080";
  await page.route("**/api/v1/cli/install", (route) => route.fulfill({ json: {
    ...guide, serverUrl: companyUrl,
    windowsInstaller: { file: "MiySetup-0.6.0-windows-x64.exe", url: "/cli/installers/fixture/setup.exe", checksumUrl: "/cli/installers/fixture/setup.exe.sha256", sha256: "e".repeat(64) },
  } }));
  await page.goto("/cli/install#cli-setup");
  await expect(page.getByTestId("cli-install-server")).toHaveText(companyUrl);
  await page.getByText("설치 파일 직접 다운로드", { exact: true }).click();
  await expect(page.getByLabel("설치에 입력할 서버 주소", { exact: true })).toHaveText(companyUrl);
  await expect(page.getByText("이 서버는 사내망 HTTP 구성으로 통신이 암호화되지 않습니다.")).toBeVisible();
});


test("missing packages do not advertise commands that cannot install", async ({ page }) => {
  await setup(page);
  await page.route("**/api/v1/cli/install", (route) => route.fulfill({ json: { ...guide, downloads: [] } }));
  await page.goto("/cli/install#cli-setup");
  await expect(page.getByText("이 서버에 Linux·macOS CLI 패키지가 아직 게시되지 않았습니다.", { exact: false })).toBeVisible();
  await expect(page.getByText("이 서버에 Windows CLI 패키지가 아직 게시되지 않았습니다.", { exact: false })).toBeVisible();
  await expect(page.getByLabel("Linux·macOS 설치 명령", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Windows 설치 명령", { exact: true })).toHaveCount(0);
});

test("guide chapters support shared URLs, browser history and mobile navigation", async ({ page }) => {
  await setup(page);
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto("/cli/install#cli-usage");
  const contents = page.getByRole("navigation", { name: "통합 설치 문서 목차" });
  const chapter = page.getByRole("article");
  await expect(chapter).toContainText("miy app init ./my-app");
  for (const [name, expected] of [
    ["CLI 명령 문서", "docs/CLI-REFERENCE.md"],
    ["MCP 설치와 연결", serverUrl + "/mcp"],
    ["업데이트와 연결 관리", "miy auth sessions list"],
  ]) {
    await contents.getByRole("link", { name, exact: true }).click();
    await expect(chapter).toHaveAccessibleName(name);
    await expect(contents.locator('a[aria-current="page"]')).toHaveText(name);
    await expect(chapter).toContainText(expected);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  }
  await expect(chapter.getByRole("link", { name: "콘솔에서 내 CLI 연결 열기" })).toHaveAttribute("href", serverUrl + "/cli/sessions");
  await page.goBack();
  await expect(chapter).toContainText("claude mcp get miy_app");
  await page.reload();
  await expect(chapter).toContainText("codex mcp get miy_app");
  await page.goForward();
  await expect(chapter).toHaveAccessibleName("업데이트와 연결 관리");
});

test("English setup guide uses the same project and remote MCP workflow", async ({ page }) => {
  await setup(page);
  await page.addInitScript(() => localStorage.setItem("miy:display-settings", JSON.stringify({ locale: "en-US" })));
  await page.goto("/cli/install#agent-setup");
  const contents = page.getByRole("navigation", { name: "Setup guide contents" });
  await expect(contents.getByRole("link")).toHaveCount(6);
  await expect(page.getByRole("article")).toContainText("platformBundleVersion");
  await contents.getByRole("link", { name: "MCP setup" }).click();
  await expect(page.getByRole("article")).toContainText("get_cli_help");
  await expect(page.getByRole("article")).toContainText(serverUrl + "/mcp");
});
