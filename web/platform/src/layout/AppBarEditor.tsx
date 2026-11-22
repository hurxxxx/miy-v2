// Ported from AI-DO apps/web/src/components/layout/AppBarEditor.tsx (0002af062).
// miy apps have no icon, so a row shows the app's monogram; the dialog takes
// the full width under the header on a phone, where the rail is a drawer.
import { ArrowDown, ArrowUp, ChevronDown, RotateCcw, Save, Search } from "lucide-react";
import { useEffect, useMemo, useState, type RefObject } from "react";
import { cn } from "@miy/ui";

import type { LauncherCategory } from "../data/launcher";
import { AppMark } from "../ui";
import {
  buildCategoryGroups,
  filterCategoryGroups,
  type AppBarAppItem,
  type AppBarTranslator,
} from "./app-bar-model";

export function AppBarEditor({
  appBarLayoutError,
  appBarLayoutSaving,
  dialogRef,
  draftItems,
  draftPinnedAppIds,
  launcherCategories,
  onClose,
  onMovePinnedApp,
  onReset,
  onSave,
  onTogglePinnedApp,
  pinnedEligibleAppIds,
  t,
}: {
  appBarLayoutError: string | null;
  appBarLayoutSaving: boolean;
  dialogRef: RefObject<HTMLDialogElement | null>;
  draftItems: AppBarAppItem[];
  draftPinnedAppIds: string[];
  launcherCategories: readonly LauncherCategory[];
  onClose: () => void;
  onMovePinnedApp: (appId: string, direction: -1 | 1) => void;
  onReset: () => void;
  onSave: () => void;
  onTogglePinnedApp: (appId: string, checked: boolean) => void;
  pinnedEligibleAppIds: ReadonlySet<string>;
  t: AppBarTranslator;
}) {
  const [query, setQuery] = useState("");
  const itemById = useMemo(() => new Map(draftItems.map((item) => [item.id, item])), [draftItems]);
  const selectedItems = useMemo(
    () => draftPinnedAppIds.flatMap((appId) => itemById.get(appId) ?? []),
    [draftPinnedAppIds, itemById],
  );
  const categoryGroups = useMemo(
    () =>
      buildCategoryGroups({
        categories: launcherCategories,
        items: draftItems.filter((item) => pinnedEligibleAppIds.has(item.id)),
        t,
      }),
    [draftItems, launcherCategories, pinnedEligibleAppIds, t],
  );
  const filteredCategoryGroups = useMemo(() => filterCategoryGroups(categoryGroups, query), [categoryGroups, query]);

  // Keyboard users land in the dialog it opened.
  useEffect(() => {
    dialogRef.current?.focus();
  }, [dialogRef]);

  return (
    <dialog
      aria-label={t("shell:appBarEditor.title")}
      className="app-launcher fixed left-[5.75rem] top-4 z-50 m-0 flex max-h-[calc(100dvh-2rem)] w-[38rem] max-w-[calc(100vw-7rem)] flex-col overflow-hidden rounded-2xl border border-app-border bg-app-surface p-3 text-app-ink shadow-2xl outline-none max-md:left-3 max-md:right-3 max-md:top-[4.25rem] max-md:max-h-[calc(100dvh-5rem)] max-md:w-auto max-md:max-w-none"
      open
      ref={dialogRef}
      tabIndex={-1}
    >
      <div className="flex shrink-0 items-center justify-between gap-3 px-1 pb-2">
        <div>
          <div className="app-text-overline text-app-ink/55">{t("shell:appBarEditor.eyebrow")}</div>
          <div className="app-text-body-sm font-semibold text-app-ink">{t("shell:appBarEditor.title")}</div>
          <div className="app-text-caption text-app-ink/55">
            {t("shell:appBarEditor.count", {
              count: draftPinnedAppIds.length,
            })}
          </div>
        </div>
        <button
          aria-label={t("common:actions.close")}
          className="rounded-lg p-1.5 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
          onClick={onClose}
          type="button"
        >
          <ChevronDown size={16} className="rotate-90" />
        </button>
      </div>

      <label className="mb-3 flex h-9 shrink-0 items-center gap-2 rounded-lg border border-app-border bg-app-bg px-2.5 text-app-ink/55 focus-within:border-app-accent focus-within:text-app-ink">
        <Search size={16} className="shrink-0" />
        <input
          aria-label={t("shell:appBarEditor.searchPlaceholder")}
          className="app-text-body-sm min-w-0 flex-1 bg-transparent text-app-ink outline-none placeholder:text-app-ink/45"
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder={t("shell:appBarEditor.searchPlaceholder")}
          type="search"
          value={query}
        />
      </label>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        {selectedItems.length > 0 ? (
          <section className="space-y-2">
            <div className="app-text-overline px-1 text-app-ink/50">{t("shell:appBarEditor.pinned")}</div>
            <div className="space-y-1">
              {selectedItems.map((item) => {
                const pinnedIndex = draftPinnedAppIds.indexOf(item.id);
                return (
                  <EditorAppRow
                    item={item}
                    key={item.id}
                    onMovePinnedApp={onMovePinnedApp}
                    onTogglePinnedApp={onTogglePinnedApp}
                    pinned
                    pinnedIndex={pinnedIndex}
                    pinnedTotal={draftPinnedAppIds.length}
                    reorderable
                    t={t}
                  />
                );
              })}
            </div>
          </section>
        ) : null}

        <section className="space-y-2">
          <div className="app-text-overline px-1 text-app-ink/50">{t("shell:appBarEditor.allApps")}</div>
          {filteredCategoryGroups.length > 0 ? (
            <div className="space-y-3">
              {filteredCategoryGroups.map((group) => (
                <section className="space-y-1.5" key={group.id}>
                  <div className="app-text-caption px-1 font-semibold text-app-ink/65">{group.title}</div>
                  <div className="space-y-1">
                    {group.items.map((item) => {
                      const pinned = draftPinnedAppIds.includes(item.id);
                      return (
                        <EditorAppRow
                          item={item}
                          key={item.id}
                          onMovePinnedApp={onMovePinnedApp}
                          onTogglePinnedApp={onTogglePinnedApp}
                          pinned={pinned}
                          pinnedIndex={draftPinnedAppIds.indexOf(item.id)}
                          pinnedTotal={draftPinnedAppIds.length}
                          reorderable={false}
                          t={t}
                        />
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>
          ) : (
            <div className="app-text-body-sm rounded-lg border border-dashed border-app-border bg-app-bg px-3 py-5 text-center text-app-ink/55">
              {t("shell:appBarEditor.noResults")}
            </div>
          )}
        </section>
      </div>

      {appBarLayoutError ? (
        <div
          className="app-text-caption mt-2 rounded-xl border border-app-danger/30 bg-app-danger/10 px-3 py-2 text-app-danger"
          role="alert"
        >
          {appBarLayoutError}
        </div>
      ) : null}

      <div className="mt-3 flex shrink-0 items-center justify-between gap-2 border-t border-app-border pt-3">
        <button
          className="app-text-body-sm inline-flex items-center gap-2 rounded-xl px-3 py-2 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
          onClick={onReset}
          type="button"
        >
          <RotateCcw size={15} />
          <span>{t("shell:appBarEditor.reset")}</span>
        </button>
        <button
          className="app-text-body-sm inline-flex items-center gap-2 rounded-xl bg-app-accent px-3 py-2 font-semibold text-app-accent-fg transition-opacity disabled:cursor-wait disabled:opacity-60"
          disabled={appBarLayoutSaving}
          onClick={onSave}
          type="button"
        >
          <Save size={15} />
          <span>{appBarLayoutSaving ? t("shell:appBarEditor.saving") : t("shell:appBarEditor.save")}</span>
        </button>
      </div>
    </dialog>
  );
}

function EditorAppRow({
  item,
  onMovePinnedApp,
  onTogglePinnedApp,
  pinned,
  pinnedIndex,
  pinnedTotal,
  reorderable,
  t,
}: {
  item: AppBarAppItem;
  onMovePinnedApp: (appId: string, direction: -1 | 1) => void;
  onTogglePinnedApp: (appId: string, checked: boolean) => void;
  pinned: boolean;
  pinnedIndex: number;
  pinnedTotal: number;
  reorderable: boolean;
  t: AppBarTranslator;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-xl border p-2",
        pinned ? "border-app-accent/25 bg-app-bg text-app-ink" : "border-app-border bg-transparent text-app-ink/70",
      )}
    >
      <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
        <input
          aria-label={t("shell:appBarEditor.pinLabel", {
            title: item.title,
          })}
          checked={pinned}
          className="size-4 shrink-0 accent-app-accent"
          onChange={(event) => onTogglePinnedApp(item.id, event.currentTarget.checked)}
          type="checkbox"
        />
        <AppMark mark={item.mark} colorKey={item.id} size="sm" />
        <span className="app-text-body-sm min-w-0 flex-1 truncate">{item.title}</span>
      </label>

      <span className="app-text-micro shrink-0 rounded-full border border-app-border px-1.5 py-0.5 text-app-ink/55">
        {pinned ? t("shell:appBarEditor.pinned") : t("shell:appBarEditor.more")}
      </span>

      {reorderable ? (
        <div className="flex shrink-0 items-center gap-0.5">
          <button
            aria-label={t("shell:appBarEditor.moveUp", {
              title: item.title,
            })}
            className="rounded-md p-1 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:cursor-not-allowed disabled:opacity-30"
            disabled={pinnedIndex <= 0}
            onClick={() => onMovePinnedApp(item.id, -1)}
            type="button"
          >
            <ArrowUp size={14} />
          </button>
          <button
            aria-label={t("shell:appBarEditor.moveDown", {
              title: item.title,
            })}
            className="rounded-md p-1 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:cursor-not-allowed disabled:opacity-30"
            disabled={pinnedIndex < 0 || pinnedIndex >= pinnedTotal - 1}
            onClick={() => onMovePinnedApp(item.id, 1)}
            type="button"
          >
            <ArrowDown size={14} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
