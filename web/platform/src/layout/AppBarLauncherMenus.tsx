// Ported from AI-DO apps/web/src/components/layout/AppBarLauncherMenus.tsx
// (0002af062): LauncherPopover, CategoryItemSearch, LauncherGrid,
// LauncherLink, LauncherEditorButton and EmptyLauncherMessage. AI-DO opens a
// favourites menu and one menu per category from rail buttons; miy's rail
// already lists the pinned apps, so one launcher button opens a single menu of
// every app the person can open, grouped by the administrator's categories,
// with the editor reached from it as in AI-DO's favourites menu.
import { Check, LayoutGrid, Search, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { Link } from "@tanstack/react-router";
import { cn } from "@miy/ui";

import { AppMark } from "../ui";
import { launcherIconForKey } from "./launcher-icons";
import {
  buildCategoryGroups,
  filterCategoryGroups,
  isLauncherPathActive,
  launcherGridColumnCount,
  UNCATEGORIZED_GROUP_ID,
  type AppBarAppItem,
  type AppBarTranslator,
  type LauncherGridColumnCount,
} from "./app-bar-model";
import type { LauncherCategoriesState } from "./useAppBarController";

export const APP_LAUNCHER_MENU_ID = "app-launcher-menu";

const launcherGridColumnClassName: Record<LauncherGridColumnCount, string> = {
  3: "grid-cols-3",
  4: "grid-cols-4",
  5: "grid-cols-5",
};

const launcherPopoverWidthClassName: Record<LauncherGridColumnCount, string> = {
  3: "w-[28rem]",
  4: "w-[36rem]",
  5: "w-[44rem]",
};

/** The rail entry: a miy rail item (same look as the app links) that opens the menu. */
export function AppLauncherButton({
  buttonRef,
  onToggle,
  open,
  t,
}: {
  buttonRef: RefObject<HTMLButtonElement | null>;
  onToggle: () => void;
  open: boolean;
  t: AppBarTranslator;
}) {
  const label = t("shell:appBar.launcher");
  const short = t("shell:appBarEditor.allApps");
  return (
    <button
      aria-controls={open ? APP_LAUNCHER_MENU_ID : undefined}
      aria-expanded={open}
      aria-haspopup="menu"
      aria-label={label}
      className="rail-item rail-launcher"
      onClick={onToggle}
      ref={buttonRef}
      type="button"
    >
      <span className="rail-icon">
        <LayoutGrid aria-hidden size={20} strokeWidth={1.8} />
      </span>
      <span className="rail-label rail-label-short">{short}</span>
      <span className="rail-label rail-label-full">{label}</span>
    </button>
  );
}

export function AppBarLauncherMenu({
  categories,
  contextLabel,
  currentPathname,
  appsStatus,
  items,
  linkFor,
  menuRef,
  onClose,
  onOpenEditor,
  t,
}: {
  categories: LauncherCategoriesState;
  contextLabel: string;
  currentPathname: string;
  /** The person's installation list: still loading, failed before any answer, or read. */
  appsStatus: "loading" | "error" | "ready";
  items: AppBarAppItem[];
  linkFor: (appId: string) => string;
  menuRef: RefObject<HTMLDivElement | null>;
  onClose: () => void;
  onOpenEditor: () => void;
  t: AppBarTranslator;
}) {
  const [query, setQuery] = useState("");
  const groups = useMemo(
    () => buildCategoryGroups({ categories: categories.categories ?? [], items, t }),
    [categories.categories, items, t],
  );
  const filteredGroups = useMemo(() => filterCategoryGroups(groups, query), [groups, query]);
  const columnCount = launcherGridColumnCount(items.length);
  // The first read of the arrangement: wait for it rather than regroup the menu.
  const waiting = appsStatus === "loading" || (categories.categories === null && categories.status === "loading");

  return (
    <LauncherPopover
      columnCount={columnCount}
      contextLabel={contextLabel}
      iconKey="layout"
      menuRef={menuRef}
      title={t("shell:appBar.launcher")}
    >
      {waiting ? (
        <EmptyLauncherMessage>{t("shell:appLauncher.loading")}</EmptyLauncherMessage>
      ) : appsStatus === "error" ? (
        <EmptyLauncherMessage>{t("portal:shell.appsLoadFailed")}</EmptyLauncherMessage>
      ) : items.length === 0 ? (
        <EmptyLauncherMessage>{t("shell:appLauncher.empty")}</EmptyLauncherMessage>
      ) : (
        <>
          <label className="flex h-9 shrink-0 items-center gap-2 rounded-lg border border-app-border bg-app-bg px-2.5 text-app-ink/55 focus-within:border-app-accent focus-within:text-app-ink">
            <Search size={16} className="shrink-0" />
            <input
              aria-label={t("shell:appBar.searchApps")}
              className="app-text-body-sm min-w-0 flex-1 bg-transparent text-app-ink outline-none placeholder:text-app-ink/45"
              onChange={(event) => setQuery(event.currentTarget.value)}
              placeholder={t("shell:appBar.searchApps")}
              type="search"
              value={query}
            />
          </label>
          {categories.status === "error" ? (
            <p className="app-text-caption px-1 text-app-ink/55" role="status">
              {t("launcher:menu.categoriesLoadFailed")}
            </p>
          ) : null}
          {filteredGroups.length > 0 ? (
            filteredGroups.map((group) => {
              const GroupIcon = launcherIconForKey(group.iconKey);
              // Without any category there is nothing to name the one group by.
              const titled = !(groups.length === 1 && group.id === UNCATEGORIZED_GROUP_ID);
              return (
                <section aria-label={group.title} className="space-y-1.5" key={group.id} role="group">
                  {titled ? (
                    <div className="app-text-caption flex items-center gap-1.5 px-1 font-semibold text-app-ink/65">
                      <GroupIcon aria-hidden size={14} className="shrink-0" />
                      <span className="truncate">{group.title}</span>
                    </div>
                  ) : null}
                  <LauncherGrid columnCount={columnCount}>
                    {group.items.map((item) => {
                      const link = linkFor(item.id);
                      return (
                        <LauncherLink
                          active={isLauncherPathActive({ currentPathname, link })}
                          item={item}
                          key={item.id}
                          onClose={onClose}
                          to={link}
                          t={t}
                        />
                      );
                    })}
                  </LauncherGrid>
                </section>
              );
            })
          ) : (
            <EmptyLauncherMessage>{t("shell:appBar.emptySearch")}</EmptyLauncherMessage>
          )}
        </>
      )}
      <LauncherEditorButton onOpenEditor={onOpenEditor} t={t} />
    </LauncherPopover>
  );
}

// Arrow keys move between the menu's controls; Tab keeps working as usual.
function moveFocus(event: KeyboardEvent<HTMLDivElement>) {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
  // Home and End keep moving the caret in the search field.
  if ((event.key === "Home" || event.key === "End") && event.target instanceof HTMLInputElement) return;
  const controls = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>('input, [role="menuitem"], button:not([disabled])'),
  );
  if (controls.length === 0) return;
  event.preventDefault();
  const index = controls.indexOf(document.activeElement as HTMLElement);
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? controls.length - 1
        : event.key === "ArrowDown"
          ? (index + 1) % controls.length
          : (index - 1 + controls.length) % controls.length;
  controls[next]?.focus();
}

function LauncherPopover({
  children,
  columnCount = 3,
  contextLabel,
  iconKey,
  menuRef,
  title,
}: {
  children: ReactNode;
  columnCount?: LauncherGridColumnCount;
  contextLabel: string | null;
  iconKey: string;
  menuRef: RefObject<HTMLDivElement | null>;
  title: string;
}) {
  const Icon = launcherIconForKey(iconKey);
  // Keyboard users land in the menu they opened, without raising a phone keyboard.
  useEffect(() => {
    menuRef.current?.focus();
  }, [menuRef]);
  return (
    <div
      aria-label={title}
      className={cn(
        "app-launcher fixed left-[5.75rem] top-4 z-50 flex max-h-[calc(100dvh-2rem)] max-w-[calc(100vw-7rem)] flex-col gap-3 overflow-hidden rounded-xl border border-app-border bg-app-surface p-3 text-app-ink shadow-2xl outline-none",
        "max-md:left-3 max-md:right-3 max-md:top-[4.25rem] max-md:max-h-[calc(100dvh-5rem)] max-md:w-auto max-md:max-w-none",
        launcherPopoverWidthClassName[columnCount],
      )}
      id={APP_LAUNCHER_MENU_ID}
      onKeyDown={moveFocus}
      ref={menuRef}
      role="menu"
      tabIndex={-1}
    >
      <div className="flex shrink-0 items-center gap-3 border-b border-app-border pb-3">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-lg border border-app-border bg-app-bg text-app-accent">
          <Icon size={21} />
        </span>
        <div className="min-w-0">
          <div className="app-text-body-sm font-semibold text-app-ink">{title}</div>
          {contextLabel ? <div className="app-text-caption truncate text-app-ink/55">{contextLabel}</div> : null}
        </div>
      </div>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">{children}</div>
    </div>
  );
}

function LauncherGrid({ children, columnCount }: { children: ReactNode; columnCount: LauncherGridColumnCount }) {
  return (
    <div className={cn("grid gap-2 max-md:grid-cols-3", launcherGridColumnClassName[columnCount])}>{children}</div>
  );
}

function LauncherLink({
  active,
  item,
  onClose,
  to,
  t,
}: {
  active: boolean;
  item: AppBarAppItem;
  onClose: () => void;
  to: string;
  t: AppBarTranslator;
}) {
  return (
    <Link
      aria-current={active ? "page" : undefined}
      aria-label={t("shell:appLauncher.openApp", { title: item.title })}
      className={cn(
        "group/app relative flex h-24 min-w-0 flex-col items-center justify-center gap-1.5 rounded-lg border px-2 py-2 text-center text-app-ink transition-colors hover:border-app-border hover:bg-app-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-app-accent/40",
        active ? "border-app-accent/35 bg-app-bg text-app-accent" : "border-transparent",
      )}
      onClick={onClose}
      role="menuitem"
      to={to as "/"}
    >
      <AppMark mark={item.mark} colorKey={item.id} />
      <span className="app-text-caption line-clamp-2 max-w-full font-medium leading-snug">{item.title}</span>
      {active ? <Check size={15} className="absolute right-1.5 top-1.5 shrink-0" /> : null}
    </Link>
  );
}

function LauncherEditorButton({ onOpenEditor, t }: { onOpenEditor: () => void; t: AppBarTranslator }) {
  return (
    <button
      className="app-text-body-sm flex w-full items-center justify-center gap-2 rounded-lg border border-app-border bg-app-bg px-3 py-2 font-medium text-app-ink transition-colors hover:border-app-accent/40 hover:bg-app-surface-hover"
      onClick={onOpenEditor}
      type="button"
    >
      <SlidersHorizontal size={16} className="text-app-ink/55" />
      <span>{t("shell:appBarEditor.open")}</span>
    </button>
  );
}

function EmptyLauncherMessage({ children }: { children: ReactNode }) {
  return (
    <div className="app-text-body-sm rounded-lg border border-dashed border-app-border bg-app-bg px-3 py-5 text-center text-app-ink/55">
      {children}
    </div>
  );
}
