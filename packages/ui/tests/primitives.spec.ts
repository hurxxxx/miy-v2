// The AI-DO primitives of common UI 2.0.0, built the way an app builds them:
// Tailwind entry CSS with theme.css, platform tokens, components from source.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

type Theme = "light" | "dark";
let tokens: Record<Theme, Map<string, string>>;
test.beforeAll(() => {
  const css = readFileSync(
    join(test.info().config.rootDir, "../../tokens/tokens.css"),
    "utf8",
  );
  const block = (selector: string) => {
    const start = css.indexOf(`\n${selector} {`);
    if (start < 0) throw new Error(`token block not found: ${selector}`);
    return new Map(
      [
        ...css
          .slice(start, css.indexOf("\n}", start))
          .matchAll(/(--ui-[a-z0-9-]+):\s*([^;]+);/g),
      ].map((match) => [match[1], match[2].trim()]),
    );
  };
  tokens = {
    light: block(":root"),
    dark: block('.dark,\n[data-miy-theme="dark"]'),
  };
});
/** The computed form of a hex color token, read from the platform token file. */
function color(theme: Theme, name: string): string {
  const value = tokens[theme].get(name);
  if (!value || !/^#[0-9a-f]{6}$/i.test(value))
    throw new Error(`not a hex color token: ${name}`);
  const part = (at: number) => parseInt(value.slice(at, at + 2), 16);
  return `rgb(${part(1)}, ${part(3)}, ${part(5)})`;
}
const size = (name: string) => tokens.light.get(name)!;
const button = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true });
const focusInside = (target: Locator) =>
  target.evaluate((element) => element.contains(document.activeElement));
async function open(page: Page, query = "") {
  await page.goto(`/primitives.html${query}`);
  await expect(button(page, "기본 동작")).toBeVisible();
}

test("Tailwind builds the primitives: button variants and sizes resolve to the tokens", async ({
  page,
}) => {
  await open(page);
  const primary = button(page, "기본 동작");
  await expect(primary).toHaveCSS(
    "background-color",
    color("light", "--ui-color-accent"),
  );
  await expect(primary).toHaveCSS(
    "color",
    color("light", "--ui-color-accent-fg"),
  );
  await expect(primary).toHaveCSS("height", size("--ui-density-dense"));
  await expect(primary).toHaveCSS(
    "border-top-left-radius",
    size("--ui-radius-sm"),
  );
  await expect(primary).toHaveCSS("font-weight", "600");
  await expect(button(page, "보조 동작")).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-raised"),
  );
  await expect(button(page, "고스트")).toHaveCSS(
    "background-color",
    "rgba(0, 0, 0, 0)",
  );
  await expect(button(page, "서브틀")).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-subtle"),
  );
  await expect(button(page, "넉넉한 크기")).toHaveCSS(
    "height",
    size("--ui-density-comfortable"),
  );
  const icon = button(page, "항목 추가");
  await expect(icon).toHaveCSS("width", size("--ui-density-dense"));
  await expect(icon).toHaveCSS("height", size("--ui-density-dense"));
  const disabled = button(page, "비활성");
  await expect(disabled).toBeDisabled();
  await expect(disabled).toHaveCSS("opacity", "0.5");
  await expect(page.getByText("완료", { exact: true }).first()).toHaveCSS(
    "color",
    color("light", "--ui-color-success"),
  );
  // Every button is type="button" unless the app asks otherwise.
  await expect(primary).toHaveAttribute("type", "button");
});

test("the dark class on the document root moves surfaces, text and the miy composites to the dark tokens", async ({
  page,
}) => {
  await open(page);
  const panel = page.locator("section").filter({ hasText: "버튼과 배지" });
  const primary = button(page, "기본 동작");
  const kind = button(page, "사용자");
  const marker = page.getByTestId("dark-variant");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    color("light", "--ui-color-bg"),
  );
  await expect(panel).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-raised"),
  );
  await expect(kind).toHaveCSS(
    "background-color",
    color("light", "--ui-color-accent"),
  );
  await expect(marker).toBeHidden();
  await button(page, "테마 전환").click();
  await expect(page.locator("html")).toHaveClass(/(^| )dark( |$)/);
  await expect(page.locator("html")).toHaveAttribute("data-miy-theme", "dark");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    color("dark", "--ui-color-bg"),
  );
  await expect(page.locator("body")).toHaveCSS(
    "color",
    color("dark", "--ui-color-ink"),
  );
  await expect(panel).toHaveCSS(
    "background-color",
    color("dark", "--ui-color-surface-raised"),
  );
  await expect(primary).toHaveCSS(
    "background-color",
    color("dark", "--ui-color-accent"),
  );
  await expect(primary).toHaveCSS(
    "color",
    color("dark", "--ui-color-accent-fg"),
  );
  // The miy composite components read the same tokens.
  await expect(kind).toHaveCSS(
    "background-color",
    color("dark", "--ui-color-accent"),
  );
  // theme.css registers the class based dark variant for the app's own classes.
  await expect(marker).toBeVisible();
  expect(color("dark", "--ui-color-surface-raised")).not.toBe(
    color("light", "--ui-color-surface-raised"),
  );
});

test("Dialog keeps focus inside, closes on Escape, its close button and the overlay", async ({
  page,
}) => {
  await open(page);
  const opener = button(page, "대화상자 열기");
  const dialog = page.getByRole("dialog", { name: "설정 변경" });
  await opener.click();
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveAccessibleDescription(
    "변경한 내용은 저장해야 적용됩니다.",
  );
  await expect(dialog).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-raised"),
  );
  expect(await focusInside(dialog)).toBe(true);
  for (let step = 0; step < 6; step += 1) {
    await page.keyboard.press("Tab");
    expect(await focusInside(dialog)).toBe(true);
  }
  // The page behind is hidden from assistive technology meanwhile, and a
  // script that moves focus there is brought back.
  await expect(
    page.locator("section").filter({ hasText: "버튼과 배지" }),
  ).toHaveAttribute("aria-hidden", "true");
  await page.evaluate(() =>
    document.querySelector<HTMLElement>("main button")!.focus(),
  );
  expect(await focusInside(dialog)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  // AI-DO's Dialog is opened by state, not by a Radix trigger, so it has no
  // element to hand focus back to: focus is left on the document body.
  expect(
    await page.evaluate(() => document.activeElement === document.body),
  ).toBe(true);
  await opener.click();
  await dialog.getByRole("button", { name: "닫기" }).click();
  await expect(dialog).toHaveCount(0);
  await opener.click();
  await expect(dialog).toBeVisible();
  await page.mouse.click(5, 5);
  await expect(dialog).toHaveCount(0);
  // The page lock is gone with the dialog.
  await button(page, "보조 동작").click();
});

test("DetailDrawer hands focus back to its opener; miy composites inside it keep Escape to themselves", async ({
  page,
}) => {
  await open(page);
  const opener = button(page, "공유 서랍 열기");
  const drawer = page.getByRole("dialog", { name: "공유 설정" });
  await opener.click();
  await expect(drawer).toBeVisible();
  const search = drawer.getByRole("combobox", {
    name: "공유 대상",
    exact: true,
  });
  await search.focus();
  await expect(drawer.getByRole("option").first()).toBeVisible();
  // Escape collapses the open list and nothing else.
  await page.keyboard.press("Escape");
  await expect(search).toHaveAttribute("aria-expanded", "false");
  await expect(drawer).toBeVisible();
  // A draft exists, so reloading asks first, in the composite's own dialog.
  await drawer.getByRole("button", { name: "설계 담당자 공유 해제" }).click();
  await drawer.getByRole("button", { name: "다시 불러오기" }).click();
  const confirm = page.locator("dialog.miy-dialog");
  await expect(confirm).toBeVisible();
  const box = (await confirm.boundingBox())!;
  const viewport = page.viewportSize()!;
  expect(Math.abs(box.x + box.width / 2 - viewport.width / 2)).toBeLessThan(2);
  expect(Math.abs(box.y + box.height / 2 - viewport.height / 2)).toBeLessThan(
    2,
  );
  await page.keyboard.press("Escape");
  await expect(confirm).toHaveCount(0);
  await expect(drawer).toBeVisible();
  await expect(drawer.getByText("저장하지 않은 변경")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(drawer).toHaveCount(0);
  await expect(opener).toBeFocused();
});

test("Tabs switch panels by pointer and arrow keys", async ({ page }) => {
  await open(page);
  const tabs = page.getByRole("tablist", { name: "표시 방식" });
  await expect(
    tabs.getByRole("tab", { name: "표", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await tabs.getByRole("tab", { name: "상태" }).click();
  await expect(page.getByRole("tabpanel")).toContainText(
    "접근 권한이 없습니다",
  );
  await expect(page.getByRole("columnheader")).toHaveCount(0);
  const active = tabs.getByRole("tab", { name: "상태" });
  await expect(active).toHaveCSS("color", color("light", "--ui-color-accent"));
  await page.keyboard.press("ArrowRight");
  await expect(tabs.getByRole("tab", { name: "차트" })).toBeFocused();
  await expect(tabs.getByRole("tab", { name: "차트" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("tabpanel")).toContainText("월별 추이");
});

test("Select lists its options, changes the value and closes on Escape", async ({
  page,
}) => {
  await open(page);
  // The Select has no label prop; its trigger is the only button combobox here.
  const trigger = page.locator('button[role="combobox"]');
  await expect(trigger).toHaveText(/보기/);
  await trigger.click();
  const list = page.getByRole("listbox");
  await expect(list.getByRole("option")).toHaveText(["보기", "편집", "소유자"]);
  await expect(list).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-raised"),
  );
  await list.getByRole("option", { name: "편집" }).click();
  await expect(page.getByTestId("role")).toHaveText("editor");
  await expect(trigger).toHaveText(/편집/);
  await expect(list).toHaveCount(0);
  await trigger.focus();
  await page.keyboard.press("Enter");
  // Opening by keyboard starts on the current value.
  await expect(list.getByRole("option", { name: "편집" })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(list.getByRole("option", { name: "소유자" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("role")).toHaveText("owner");
  await trigger.click();
  await page.keyboard.press("Escape");
  await expect(list).toHaveCount(0);
  await expect(page.getByTestId("role")).toHaveText("owner");
});

test("DropdownMenu runs an item, leaves a disabled one alone and returns focus", async ({
  page,
}) => {
  await open(page);
  const trigger = button(page, "더 보기 메뉴");
  await trigger.click();
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem")).toHaveText(["복제", "이동", "삭제"]);
  await expect(menu.getByRole("menuitem", { name: "이동" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await expect(menu.getByRole("menuitem", { name: "삭제" })).toHaveCSS(
    "color",
    color("light", "--ui-color-danger"),
  );
  await menu.getByRole("menuitem", { name: "복제" }).click();
  await expect(page.getByTestId("overlay-result")).toHaveText("copy");
  await expect(menu).toHaveCount(0);
  await trigger.click();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await expect(page.getByTestId("overlay-result")).toHaveText("copy");
});

test("Tooltip shows its content on hover and on keyboard focus", async ({
  page,
}) => {
  await open(page);
  const trigger = button(page, "동기화 정보");
  await trigger.hover();
  await expect(page.getByRole("tooltip")).toHaveText("마지막 동기화 10:42");
  await page.mouse.move(5, 5, { steps: 5 });
  await expect(page.getByRole("tooltip")).toHaveCount(0);
  await button(page, "오류 알림").focus();
  await page.keyboard.press("Tab");
  await expect(trigger).toBeFocused();
  await expect(page.getByRole("tooltip")).toHaveText("마지막 동기화 10:42");
});

test("DataTable sorts a column both ways, selects a row and turns into cards on a narrow screen", async ({
  page,
}) => {
  await open(page);
  const counts = () =>
    page.locator("tbody tr td:nth-child(3)").allTextContents();
  expect(await counts()).toEqual(["12", "3", "27"]);
  // TanStack Table sorts a number column largest first, a text column A to Z.
  const header = page.getByRole("columnheader", { name: "건수" });
  await header.click();
  await expect(header).toHaveText("건수 ↓");
  expect(await counts()).toEqual(["27", "12", "3"]);
  await header.click();
  await expect(header).toHaveText("건수 ↑");
  expect(await counts()).toEqual(["3", "12", "27"]);
  await header.click();
  await expect(header).toHaveText("건수");
  expect(await counts()).toEqual(["12", "3", "27"]);
  const title = page.getByRole("columnheader", { name: "업무" });
  await title.click();
  await expect(title).toHaveText("업무 ↑");
  expect(
    await page.locator("tbody tr td:nth-child(1)").allTextContents(),
  ).toEqual(["도면 검토", "설비 점검", "자재 발주"]);
  const row = page.getByRole("row", { name: /도면 검토/ });
  await row.click();
  await page.mouse.move(5, 5);
  await expect(row).toHaveCSS(
    "background-color",
    color("light", "--ui-color-accent-weak"),
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("columnheader")).toHaveCount(0);
  const card = page.getByRole("button", { name: /자재 발주/ });
  await card.focus();
  await page.keyboard.press("Enter");
  await expect(card).toHaveCSS(
    "background-color",
    color("light", "--ui-color-accent-weak"),
  );
});

test("useConfirm and usePrompt resolve with the choice; the danger variant is marked", async ({
  page,
}) => {
  await open(page);
  const result = page.getByTestId("overlay-result");
  await button(page, "삭제 확인").click();
  const confirm = page.getByRole("dialog", { name: "항목 삭제" });
  await expect(confirm).toHaveAccessibleDescription("이 항목을 삭제할까요?");
  await expect(confirm.getByRole("button", { name: "삭제" })).toHaveCSS(
    "background-color",
    color("light", "--ui-color-danger"),
  );
  await confirm.getByRole("button", { name: "삭제" }).click();
  await expect(result).toHaveText("true");
  await expect(confirm).toHaveCount(0);
  await button(page, "삭제 확인").click();
  await confirm.getByRole("button", { name: "취소" }).click();
  await expect(result).toHaveText("false");
  await button(page, "이름 입력").click();
  const prompt = page.getByRole("dialog", { name: "이름 변경" });
  const input = prompt.getByRole("textbox");
  await expect(input).toBeFocused();
  await expect(input).toHaveValue("기존 이름");
  await input.fill("  분기 보고서  ");
  await prompt.getByRole("button", { name: "변경" }).click();
  await expect(result).toHaveText("분기 보고서");
  await button(page, "이름 입력").click();
  await prompt.getByRole("button", { name: "취소" }).click();
  await expect(result).toHaveText("null");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("FeedbackProvider shows toasts by tone, keeps three visible and promotes the waiting one", async ({
  page,
}) => {
  await open(page);
  const region = page.getByRole("region", { name: "알림 영역" });
  const toasts = region.getByRole("listitem");
  await button(page, "저장 알림").click();
  await expect(toasts).toHaveCount(1);
  await expect(toasts.first()).toContainText("저장했습니다");
  await expect(toasts.first()).toContainText("변경 내용을 반영했습니다.");
  await expect(toasts.first()).toHaveCSS(
    "border-left-color",
    color("light", "--ui-color-success"),
  );
  await expect(region.getByRole("list")).toHaveCSS("position", "fixed");
  await button(page, "오류 알림").click();
  await expect(toasts).toHaveCount(2);
  await expect(toasts.nth(1)).toHaveCSS(
    "border-left-color",
    color("light", "--ui-color-danger"),
  );
  await button(page, "저장 알림").click();
  await button(page, "오류 알림").click();
  // Four were sent: three are shown, the fourth waits.
  await expect(toasts).toHaveCount(3);
  await expect(toasts.nth(2)).toContainText("저장했습니다");
  await toasts.first().getByRole("button", { name: "알림 닫기" }).click();
  await expect(toasts).toHaveCount(3);
  await expect(toasts.nth(2)).toContainText("저장하지 못했습니다");
});

test("ContentState, EmptyState, StatusSlot, FormMessage and ContextNote expose their state", async ({
  page,
}) => {
  await open(page);
  const message = page.locator("#name-message");
  await expect(message).toHaveRole("alert");
  await expect(message).toHaveText("두 글자 이상 입력해 주세요.");
  await expect(
    page.getByRole("textbox", { name: "프로젝트 이름" }),
  ).toHaveAccessibleDescription("두 글자 이상 입력해 주세요.");
  await expect(message).toHaveCSS("color", color("light", "--ui-color-danger"));
  await page.getByRole("tab", { name: "상태" }).click();
  const panel = page.getByRole("tabpanel");
  const loading = panel.getByRole("status").filter({ hasText: "불러오는 중" });
  await expect(loading).toHaveAttribute("aria-busy", "true");
  const failure = panel
    .getByRole("alert")
    .filter({ hasText: "목록을 불러올 수 없습니다" });
  await expect(failure).toContainText("잠시 후 다시 시도해 주세요.");
  await failure.getByRole("button", { name: "다시 시도" }).click();
  expect(await page.evaluate(() => window.primitives.results)).toEqual([
    "retry",
  ]);
  await expect(panel.getByText("등록된 업무가 없습니다")).toBeVisible();
  await expect(
    panel.getByRole("status").filter({ hasText: "저장했습니다." }),
  ).toHaveAttribute("data-state", "populated");
  await panel
    .getByRole("alert")
    .filter({ hasText: "저장하지 못했습니다." })
    .getByRole("button", { name: "다시 시도" })
    .click();
  expect(await page.evaluate(() => window.primitives.results)).toEqual([
    "retry",
    "again",
  ]);
  await expect(panel.getByText("확인 필요")).toBeVisible();
  // lucide-react icons are drawn as inline SVG.
  await expect(panel.locator("svg.lucide").first()).toBeVisible();
});

test("charts load Recharts on demand and draw every series", async ({
  page,
}) => {
  const chunks: string[] = [];
  page.on("request", (request) => {
    if (/recharts/.test(request.url())) chunks.push(request.url());
  });
  await open(page);
  expect(chunks).toEqual([]);
  await page.getByRole("tab", { name: "차트" }).click();
  const panel = page.getByRole("tabpanel");
  await expect(panel.locator(".recharts-line-curve")).toHaveCount(2);
  await expect(panel.locator(".recharts-bar-rectangle")).toHaveCount(8);
  await expect(panel.locator(".recharts-pie-sector")).toHaveCount(4);
  expect(chunks.length).toBeGreaterThan(0);
  await expect(panel.locator(".recharts-line-curve").first()).toHaveAttribute(
    "stroke",
    "var(--ui-color-chart-1)",
  );
});

test("RangeSlider, SearchField and FilterBar take keyboard and pointer input", async ({
  page,
}) => {
  await open(page);
  const slider = page.getByRole("slider", { name: "진행률" });
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByTestId("progress")).toHaveText("45");
  await page.keyboard.press("Home");
  await expect(page.getByTestId("progress")).toHaveText("0");
  const search = page.getByRole("searchbox", { name: "업무 검색" });
  await search.fill("도면");
  await expect(search).toHaveValue("도면");
  const mine = button(page, "내 업무");
  await expect(mine).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await mine.click();
  await page.mouse.move(5, 5);
  await expect(mine).toHaveCSS(
    "background-color",
    color("light", "--ui-color-surface-subtle"),
  );
});

test("AppShell shows the sidebar on a wide screen and a sheet with a menu button on a narrow one", async ({
  page,
}) => {
  await open(page, "?view=shell");
  const nav = page.getByRole("navigation", { name: "앱 메뉴" });
  await expect(nav).toBeVisible();
  await nav.getByRole("button", { name: /파일/ }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  // The wide sidebar is gone; the sheet's copy waits outside the viewport.
  const sheet = page.getByRole("navigation", { name: "앱 메뉴" });
  await expect(sheet).toHaveCount(1);
  await expect(sheet).not.toBeInViewport();
  await page.getByRole("button", { name: "메뉴", exact: true }).click();
  await expect(sheet).toBeInViewport();
  expect(await page.evaluate(() => document.body.style.overflow)).toBe(
    "hidden",
  );
  await page.getByRole("button", { name: "메뉴 닫기" }).last().click();
  await expect(sheet).not.toBeInViewport();
  expect(await page.evaluate(() => document.body.style.overflow)).toBe("");
});

for (const theme of ["light", "dark"] as const)
  for (const [name, width, height] of [
    ["desktop", 1280, 900],
    ["mobile", 390, 844],
  ] as const)
    test(`${theme} ${name} gallery fits the viewport and is recorded`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height });
      await open(page, theme === "dark" ? "?theme=dark" : "");
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      await page.screenshot({
        path: `.local/evidence/common-ui/primitives-${theme}-${name}.png`,
        animations: "disabled",
        fullPage: true,
      });
    });
