import { useMemo, useState } from 'react';
import { Button, Select } from '@miy/ui';
import { useTranslation } from 'react-i18next';

import type { PmsUserSummary } from '../api/pms-api';
import {
  spaceDepartmentCandidates,
  spaceDepartmentOptions,
} from './space-members-modal-model';

/**
 * Pick a department, untick whoever should stay out, and add the rest at once
 * — like choosing recipients by team in the groupware approval form.
 */
export function SpaceDepartmentInvite({
  allUsers,
  busy,
  locale,
  memberIds,
  onAdd,
}: {
  allUsers: readonly PmsUserSummary[];
  busy: boolean;
  locale: string;
  memberIds: ReadonlySet<string>;
  /** Resolves true when the users were added. */
  onAdd: (userIds: string[]) => Promise<boolean>;
}) {
  const { t } = useTranslation('apps');
  const [departmentId, setDepartmentId] = useState('');
  const [excludedIds, setExcludedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const departments = useMemo(
    () => spaceDepartmentOptions({ allUsers, locale, memberIds }),
    [allUsers, locale, memberIds],
  );
  const candidates = useMemo(
    () =>
      departmentId
        ? spaceDepartmentCandidates({
            allUsers,
            departmentId,
            locale,
            memberIds,
          })
        : [],
    [allUsers, departmentId, locale, memberIds],
  );
  const selectedIds = candidates
    .filter((candidate) => !excludedIds.has(candidate.id))
    .map((candidate) => candidate.id);

  function chooseDepartment(nextDepartmentId: string) {
    setDepartmentId(nextDepartmentId);
    setExcludedIds(new Set());
  }

  function toggleCandidate(userId: string) {
    setExcludedIds((current) => {
      const next = new Set(current);
      if (next.has(userId)) next.delete(userId);
      else next.add(userId);
      return next;
    });
  }

  async function addSelected() {
    if (await onAdd(selectedIds)) chooseDepartment('');
  }

  return (
    <section className="space-y-2">
      <h3 className="app-text-overline text-app-ink/50">
        {t('pms.spaceMembers.department.title')}
      </h3>
      {departments.length === 0 && !departmentId ? (
        <p className="app-text-caption text-app-ink/40">
          {t('pms.spaceMembers.department.empty')}
        </p>
      ) : (
        <label className="flex flex-col gap-1">
          <span className="sr-only">
            {t('pms.spaceMembers.department.select')}
          </span>
          <Select
            className="w-full"
            disabled={busy}
            onValueChange={chooseDepartment}
            options={departments.map((department) => ({
              label: t('pms.spaceMembers.department.option', {
                count: department.candidateCount,
                name: department.name,
              }),
              value: department.id,
            }))}
            placeholder={t('pms.spaceMembers.department.select')}
            value={departmentId}
          />
        </label>
      )}
      {candidates.length > 0 ? (
        <div className="rounded-md border border-app-border bg-app-surface">
          <ul className="max-h-48 divide-y divide-app-border overflow-y-auto custom-scrollbar">
            {candidates.map((candidate) => (
              <li key={candidate.id}>
                <label className="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-app-surface-hover">
                  <input
                    checked={!excludedIds.has(candidate.id)}
                    className="size-3.5 rounded accent-app-accent"
                    onChange={() => toggleCandidate(candidate.id)}
                    type="checkbox"
                  />
                  <span className="app-text-body min-w-0 flex-1 truncate text-app-ink">
                    {candidate.full_name}
                  </span>
                  <span className="app-text-caption truncate text-app-ink/40">
                    {candidate.email}
                  </span>
                </label>
              </li>
            ))}
          </ul>
          <div className="flex items-center justify-between gap-3 border-t border-app-border px-3 py-2">
            <p className="app-text-caption text-app-ink/40">
              {t('pms.spaceMembers.department.hint')}
            </p>
            <div className="flex shrink-0 items-center gap-2">
              <Button
                onClick={() => chooseDepartment('')}
                size="dense"
                variant="ghost"
              >
                {t('common:actions.cancel')}
              </Button>
              <Button
                disabled={busy || selectedIds.length === 0}
                onClick={() => void addSelected()}
                size="dense"
                variant="primary"
              >
                {t('pms.spaceMembers.department.add', {
                  count: selectedIds.length,
                })}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
