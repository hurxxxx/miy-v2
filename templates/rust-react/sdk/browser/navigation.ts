/** Host-managed navigation protection. Return a message while work is unsaved/in flight. */
export type NavigationGuard = () => string | undefined;
export function createNavigationGuards() {
  const guards = new Set<NavigationGuard>();
  const message = (): string | undefined => {
    for (const guard of guards) {
      try {
        const reason = guard();
        if (reason) return reason;
      } catch {
        return "작업 상태를 확인할 수 없습니다. 화면을 이동하시겠습니까?";
      }
    }
  };
  return {
    register: (guard: NavigationGuard) => {
      guards.add(guard);
      return () => {
        guards.delete(guard);
      };
    },
    message,
    clear: () => guards.clear(),
    confirmNavigation: () => {
      const reason = message();
      return !reason || window.confirm(reason);
    },
  };
}
