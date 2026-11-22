import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithRuntime, TEST_USER } from '../../../test-utils/runtime';

import { bulkUpdateTasks } from '../api/pms-api';
import { BulkActionBar } from './BulkActionBar';

const { feedback, translate } = vi.hoisted(() => ({
  feedback: { error: vi.fn(), success: vi.fn(), warning: vi.fn() },
  translate: (key: string, values?: Record<string, unknown>) =>
    values ? `${key}:${JSON.stringify(values)}` : key,
}));

vi.mock('@miy/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@miy/ui')>()),
  useFeedback: () => feedback,
}));

vi.mock('../api/pms-api', () => ({
  bulkUpdateTasks: vi.fn(),
}));

vi.mock('./TaskCopyDialog', () => ({
  TaskCopyDialog: () => null,
}));

vi.mock('react-i18next', () => ({
  // DateInput reads the language to parse typed dates.
  useTranslation: () => ({
    i18n: { language: 'ko-KR', resolvedLanguage: 'ko-KR' },
    t: translate,
  }),
}));

function renderBar(selectedCount: number) {
  const onDone = vi.fn();
  // miy: frame runtime and `/v1/pms/me` user instead of AI-DO's useAuth().
  renderWithRuntime(
    <BulkActionBar
      workspaceSlug="pms-space-a"
      labels={[]}
      members={[]}
      onDeselectAll={vi.fn()}
      onDone={onDone}
      onSelectAll={vi.fn()}
      selectedIds={
        new Set(Array.from({ length: selectedCount }, (_, i) => `t${i}`))
      }
      taskListId="list"
      totalCount={selectedCount}
    />,
    { user: { ...TEST_USER, id: 'me' } },
  );
  return { onDone };
}

describe('BulkActionBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('warns instead of updating when more than 200 tasks are selected', () => {
    const { onDone } = renderBar(201);

    fireEvent.click(
      screen.getByRole('button', { name: 'common:actions.archive' }),
    );

    expect(feedback.warning).toHaveBeenCalledWith(
      'pms.bulk.tooMany:{"max":200,"selected":201}',
    );
    expect(bulkUpdateTasks).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
  });

  it('warns before asking to confirm a delete of more than 200 tasks', () => {
    renderBar(250);

    fireEvent.click(
      screen.getByRole('button', { name: 'common:actions.delete' }),
    );

    expect(feedback.warning).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByRole('button', {
        name: 'pms.bulk.deleteConfirm:{"count":250}',
      }),
    ).toBeNull();
  });

  it('names how many tasks a confirmed delete removes', async () => {
    vi.mocked(bulkUpdateTasks).mockResolvedValue({
      updated_count: 0,
      deleted_count: 178,
    });
    const { onDone } = renderBar(178);

    fireEvent.click(
      screen.getByRole('button', { name: 'common:actions.delete' }),
    );
    expect(screen.getByText('pms.bulk.deleteIrreversible')).toBeTruthy();
    fireEvent.click(
      screen.getByRole('button', {
        name: 'pms.bulk.deleteConfirm:{"count":178}',
      }),
    );

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(bulkUpdateTasks).toHaveBeenCalledWith(
      expect.any(Function),
      'list',
      expect.objectContaining({ delete: true }),
      'pms-space-a',
    );
    expect(vi.mocked(bulkUpdateTasks).mock.calls[0][2].task_ids).toHaveLength(
      178,
    );
  });

  it('updates up to 200 tasks', async () => {
    vi.mocked(bulkUpdateTasks).mockResolvedValue({
      updated_count: 200,
      deleted_count: 0,
    });
    const { onDone } = renderBar(200);

    fireEvent.click(
      screen.getByRole('button', { name: 'common:actions.archive' }),
    );

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(bulkUpdateTasks).toHaveBeenCalledWith(
      expect.any(Function),
      'list',
      expect.objectContaining({ archived: true }),
      'pms-space-a',
    );
    expect(vi.mocked(bulkUpdateTasks).mock.calls[0][2].task_ids).toHaveLength(
      200,
    );
    expect(feedback.warning).not.toHaveBeenCalled();
  });

  it('reports a failed update', async () => {
    vi.mocked(bulkUpdateTasks).mockRejectedValue(new Error('server refused'));
    const { onDone } = renderBar(2);

    fireEvent.click(screen.getByRole('button', { name: 'pms.bulk.restore' }));

    await waitFor(() =>
      expect(feedback.error).toHaveBeenCalledWith(
        'pms.bulk.failed',
        'server refused',
      ),
    );
    expect(onDone).not.toHaveBeenCalled();
  });

  function typeDate(label: string, value: string) {
    const input = screen.getByRole('textbox', { name: label });
    fireEvent.change(input, { target: { value } });
    fireEvent.blur(input);
  }

  function applyButton() {
    return screen.getByRole('button', {
      name: 'pms.bulk.dates.apply',
    }) as HTMLButtonElement;
  }

  it('sets start and due dates on the selected tasks', async () => {
    vi.mocked(bulkUpdateTasks).mockResolvedValue({
      updated_count: 2,
      deleted_count: 0,
    });
    const { onDone } = renderBar(2);

    fireEvent.click(
      screen.getByRole('button', { name: 'pms.bulk.dates.action' }),
    );
    typeDate('pms.bulk.dates.startDate', '2026-11-02');
    typeDate('pms.bulk.dates.dueDate', '2026-11-06');
    fireEvent.click(applyButton());

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(vi.mocked(bulkUpdateTasks).mock.calls[0][2]).toEqual({
      task_ids: ['t0', 't1'],
      start_date: '2026-11-02',
      due_date: '2026-11-06',
    });
  });

  it('clears due dates and leaves start dates alone', async () => {
    vi.mocked(bulkUpdateTasks).mockResolvedValue({
      updated_count: 2,
      deleted_count: 0,
    });
    const { onDone } = renderBar(2);

    fireEvent.click(
      screen.getByRole('button', { name: 'pms.bulk.dates.action' }),
    );
    const [, clearDue] = screen.getAllByRole('checkbox', {
      name: 'pms.bulk.dates.clear',
    });
    fireEvent.click(clearDue);
    fireEvent.click(applyButton());

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(vi.mocked(bulkUpdateTasks).mock.calls[0][2]).toEqual({
      task_ids: ['t0', 't1'],
      due_date: null,
    });
  });

  it('keeps the date menu open while a day is picked in the calendar portal', () => {
    renderBar(2);
    fireEvent.click(
      screen.getByRole('button', { name: 'pms.bulk.dates.action' }),
    );
    const calendar = document.createElement('div');
    calendar.setAttribute('data-ui-floating-layer', '');
    document.body.appendChild(calendar);

    try {
      fireEvent.mouseDown(calendar);
      expect(applyButton()).not.toBeNull();

      fireEvent.mouseDown(document.body);
      expect(
        screen.queryByRole('button', { name: 'pms.bulk.dates.apply' }),
      ).toBeNull();
    } finally {
      calendar.remove();
    }
  });

  it('does not send a start date after the due date', () => {
    renderBar(2);

    fireEvent.click(
      screen.getByRole('button', { name: 'pms.bulk.dates.action' }),
    );
    expect(applyButton().disabled).toBe(true);

    typeDate('pms.bulk.dates.startDate', '2026-11-06');
    typeDate('pms.bulk.dates.dueDate', '2026-11-02');

    expect(screen.getByText('pms.bulk.dates.invalidRange')).not.toBeNull();
    expect(applyButton().disabled).toBe(true);
    expect(bulkUpdateTasks).not.toHaveBeenCalled();
  });
});
