// AI-DO packages/ui/src/lib/feedback/confirm-dialog-state.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged.
import { expect, test } from "@playwright/test";

import {
  cancelCurrentDialog,
  confirmCurrentDialog,
  openConfirmDialog,
  type ConfirmDialogRequest,
} from "../../src/lib/feedback/confirm-dialog-state";

function request(
  overrides: Partial<ConfirmDialogRequest> = {},
): ConfirmDialogRequest {
  return {
    title: "Delete item",
    description: "This cannot be undone.",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
    resolve: () => {},
    ...overrides,
  };
}

test.describe("confirm dialog state", () => {
  test("opens a confirm request without completing another request", () => {
    const nextRequest = request();

    const transition = openConfirmDialog(null, nextRequest);

    expect(transition).toEqual({
      state: nextRequest,
      completion: null,
    });
  });

  test("replaces an open confirm request and completes the current request as cancelled", () => {
    const currentRequest = request({ title: "First" });
    const nextRequest = request({ title: "Second" });

    const transition = openConfirmDialog(currentRequest, nextRequest);

    expect(transition.state).toBe(nextRequest);
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: false,
    });
  });

  test("confirms the current request and closes the dialog", () => {
    const currentRequest = request();

    const transition = confirmCurrentDialog(currentRequest);

    expect(transition.state).toBeNull();
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: true,
    });
  });

  test("cancels the current request and closes the dialog", () => {
    const currentRequest = request();

    const transition = cancelCurrentDialog(currentRequest);

    expect(transition.state).toBeNull();
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: false,
    });
  });

  test("ignores confirm and cancel transitions when no request is open", () => {
    expect(confirmCurrentDialog(null)).toEqual({
      state: null,
      completion: null,
    });
    expect(cancelCurrentDialog(null)).toEqual({
      state: null,
      completion: null,
    });
  });
});
