import { expect, test, type Page, type Route } from "@playwright/test";
import type { FileMetadata, FileTransferState } from "../src";

type Operation =
  | "create"
  | "get"
  | "byRequest"
  | "upload"
  | "attach"
  | "cancel"
  | "download";
type Record = { metadata: FileMetadata; requestId: string; bytes: Buffer };
async function backend(
  page: Page,
  hook?: (
    operation: Operation,
    route: Route,
    record?: Record,
  ) => Promise<boolean> | boolean,
) {
  const records = new Map<string, Record>();
  const calls: { operation: Operation; id?: string; input?: unknown }[] = [];
  await page.route(/\/fixture-files(?:\/.*)?$/, async (route) => {
    const parts = new URL(route.request().url()).pathname.split("/").slice(2);
    const id = parts[0];
    const operation: Operation = !id
      ? "create"
      : id === "by-request"
        ? "byRequest"
        : parts[1] === "bytes"
          ? "upload"
          : parts[1] === "attach"
            ? "attach"
            : parts[1] === "cancel"
              ? "cancel"
              : parts[1] === "download"
                ? "download"
                : "get";
    let record =
      operation === "byRequest"
        ? [...records.values()].find((item) => item.requestId === parts[1])
        : records.get(id);
    const input = ["create", "attach"].includes(operation)
      ? route.request().postDataJSON()
      : undefined;
    calls.push({ operation, id, input });
    if (operation === "create") {
      record = {
        metadata: {
          id: `file-${records.size + 1}`,
          version: 1,
          name: input.name,
          mime: input.mime,
          size: input.size,
          sha256: null,
          status: "pending",
        },
        requestId: input.requestId,
        bytes: Buffer.alloc(0),
      };
      records.set(record.metadata.id, record);
    }
    if (await hook?.(operation, route, record)) return;
    if (!record)
      return route.fulfill({ body: "null", contentType: "application/json" });
    if (operation === "upload") {
      record.bytes = route.request().postDataBuffer()!;
      record.metadata = { ...record.metadata, version: 2, status: "ready" };
    }
    if (operation === "attach")
      record.metadata = { ...record.metadata, version: 3, status: "attached" };
    if (operation === "cancel")
      record.metadata = { ...record.metadata, version: 4, status: "cancelled" };
    if (operation === "download")
      return route.fulfill({
        body: record.bytes,
        contentType: record.metadata.mime,
      });
    return route.fulfill({ json: record.metadata });
  });
  return { records, calls };
}
const file = (
  name = "설계.txt",
  buffer = Buffer.from("design"),
  mimeType = "text/plain",
) => ({ name, buffer, mimeType });
const choose = (page: Page, values: ReturnType<typeof file>[]) =>
  page.getByLabel("파일 선택", { exact: true }).setInputFiles(values);
const row = (page: Page, name = "설계.txt") =>
  page.getByRole("listitem", { name, exact: true });
function transfer(
  metadata: FileMetadata,
  status: FileTransferState["status"] = "complete",
): FileTransferState {
  return {
    clientId: metadata.id,
    requestId: `create-${metadata.id}`,
    attachRequestId: `attach-${metadata.id}`,
    name: metadata.name,
    mime: metadata.mime,
    size: metadata.size,
    metadata,
    status,
    progress: 100,
  };
}

test("uploads are sequential and attachment completion is distinct from transferred bytes", async ({
  page,
}) => {
  let heldAttach: Route | undefined;
  const api = await backend(page, (operation, route, record) => {
    if (operation === "attach" && record?.metadata.id === "file-1") {
      heldAttach = route;
      return true;
    }
    return false;
  });
  await page.goto("/?view=files");
  await choose(page, [file(), file("두번째.txt")]);
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(true);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("업무 항목에 연결 중");
  await expect(row(page).getByRole("progressbar")).toHaveAttribute(
    "value",
    "100",
  );
  await expect(
    row(page).getByRole("button", { name: "다운로드" }),
  ).toBeDisabled();
  expect(api.calls.map((value) => value.operation)).toEqual([
    "create",
    "upload",
    "attach",
  ]);
  const first = api.records.get("file-1")!;
  first.metadata = { ...first.metadata, status: "attached", version: 3 };
  await heldAttach!.fulfill({ json: first.metadata });
  await expect(row(page)).toContainText("첨부 완료");
  await expect(row(page, "두번째.txt")).toContainText("첨부 완료");
  expect(api.calls.map((value) => value.operation)).toEqual([
    "create",
    "upload",
    "attach",
    "create",
    "upload",
    "attach",
  ]);
  expect(api.calls[2].input).toMatchObject({
    reference: "document:42",
    requestId: expect.stringMatching(/^[0-9a-f-]{36}$/),
  });
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(false);
  const state = await page.evaluate(() => window.uiFixture.transfers);
  expect(JSON.stringify(state)).not.toContain("blob:");
  expect(Object.keys(state![0])).not.toContain("file");
});

test("lost create response is looked up before an explicit continuation without duplicate create", async ({
  page,
}) => {
  const api = await backend(page, async (operation, route) => {
    if (operation !== "create") return false;
    await route.abort("connectionreset");
    return true;
  });
  await page.goto("/?view=files");
  await choose(page, [file()]);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("결과 확인 필요");
  await expect(
    row(page).getByRole("button", { name: "명시적으로 다시 진행" }),
  ).toHaveCount(0);
  expect(api.calls.map((value) => value.operation)).toEqual(["create"]);
  await row(page).getByRole("button", { name: "결과 조회" }).click();
  await expect(row(page)).toContainText("전송 대기");
  await row(page).getByRole("button", { name: "명시적으로 다시 진행" }).click();
  await expect(row(page)).toContainText("첨부 완료");
  expect(api.calls.map((value) => value.operation)).toEqual([
    "create",
    "byRequest",
    "get",
    "upload",
    "attach",
  ]);
});

test("lost attach response is resolved without compensating cancellation or retransmission", async ({
  page,
}) => {
  const api = await backend(page, async (operation, route, record) => {
    if (operation !== "attach") return false;
    record!.metadata = { ...record!.metadata, status: "attached", version: 3 };
    await route.abort("connectionreset");
    return true;
  });
  await page.goto("/?view=files");
  await choose(page, [file()]);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("결과 확인 필요");
  await row(page).getByRole("button", { name: "결과 조회" }).click();
  await expect(row(page)).toContainText("첨부 완료");
  expect(api.calls.map((value) => value.operation)).toEqual([
    "create",
    "upload",
    "attach",
    "get",
  ]);
});

test("attachment denial preserves uploaded bytes and permits explicit result lookup", async ({
  page,
}) => {
  let deny = true;
  const api = await backend(page, async (operation, route) => {
    if (operation !== "attach" || !deny) return false;
    await route.fulfill({
      status: 403,
      json: {
        error: {
          code: "forbidden",
          detail: "never expose this internal detail",
        },
      },
    });
    return true;
  });
  await page.goto("/?view=files");
  await choose(page, [file()]);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("현재 파일 작업 권한이 없습니다");
  await expect(row(page)).not.toContainText("never expose");
  await row(page).getByRole("button", { name: "결과 조회" }).click();
  await expect(row(page)).toContainText("전송 완료 · 연결 대기");
  deny = false;
  await row(page)
    .getByRole("button", { name: "업무 항목에 연결", exact: true })
    .click();
  await expect(row(page)).toContainText("첨부 완료");
  expect(
    api.calls.filter((value) => value.operation === "upload"),
  ).toHaveLength(1);
  expect(
    api.calls.filter((value) => value.operation === "cancel"),
  ).toHaveLength(0);
});

test("cancellation ignores a later upload response from an adapter that ignores abort", async ({
  page,
}) => {
  let upload: Route | undefined;
  const api = await backend(page, (operation, route) => {
    if (operation !== "upload") return false;
    upload = route;
    return true;
  });
  await page.goto("/?view=files&ignore-upload-abort");
  await choose(page, [file()]);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("전송 중");
  await row(page).getByRole("button", { name: "취소", exact: true }).click();
  await expect(row(page)).toContainText("취소됨");
  await upload!.fulfill({
    json: { ...api.records.get("file-1")!.metadata, status: "ready" },
  });
  await expect(
    page.getByRole("button", { name: "업로드 시작" }),
  ).not.toHaveAttribute("aria-busy", "true");
  await expect(row(page)).toContainText("취소됨");
  expect(
    api.calls.filter((value) => value.operation === "attach"),
  ).toHaveLength(0);
});

test("restored inflight transfer requires lookup and original file selection before upload", async ({
  page,
}) => {
  const metadata: FileMetadata = {
    id: "restored",
    version: 1,
    name: "설계.txt",
    mime: "text/plain",
    size: 6,
    sha256: null,
    status: "pending",
  };
  const api = await backend(page);
  api.records.set(metadata.id, {
    metadata,
    requestId: "create-restored",
    bytes: Buffer.alloc(0),
  });
  await page.addInitScript(
    (value) => {
      window.uiInitialTransfers = value;
    },
    [transfer(metadata, "uploading")],
  );
  await page.goto("/?view=files");
  await expect(row(page)).toContainText("이전 요청의 결과를 먼저 확인");
  expect(api.calls).toHaveLength(0);
  await row(page).getByRole("button", { name: "결과 조회" }).click();
  await expect(
    row(page).getByRole("button", { name: "명시적으로 다시 진행" }),
  ).toBeDisabled();
  await page.getByLabel("설계.txt 다시 선택").setInputFiles(file("다른.txt"));
  await expect(row(page)).toContainText("원래 파일을 선택");
  await page.getByLabel("설계.txt 다시 선택").setInputFiles(file());
  await row(page).getByRole("button", { name: "명시적으로 다시 진행" }).click();
  await expect(row(page)).toContainText("첨부 완료");
  expect(api.calls.map((value) => value.operation)).toEqual([
    "get",
    "get",
    "upload",
    "attach",
  ]);
});

test("oversized files are rejected before requests and downloads revoke their temporary URL", async ({
  page,
}) => {
  const api = await backend(page);
  await page.addInitScript(() => {
    const original = URL.revokeObjectURL;
    (window as unknown as { revoked: string[] }).revoked = [];
    URL.revokeObjectURL = (value) => {
      (window as unknown as { revoked: string[] }).revoked.push(value);
      original(value);
    };
  });
  await page.goto("/?view=files");
  await choose(page, [file("큰파일.txt", Buffer.alloc(10 * 1024 * 1024 + 1))]);
  await expect(page.getByRole("status")).toContainText("10 MB 이하");
  expect(api.calls).toHaveLength(0);
  await choose(page, [file()]);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(row(page)).toContainText("첨부 완료");
  const downloaded = page.waitForEvent("download");
  await row(page).getByRole("button", { name: "다운로드" }).click();
  expect((await downloaded).suggestedFilename()).toBe("설계.txt");
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { revoked: string[] }).revoked.length,
      ),
    )
    .toBe(1);
});

test("raster preview uses a local blob URL, restores focus, and rejects disguised active content", async ({
  page,
}) => {
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=",
    "base64",
  );
  const metadata: FileMetadata = {
    id: "image",
    version: 3,
    name: "사진.png",
    mime: "image/png",
    size: png.length,
    sha256: null,
    status: "attached",
  };
  const api = await backend(page);
  api.records.set(metadata.id, {
    metadata,
    requestId: "image-request",
    bytes: png,
  });
  await page.addInitScript(
    (value) => {
      window.uiInitialTransfers = value;
    },
    [transfer(metadata)],
  );
  await page.addInitScript(() => {
    const original = URL.revokeObjectURL;
    (window as unknown as { revoked: string[] }).revoked = [];
    URL.revokeObjectURL = (value) => {
      (window as unknown as { revoked: string[] }).revoked.push(value);
      original(value);
    };
  });
  await page.goto("/?view=files");
  const preview = page.getByRole("button", { name: "이미지 보기" });
  await preview.click();
  await expect(page.getByRole("dialog").getByRole("img")).toHaveAttribute(
    "src",
    /^blob:/,
  );
  await page.getByRole("button", { name: "이미지 확대" }).click();
  await expect(page.getByRole("dialog").getByRole("img")).toHaveAttribute(
    "style",
    /125%/,
  );
  await page.keyboard.press("Escape");
  await expect(preview).toBeFocused();
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { revoked: string[] }).revoked.length,
      ),
    )
    .toBe(1);
  api.records.get(metadata.id)!.bytes = Buffer.from(
    '<svg onload="window.previewExecuted=true"></svg>'.padEnd(png.length, " "),
  );
  await preview.click();
  await expect(page.getByRole("dialog")).toContainText(
    "이미지 미리보기를 제공할 수 없습니다",
  );
  await expect(page.getByRole("dialog").getByRole("img")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { previewExecuted?: boolean }).previewExecuted,
    ),
  ).toBeUndefined();
});
