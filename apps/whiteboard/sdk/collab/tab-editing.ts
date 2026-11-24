import { useEffect, useRef, useState } from "react";
import type { AppRuntime } from "../browser";

/** Drain edits after a portal switch. Browser visibility never ends an editor. */
export function useTabEditing(
  activity: AppRuntime["activity"],
  editing: () => boolean,
  finish: () => Promise<boolean>,
  refresh: () => void,
) {
  const callbacks = useRef({ editing, finish, refresh });
  callbacks.current = { editing, finish, refresh };
  const [pending, setPending] = useState(false);
  const [unconfirmed, setUnconfirmed] = useState(false);
  useEffect(() => {
    if (!activity) return;
    let disposed = false;
    let ending = false;
    let owedRefresh = false;
    let inFlight = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let delay = 1000;
    function refreshIfVisible() {
      if (owedRefresh && activity!.getSnapshot() && !ending) {
        owedRefresh = false;
        callbacks.current.refresh();
      }
    }
    async function drain() {
      clearTimeout(timer);
      if (disposed || inFlight || !ending) return;
      inFlight = true;
      let saved = false;
      try { saved = !callbacks.current.editing() || await callbacks.current.finish(); }
      catch { /* The in-memory draft stays protected for retry. */ }
      inFlight = false;
      if (disposed) return;
      if (saved) {
        ending = false;
        setPending(false);
        setUnconfirmed(false);
        delay = 1000;
        refreshIfVisible();
      } else {
        setUnconfirmed(true);
        if (navigator.onLine) {
          timer = setTimeout(drain, delay);
          delay = Math.min(30000, delay * 2);
        }
      }
    }
    function changed() {
      if (!activity!.getSnapshot()) {
        owedRefresh = true;
        if (callbacks.current.editing()) {
          ending = true;
          setPending(true);
          void drain();
        }
      } else refreshIfVisible();
    }
    const unsubscribe = activity.subscribe(changed);
    changed();
    window.addEventListener("online", drain);
    return () => {
      disposed = true;
      clearTimeout(timer);
      unsubscribe();
      window.removeEventListener("online", drain);
    };
  }, [activity]);
  return { pending, unconfirmed };
}
