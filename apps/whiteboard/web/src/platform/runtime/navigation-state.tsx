import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import type { NavigationState } from '@miy/sdk';

/**
 * The miy contract asks the app to report unsaved input and in-flight work
 * with `runtime.setNavigationState` whenever it changes. Several screens can
 * hold work at once (a post draft, a comment, a notice being edited), so each
 * registers its own flag and the provider reports their union.
 */
type Register = (key: string, dirty: boolean) => void;
type PrepareToLeave = () => Promise<boolean>;

interface NavigationController {
  register: Register;
  registerEditor: (guard: PrepareToLeave) => () => void;
  runAfterSave: (navigate: () => void) => void;
}

const NavigationStateContext = createContext<NavigationController>({
  register: () => undefined,
  registerEditor: () => () => undefined,
  runAfterSave: (navigate: () => void) => navigate(),
});

export function NavigationStateProvider({
  report,
  message,
  children,
}: {
  report: (state: NavigationState) => void;
  message: string;
  children: ReactNode;
}) {
  const [dirtyKeys, setDirtyKeys] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const register = useCallback<Register>((key, dirty) => {
    setDirtyKeys((current) => {
      if (current.has(key) === dirty) return current;
      const next = new Set(current);
      if (dirty) next.add(key);
      else next.delete(key);
      return next;
    });
  }, []);
  const dirty = dirtyKeys.size > 0;
  const editor = useRef<PrepareToLeave | null>(null);
  const leaving = useRef(false);
  const registerEditor = useCallback((guard: PrepareToLeave) => {
    editor.current = guard;
    return () => {
      if (editor.current === guard) editor.current = null;
    };
  }, []);
  const runAfterSave = useCallback((navigate: () => void) => {
    const guard = editor.current;
    if (!guard) {
      navigate();
      return;
    }
    if (leaving.current) return;
    leaving.current = true;
    void guard().then((saved) => {
      if (saved && editor.current === guard) navigate();
    }).finally(() => {
      leaving.current = false;
    });
  }, []);
  const context = useMemo(
    () => ({ register, registerEditor, runAfterSave }),
    [register, registerEditor, runAfterSave],
  );
  useEffect(() => {
    report({ dirty, message });
  }, [dirty, message, report]);
  return (
    <NavigationStateContext.Provider value={context}>
      {children}
    </NavigationStateContext.Provider>
  );
}

let nextKey = 0;

/** Report one screen's unsaved/in-flight state; cleared on unmount. */
export function useNavigationDirty(dirty: boolean): void {
  const { register } = useContext(NavigationStateContext);
  const keyRef = useRef<string | null>(null);
  if (keyRef.current === null) {
    nextKey += 1;
    keyRef.current = `dirty-${nextKey}`;
  }
  const key = keyRef.current;
  useEffect(() => {
    register(key, dirty);
  }, [dirty, key, register]);
  useEffect(() => () => register(key, false), [key, register]);
}

/** Keep the active editor mounted until its saves finish on app-owned routes. */
export function usePrepareToLeaveEditor(prepareToLeave: PrepareToLeave): void {
  const { registerEditor } = useContext(NavigationStateContext);
  const prepareRef = useRef(prepareToLeave);
  useLayoutEffect(() => {
    prepareRef.current = prepareToLeave;
  }, [prepareToLeave]);
  useLayoutEffect(() => registerEditor(() => prepareRef.current()), [registerEditor]);
}

export function useNavigateAfterEditorSave(): (navigate: () => void) => void {
  return useContext(NavigationStateContext).runAfterSave;
}
