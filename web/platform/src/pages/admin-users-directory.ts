import { request, type AdminDirectoryUser, type AdminUserDirectoryPage } from "@miy/sdk";

export type AdminUser = AdminDirectoryUser;

export type AdminUserFilters = {
  q?: string;
  role?: "admin" | "developer" | "member";
  status?: "enabled" | "disabled";
  organization?: string;
  descendants?: boolean;
};

type UserDirectoryOptions = { cursor?: string | null; limit?: number; signal?: AbortSignal };

export function fetchAdminUsers(filters: AdminUserFilters, options: UserDirectoryOptions = {}) {
  const { cursor, limit = 25, signal } = options;
  const params = new URLSearchParams({ limit: String(limit) });
  for (const [key, value] of Object.entries(filters)) {
    if (value !== undefined) params.set(key, String(value));
  }
  if (cursor) params.set("cursor", cursor);
  return request<AdminUserDirectoryPage>(`/api/v1/users/directory?${params}`, undefined, { signal });
}
