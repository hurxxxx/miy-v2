import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PanelRight, Star } from "lucide-react";
import { panelKey, type OpenPanel } from "./SidePanel";

/**
 * The header's side panels: pinned panels as buttons, and a menu of every panel
 * the apps declare with a pin toggle. The pin is a personal preference.
 * Nothing shows when no app has a panel.
 */
export function PanelMenu({
  panels,
  pinned,
  current,
  saving,
  onOpen,
  failed,
  onSetPin,
}: {
  panels: OpenPanel[];
  pinned: string[];
  current: string | null;
  saving: boolean;
  onOpen: (open: OpenPanel) => void;
  failed: boolean;
  onSetPin: (id: string, pinned: boolean) => void;
}) {
  const { t } = useTranslation("portal");
  const [expanded, setExpanded] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const pinnedPanels = pinned.flatMap((id) => panels.filter((open) => panelKey(open) === id));
  const hiddenPins = pinned.filter((id) => !pinnedPanels.some((open) => panelKey(open) === id));
  const ordered = [...pinnedPanels, ...panels.filter((open) => !pinned.includes(panelKey(open)))];
  useEffect(() => {
    if (!expanded) return;
    const outside = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setExpanded(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setExpanded(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [expanded]);
  if (panels.length === 0 && hiddenPins.length === 0) return null;
  return (
    <div className="panel-menu" ref={root}>
      {pinnedPanels.length > 0 && (
        <div className="panel-shortcuts">
          {pinnedPanels.map((open) => (
            <button
              key={panelKey(open)}
              type="button"
              className="panel-chip"
              aria-pressed={current === panelKey(open)}
              title={`${open.panel.title} · ${open.app.name}`}
              onClick={() => onOpen(open)}
            >
              {open.panel.title}
            </button>
          ))}
        </div>
      )}
      <button
        ref={trigger}
        type="button"
        className="notification-bell"
        aria-label={t("panels.menu")}
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
      >
        <PanelRight size={18} aria-hidden="true" />
      </button>
      {expanded && (
        <ul className="panel-menu-list" aria-label={t("panels.menu")}>
          {ordered.map((open) => {
            const isPinned = pinned.includes(panelKey(open));
            return (
              <li key={panelKey(open)}>
                <button
                  type="button"
                  className="panel-menu-open"
                  onClick={() => {
                    setExpanded(false);
                    onOpen(open);
                  }}
                >
                  <strong>{open.panel.title}</strong>
                  <span>{open.app.name}</span>
                </button>
                <button
                  type="button"
                  className="panel-menu-pin"
                  aria-pressed={isPinned}
                  aria-label={t(isPinned ? "panels.unpin" : "panels.pin", { title: open.panel.title })}
                  disabled={saving}
                  onClick={() => onSetPin(panelKey(open), !isPinned)}
                >
                  <Star size={14} aria-hidden="true" fill={isPinned ? "currentColor" : "none"} />
                </button>
              </li>
            );
          })}
          {hiddenPins.map((id) => (
            <li key={id}>
              <span className="panel-menu-open">
                <strong>{id}</strong>
                <span>{t("panels.unavailable")}</span>
              </span>
              <button
                type="button"
                className="panel-menu-pin"
                aria-label={t("panels.unpin", { title: id })}
                disabled={saving}
                onClick={() => onSetPin(id, false)}
              >
                <Star size={14} aria-hidden="true" fill="currentColor" />
              </button>
            </li>
          ))}
          {failed && <li className="panel-menu-open" role="status">{t("panels.pinFailed")}</li>}
        </ul>
      )}
    </div>
  );
}
