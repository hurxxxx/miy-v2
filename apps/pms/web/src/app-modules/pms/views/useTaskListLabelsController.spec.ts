import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { PmsLabel } from '../api/pms-api';
import {
  LABEL_PRESET_COLORS,
  useTaskListLabelsController,
} from './useTaskListLabelsController';
import type { AppApi } from '../../../platform/runtime/runtime-context';

// miy: the screens pass `runtime.api` where AI-DO passed the bearer token.
const testApi = vi.fn() as unknown as AppApi;

const createTaskListLabel = vi.fn();
const deleteLabel = vi.fn();
const updateLabel = vi.fn();

vi.mock('../api/pms-api', () => ({
  createTaskListLabel: (...args: unknown[]) => createTaskListLabel(...args),
  deleteLabel: (...args: unknown[]) => deleteLabel(...args),
  updateLabel: (...args: unknown[]) => updateLabel(...args),
}));

function label(overrides: Partial<PmsLabel> = {}): PmsLabel {
  return {
    id: 'label-1',
    color: '#1d4ed8',
    name: 'Bug',
    task_count: 0,
    ...overrides,
  } as PmsLabel;
}

function renderController() {
  const onError = vi.fn();
  const onLabelsChanged = vi.fn();
  const rendered = renderHook(() =>
    useTaskListLabelsController({
      workspaceSlug: 'pms-space-a',
      errorMessages: {
        createFailed: 'create failed',
        deleteFailed: 'delete failed',
        updateFailed: 'update failed',
      },
      onError,
      onLabelsChanged,
      taskListId: 'list-1',
      api: testApi,
    }),
  );
  return { ...rendered, onError, onLabelsChanged };
}

describe('useTaskListLabelsController', () => {
  it('creates labels and resets the create draft', async () => {
    createTaskListLabel.mockResolvedValueOnce(label());
    const { onLabelsChanged, result } = renderController();

    act(() => {
      result.current.setNewName(' Bug ');
      result.current.setNewColor('#1d4ed8');
    });
    await act(async () => {
      await result.current.handleCreate();
    });

    expect(createTaskListLabel).toHaveBeenCalledWith(
      testApi,
      'list-1',
      {
        color: '#1d4ed8',
        name: 'Bug',
      },
      'pms-space-a',
    );
    expect(result.current.labels).toEqual([label()]);
    expect(result.current.newName).toBe('');
    expect(result.current.newColor).toBe(LABEL_PRESET_COLORS[0]);
    expect(onLabelsChanged).toHaveBeenCalledWith([label()]);
  });

  it('updates and deletes labels through one controller interface', async () => {
    const original = label();
    const updated = label({ color: '#dc2626', name: 'Critical' });
    updateLabel.mockResolvedValueOnce(updated);
    deleteLabel.mockResolvedValueOnce(undefined);
    const { onLabelsChanged, result } = renderController();

    act(() => {
      result.current.replaceLabels([original]);
      result.current.startEdit(original);
      result.current.setEditName(' Critical ');
      result.current.setEditColor('#dc2626');
    });
    await act(async () => {
      await result.current.handleSaveEdit();
    });

    expect(updateLabel).toHaveBeenCalledWith(
      testApi,
      original.id,
      {
        color: '#dc2626',
        name: 'Critical',
      },
      'pms-space-a',
    );
    expect(result.current.labels).toEqual([updated]);
    expect(result.current.editingId).toBe(null);

    await act(async () => {
      await result.current.handleDelete(updated.id);
    });

    expect(deleteLabel).toHaveBeenCalledWith(
      testApi,
      updated.id,
      'pms-space-a',
    );
    expect(result.current.labels).toEqual([]);
    expect(onLabelsChanged).toHaveBeenLastCalledWith([]);
  });
});
