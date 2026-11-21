// AI-DO packages/ui/src/lib/overlay/dialog-surface-model.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged. The two cases
// that need a document ran in jsdom there; here they run in the browser, on
// the module the fixture server serves.
import { expect, test, type Page } from "@playwright/test";
import { join } from "node:path";

import {
  dialogAccessibilityProps,
  dialogContentLayerClass,
  dialogOverlayLayerClass,
  dialogSizeClasses,
  drawerModeClass,
  drawerSideClass,
  shouldBlockOutsideInteraction,
} from "../../src/lib/overlay/dialog-surface-model";

type Model = typeof import("../../src/lib/overlay/dialog-surface-model");
declare global {
  interface Window {
    dialogSurfaceModel: Model;
  }
}
async function loadModel(page: Page) {
  const source = join(
    test.info().config.rootDir,
    "../src/lib/overlay/dialog-surface-model.ts",
  );
  await page.goto("/primitives.html");
  await page.addScriptTag({
    type: "module",
    content: `import * as model from ${JSON.stringify(`/@fs${source}`)}; window.dialogSurfaceModel = model;`,
  });
  await page.waitForFunction(() => Boolean(window.dialogSurfaceModel));
}

test.describe("dialog surface model", () => {
  test("projects accessibility props for dialogs without descriptions", () => {
    expect(dialogAccessibilityProps(true)).toEqual({});
    expect(dialogAccessibilityProps(false)).toEqual({
      "aria-describedby": undefined,
    });
  });

  test("projects dialog layer classes", () => {
    expect(dialogOverlayLayerClass("default")).toBe(
      "z-[calc(var(--ui-z-drawer)-1)]",
    );
    expect(dialogContentLayerClass("default")).toBe("z-[var(--ui-z-drawer)]");
    expect(dialogOverlayLayerClass("elevated")).toBe(
      "z-[calc(var(--ui-z-dialog-elevated)-1)]",
    );
    expect(dialogContentLayerClass("elevated")).toBe(
      "z-[var(--ui-z-dialog-elevated)]",
    );
  });

  test("projects dialog size classes without applying max width to full-size dialogs", () => {
    expect(dialogSizeClasses({ fullSize: true, maxWidth: "max-w-lg" })).toBe(
      "h-[92vh] w-[96vw] max-w-[1600px]",
    );
    expect(
      dialogSizeClasses({ fullSize: false, maxWidth: "max-w-3xl" }),
    ).toEqual(["w-[calc(100vw-2rem)] max-h-[85vh]", "max-w-3xl"]);
  });

  test("projects outside interaction blocking", () => {
    expect(shouldBlockOutsideInteraction(true)).toBe(false);
    expect(shouldBlockOutsideInteraction(false)).toBe(true);
  });

  test("keeps floating layers from dismissing dialogs", async ({ page }) => {
    await loadModel(page);
    const result = await page.evaluate(() => {
      const { isFloatingLayerOutsideEvent, shouldPreventOutsideInteraction } =
        window.dialogSurfaceModel;
      const layer = document.createElement("div");
      const target = document.createElement("button");
      layer.setAttribute("data-ui-floating-layer", "");
      layer.append(target);
      document.body.append(layer);

      const event = new MouseEvent("mousedown", { bubbles: true });
      target.dispatchEvent(event);

      const observed = [
        isFloatingLayerOutsideEvent(event),
        shouldPreventOutsideInteraction(event, true),
      ];

      layer.remove();
      return observed;
    });

    expect(result).toEqual([true, true]);
  });

  test("recognizes floating layer targets from Radix outside event details", async ({
    page,
  }) => {
    await loadModel(page);
    const result = await page.evaluate(() => {
      const { isFloatingLayerOutsideEvent, shouldPreventOutsideInteraction } =
        window.dialogSurfaceModel;
      const layer = document.createElement("div");
      const target = document.createElement("button");
      layer.setAttribute("data-ui-floating-layer", "");
      layer.append(target);
      document.body.append(layer);

      const originalEvent = new MouseEvent("pointerdown", { bubbles: true });
      target.dispatchEvent(originalEvent);

      const outsideEvent = new CustomEvent(
        "dismissableLayer.pointerDownOutside",
        {
          bubbles: false,
          cancelable: true,
          detail: { originalEvent },
        },
      );
      document.body.dispatchEvent(outsideEvent);

      const observed = [
        isFloatingLayerOutsideEvent(outsideEvent),
        shouldPreventOutsideInteraction(outsideEvent, true),
      ];

      layer.remove();
      return observed;
    });

    expect(result).toEqual([true, true]);
  });

  test("projects drawer side and mode classes", () => {
    expect(drawerSideClass("left")).toBe(
      "left-0 border-r border-r-[var(--ui-color-border)]",
    );
    expect(drawerSideClass("right")).toBe(
      "right-0 border-l border-l-[var(--ui-color-border)]",
    );
    expect(drawerModeClass(true)).toBe("overflow-hidden");
    expect(drawerModeClass(false)).toBe("gap-3 p-4");
  });
});
