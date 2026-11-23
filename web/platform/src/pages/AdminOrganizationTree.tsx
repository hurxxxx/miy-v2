import { ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Group } from "@miy/sdk";
import { Badge } from "../ui";
import "./admin-users.css";

type Props = {
  groups: Group[];
  selected: string | null;
  onSelect: (id: string) => void;
  ariaLabel?: string;
};

export function AdminOrganizationTree({ groups, selected, onSelect, ariaLabel }: Props) {
  const { t, i18n } = useTranslation("console");
  const locale = i18n.resolvedLanguage;
  const { byId, byParent } = useMemo(() => {
    const organizations = groups.filter((group) => group.source === "hr");
    const byId = new Map(organizations.map((group) => [group.id, group]));
    const byParent = new Map<string | null, Group[]>();
    for (const group of organizations.sort((a, b) => a.name.localeCompare(b.name, locale) || a.id.localeCompare(b.id))) {
      const parent = group.parentId && byId.has(group.parentId) ? group.parentId : null;
      const siblings = byParent.get(parent) ?? [];
      siblings.push(group);
      byParent.set(parent, siblings);
    }
    return { byId, byParent };
  }, [groups, locale]);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    let ancestor = selected ? byId.get(selected)?.parentId : null;
    const parents = new Set<string>();
    while (ancestor && !parents.has(ancestor)) {
      parents.add(ancestor);
      ancestor = byId.get(ancestor)?.parentId;
    }
    if (parents.size) setExpanded((current) => new Set([...current, ...parents]));
  }, [byId, selected]);

  function toggle(id: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function renderBranches(parent: string | null) {
    const branches = byParent.get(parent);
    if (!branches?.length) return null;
    return (
      <ul className="admin-org-branches">
        {branches.map((group) => {
          const open = expanded.has(group.id);
          return (
            <li key={group.id}>
              <div className="admin-org-branch">
                {byParent.has(group.id) ? (
                  <button
                    type="button"
                    className="admin-org-disclosure"
                    aria-expanded={open}
                    aria-label={t(open ? "users.collapseOrganization" : "users.expandOrganization", { name: group.name })}
                    onClick={() => toggle(group.id)}
                  >
                    {open ? <ChevronDown size={16} aria-hidden /> : <ChevronRight size={16} aria-hidden />}
                  </button>
                ) : <span className="admin-org-disclosure" />}
                <button type="button" className="admin-org-choice" aria-current={selected === group.id || undefined} onClick={() => onSelect(group.id)}>
                  <span>{group.name}</span>
                  {!group.active && <Badge tone="warning">{t("users.inactiveOrganization")}</Badge>}
                </button>
              </div>
              {open && renderBranches(group.id)}
            </li>
          );
        })}
      </ul>
    );
  }

  return byId.size > 0 ? (
    <nav className="admin-org-tree" aria-label={ariaLabel ?? t("users.organizationTree")}>
      {renderBranches(null)}
    </nav>
  ) : <p className="admin-users-muted">{t("users.noOrganizations")}</p>;
}
