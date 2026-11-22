// Ported from AI-DO apps/web/src/platform/admin/admin-apps-section.tsx
// (0002af062): AppBarCategoriesAdmin, CategoryIconButton,
// AdminAppCategoryItem and copyAdminAppBarCategory, with EmptyPanel and
// FORM_FIELD_CLASS from admin-shared-components.tsx / admin-shared-model.ts.
// What miy changes:
// - the arrangement is /api/v1/launcher/categories (camelCase, CSRF header) and
//   the assignable apps are the platform catalog (GET /api/v1/apps), named as
//   the catalog names them and drawn with their monogram (apps have no icon);
// - the icon keys are the launcher's fixed list (no server-supplied keys);
// - a failed read is shown in place with a retry instead of a toast, as the
//   other console sections do;
// - it is one section of the console's administration page, under the title
//   AI-DO gives its tab, so its headings sit one level lower.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowDown, ArrowRight, ArrowUp, Plus, RefreshCw, Save, Search, Trash2, X } from "lucide-react";
import { Button, SearchField, Tooltip, useFeedback } from "@miy/ui";

import {
  createLauncherCategory,
  deleteLauncherCategory,
  launcherErrorKey,
  listCatalogApps,
  listLauncherCategories,
  updateLauncherCategoryLayout,
  type CatalogApp,
  type LauncherCategoriesResponse,
} from "../data/launcher";
import { LAUNCHER_ICON_KEYS, LAUNCHER_ICON_PICKER_GROUPS, launcherIconForKey } from "../layout/launcher-icons";
import { LoadError } from "../components/LoadState";
import { AppMark, monograms } from "../ui";
import { IconPickerDialog, type IconPickerGroup } from "./IconPickerDialog";

const fieldClassName =
  "app-text-body w-full rounded-lg border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none transition-colors focus:border-app-accent";

/** A category being edited: its apps by ID, in display order. */
type AdminLauncherCategory = {
  id: string;
  title: string;
  iconKey: string;
  appIds: string[];
};

/** An app the editor can place, as the catalog names it. */
type AdminLauncherApp = {
  appId: string;
  title: string;
  mark: string;
};

function launcherIconPickerGroups(
  iconKeys: readonly string[],
  labelForGroup: (groupId: string) => string,
): readonly IconPickerGroup[] {
  const allowedIconKeys = new Set(iconKeys);
  const groups: IconPickerGroup[] = LAUNCHER_ICON_PICKER_GROUPS.map((group) => ({
    id: group.id,
    iconKeys: group.iconKeys.filter((iconKey) => allowedIconKeys.has(iconKey)),
    label: labelForGroup(group.id),
  })).filter((group) => group.iconKeys.length > 0);
  const groupedIconKeys = new Set(groups.flatMap((group) => [...group.iconKeys]));
  const otherIconKeys = iconKeys.filter((iconKey) => !groupedIconKeys.has(iconKey));
  if (otherIconKeys.length > 0) {
    groups.push({
      id: "other",
      iconKeys: otherIconKeys,
      label: labelForGroup("other"),
    });
  }
  return groups;
}

function EmptyPanel({ title, description }: { title: string; description: string }) {
  return (
    <div className="rounded-md border border-dashed border-app-border bg-app-surface-sidebar px-4 py-10 text-center">
      <div className="space-y-1">
        <div className="app-text-body font-medium text-app-ink">{title}</div>
        <div className="app-text-body text-app-ink/55">{description}</div>
      </div>
    </div>
  );
}

export function LauncherCategoriesAdmin({ csrf }: { csrf: string }) {
  const { t, i18n } = useTranslation(["launcher", "shell", "common"]);
  const feedback = useFeedback();
  const [catalog, setCatalog] = useState<CatalogApp[]>([]);
  const [draftCategories, setDraftCategories] = useState<AdminLauncherCategory[]>([]);
  const [revision, setRevision] = useState("");
  const [selectedCategoryId, setSelectedCategoryId] = useState<string | null>(null);
  const [iconPickerTarget, setIconPickerTarget] = useState<string | null>(null);
  const [appSearchQuery, setAppSearchQuery] = useState("");
  const [newCategoryTitle, setNewCategoryTitle] = useState("");
  const [newCategoryIcon, setNewCategoryIcon] = useState("layout");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  /** Server error codes the launcher explains; anything else gets the fallback. */
  const errorText = useCallback(
    (error: unknown, fallback: string) => {
      const key = launcherErrorKey(error);
      return key && i18n.exists(key) ? t(key) : fallback;
    },
    [i18n, t],
  );

  const applyResponse = useCallback((next: LauncherCategoriesResponse) => {
    setRevision(next.revision);
    setDraftCategories(next.categories.map(copyLauncherCategory));
    setSelectedCategoryId((current) =>
      current && next.categories.some((category) => category.id === current)
        ? current
        : (next.categories[0]?.id ?? null),
    );
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [next, apps] = await Promise.all([listLauncherCategories(), listCatalogApps()]);
      setCatalog(apps);
      applyResponse(next);
      setLoadError(null);
    } catch (caughtError) {
      setLoadError(errorText(caughtError, t("launcher:admin.appBarLoadFailed")));
    } finally {
      setLoading(false);
    }
  }, [applyResponse, errorText, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const iconKeys = LAUNCHER_ICON_KEYS;
  const iconPickerGroups = useMemo(
    () => launcherIconPickerGroups(iconKeys, (groupId) => t(`launcher:admin.appBarIconGroups.${groupId}`)),
    [iconKeys, t],
  );
  const availableApps = useMemo<AdminLauncherApp[]>(() => {
    const marks = monograms(catalog.map((app) => ({ key: app.id, name: app.name })));
    return catalog.map((app) => ({ appId: app.id, title: app.name, mark: marks.get(app.id) ?? "?" }));
  }, [catalog]);
  const availableAppById = useMemo(() => {
    const items = new Map<string, AdminLauncherApp>();
    for (const app of availableApps) {
      items.set(app.appId, app);
    }
    return items;
  }, [availableApps]);
  // A category may still name an app the catalog no longer lists.
  const appFor = useCallback(
    (appId: string): AdminLauncherApp =>
      availableAppById.get(appId) ?? { appId, title: appId, mark: (Array.from(appId)[0] ?? "?").toUpperCase() },
    [availableAppById],
  );
  const assignedCategoryByAppId = useMemo(() => {
    const items = new Map<string, AdminLauncherCategory>();
    for (const category of draftCategories) {
      for (const appId of category.appIds) {
        items.set(appId, category);
      }
    }
    return items;
  }, [draftCategories]);
  const selectedCategory = useMemo(() => {
    if (draftCategories.length === 0) {
      return null;
    }
    return draftCategories.find((category) => category.id === selectedCategoryId) ?? draftCategories[0]!;
  }, [draftCategories, selectedCategoryId]);
  const selectedCategoryApps = useMemo(
    () => (selectedCategory?.appIds ?? []).map(appFor),
    [appFor, selectedCategory?.appIds],
  );
  const selectedCategoryAssignableApps = useMemo(() => {
    if (!selectedCategory) {
      return [];
    }
    const selectedAppIds = new Set(selectedCategory.appIds);
    return availableApps.filter((app) => !selectedAppIds.has(app.appId));
  }, [availableApps, selectedCategory]);
  const iconPickerValue =
    iconPickerTarget === "new"
      ? newCategoryIcon
      : (draftCategories.find((category) => category.id === iconPickerTarget)?.iconKey ?? "layout");
  const iconPickerTitle =
    iconPickerTarget === "new"
      ? t("launcher:admin.appBarNewCategoryTitle")
      : (draftCategories.find((category) => category.id === iconPickerTarget)?.title ??
        t("launcher:admin.appBarCategoryIcon"));
  const SelectedCategoryIcon = selectedCategory ? launcherIconForKey(selectedCategory.iconKey) : null;

  useEffect(() => {
    setAppSearchQuery("");
  }, [selectedCategory?.id]);

  const normalizedAppSearchQuery = appSearchQuery.trim().toLowerCase();
  const hasAppSearchQuery = normalizedAppSearchQuery.length > 0;
  const filteredSelectedCategoryAssignableApps = useMemo(() => {
    if (!hasAppSearchQuery) {
      return selectedCategoryAssignableApps;
    }
    return selectedCategoryAssignableApps.filter((app) => {
      const assignedCategory = assignedCategoryByAppId.get(app.appId);
      const searchableValues = [app.appId, app.title, assignedCategory?.title];
      return searchableValues.some((value) => (value ?? "").toLowerCase().includes(normalizedAppSearchQuery));
    });
  }, [assignedCategoryByAppId, hasAppSearchQuery, normalizedAppSearchQuery, selectedCategoryAssignableApps]);
  const unassignedAssignableApps = useMemo(
    () => filteredSelectedCategoryAssignableApps.filter((app) => !assignedCategoryByAppId.has(app.appId)),
    [assignedCategoryByAppId, filteredSelectedCategoryAssignableApps],
  );
  const movableAssignableApps = useMemo(
    () => filteredSelectedCategoryAssignableApps.filter((app) => assignedCategoryByAppId.has(app.appId)),
    [assignedCategoryByAppId, filteredSelectedCategoryAssignableApps],
  );

  const updateDraftCategory = useCallback((categoryId: string, patch: Partial<AdminLauncherCategory>) => {
    setDraftCategories((current) =>
      current.map((category) => (category.id === categoryId ? { ...category, ...patch } : category)),
    );
  }, []);

  const selectIcon = useCallback(
    (iconKey: string) => {
      if (iconPickerTarget === "new") {
        setNewCategoryIcon(iconKey);
      } else if (iconPickerTarget) {
        updateDraftCategory(iconPickerTarget, { iconKey });
      }
      setIconPickerTarget(null);
    },
    [iconPickerTarget, updateDraftCategory],
  );

  const moveDraftCategory = useCallback((categoryId: string, direction: -1 | 1) => {
    setDraftCategories((current) => {
      const index = current.findIndex((category) => category.id === categoryId);
      const nextIndex = index + direction;
      if (index < 0 || nextIndex < 0 || nextIndex >= current.length) {
        return current;
      }
      const next = [...current];
      [next[index], next[nextIndex]] = [next[nextIndex]!, next[index]!];
      return next;
    });
  }, []);

  const toggleCategoryApp = useCallback((categoryId: string, appId: string, checked: boolean) => {
    setDraftCategories((current) =>
      current.map((category) => {
        const appIds = category.appIds.filter((item) => item !== appId);
        if (checked && category.id === categoryId) {
          appIds.push(appId);
        }
        return { ...category, appIds };
      }),
    );
  }, []);

  const createCategory = useCallback(async () => {
    const title = newCategoryTitle.trim();
    if (!title) {
      feedback.error(t("launcher:admin.appBarCategoryTitleRequired"));
      return;
    }
    setSaving(true);
    try {
      const next = await createLauncherCategory(csrf, {
        iconKey: newCategoryIcon,
        title,
      });
      applyResponse(next);
      setSelectedCategoryId(next.categories[next.categories.length - 1]?.id ?? null);
      setIconPickerTarget(null);
      setNewCategoryTitle("");
      feedback.success(t("launcher:admin.appBarCategoryCreated"));
    } catch (caughtError) {
      feedback.error(errorText(caughtError, t("launcher:admin.appBarCategoryCreateFailed")));
    } finally {
      setSaving(false);
    }
  }, [applyResponse, csrf, errorText, feedback, newCategoryIcon, newCategoryTitle, t]);

  const deleteCategory = useCallback(
    async (category: AdminLauncherCategory) => {
      if (
        !window.confirm(
          t("launcher:admin.appBarCategoryDeleteConfirm", {
            title: category.title,
          }),
        )
      ) {
        return;
      }
      setSaving(true);
      try {
        const next = await deleteLauncherCategory(csrf, category.id);
        applyResponse(next);
        setIconPickerTarget((current) => (current === category.id ? null : current));
        feedback.success(t("launcher:admin.appBarCategoryDeleted"));
      } catch (caughtError) {
        feedback.error(errorText(caughtError, t("launcher:admin.appBarCategoryDeleteFailed")));
      } finally {
        setSaving(false);
      }
    },
    [applyResponse, csrf, errorText, feedback, t],
  );

  const saveLayout = useCallback(async () => {
    if (!revision) return;
    setSaving(true);
    try {
      const next = await updateLauncherCategoryLayout(csrf, {
        expectedRevision: revision,
        categories: draftCategories.map((category) => ({
          appIds: category.appIds,
          iconKey: category.iconKey,
          id: category.id,
          title: category.title.trim(),
        })),
      });
      applyResponse(next);
      feedback.success(t("launcher:admin.appBarLayoutSaved"));
    } catch (caughtError) {
      feedback.error(errorText(caughtError, t("launcher:admin.appBarLayoutSaveFailed")));
    } finally {
      setSaving(false);
    }
  }, [applyResponse, csrf, draftCategories, errorText, feedback, revision, t]);

  return (
    <>
      <h2 className="page-section-title">{t("launcher:admin.appBarTab")}</h2>
      {/* The console panels below expect a gap like the one between panels. */}
      <section className="mb-4 space-y-3">
        <div className="flex flex-col gap-2 border-b border-app-border pb-2 lg:flex-row lg:items-center lg:justify-between">
          <div className="min-w-0">
            <h3 className="app-text-control text-app-ink">{t("launcher:admin.appBarTitle")}</h3>
            <p className="app-text-caption mt-0.5 max-w-3xl text-app-ink/55">{t("launcher:admin.appBarDescription")}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Tooltip content={t("launcher:admin.refresh")}>
              <button
                aria-label={t("launcher:admin.refresh")}
                className="inline-flex size-8 items-center justify-center rounded-md border border-app-border text-app-ink transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-50"
                disabled={loading || saving}
                onClick={() => void load()}
                type="button"
              >
                <RefreshCw size={14} />
              </button>
            </Tooltip>
            <Button disabled={loading || saving || loadError !== null || !revision} onClick={() => void saveLayout()}>
              <Save size={15} />
              {t("launcher:admin.appBarSaveLayout")}
            </Button>
          </div>
        </div>

        {loadError ? (
          <LoadError message={loadError} onRetry={() => void load()} />
        ) : (
          <>
            <div className="rounded-md border border-app-border bg-app-bg p-3">
              <div className="grid gap-3 lg:grid-cols-[minmax(260px,1fr)_auto_auto] lg:items-end">
                <label className="space-y-1">
                  <span className="app-text-caption text-app-ink/60">{t("launcher:admin.appBarNewCategoryTitle")}</span>
                  <input
                    className={fieldClassName}
                    onChange={(event) => setNewCategoryTitle(event.currentTarget.value)}
                    placeholder={t("launcher:admin.appBarNewCategoryPlaceholder")}
                    value={newCategoryTitle}
                  />
                </label>
                <CategoryIconButton
                  disabled={loading || saving}
                  iconKey={newCategoryIcon}
                  label={t("launcher:admin.appBarCategoryIcon")}
                  onClick={() => setIconPickerTarget("new")}
                />
                <Button disabled={loading || saving} onClick={() => void createCategory()}>
                  <Plus size={15} />
                  {t("launcher:admin.appBarAddCategory")}
                </Button>
              </div>
            </div>

            {loading && draftCategories.length === 0 ? (
              <EmptyPanel
                description={t("launcher:admin.appBarLoadingDescription")}
                title={t("launcher:admin.appBarLoadingTitle")}
              />
            ) : draftCategories.length === 0 ? (
              <EmptyPanel
                description={t("launcher:admin.appBarEmptyDescription")}
                title={t("launcher:admin.appBarEmptyTitle")}
              />
            ) : (
              <div className="grid gap-3 xl:grid-cols-[minmax(260px,340px)_minmax(0,1fr)]">
                <aside className="min-w-0 self-start rounded-md border border-app-border bg-app-bg">
                  <div className="flex items-center justify-between gap-2 border-b border-app-border bg-app-surface-sidebar px-3 py-2">
                    <h4 className="app-text-caption font-medium text-app-ink">{t("launcher:admin.appBarCategoriesList")}</h4>
                    <span className="app-text-micro text-app-ink/45">{draftCategories.length}</span>
                  </div>
                  <div className="divide-y divide-app-border">
                    {draftCategories.map((category, index) => {
                      const CategoryIcon = launcherIconForKey(category.iconKey);
                      const selected = selectedCategory?.id === category.id;
                      return (
                        <div
                          className={`flex min-w-0 items-center gap-1 border-l-2 px-2 py-1.5 ${
                            selected ? "border-app-accent bg-app-accent/10" : "border-transparent bg-app-bg"
                          }`}
                          key={category.id}
                        >
                          <button
                            aria-pressed={selected}
                            className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-app-surface-hover"
                            onClick={() => setSelectedCategoryId(category.id)}
                            type="button"
                          >
                            <span
                              className={`flex size-8 shrink-0 items-center justify-center rounded-md border ${
                                selected
                                  ? "border-app-accent/35 bg-app-bg text-app-accent"
                                  : "border-app-border bg-app-surface-sidebar text-app-ink/55"
                              }`}
                            >
                              <CategoryIcon size={15} />
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="app-text-body-sm block truncate font-medium text-app-ink">
                                {category.title}
                              </span>
                              <span className="app-text-micro block truncate text-app-ink/45">
                                {t("launcher:admin.appBarCategoryAppCount", {
                                  count: category.appIds.length,
                                })}
                              </span>
                            </span>
                          </button>
                          <div className="flex shrink-0 items-center gap-1">
                            <Tooltip content={t("launcher:admin.appBarMoveUp")}>
                              <button
                                aria-label={t("launcher:admin.appBarMoveUp")}
                                className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/60 transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
                                disabled={saving || index === 0}
                                onClick={() => moveDraftCategory(category.id, -1)}
                                type="button"
                              >
                                <ArrowUp size={13} />
                              </button>
                            </Tooltip>
                            <Tooltip content={t("launcher:admin.appBarMoveDown")}>
                              <button
                                aria-label={t("launcher:admin.appBarMoveDown")}
                                className="inline-flex size-7 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-ink/60 transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
                                disabled={saving || index === draftCategories.length - 1}
                                onClick={() => moveDraftCategory(category.id, 1)}
                                type="button"
                              >
                                <ArrowDown size={13} />
                              </button>
                            </Tooltip>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </aside>

                <section className="min-w-0 rounded-md border border-app-border bg-app-bg">
                  {selectedCategory ? (
                    <>
                      <div className="grid gap-3 border-b border-app-border bg-app-surface-sidebar p-3 lg:grid-cols-[auto_minmax(260px,1fr)_auto_auto] lg:items-end">
                        <span className="flex size-10 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-accent">
                          {SelectedCategoryIcon ? <SelectedCategoryIcon size={18} /> : null}
                        </span>
                        <label className="space-y-1">
                          <span className="app-text-caption text-app-ink/60">{t("launcher:admin.appBarCategoryTitle")}</span>
                          <input
                            aria-label={t("launcher:admin.appBarCategoryTitle")}
                            className={fieldClassName}
                            onChange={(event) =>
                              updateDraftCategory(selectedCategory.id, {
                                title: event.currentTarget.value,
                              })
                            }
                            value={selectedCategory.title}
                          />
                        </label>
                        <CategoryIconButton
                          disabled={saving}
                          iconKey={selectedCategory.iconKey}
                          label={t("launcher:admin.appBarCategoryIcon")}
                          onClick={() => setIconPickerTarget(selectedCategory.id)}
                        />
                        <Tooltip content={t("common:actions.delete")}>
                          <button
                            aria-label={t("common:actions.delete")}
                            className="inline-flex size-9 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-danger transition-colors hover:bg-app-danger/10 disabled:cursor-not-allowed disabled:opacity-40"
                            disabled={saving}
                            onClick={() => void deleteCategory(selectedCategory)}
                            type="button"
                          >
                            <Trash2 size={15} />
                          </button>
                        </Tooltip>
                      </div>
                      <div className="grid gap-3 p-3 xl:grid-cols-[minmax(280px,0.8fr)_minmax(360px,1.2fr)]">
                        <section className="space-y-2">
                          <div className="flex items-center justify-between gap-2">
                            <h4 className="app-text-caption font-medium text-app-ink">{t("launcher:admin.appBarSelectedApps")}</h4>
                            <span className="app-text-micro text-app-ink/45">{selectedCategoryApps.length}</span>
                          </div>
                          {selectedCategoryApps.length === 0 ? (
                            <div className="app-text-caption rounded-md border border-dashed border-app-border bg-app-surface-sidebar px-3 py-6 text-center text-app-ink/50">
                              {t("launcher:admin.appBarNoSelectedApps")}
                            </div>
                          ) : (
                            <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-1 2xl:grid-cols-2">
                              {selectedCategoryApps.map((app) => (
                                <AdminAppCategoryItem
                                  app={app}
                                  disabled={saving}
                                  key={app.appId}
                                  onClick={() => toggleCategoryApp(selectedCategory.id, app.appId, false)}
                                  action="remove"
                                  tooltip={t("launcher:admin.appBarRemoveApp")}
                                  variant="selected"
                                />
                              ))}
                            </div>
                          )}
                        </section>
                        <section className="space-y-2">
                          <div className="flex items-center justify-between gap-2">
                            <h4 className="app-text-caption font-medium text-app-ink">{t("launcher:admin.appBarAvailableApps")}</h4>
                            <span className="app-text-micro text-app-ink/45">
                              {filteredSelectedCategoryAssignableApps.length}
                            </span>
                          </div>
                          <SearchField
                            aria-label={t("launcher:admin.appBarSearchApps")}
                            endAdornment={<Search size={14} className="text-app-ink/50" />}
                            onChange={(event) => setAppSearchQuery(event.currentTarget.value)}
                            placeholder={t("launcher:admin.appBarSearchAppsPlaceholder")}
                            value={appSearchQuery}
                          />
                          {filteredSelectedCategoryAssignableApps.length === 0 ? (
                            <div className="app-text-caption rounded-md border border-dashed border-app-border bg-app-surface-sidebar px-3 py-6 text-center text-app-ink/50">
                              {t(
                                hasAppSearchQuery
                                  ? "launcher:admin.appBarNoFilteredApps"
                                  : "launcher:admin.appBarNoAvailableApps",
                              )}
                            </div>
                          ) : (
                            <div className="max-h-[28rem] space-y-3 overflow-y-auto pr-1">
                              {unassignedAssignableApps.length > 0 ? (
                                <section className="space-y-2">
                                  <div className="flex items-center justify-between gap-2">
                                    <h5 className="app-text-micro font-medium text-app-ink/55">
                                      {t("launcher:admin.appBarUnassignedApps")}
                                    </h5>
                                    <span className="app-text-micro text-app-ink/40">{unassignedAssignableApps.length}</span>
                                  </div>
                                  <div className="grid gap-2 sm:grid-cols-2 2xl:grid-cols-3">
                                    {unassignedAssignableApps.map((app) => (
                                      <AdminAppCategoryItem
                                        action="add"
                                        app={app}
                                        disabled={saving}
                                        key={app.appId}
                                        onClick={() => toggleCategoryApp(selectedCategory.id, app.appId, true)}
                                        tooltip={t("launcher:admin.appBarAddApp")}
                                        variant="available"
                                      />
                                    ))}
                                  </div>
                                </section>
                              ) : null}
                              {movableAssignableApps.length > 0 ? (
                                <section className="space-y-2">
                                  <div className="flex items-center justify-between gap-2">
                                    <h5 className="app-text-micro font-medium text-app-ink/55">
                                      {t("launcher:admin.appBarMoveFromOtherCategories")}
                                    </h5>
                                    <span className="app-text-micro text-app-ink/40">{movableAssignableApps.length}</span>
                                  </div>
                                  <div className="grid gap-2 sm:grid-cols-2 2xl:grid-cols-3">
                                    {movableAssignableApps.map((app) => {
                                      const assignedCategory = assignedCategoryByAppId.get(app.appId);
                                      return (
                                        <AdminAppCategoryItem
                                          action="move"
                                          app={app}
                                          detail={
                                            assignedCategory
                                              ? t("launcher:admin.appBarAssignedTo", {
                                                  title: assignedCategory.title,
                                                })
                                              : null
                                          }
                                          disabled={saving}
                                          key={app.appId}
                                          onClick={() => toggleCategoryApp(selectedCategory.id, app.appId, true)}
                                          tooltip={t("launcher:admin.appBarMoveAppHere")}
                                          variant="available"
                                        />
                                      );
                                    })}
                                  </div>
                                </section>
                              ) : null}
                            </div>
                          )}
                        </section>
                      </div>
                    </>
                  ) : (
                    <div className="app-text-caption p-6 text-center text-app-ink/55">
                      {t("launcher:admin.appBarNoCategorySelected")}
                    </div>
                  )}
                </section>
              </div>
            )}
          </>
        )}
        <IconPickerDialog
          allLabel={t("launcher:admin.appBarIconGroups.all")}
          closeLabel={t("common:actions.close")}
          emptyLabel={t("launcher:admin.appBarNoFilteredIcons")}
          groups={iconPickerGroups}
          iconForKey={launcherIconForKey}
          onClose={() => setIconPickerTarget(null)}
          onSelect={selectIcon}
          open={iconPickerTarget !== null}
          searchLabel={t("launcher:admin.appBarSearchIcons")}
          searchPlaceholder={t("launcher:admin.appBarSearchIconsPlaceholder")}
          title={t("launcher:admin.appBarChooseIcon", {
            title: iconPickerTitle,
          })}
          value={iconPickerValue}
        />
      </section>
    </>
  );
}

function CategoryIconButton({
  disabled,
  iconKey,
  label,
  onClick,
}: {
  disabled: boolean;
  iconKey: string;
  label: string;
  onClick: () => void;
}) {
  const Icon = launcherIconForKey(iconKey);
  return (
    <div className="space-y-1">
      <span className="app-text-caption text-app-ink/60">{label}</span>
      <Tooltip content={label}>
        <button
          aria-label={label}
          className="inline-flex size-9 items-center justify-center rounded-md border border-app-border bg-app-bg text-app-accent transition-colors hover:bg-app-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
          disabled={disabled}
          onClick={onClick}
          type="button"
        >
          <Icon size={17} />
        </button>
      </Tooltip>
    </div>
  );
}

function AdminAppCategoryItem({
  action,
  app,
  detail,
  disabled,
  onClick,
  tooltip,
  variant,
}: {
  action?: "add" | "move" | "remove";
  app: AdminLauncherApp;
  detail?: string | null;
  disabled: boolean;
  onClick: () => void;
  tooltip: string;
  variant: "available" | "selected";
}) {
  const itemAction = action ?? (variant === "selected" ? "remove" : "add");
  const ActionIcon = itemAction === "remove" ? X : itemAction === "move" ? ArrowRight : Plus;
  return (
    <Tooltip content={tooltip}>
      <button
        aria-label={`${app.title} · ${tooltip}`}
        className={`flex min-h-11 min-w-0 items-center gap-2 rounded-md border px-2.5 py-2 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
          variant === "selected"
            ? "border-app-accent/25 bg-app-accent/10 text-app-ink hover:border-app-accent/45"
            : "border-app-border bg-app-surface-sidebar text-app-ink/75 hover:bg-app-surface-hover hover:text-app-ink"
        }`}
        disabled={disabled}
        onClick={onClick}
        type="button"
      >
        <AppMark mark={app.mark} colorKey={app.appId} size="sm" />
        <span className="min-w-0 flex-1">
          <span className="app-text-body-sm block truncate font-medium">{app.title}</span>
          {detail ? <span className="app-text-micro block truncate text-app-ink/45">{detail}</span> : null}
        </span>
        <span
          className={`inline-flex size-6 shrink-0 items-center justify-center rounded-md border ${
            variant === "selected"
              ? "border-app-accent/25 bg-app-bg text-app-accent"
              : itemAction === "move"
                ? "border-app-accent/25 bg-app-bg text-app-accent"
                : "border-app-border bg-app-bg text-app-ink/55"
          }`}
        >
          <ActionIcon size={13} />
        </span>
      </button>
    </Tooltip>
  );
}

function copyLauncherCategory(category: LauncherCategoriesResponse["categories"][number]): AdminLauncherCategory {
  return {
    id: category.id,
    title: category.title,
    iconKey: category.iconKey,
    appIds: [...category.appIds],
  };
}
