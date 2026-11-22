import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, cn } from "@miy/ui";
import { platformShortcut } from "@miy/frame-protocol";

/**
 * Ctrl/⌘+K command palette. AI-DO has none; this follows the plan's first
 * version (P1): go to an app or to a platform page. Commands are links the
 * person could already click; the palette grants nothing.
 */
export type Command = {
  id: string;
  label: string;
  /** Shown after the label, e.g. the app ID or "설정". */
  hint?: string;
  run: () => void;
};

function matches(command: Command, query: string) {
  const needle = query.trim().toLowerCase();
  return !needle || `${command.label} ${command.hint ?? ""}`.toLowerCase().includes(needle);
}

/** Raised when an app frame forwards the shortcut (app/AppFrame.tsx). */
export const COMMAND_PALETTE_EVENT = "miy:command-palette";

/** Opens on Ctrl/⌘+K in the portal document; frames forward the same keys. */
export function useCommandPaletteShortcut(open: () => void) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!platformShortcut(event) || event.repeat) return;
      event.preventDefault();
      open();
    };
    document.addEventListener("keydown", onKey);
    window.addEventListener(COMMAND_PALETTE_EVENT, open);
    return () => {
      document.removeEventListener("keydown", onKey);
      window.removeEventListener(COMMAND_PALETTE_EVENT, open);
    };
  }, [open]);
}

export function CommandPalette({
  open,
  onOpenChange,
  commands,
  onSearch,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  commands: Command[];
  /** Business portals: the typed text can also go to the integrated search (M12). */
  onSearch?: (query: string) => void;
}) {
  const { t } = useTranslation("portal");
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const shown = useMemo(() => {
    const found = commands.filter((command) => matches(command, query));
    const text = query.trim();
    return onSearch && text
      ? [...found, { id: "search-apps", label: t("commandPalette.searchApps", { query: text }), run: () => onSearch(text) }]
      : found;
  }, [commands, query, onSearch, t]);
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
  }, [open]);
  useEffect(() => setActive(0), [query]);

  function choose(command: Command | undefined) {
    if (!command) return;
    onOpenChange(false);
    command.run();
  }
  function onKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (shown.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((current) => (current + step + shown.length) % shown.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(shown[active]);
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("commandPalette.title")}
      description={t("commandPalette.description")}
      closeLabel={t("commandPalette.close")}
      maxWidth="max-w-xl"
    >
      <input
        ref={input}
        autoFocus
        className="app-field-input"
        role="combobox"
        aria-expanded="true"
        aria-controls="command-palette-list"
        aria-activedescendant={shown[active] ? `command-${shown[active].id}` : undefined}
        aria-label={t("commandPalette.search")}
        placeholder={t("commandPalette.placeholder")}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <ul id="command-palette-list" role="listbox" aria-label={t("commandPalette.results")} className="mt-3 max-h-80 overflow-y-auto">
        {shown.map((command, index) => (
          <li
            key={command.id}
            id={`command-${command.id}`}
            role="option"
            aria-selected={index === active}
            className={cn(
              "app-menu-item cursor-pointer rounded-md",
              index === active && "bg-app-surface-hover",
            )}
            onMouseEnter={() => setActive(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(command)}
          >
            <span className="min-w-0 flex-1 truncate">{command.label}</span>
            {command.hint ? <span className="app-text-caption text-app-ink/55">{command.hint}</span> : null}
          </li>
        ))}
        {shown.length === 0 ? (
          <li role="option" aria-selected="false" aria-disabled="true" className="app-menu-item app-menu-item-muted">
            {t("commandPalette.empty")}
          </li>
        ) : null}
      </ul>
    </Dialog>
  );
}
