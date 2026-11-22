import type { AppRuntime } from "@miy/sdk";
import type { paths } from "./api.generated";

const pingPath = "/ping";
type PingResponse = paths[typeof pingPath]["get"]["responses"][200]["content"]["application/json"];

export function getPing(runtime: Pick<AppRuntime, "api">, signal?: AbortSignal) {
  return runtime.api<PingResponse>(pingPath, { signal });
}
