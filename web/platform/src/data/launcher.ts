// Launcher API of the portal and the console, ported from AI-DO's
// apps/web/src/platform/admin/admin-api.ts (listAdminAppBarCategories,
// createAdminAppBarCategory, deleteAdminAppBarCategory,
// updateAdminAppBarCategoryLayout at 0002af062) onto miy's Core API: categories
// live at /api/v1/launcher/categories (camelCase JSON, console administrators
// write), and a person's pinned apps are `preferences.pinnedApps`.
import { ApiError, request } from "@miy/sdk";
import type { Preferences } from "./session";

/** One launcher category with its app IDs in display order. */
export type LauncherCategory = {
  id: string;
  title: string;
  iconKey: string;
  position: number;
  appIds: string[];
  updatedAt: string;
};

/** `GET /api/v1/launcher/categories` and the answer of every category change. */
export type LauncherCategoriesResponse = { categories: LauncherCategory[]; revision: string };

/** One category of `PUT /api/v1/launcher/categories/layout`, in display order. */
export type LauncherLayoutCategory = {
  id: string;
  title: string;
  iconKey: string;
  appIds: string[];
};

/** An app of the platform catalog (`GET /api/v1/apps`). */
export type CatalogApp = { id: string; name: string };

const CATEGORIES_PATH = "/api/v1/launcher/categories";

function text(value: unknown) {
  return typeof value === "string" ? value : "";
}

// A malformed answer is a failed read, never an empty arrangement.
function checkedCategories(value: unknown): LauncherCategoriesResponse {
  const response = value as { categories?: unknown; revision?: unknown } | null;
  const categories = response?.categories;
  if (!Array.isArray(categories)) throw new ApiError("parse", "invalid_launcher_categories");
  return {
    revision: text(response?.revision),
    categories: categories.flatMap((item) => {
      const category = item as Partial<LauncherCategory> | null;
      if (!category || typeof category.id !== "string") return [];
      return [
        {
          id: category.id,
          title: text(category.title),
          iconKey: text(category.iconKey),
          position: typeof category.position === "number" ? category.position : 0,
          appIds: Array.isArray(category.appIds)
            ? category.appIds.filter((appId): appId is string => typeof appId === "string")
            : [],
          updatedAt: text(category.updatedAt),
        },
      ];
    }),
  };
}

export async function listLauncherCategories(signal?: AbortSignal): Promise<LauncherCategoriesResponse> {
  return checkedCategories(await request<unknown>(CATEGORIES_PATH, undefined, { signal }));
}

export async function createLauncherCategory(
  csrf: string,
  payload: { title: string; iconKey: string },
): Promise<LauncherCategoriesResponse> {
  return checkedCategories(
    await request<unknown>(CATEGORIES_PATH, csrf, {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  );
}

export async function deleteLauncherCategory(csrf: string, categoryId: string): Promise<LauncherCategoriesResponse> {
  return checkedCategories(
    await request<unknown>(`${CATEGORIES_PATH}/${encodeURIComponent(categoryId)}`, csrf, {
      method: "DELETE",
    }),
  );
}

export async function updateLauncherCategoryLayout(
  csrf: string,
  payload: { categories: LauncherLayoutCategory[]; expectedRevision: string },
): Promise<LauncherCategoriesResponse> {
  return checkedCategories(
    await request<unknown>(`${CATEGORIES_PATH}/layout`, csrf, {
      method: "PUT",
      body: JSON.stringify(payload),
    }),
  );
}

/** Every app of the catalog, following the server's pages. */
export async function listCatalogApps(): Promise<CatalogApp[]> {
  const apps: CatalogApp[] = [];
  let cursor: string | null = null;
  // The catalog holds at most a few hundred apps; the bound only stops a loop.
  for (let page = 0; page < 50; page++) {
    const path: string = cursor
      ? `/api/v1/apps?limit=200&cursor=${encodeURIComponent(cursor)}`
      : "/api/v1/apps?limit=200";
    const result = await request<{ apps?: unknown; nextCursor?: unknown }>(path);
    if (!Array.isArray(result?.apps)) throw new ApiError("parse", "invalid_app_catalog");
    for (const app of result.apps as Partial<CatalogApp>[])
      if (app && typeof app.id === "string") apps.push({ id: app.id, name: text(app.name) || app.id });
    cursor = typeof result.nextCursor === "string" && result.nextCursor ? result.nextCursor : null;
    if (!cursor) break;
  }
  return apps;
}

/** Save the person's pinned apps; `[]` returns the rail to every app they can open. */
export function savePinnedApps(csrf: string, pinnedApps: string[]) {
  return request<{ name: string; preferences: Preferences }>("/api/v1/me/preferences", csrf, {
    method: "PATCH",
    body: JSON.stringify({ pinnedApps }),
  });
}

/** Translation key of a server error code the launcher screens explain, if any. */
export function launcherErrorKey(error: unknown): string | null {
  return error instanceof ApiError && /^[a-z_]+$/.test(error.code) ? `launcher:errors.${error.code}` : null;
}
