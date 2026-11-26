import type { DirectoryPage } from "./api.gen";

export interface DirectoryQuery {
  kind?: "users" | "groups" | "memberships";
  q?: string;
  limit?: number;
  cursor?: string;
  /** Resolve selected stable references; group members are never copied into ACLs. */
  ids?: readonly string[];
}
export interface DirectoryRequestOptions {
  signal?: AbortSignal;
}
export type DirectoryReader = (
  query?: DirectoryQuery,
  options?: DirectoryRequestOptions,
) => Promise<DirectoryPage>;

/** Query encoding only; the path is fixed by the host and the server authorizes it. */
export function directoryQuery(query: DirectoryQuery = {}): string {
  const kind = query.kind ?? "users";
  if (!["users", "groups", "memberships"].includes(kind)) throw new Error("invalid_directory_query");
  const params = new URLSearchParams({ kind });
  if (query.q !== undefined) {
    if (typeof query.q !== "string" || Array.from(query.q.trim()).length > 254 ||
        (kind === "memberships" && query.q.trim() !== ""))
      throw new Error("invalid_directory_query");
    params.set("q", query.q.trim());
  }
  if (query.limit !== undefined) {
    if (!Number.isInteger(query.limit) || query.limit < 1 || query.limit > 100)
      throw new Error("invalid_directory_query");
    params.set("limit", String(query.limit));
  }
  if (query.cursor !== undefined) {
    if (typeof query.cursor !== "string" || query.cursor.length === 0)
      throw new Error("invalid_directory_query");
    params.set("cursor", query.cursor);
  }
  if (query.ids !== undefined) {
    if (!Array.isArray(query.ids) || query.ids.length === 0 || query.ids.length > 100 || query.ids.some(
      (id) => typeof id !== "string" || id.length === 0 || Array.from(id).length > 256 || id.includes(","),
    )) throw new Error("invalid_directory_query");
    params.set("ids", query.ids.join(","));
  }
  return `?${params.toString()}`;
}

/**
 * For the untyped `platformApi("directory")` read made without a query. Such a
 * caller reads one page and cannot continue it, so a directory that does not
 * fit is an error rather than a silently shortened list. `directory()` with a
 * query returns pages and their `nextCursor` as they are.
 */
export function completeDirectoryPage<T>(page: T, query?: DirectoryQuery): T {
  if (query === undefined && (page as { nextCursor?: unknown } | null)?.nextCursor != null)
    throw new Error("directory_page_incomplete");
  return page;
}
