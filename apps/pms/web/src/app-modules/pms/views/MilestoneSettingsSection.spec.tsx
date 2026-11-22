import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { FeedbackProvider } from '@miy/ui';
import { expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n';
import type { AppApi } from '../../../platform/runtime/runtime-context';
import { createTestRuntime, renderWithRuntime } from '../../../test-utils/runtime';
import type { PmsMilestone } from '../api/pms-api';
import { MilestoneSettingsSection } from './MilestoneSettingsSection';

it('lets a manager edit and delete a milestone from a later server page', async () => {
  await i18n.changeLanguage('en-US');
  let milestones: PmsMilestone[] = Array.from({ length: 101 }, (_, index) => ({
    id: `milestone-${index + 1}`,
    title: `Milestone ${index + 1}`,
    list_id: 'list-1',
    list_name: null,
    team_id: 'space-1',
    scope: 'list',
    description: '',
    status: 'planned',
    start_date: null,
    due_date: null,
    sort_order: index,
    progress: 0,
    task_count: 0,
    completed_task_count: 0,
    updated_at: '2026-11-22T00:00:00Z',
  }));
  const itemPath = '/v1/pms/milestones/milestone-101';
  const api = vi.fn(async (path: string, init?: RequestInit) => {
    const url = new URL(path, 'http://app.invalid');
    if (url.pathname === '/v1/pms/lists/list-1/milestones') {
      const page = Number(url.searchParams.get('page') ?? '1');
      const pageSize = Number(url.searchParams.get('page_size') ?? '20');
      const start = (page - 1) * pageSize;
      return {
        items: milestones.slice(start, start + pageSize),
        total: milestones.length,
        page,
        page_size: pageSize,
      };
    }
    if (path === itemPath && init?.method === 'PATCH') {
      const change = JSON.parse(String(init.body)) as Partial<PmsMilestone>;
      milestones = milestones.map((milestone) =>
        milestone.id === 'milestone-101' ? { ...milestone, ...change } : milestone,
      );
      return milestones.at(-1);
    }
    if (path === itemPath && init?.method === 'DELETE') {
      milestones = milestones.filter((milestone) => milestone.id !== 'milestone-101');
      return undefined;
    }
    throw new Error(`Unexpected PMS request: ${path}`);
  });
  const onMilestonesChanged = vi.fn();

  renderWithRuntime(
    <FeedbackProvider labels={{ close: 'Close', item: 'Notice', region: 'Notices' }}>
      <MilestoneSettingsSection
        canManage
        onMilestonesChanged={onMilestonesChanged}
        spaceId="space-1"
        taskListId="list-1"
      />
    </FeedbackProvider>,
    { runtime: createTestRuntime({ api: api as unknown as AppApi }) },
  );

  fireEvent.click(await screen.findByRole('button', { name: 'Edit Milestone 101' }));
  expect(onMilestonesChanged).toHaveBeenLastCalledWith(milestones);
  fireEvent.change(screen.getByRole('textbox', { name: 'Edit Milestone 101' }), {
    target: { value: 'Updated milestone' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Save' }));

  fireEvent.click(await screen.findByRole('button', { name: 'Delete Updated milestone' }));
  const dialog = screen.getByRole('dialog', { name: 'Delete milestone' });
  fireEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));

  await waitFor(() => expect(onMilestonesChanged).toHaveBeenLastCalledWith(milestones));
  expect(milestones).toHaveLength(100);
  expect(screen.queryByText('Updated milestone')).toBeNull();
  expect(screen.getByText('Milestone 100')).toBeTruthy();
});
