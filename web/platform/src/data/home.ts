import { useEffect, useState } from "react";
import { request, type HomeCard, type HomeCardPage } from "@miy/sdk";

export type HomeCardsState =
  | { status: "loading"; cards: HomeCard[] }
  | { status: "ready"; cards: HomeCard[] }
  | { status: "error"; cards: HomeCard[] };

/**
 * Summary cards of the apps on the portal home (`GET /api/v1/home/cards`). The
 * Core asks each app that declares a card; an app that did not answer comes
 * back `unavailable`. Read once per visit of the home page.
 */
export function useHomeCards(enabled: boolean, attempt: number): HomeCardsState {
  const [state, setState] = useState<HomeCardsState>({ status: "loading", cards: [] });
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    setState((current) => ({ status: "loading", cards: current.cards }));
    request<HomeCardPage>("/api/v1/home/cards", undefined, { signal: controller.signal })
      .then((page) => {
        if (!controller.signal.aborted)
          setState({ status: "ready", cards: Array.isArray(page.cards) ? page.cards : [] });
      })
      .catch(() => {
        if (!controller.signal.aborted) setState((current) => ({ status: "error", cards: current.cards }));
      });
    return () => controller.abort();
  }, [enabled, attempt]);
  return state;
}
