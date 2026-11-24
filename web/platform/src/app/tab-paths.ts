import { createContext, useContext } from "react";
import { environmentPath } from "../lib/paths";

// Each signed-in shell owns its own map. Never persist frame locations or sessions.
export const TabPaths = createContext<ReadonlyMap<string, string>>(new Map());
export function useTabPath(environment: string, appId: string) {
  const base = environmentPath(environment, appId);
  return useContext(TabPaths).get(base) ?? base;
}
