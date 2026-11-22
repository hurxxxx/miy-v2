import { cleanup, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderWithRuntime } from '../../../test-utils/runtime';
import type { PmsSpace, PmsTaskList } from '../api/pms-api';
import { PmsGlobalOverview } from './PmsGlobalOverview';

afterEach(() => {
  cleanup();
});

// miy: one PMS per installation, so the overview links by space and list id
// (`/spaces/<id>`, `/lists/<id>`) and every PMS user may create a space.
function renderOverview(
  listOverrides: Partial<PmsTaskList> = {},
  { withSpaces = true } = {},
) {
  const allSpaces = [
    {
      id: 'space-a',
      workspace_id: 'space-a',
      workspace_key: 'pms-a',
      key: 'pms-a',
      name: 'Project A',
      description: '',
      member_count: 1,
      current_user_role: 'member',
      created_at: '',
      updated_at: '',
    },
  ] satisfies PmsSpace[];
  const spaces = withSpaces ? allSpaces : [];
  const taskLists = [
    {
      id: 'list-a',
      name: 'Backlog',
      team_id: 'space-a',
      archived: false,
      open_task_count: 3,
      overdue_task_count: 1,
      task_count: 4,
      active_task_count: 4,
      // The API sends a 0..1 weighted ratio on a different base; the screen
      // must not print it as a percentage.
      progress: 0.38,
      ...listOverrides,
    },
  ] as PmsTaskList[];
  return renderWithRuntime(
    <PmsGlobalOverview
      spaces={spaces}
      taskLists={taskLists}
      onCreateSpace={vi.fn()}
    />,
  );
}

describe('PMS global overview', () => {
  it('links every resource to its returned owner space without selecting the first space', async () => {
    renderOverview();
    expect(
      (await screen.findByRole('link', { name: 'Project A' })).getAttribute(
        'href',
      ),
    ).toBe('/spaces/space-a');
    expect(
      screen.getByRole('link', { name: /Backlog/ }).getAttribute('href'),
    ).toBe('/lists/list-a');
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('shows list progress on the same open/total base as the space overview', async () => {
    renderOverview();
    const listLink = await screen.findByRole('link', { name: /Backlog/ });
    // 4 tasks, 3 still open: one in four is done.
    expect(listLink.textContent).toMatch(/\b25%/);
  });

  it('leaves an archived unfinished task out of the count and the percent', async () => {
    // The only task was archived while still todo: it is not in use.
    renderOverview({
      task_count: 1,
      active_task_count: 0,
      open_task_count: 0,
      overdue_task_count: 0,
    });
    const listLink = await screen.findByRole('link', { name: /Backlog/ });
    expect(listLink.textContent).toContain('0개 태스크 · 0% 완료');
  });

  it('offers space creation to an ordinary PMS user', async () => {
    renderOverview();
    await screen.findByRole('link', { name: 'Project A' });
    expect(
      screen.getByRole('button', { name: '워크스페이스 만들기' }),
    ).toBeTruthy();
  });

  it('tells a person without spaces to create one', async () => {
    renderOverview({}, { withSpaces: false });
    expect(await screen.findByText(/새 워크스페이스를 만들고/)).toBeTruthy();
    expect(
      screen.getByRole('button', { name: '워크스페이스 만들기' }),
    ).toBeTruthy();
  });

  it('navigates through the runtime when a space link is clicked', async () => {
    const { runtime } = renderOverview();
    (await screen.findByRole('link', { name: 'Project A' })).click();
    expect(runtime.navigate).toHaveBeenCalledWith('/spaces/space-a', {
      replace: undefined,
    });
  });
});
