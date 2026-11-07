import { browserUser, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import { resolve, sep } from "node:path";
import type { Installation } from "../packages/sdk/src/index";

async function openRemoteApp(page: Page) {
  await mockSurface(page);
  const remote = resolve(".local/evidence/runtime-health-ui/remote");
  const app: Installation = {
    id: "60000000-0000-4000-8000-000000000001",
    appId: "css-isolation",
    name: "스타일 격리 검증 앱",
    releaseId: "70000000-0000-4000-8000-000000000001",
    environment: "dev",
    version: "1.0.0",
    dbStatus: "ready",
    runtimeStatus: "running",
    desiredState: "running",
    kind: "docker_app",
    canManage: false,
    ui: {
      entry: "/artifacts/health-fixture/mf-manifest.json",
      export: "./AppRoot",
      contractVersion: "miy.ui/v1alpha1",
    },
  };
  await page.route("**/artifacts/health-fixture/**", async (route) => {
    const path = resolve(
      remote,
      new URL(route.request().url()).pathname.slice(
        "/artifacts/health-fixture/".length,
      ),
    );
    expect(path.startsWith(remote + sep)).toBe(true);
    await route.fulfill({ path });
  });
  await page.route("**/api/**", async (route) => {
    expect(route.request().method()).toBe("GET");
    const path = new URL(route.request().url()).pathname;
    const json =
      path === "/api/v1/me"
        ? browserUser(route, { id: "fixture", name: "검증 사용자", admin: false, csrf: "fixture" })
        : path === "/api/v1/installations"
          ? [app]
          : { title: "저장된 제목" };
    await route.fulfill({ json });
  });
  await page.goto("/environments/dev/apps/css-isolation");
  await expect(page.getByLabel("문서 제목")).toHaveValue("저장된 제목");
}

for (const width of [1440, 390]) {
  test(`platform class names do not restyle remote app elements at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await openRemoteApp(page);
    const differences = await page.evaluate(() => {
      const root = document.querySelector(".miy-app-root")!;
      const probe = document.createElement("section");
      // Use real remote app descendants, without CSS modules or app overrides
      // that could conceal a global platform rule. Adding a class owned by the
      // shell must leave the app's existing browser/base styles unchanged.
      probe.innerHTML = `
        <div data-case="base-shell" data-classes="shell">앱 기본 글꼴</div>
        <span data-case="base-hidden" data-classes="visually-hidden">앱 안내</span>
        <div data-case="page-header" data-classes="page-header">
          <h1 data-case="page-heading">앱 제목</h1><p data-case="page-summary">앱 설명</p>
        </div>
        <div data-case="page-grid" data-classes="grid"><div>첫째</div><div>둘째</div></div>
        <button type="button" data-case="button" data-classes="ui-button ui-button-primary">앱 버튼</button>
        <div data-case="panel" data-classes="ui-panel">
          <div data-classes="ui-panel-heading"><h2 data-case="panel-heading">앱 패널</h2></div>
          <p data-case="panel-body">앱 내용</p>
        </div>
        <label data-case="field" data-classes="ui-field">앱 입력<input data-case="field-input"></label>
        <p data-case="shell-title" data-classes="header-title">앱 헤더</p>
        <div data-case="shell-workspace" data-classes="workspace"><main data-case="workspace-main">앱 본문</main></div>
        <div data-classes="env-is-prod"><header data-case="shell-header" data-classes="shell-header">앱 헤더</header></div>
        <a href="#" aria-current="page" data-case="rail-link" data-classes="rail-item">앱 링크</a>
        <span data-case="mobile-account" data-classes="account-name">앱 사용자</span>
        <div data-case="mobile-toggle" data-classes="navigation-toggle">앱 메뉴</div>
        <div data-case="mobile-backdrop" data-classes="drawer-backdrop">앱 배경</div>
        <div data-case="dialog" data-classes="shell-dialog"><h2 data-case="dialog-title">앱 대화상자</h2></div>
      `;
      root.append(probe);
      const properties = [
        "display",
        "position",
        "width",
        "height",
        "min-height",
        "margin-top",
        "margin-bottom",
        "padding-top",
        "padding-right",
        "border-top-width",
        "border-radius",
        "background-color",
        "color",
        "font-family",
        "font-size",
        "font-weight",
        "line-height",
        "letter-spacing",
        "opacity",
        "visibility",
        "overflow",
        "gap",
        "grid-template-columns",
        "white-space",
        "box-shadow",
      ];
      function snapshot() {
        return Array.from(
          probe.querySelectorAll<HTMLElement>("[data-case]"),
        ).map((node) => {
          const style = getComputedStyle(node);
          return {
            name: node.dataset.case!,
            values: Object.fromEntries(
              properties.map((property) => [
                property,
                style.getPropertyValue(property),
              ]),
            ),
            beforeContent: getComputedStyle(node, "::before").content,
          };
        });
      }
      const before = snapshot();
      for (const node of probe.querySelectorAll<HTMLElement>(
        "[data-classes]",
      )) {
        node.className = node.dataset.classes!;
      }
      const after = snapshot();
      const changes = before.flatMap((previous, index) => {
        const current = after[index]!;
        return [
          ...properties
            .filter(
              (property) =>
                previous.values[property] !== current.values[property],
            )
            .map((property) => ({
              element: previous.name,
              property,
              before: previous.values[property],
              after: current.values[property],
            })),
          ...(previous.beforeContent !== current.beforeContent
            ? [
                {
                  element: previous.name,
                  property: "::before content",
                  before: previous.beforeContent,
                  after: current.beforeContent,
                },
              ]
            : []),
        ];
      });
      probe.remove();
      return changes;
    });
    expect(
      differences,
      "Platform selectors changed remote app styles merely because class names collided",
    ).toEqual([]);
  });
}
