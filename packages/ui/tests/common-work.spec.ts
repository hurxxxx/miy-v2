import { expect, test, type Page } from "@playwright/test";

// This suite exercises the real example AppRoot and SDK transport against synthetic HTTP responses.
// It does not stand in for the independent Core/app PostgreSQL or frame-origin acceptance tests.
const projectId = "22222222-2222-4222-8222-222222222222";
const ownerId = "33333333-3333-4333-8333-333333333333";
const colleagueId = "44444444-4444-4444-8444-444444444444";
const groupId = "55555555-5555-4555-8555-555555555555";
type Project = { id: string; title: string };
type Task = { id: string; title: string; completed: boolean };
type Grant = { subject: { kind: "user" | "group"; id: string }; role: string };
type Sharing = {
  revision: number;
  canManage: boolean;
  entries: Array<{
    subject: Grant["subject"] & { name: string };
    role: string;
    protected: boolean;
  }>;
  roles: Array<{ id: string; label: string }>;
};
type Metadata = {
  id: string;
  version: number;
  name: string;
  mime: string;
  size: number;
  status: string;
  sha256: string | null;
};
type FileRecord = {
  project: string;
  requestId: string;
  metadata: Metadata;
  bytes: Buffer;
};
type Call = { gateway: string; method: string; path: string; body?: unknown };
function sharing(): Sharing {
  return {
    revision: 1,
    canManage: true,
    entries: [
      {
        subject: { kind: "user", id: ownerId, name: "프로젝트 소유자" },
        role: "owner",
        protected: true,
      },
    ],
    roles: [
      { id: "viewer", label: "조회자" },
      { id: "editor", label: "편집자" },
      { id: "owner", label: "소유자" },
    ],
  };
}
async function backend(page: Page, seeded = true) {
  const state = {
    projects: new Map<string, Project>(
      seeded ? [[projectId, { id: projectId, title: "공동 설계" }]] : [],
    ),
    tasks: new Map<string, Map<string, Task>>(),
    sharing: new Map<string, Sharing>(),
    shareResults: new Map<string, Sharing>(),
    files: new Map<string, FileRecord>(),
    calls: [] as Call[],
    deniedSharing: false,
    deniedTasks: false,
    loseProject: false,
    loseTask: false,
    // The request is lost before the server stores anything.
    dropProject: false,
    dropTask: false,
    loseSharing: false,
    loseAttach: false,
    subjectId: ownerId,
  };
  await page.route(/\/work-directory\?/, (route) => {
    const url = new URL(route.request().url()),
      kind = url.searchParams.get("kind") ?? "users";
    state.calls.push({
      gateway: "directory",
      method: "GET",
      path: url.pathname + url.search,
    });
    return route.fulfill({
      json: {
        kind,
        installationId: "11111111-1111-4111-8111-111111111111",
        environment: "dev",
        subjectId: state.subjectId,
        users:
          kind === "users"
            ? [
                { id: ownerId, name: "프로젝트 소유자", primaryGroup: null },
                {
                  id: colleagueId,
                  name: "설계 담당자",
                  primaryGroup: { id: groupId, name: "설계팀" },
                },
              ]
            : [],
        groups:
          kind === "groups" || kind === "memberships"
            ? [{ id: groupId, name: "설계팀", source: "hr" }]
            : [],
        nextCursor: null,
      },
    });
  });
  await page.route(/\/work-(api|files)\//, async (route) => {
    const request = route.request(),
      url = new URL(request.url()),
      method = request.method();
    const gateway = url.pathname.startsWith("/work-files") ? "files" : "api";
    const path = url.pathname.replace(/^\/work-(api|files)/, "");
    const body = request
      .headers()
      ["content-type"]?.startsWith("application/json")
      ? request.postDataJSON()
      : undefined;
    state.calls.push({ gateway, method, path, body });
    const parts = path.split("/").filter(Boolean),
      id = parts[1];
    const respond = (json: unknown, status = 200) =>
      route.fulfill({ status, json });
    const denied = () => respond({ error: { code: "acl_denied" } }, 403);
    if (parts[0] !== "projects")
      return respond({ error: { code: "not_found" } }, 404);
    if (gateway === "files") {
      const file = state.files.get(parts[2]);
      if (!file || file.project !== id || parts[3] !== "content")
        return respond({ error: { code: "not_found" } }, 404);
      if (method === "PUT") {
        file.bytes = request.postDataBuffer()!;
        file.metadata = {
          ...file.metadata,
          version: 2,
          status: "ready",
          sha256: "0".repeat(64),
        };
        return respond(file.metadata);
      }
      return route.fulfill({
        body: file.bytes,
        contentType: file.metadata.mime,
      });
    }
    if (parts.length === 1) {
      if (method === "GET")
        return respond({
          projects: [...state.projects.values()],
          nextCursor: null,
        });
      if (state.dropProject) {
        state.dropProject = false;
        return route.abort("connectionreset");
      }
      state.projects.set(body.id, body);
      if (state.loseProject) {
        state.loseProject = false;
        return route.abort("connectionreset");
      }
      return respond(body);
    }
    const project = state.projects.get(id);
    // Like the app's ACL, an unknown project is indistinguishable from a denied one.
    if (!project)
      return parts.length === 2
        ? denied()
        : respond({ error: { code: "not_found" } }, 404);
    if (parts.length === 2) return respond(project);
    if (parts[2] === "tasks") {
      if (method !== "GET" && state.deniedTasks) return denied();
      const tasks = state.tasks.get(id) ?? new Map<string, Task>();
      state.tasks.set(id, tasks);
      if (method === "GET") {
        if (parts[3])
          return tasks.has(parts[3])
            ? respond(tasks.get(parts[3]))
            : respond({ error: { code: "not_found" } }, 404);
        return respond([...tasks.values()]);
      }
      if (method === "POST" && state.dropTask) {
        state.dropTask = false;
        return route.abort("connectionreset");
      }
      const value =
        method === "POST"
          ? { id: body.id, title: body.title, completed: false }
          : { ...tasks.get(parts[3])!, completed: body.completed };
      tasks.set(value.id, value);
      if (state.loseTask) {
        state.loseTask = false;
        return route.abort("connectionreset");
      }
      return respond(value);
    }
    if (parts[2] === "sharing") {
      if (state.deniedSharing) return denied();
      if (parts[3] === "results")
        return respond(state.shareResults.get(parts[4]) ?? null);
      const snapshot = state.sharing.get(id) ?? sharing();
      state.sharing.set(id, snapshot);
      if (method === "GET") return respond(snapshot);
      const updated: Sharing = {
        ...snapshot,
        revision: snapshot.revision + 1,
        entries: body.entries.map((entry: Grant) => ({
          ...entry,
          subject: {
            ...entry.subject,
            name:
              entry.subject.id === ownerId
                ? "프로젝트 소유자"
                : entry.subject.kind === "group"
                  ? "설계팀"
                  : "설계 담당자",
          },
          protected: entry.subject.id === ownerId,
        })),
      };
      state.sharing.set(id, updated);
      state.shareResults.set(body.requestId, updated);
      if (state.loseSharing) {
        state.loseSharing = false;
        return route.abort("connectionreset");
      }
      return respond(updated);
    }
    if (parts[2] === "files") {
      if (parts.length === 3) {
        if (method === "GET")
          return respond(
            [...state.files.values()]
              .filter(
                (value) =>
                  value.project === id && value.metadata.status === "attached",
              )
              .map((value) => ({
                id: value.metadata.id,
                name: value.metadata.name,
                mime: value.metadata.mime,
                size: value.metadata.size,
              })),
          );
        const fileId = `66666666-6666-4666-8666-${String(state.files.size + 1).padStart(12, "0")}`;
        const file: FileRecord = {
          project: id,
          requestId: body.requestId,
          bytes: Buffer.alloc(0),
          metadata: {
            id: fileId,
            version: 1,
            name: body.name,
            mime: body.mime,
            size: body.size,
            sha256: null,
            status: "pending",
          },
        };
        state.files.set(fileId, file);
        return respond(file.metadata);
      }
      if (parts[3] === "by-request")
        return respond(
          [...state.files.values()].find(
            (value) => value.requestId === parts[4] && value.project === id,
          )?.metadata ?? null,
        );
      const file = state.files.get(parts[3]);
      if (!file || file.project !== id)
        return respond({ error: { code: "not_found" } }, 404);
      if (parts[4] === "attach") {
        expect(body.reference).toBe(id);
        file.metadata = { ...file.metadata, version: 3, status: "attached" };
        if (state.loseAttach) {
          state.loseAttach = false;
          return route.abort("connectionreset");
        }
      }
      if (method === "DELETE")
        file.metadata = { ...file.metadata, version: 4, status: "cancelled" };
      return respond(file.metadata);
    }
    return respond({ error: { code: "not_found" } }, 404);
  });
  return state;
}
const open = (page: Page, id?: string) =>
  page.goto(
    `/common-work.html${id ? `?path=${encodeURIComponent(`/projects/${id}`)}` : ""}`,
  );
const dirty = (page: Page, value: boolean) =>
  expect.poll(() => page.evaluate(() => window.commonWork.dirty)).toBe(value);
async function pickColleague(page: Page) {
  await page.getByRole("combobox", { name: "공유 대상", exact: true }).focus();
  await page.getByRole("option", { name: /설계 담당자/ }).click();
  await page
    .getByRole("combobox", { name: "설계 담당자 역할" })
    .selectOption("editor");
}
const sampleFile = {
  name: "설계 메모.txt",
  mimeType: "text/plain",
  buffer: Buffer.from("example file"),
};

test("real AppRoot connects project/task, directory sharing and binary attachment flows", async ({
  page,
}) => {
  // An app frame on internal HTTP is not a secure context and has no
  // crypto.randomUUID. Every request ID in this flow must still be created.
  await page.addInitScript(() => {
    Object.defineProperty(Crypto.prototype, "randomUUID", { value: undefined });
  });
  const api = await backend(page, false);
  await open(page);
  await page
    .getByRole("textbox", { name: "새 프로젝트 이름" })
    .fill("공동 설계");
  await dirty(page, true);
  await page.getByRole("button", { name: "프로젝트 만들기" }).click();
  await dirty(page, false);
  const project = [...api.projects.values()][0];
  expect(project.id).toMatch(/^[0-9a-f-]{36}$/);
  await page.getByRole("button", { name: "공동 설계", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => window.commonWork.path))
    .toBe(`/projects/${project.id}`);
  await page
    .getByRole("textbox", { name: "새 업무", exact: true })
    .fill("도면 검토");
  await page.getByRole("button", { name: "업무 추가" }).click();
  // This app waits for server acknowledgement before updating the controlled checkbox.
  await page.getByRole("checkbox", { name: "도면 검토" }).click();
  await expect(page.getByRole("checkbox", { name: "도면 검토" })).toBeChecked();
  await dirty(page, false);
  await pickColleague(page);
  await dirty(page, true);
  await expect(
    page.getByRole("combobox", { name: "프로젝트 소유자 역할" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await dirty(page, false);
  const sharingWrite = api.calls.find(
    (call) => call.path.endsWith("/sharing") && call.method === "PUT",
  )!;
  expect(sharingWrite.body).toEqual({
    expectedRevision: 1,
    requestId: expect.stringMatching(/^[0-9a-f-]{36}$/),
    entries: [
      { subject: { kind: "user", id: ownerId }, role: "owner" },
      { subject: { kind: "user", id: colleagueId }, role: "editor" },
    ],
  });
  await page.getByLabel("파일 선택", { exact: true }).setInputFiles(sampleFile);
  await dirty(page, true);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(page.getByText("첨부 완료", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", {
      name: `${sampleFile.name} 다운로드`,
      exact: true,
    }),
  ).toBeVisible();
  await dirty(page, false);
  const upload = api.calls.find(
    (call) => call.gateway === "files" && call.method === "PUT",
  )!;
  expect(upload.path).toBe(
    `/projects/${project.id}/${[...api.files.keys()][0]}/content`,
  );
  expect([...api.files.values()][0].bytes.equals(sampleFile.buffer)).toBe(true);
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: `${sampleFile.name} 다운로드`, exact: true })
    .click();
  expect((await download).suggestedFilename()).toBe(sampleFile.name);
  await page.screenshot({
    path: ".local/evidence/common-ui/common-work-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: ".local/evidence/common-ui/common-work-mobile.png",
    fullPage: true,
  });
});

test("403 sharing and task writes show normal denial while preserving editable input", async ({
  page,
}) => {
  const api = await backend(page);
  api.deniedSharing = true;
  api.deniedTasks = true;
  await open(page, projectId);
  await expect(
    page.getByText("공유 설정은 프로젝트 소유자만 관리할 수 있습니다.", {
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: "새 업무", exact: true })
    .fill("권한 확인 업무");
  await page.getByRole("button", { name: "업무 추가" }).click();
  await expect(page.getByRole("status")).toContainText(
    "현재 이 작업을 수행할 권한이 없습니다",
  );
  await expect(
    page.getByRole("textbox", { name: "새 업무", exact: true }),
  ).toHaveValue("권한 확인 업무");
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/tasks") && call.method === "POST",
    ),
  ).toHaveLength(1);
  expect(api.tasks.get(projectId)?.size ?? 0).toBe(0);
  await dirty(page, true);
  await page.getByRole("textbox", { name: "새 업무", exact: true }).fill("");
  await dirty(page, false);
});

test("lost project and task responses survive reload and are looked up without repeated writes", async ({
  page,
}) => {
  const api = await backend(page, false);
  api.loseProject = true;
  await open(page);
  await page
    .getByRole("textbox", { name: "새 프로젝트 이름" })
    .fill("복구할 프로젝트");
  await page.getByRole("button", { name: "프로젝트 만들기" }).click();
  await expect(page.getByRole("status")).toContainText(
    "생성 결과를 확인할 수 없습니다",
  );
  await page.reload();
  await page.getByRole("button", { name: "프로젝트 생성 결과 조회" }).click();
  await dirty(page, false);
  expect(
    api.calls.filter(
      (call) => call.path === "/projects" && call.method === "POST",
    ),
  ).toHaveLength(1);
  await page
    .getByRole("button", { name: "복구할 프로젝트", exact: true })
    .click();
  api.loseTask = true;
  await page
    .getByRole("textbox", { name: "새 업무", exact: true })
    .fill("응답이 유실된 업무");
  await page.getByRole("button", { name: "업무 추가" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "업무 변경 결과" }),
  ).toContainText("확인할 수 없습니다");
  await page.reload();
  await dirty(page, true);
  await page.getByRole("button", { name: "업무 변경 결과 조회" }).click();
  await expect(
    page.getByRole("checkbox", { name: "응답이 유실된 업무" }),
  ).toBeVisible();
  await dirty(page, false);
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/tasks") && call.method === "POST",
    ),
  ).toHaveLength(1);
  const task = [...api.tasks.values()][0].values().next().value!;
  expect(
    api.calls.some(
      (call) =>
        call.path.endsWith(`/tasks/${task.id}`) && call.method === "GET",
    ),
  ).toBe(true);
});

test("a creation request that never reached the server can be sent again or cancelled", async ({
  page,
}) => {
  const api = await backend(page, false);
  api.dropProject = true;
  await open(page);
  const name = page.getByRole("textbox", { name: "새 프로젝트 이름" });
  await name.fill("도달하지 못한 프로젝트");
  await page.getByRole("button", { name: "프로젝트 만들기" }).click();
  await expect(page.getByRole("status")).toContainText(
    "생성 결과를 확인할 수 없습니다",
  );
  await page.getByRole("button", { name: "프로젝트 생성 결과 조회" }).click();
  await expect(page.getByRole("status")).toContainText(
    "서버에 만들어진 프로젝트가 없습니다",
  );
  await page.getByRole("button", { name: "같은 요청 다시 보내기" }).click();
  await dirty(page, false);
  const creations = api.calls.filter(
    (call) => call.path === "/projects" && call.method === "POST",
  );
  // The same ID: a late first attempt could only conflict, never duplicate.
  expect(creations).toHaveLength(2);
  expect((creations[1].body as Project).id).toBe(
    (creations[0].body as Project).id,
  );
  await page
    .getByRole("button", { name: "도달하지 못한 프로젝트", exact: true })
    .click();
  api.dropTask = true;
  const task = page.getByRole("textbox", { name: "새 업무", exact: true });
  await task.fill("도달하지 못한 업무");
  await page.getByRole("button", { name: "업무 추가" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "업무 변경 결과" }),
  ).toContainText("확인할 수 없습니다");
  await page.getByRole("button", { name: "업무 변경 결과 조회" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "추가된 업무가 없습니다" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "요청 취소" }).click();
  // The input is editable again and nothing was written a second time.
  await expect(task).toBeEnabled();
  await expect(task).toHaveValue("도달하지 못한 업무");
  await expect(page.getByRole("button", { name: "업무 추가" })).toBeEnabled();
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/tasks") && call.method === "POST",
    ),
  ).toHaveLength(1);
});

test("sharing recovery and file/input state contribute independently to the aggregate dirty flag", async ({
  page,
}) => {
  const api = await backend(page);
  api.loseSharing = true;
  await open(page, projectId);
  await pickColleague(page);
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await expect(page.getByRole("status")).toContainText(
    "저장 결과를 확인할 수 없습니다",
  );
  await page.reload();
  await expect(
    page.getByRole("status").filter({ hasText: "이전 저장 요청" }),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: "새 업무", exact: true })
    .fill("아직 저장하지 않은 입력");
  await page.getByLabel("파일 선택", { exact: true }).setInputFiles(sampleFile);
  await page
    .getByRole("listitem", { name: sampleFile.name, exact: true })
    .getByRole("button", { name: "취소", exact: true })
    .click();
  await dirty(page, true);
  await page.getByRole("button", { name: "저장 결과 조회" }).click();
  await expect(
    page.getByRole("combobox", { name: "설계 담당자 역할" }),
  ).toBeEnabled();
  await dirty(page, true);
  await page.getByRole("textbox", { name: "새 업무", exact: true }).fill("");
  await dirty(page, false);
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/sharing") && call.method === "PUT",
    ),
  ).toHaveLength(1);
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/files") && call.method === "POST",
    ),
  ).toHaveLength(0);
});

test("lost attachment response is recovered after reload through metadata without uploading again", async ({
  page,
}) => {
  const api = await backend(page);
  api.loseAttach = true;
  await open(page, projectId);
  await page.getByLabel("파일 선택", { exact: true }).setInputFiles(sampleFile);
  await page.getByRole("button", { name: "업로드 시작" }).click();
  await expect(
    page.getByRole("listitem", { name: sampleFile.name, exact: true }),
  ).toContainText("결과 확인 필요");
  await page.reload();
  await dirty(page, true);
  await page
    .getByRole("listitem", { name: sampleFile.name, exact: true })
    .getByRole("button", { name: "결과 조회", exact: true })
    .click();
  await expect(page.getByText("첨부 완료", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", {
      name: `${sampleFile.name} 다운로드`,
      exact: true,
    }),
  ).toBeVisible();
  await dirty(page, false);
  expect(
    api.calls.filter(
      (call) => call.gateway === "files" && call.method === "PUT",
    ),
  ).toHaveLength(1);
  expect(
    api.calls.filter(
      (call) => call.path.endsWith("/attach") && call.method === "POST",
    ),
  ).toHaveLength(1);
  expect(
    api.calls.some(
      (call) =>
        call.path.includes("/files/by-request/") && call.method === "GET",
    ),
  ).toBe(true);
});
