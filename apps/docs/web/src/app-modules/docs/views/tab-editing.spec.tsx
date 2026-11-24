import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useTabEditing } from "../../../../../sdk/collab/tab-editing";

it("waits for durable completion across a tab switch and retries after reconnecting", async () => {
  let active = true;
  let changed = () => {};
  const activity = {
    getSnapshot: () => active,
    subscribe(fn: () => void) { changed = fn; return () => { changed = () => {}; }; },
  };
  const finish = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  const refresh = vi.fn();
  const { result, unmount } = renderHook(() => useTabEditing(activity, () => true, finish, refresh));
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(finish).not.toHaveBeenCalled();
  await act(async () => { active = false; changed(); });
  expect(result.current).toEqual({ pending: true, unconfirmed: true });
  act(() => { active = true; changed(); });
  expect(refresh).not.toHaveBeenCalled();
  await act(async () => { window.dispatchEvent(new Event("online")); });
  expect(finish).toHaveBeenCalledTimes(2);
  expect(refresh).toHaveBeenCalledTimes(1);
  expect(result.current).toEqual({ pending: false, unconfirmed: false });
  unmount();
});
