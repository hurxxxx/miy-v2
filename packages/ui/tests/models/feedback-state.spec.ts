// AI-DO packages/ui/src/lib/feedback/feedback-state.spec.ts at 0002af062, moved from vitest to the
// Playwright runner. The cases and expectations are unchanged.
import { expect, test } from "@playwright/test";

import {
  createFeedbackRecord,
  dismissFeedback,
  EMPTY_FEEDBACK_QUEUE,
  enqueueFeedback,
  FEEDBACK_DURATION_MS,
  FEEDBACK_PENDING_LIMIT,
  FEEDBACK_VISIBLE_LIMIT,
  feedbackToneClass,
  type FeedbackInput,
  type FeedbackQueue,
} from "../../src/lib/feedback/feedback-state";

function record(id: string, overrides: Partial<FeedbackInput> = {}) {
  return createFeedbackRecord(id, {
    title: id,
    tone: "info",
    ...overrides,
  });
}

function enqueueMany(count: number): FeedbackQueue {
  let queue = EMPTY_FEEDBACK_QUEUE;
  for (let index = 0; index < count; index += 1) {
    queue = enqueueFeedback(queue, record(`feedback-${index}`));
  }
  return queue;
}

test.describe("feedback state", () => {
  test("uses the shared passive duration and polite announcement by default", () => {
    expect(FEEDBACK_DURATION_MS).toBe(6000);
    expect(record("default").announcement).toBe("polite");
  });

  test("keeps three items visible and promotes pending feedback in FIFO order", () => {
    const queue = enqueueMany(FEEDBACK_VISIBLE_LIMIT + 2);

    expect(queue.visible.map((item) => item.id)).toEqual([
      "feedback-0",
      "feedback-1",
      "feedback-2",
    ]);
    expect(queue.pending.map((item) => item.id)).toEqual([
      "feedback-3",
      "feedback-4",
    ]);

    const next = dismissFeedback(queue, "feedback-1");
    expect(next.visible.map((item) => item.id)).toEqual([
      "feedback-0",
      "feedback-2",
      "feedback-3",
    ]);
    expect(next.pending.map((item) => item.id)).toEqual(["feedback-4"]);
  });

  test("replaces matching visible and pending records without changing their position", () => {
    let queue = enqueueMany(FEEDBACK_VISIBLE_LIMIT);
    queue = enqueueFeedback(
      queue,
      record("pending-original", { dedupeKey: "pending-job" }),
    );
    queue = enqueueFeedback(
      queue,
      record("visible-original", { dedupeKey: "visible-job" }),
    );

    queue = enqueueFeedback(
      queue,
      record("pending-replacement", {
        dedupeKey: "pending-job",
        title: "Pending replacement",
      }),
    );
    queue = enqueueFeedback(
      queue,
      record("visible-replacement", {
        dedupeKey: "visible-job",
        title: "Visible replacement",
      }),
    );

    expect(queue.pending[0]?.id).toBe("pending-replacement");
    expect(queue.pending[1]?.id).toBe("visible-replacement");

    const visibleQueue = enqueueFeedback(
      EMPTY_FEEDBACK_QUEUE,
      record("first", {
        dedupeKey: "same",
      }),
    );
    const replacedVisible = enqueueFeedback(
      visibleQueue,
      record("second", { dedupeKey: "same" }),
    );
    expect(replacedVisible.visible.map((item) => item.id)).toEqual(["second"]);
  });

  test("bounds pending feedback and drops the oldest pending item on overflow", () => {
    const queue = enqueueMany(
      FEEDBACK_VISIBLE_LIMIT + FEEDBACK_PENDING_LIMIT + 2,
    );

    expect(queue.visible).toHaveLength(FEEDBACK_VISIBLE_LIMIT);
    expect(queue.pending).toHaveLength(FEEDBACK_PENDING_LIMIT);
    expect(queue.pending[0]?.id).toBe("feedback-5");
  });

  test("preserves action and announcement semantics while projecting tone classes", () => {
    const onAction = () => {};
    const feedback = createFeedbackRecord("action", {
      action: { altText: "Undo the save", label: "Undo", onAction },
      announcement: "assertive",
      title: "Saved",
      tone: "success",
    });

    expect(feedback.action?.onAction).toBe(onAction);
    expect(feedback.announcement).toBe("assertive");
    expect(feedbackToneClass("info")).toContain("var(--ui-color-accent)");
    expect(feedbackToneClass("success")).toContain("var(--ui-color-success)");
    expect(feedbackToneClass("warning")).toContain("var(--ui-color-warning)");
    expect(feedbackToneClass("error")).toContain("var(--ui-color-danger)");
  });
});
