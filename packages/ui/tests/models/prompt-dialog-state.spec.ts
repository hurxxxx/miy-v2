// AI-DO packages/ui/src/lib/feedback/prompt-dialog-state.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged.
import { expect, test } from "@playwright/test";

import {
  cancelCurrentPromptDialog,
  openPromptDialog,
  submitCurrentPromptDialog,
  type PromptDialogRequest,
} from "../../src/lib/feedback/prompt-dialog-state";

function request(
  overrides: Partial<PromptDialogRequest> = {},
): PromptDialogRequest {
  return {
    title: "Rename item",
    description: "Choose a visible name.",
    placeholder: "Item name",
    defaultValue: "Current name",
    submitLabel: "Save",
    cancelLabel: "Cancel",
    resolve: () => {},
    ...overrides,
  };
}

test.describe("prompt dialog state", () => {
  test("opens a prompt request without completing another request", () => {
    const nextRequest = request();

    const transition = openPromptDialog(null, nextRequest);

    expect(transition).toEqual({
      state: nextRequest,
      completion: null,
    });
  });

  test("replaces an open prompt request and completes the current request as cancelled", () => {
    const currentRequest = request({ title: "First" });
    const nextRequest = request({ title: "Second" });

    const transition = openPromptDialog(currentRequest, nextRequest);

    expect(transition.state).toBe(nextRequest);
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: null,
    });
  });

  test("submits the current prompt value and closes the dialog", () => {
    const currentRequest = request();

    const transition = submitCurrentPromptDialog(currentRequest, "New name");

    expect(transition.state).toBeNull();
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: "New name",
    });
  });

  test("cancels the current prompt and closes the dialog", () => {
    const currentRequest = request();

    const transition = cancelCurrentPromptDialog(currentRequest);

    expect(transition.state).toBeNull();
    expect(transition.completion).toEqual({
      resolve: currentRequest.resolve,
      value: null,
    });
  });

  test("ignores submit and cancel transitions when no request is open", () => {
    expect(submitCurrentPromptDialog(null, "New name")).toEqual({
      state: null,
      completion: null,
    });
    expect(cancelCurrentPromptDialog(null)).toEqual({
      state: null,
      completion: null,
    });
  });
});
