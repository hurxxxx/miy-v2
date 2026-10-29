import { randomUUID } from "node:crypto";
import { test, expect, request } from "@playwright/test";

// Device credentials must not be persisted in failure trace archives.
test.use({ trace: "off" });

const origin = new URL(
  process.env.MIY_DEVELOPMENT_LOGIN_TEST_ORIGIN || "http://localhost:18000",
).origin;
const accounts = [
  { name: "admin", label: /관리자로 시작/, admin: true },
  { name: "developer", label: /개발자/, admin: false },
  { name: "alice", label: /사용자 A/, admin: false },
  { name: "bob", label: /사용자 B/, admin: false },
];

test("all development account buttons sign in and out on the current origin without an identity redirect", async ({
  browser,
}) => {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const identityRequests: string[] = [];
    page.on("request", (entry) => {
      if (new URL(entry.url()).pathname.startsWith("/identity/"))
        identityRequests.push(entry.url());
    });
    await page.goto(origin);
    for (const account of accounts) {
      for (const option of accounts)
        await expect(
          page.getByRole("button", { name: option.label }),
        ).toBeVisible();
      await page.getByRole("button", { name: account.label }).click();
      await expect(page.locator("header")).toContainText(account.name);
      const response = await context.request.get(origin + "/api/v1/me");
      expect(response.status()).toBe(200);
      const me = await response.json();
      expect(me.name).toBe(account.name);
      expect(me.admin).toBe(account.admin);
      expect(me.development).toBe(true);
      const cookie = (await context.cookies(origin)).find(
        (entry) => entry.name === "miy_development",
      );
      expect(cookie?.httpOnly).toBe(true);
      expect(cookie?.secure).toBe(new URL(origin).protocol === "https:");
      expect(cookie?.sameSite).toBe("Lax");
      await page.getByRole("button", { name: "로그아웃", exact: true }).click();
      await expect(page).toHaveURL(origin + "/");
      await expect(
        page.getByRole("button", { name: /관리자로 시작/ }),
      ).toBeVisible();
      expect((await context.request.get(origin + "/api/v1/me")).status()).toBe(
        401,
      );
      expect(
        (await context.cookies(origin)).some(
          (entry) => entry.name === "miy_development",
        ),
      ).toBe(false);
    }
    expect(identityRequests).toEqual([]);
  } finally {
    await context.close();
  }
});

test("development login rejects foreign origins and unknown accounts", async ({
  browser,
}) => {
  const context = await browser.newContext();
  try {
    const foreign = await context.request.post(origin + "/auth/development", {
      headers: { origin: "https://untrusted.example" },
      data: { account: "admin" },
    });
    expect(foreign.status()).toBe(403);
    const unknown = await context.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account: "root" },
    });
    expect(unknown.status()).toBe(400);
    expect((await context.request.get(origin + "/api/v1/me")).status()).toBe(
      401,
    );
    expect(
      (await context.cookies(origin)).some(
        (entry) => entry.name === "miy_development",
      ),
    ).toBe(false);
  } finally {
    await context.close();
  }
});

test("development account selection preserves ordinary CLI browser approval", async ({
  browser,
}) => {
  // An independent public client exercises the CLI protocol without reading or
  // changing any installed CLI configuration. Tokens stay in memory only.
  const client = await request.newContext({ baseURL: origin });
  const context = await browser.newContext();
  let refreshToken: string | undefined;
  try {
    const started = await client.post("/auth/cli/device", {
      data: {
        clientId: randomUUID(),
        clientName: "Development login browser regression",
      },
    });
    expect(started.status()).toBe(200);
    const device = await started.json();
    expect(device.id).toMatch(/^[a-f0-9-]{36}$/);
    expect(device.verificationPath).toBe(`/cli/device/${device.id}`);
    const verificationUrl = origin + device.verificationPath;
    const page = await context.newPage();
    const identityRequests: string[] = [];
    page.on("request", (entry) => {
      if (new URL(entry.url()).pathname.startsWith("/identity/"))
        identityRequests.push(entry.url());
    });
    await page.goto(verificationUrl);
    await page.getByRole("button", { name: /사용자 A/ }).click();
    await expect(page).toHaveURL(verificationUrl);
    await expect(
      page.getByRole("heading", { name: "CLI 로그인 승인" }),
    ).toBeVisible();
    await expect(page.getByText(origin, { exact: true })).toBeVisible();
    const meResponse = await context.request.get(origin + "/api/v1/me");
    expect(meResponse.status()).toBe(200);
    const me = await meResponse.json();
    expect(me.name).toBe("alice");
    expect(me.admin).toBe(false);
    expect(me.development).toBe(true);
    const approve = page.getByRole("button", {
      name: "이 계정으로 CLI 연결 승인",
    });
    await expect(approve).toBeDisabled();
    await page.getByLabel("인증 코드").fill(device.userCode);
    await approve.click();
    await expect(page.getByRole("status")).toContainText(
      "CLI 연결을 승인했습니다",
    );
    await expect(approve).toHaveCount(0);
    const exchange = await client.post("/auth/cli/token", {
      data: {
        grantType: "device_code",
        id: device.id,
        deviceCode: device.deviceCode,
      },
    });
    expect(exchange.status()).toBe(200);
    const tokens = await exchange.json();
    // Capture before assertions so even a failed assertion revokes this session.
    if (typeof tokens.refreshToken === "string")
      refreshToken = tokens.refreshToken;
    expect(tokens.status).toBe("approved");
    expect(tokens.userId).toBe(me.id);
    expect(typeof tokens.accessToken).toBe("string");
    expect(typeof tokens.refreshToken).toBe("string");
    const cliMe = await client.get("/api/v1/me", {
      headers: { authorization: `Bearer ${tokens.accessToken}` },
    });
    expect(cliMe.status()).toBe(200);
    expect((await cliMe.json()).id).toBe(me.id);
    expect(identityRequests).toEqual([]);
  } finally {
    try {
      if (refreshToken) {
        const revoked = await client.post("/auth/cli/revoke", {
          data: { refreshToken },
        });
        expect(revoked.status()).toBe(200);
      }
    } finally {
      await context.close();
      await client.dispose();
    }
  }
});
