import { Search, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { UserOptionRow } from '../../../platform/users/UserSearchMultiSelect';
import {
  selectUserOptionsForPicker,
  userOptionMatchesQuery,
} from '../../../platform/users/user-option-picker-model';
import type { PmsTaskListMember } from '../api/pms-api';

export type ListAssigneeMenuPlacement = 'above' | 'below';

type MemberUserOption = PmsTaskListMember & { id: string };

/**
 * Which candidate Enter would pick. An explicit arrow-key highlight wins;
 * otherwise the first candidate that actually matches the typed query. The
 * current user is always listed first regardless of the query, so an empty
 * query or a query nobody matches yields -1 rather than assigning them.
 */
export function resolveAssigneeHighlightIndex({
  candidates,
  highlightIndex,
  query,
}: {
  candidates: readonly MemberUserOption[];
  highlightIndex: number | null;
  query: string;
}): number {
  if (
    highlightIndex !== null &&
    highlightIndex >= 0 &&
    highlightIndex < candidates.length
  ) {
    return highlightIndex;
  }
  if (!query.trim()) {
    return -1;
  }
  return candidates.findIndex((candidate) =>
    userOptionMatchesQuery(candidate, query),
  );
}

export function ListAssigneeMenu({
  currentUserId,
  members,
  onClearAssignees,
  onClose,
  onToggleAssignee,
  placement,
  selectedIds,
}: {
  currentUserId?: string | null;
  members: readonly PmsTaskListMember[];
  onClearAssignees: () => void;
  onClose: () => void;
  onToggleAssignee: (userId: string) => void;
  placement: ListAssigneeMenuPlacement;
  selectedIds: ReadonlySet<string>;
}) {
  const { t } = useTranslation('apps');
  const [query, setQuery] = useState('');
  const [highlightIndex, setHighlightIndex] = useState<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus the search box once the menu is actually visible. The inline menu
  // renders hidden until it has measured its position, and browsers ignore
  // focus on hidden elements, so a plain autoFocus never took effect. The
  // search box also remounts when the placement flips (top <-> bottom), so
  // re-run whenever the placement changes.
  useEffect(() => {
    let attempts = 0;
    let frame = 0;
    const focusWhenVisible = () => {
      const input = searchInputRef.current;
      if (!input) return;
      if (getComputedStyle(input).visibility !== 'hidden') {
        input.focus({ preventScroll: true });
        return;
      }
      attempts += 1;
      if (attempts < 30) {
        frame = window.requestAnimationFrame(focusWhenVisible);
      }
    };
    frame = window.requestAnimationFrame(focusWhenVisible);
    return () => window.cancelAnimationFrame(frame);
  }, [placement]);
  const candidates = useMemo(
    () =>
      selectUserOptionsForPicker({
        users: members.map((member) => ({ ...member, id: member.user_id })),
        query,
        currentUserId,
      }),
    [currentUserId, members, query],
  );
  const activeIndex = resolveAssigneeHighlightIndex({
    candidates,
    highlightIndex,
    query,
  });

  useEffect(() => {
    if (activeIndex < 0) return;
    document
      .querySelector<HTMLElement>(
        `[data-testid="list-assignee-menu"] [data-index="${activeIndex}"]`,
      )
      ?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex]);

  const updateQuery = (value: string) => {
    setQuery(value);
    setHighlightIndex(null);
  };

  const commitHighlighted = () => {
    const candidate = candidates[activeIndex];
    if (!candidate) return;
    onToggleAssignee(candidate.user_id);
    // Clear the query so the next name can be typed right away.
    updateQuery('');
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        if (candidates.length === 0) return;
        setHighlightIndex(
          activeIndex < 0
            ? 0
            : Math.min(activeIndex + 1, candidates.length - 1),
        );
        return;
      case 'ArrowUp':
        event.preventDefault();
        if (candidates.length === 0) return;
        setHighlightIndex(activeIndex <= 0 ? 0 : activeIndex - 1);
        return;
      case 'Enter':
        event.preventDefault();
        commitHighlighted();
        return;
      case 'Escape':
        event.preventDefault();
        onClose();
        return;
      default:
    }
  };

  const searchBox = (
    <div className="shrink-0 px-2 py-1">
      <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1.5">
        <Search size={13} className="shrink-0 text-app-ink/40" />
        <input
          ref={searchInputRef}
          aria-label={t('pms.searchUser')}
          className="app-text-body-sm min-w-0 flex-1 bg-transparent text-app-ink outline-none placeholder:text-app-ink/35"
          onChange={(event) => updateQuery(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('pms.searchUser')}
          value={query}
        />
        {query ? (
          <button
            aria-label={t('pms.filter.clearSearch')}
            className="text-app-ink/40 hover:text-app-ink"
            onClick={() => updateQuery('')}
            type="button"
          >
            <X size={11} />
          </button>
        ) : null}
      </div>
    </div>
  );

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-placement={placement}
      data-testid="list-assignee-menu"
    >
      {/* Keep the search box on the side that touches the task row. */}
      {placement === 'below' ? searchBox : null}
      <div className="custom-scrollbar min-h-0 flex-1 overflow-y-auto">
        <button
          className="app-text-control-sm w-full px-3 py-1.5 text-left text-app-ink/50 hover:bg-app-surface-hover"
          onClick={onClearAssignees}
          type="button"
        >
          {t('pms.taskDetail.unassigned')}
        </button>
        {candidates.map((member, index) => (
          <div
            key={member.user_id}
            className={`relative ${index === activeIndex ? 'bg-app-surface-hover' : ''}`}
            data-highlighted={index === activeIndex ? 'true' : undefined}
            data-index={index}
          >
            <input
              aria-label={member.full_name}
              checked={selectedIds.has(member.user_id)}
              className="pointer-events-none absolute right-3 top-1/2 size-3.5 -translate-y-1/2 rounded accent-app-accent"
              readOnly
              type="checkbox"
            />
            <UserOptionRow
              currentUserId={currentUserId}
              currentUserLabel={t('pms.taskDetail.me')}
              density="compact"
              onClick={() => onToggleAssignee(member.user_id)}
              selected={false}
              user={member}
            />
          </div>
        ))}
        {candidates.length === 0 ? (
          <p className="app-text-caption px-3 py-2 text-app-ink/40">
            {t('pms.noMatchingUsers')}
          </p>
        ) : null}
      </div>
      {placement === 'above' ? searchBox : null}
    </div>
  );
}
