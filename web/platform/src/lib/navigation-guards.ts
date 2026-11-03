import type { createNavigationGuards } from "@miy/sdk";

// Guards of mounted app sessions; logout must ask each before leaving.
export const activeGuards = new Set<ReturnType<typeof createNavigationGuards>>();

export function confirmLeaveAllApps() {
  return [...activeGuards].every((guards) => guards.confirmNavigation());
}

export function clearAllAppGuards() {
  for (const guards of activeGuards) guards.clear();
}
