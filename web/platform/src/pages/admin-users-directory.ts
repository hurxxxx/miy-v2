import { request } from "@miy/sdk";

export type AdminUser = {
  id: string;
  name: string;
  admin: boolean;
  canDevelop: boolean;
  enabled: boolean;
  primaryGroupId: string | null;
  primaryGroupName: string | null;
};

export type AdminUserFilters = {
  q?: string;
  role?: "admin" | "developer" | "member";
  status?: "enabled" | "disabled";
  organization?: string;
  descendants?: boolean;
};

type UserDirectoryPage = { items: AdminUser[]; nextCursor: string | null };
type UserDirectoryOptions = { cursor?: string | null; limit?: number; signal?: AbortSignal };

export function fetchAdminUsers(filters: AdminUserFilters, options: UserDirectoryOptions = {}) {
  const { cursor, limit = 25, signal } = options;
  const params = new URLSearchParams({ limit: String(limit) });
  for (const [key, value] of Object.entries(filters)) {
    if (value !== undefined) params.set(key, String(value));
  }
  if (cursor) params.set("cursor", cursor);
  return request<UserDirectoryPage>(`/api/v1/users/directory?${params}`, undefined, { signal });
}
