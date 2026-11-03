export type Environment = "dev" | "prod";

export function environmentPath(environment: string, app?: string) {
  return `/environments/${environment}` + (app ? `/apps/${app}` : "");
}

// Legacy `/apps/{app}` and platform pages default to the dev environment.
export function currentEnvironment(path: string): Environment | "invalid" {
  const parts = path.split("/");
  if (parts[1] !== "environments") return "dev";
  return parts[2] === "dev" || parts[2] === "prod" ? parts[2] : "invalid";
}

export function isCliApprovalPath(path: string | null | undefined) {
  return !!path && (path.startsWith("/cli/authorize/") || path.startsWith("/cli/device/"));
}

// getRandomValues is available on internal HTTP origins as well as HTTPS.
// randomUUID requires a secure context and would break runtime controls on IP HTTP.
export function requestId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
