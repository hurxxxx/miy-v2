import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { PmsTaskListMember } from '../api/pms-api';
import {
  ListAssigneeMenu,
  resolveAssigneeHighlightIndex,
} from './ListAssigneeMenu';

function member(
  index: number,
  overrides: Partial<PmsTaskListMember> = {},
): PmsTaskListMember {
  return {
    email: `user${index}@example.test`,
    full_name: `User ${index}`,
    is_admin: false,
    joined_at: '2026-06-10T00:00:00.000Z',
    primary_org_unit_name: 'Product',
    role: 'member',
    user_id: `user-${index}`,
    ...overrides,
  };
}

function renderMenu(
  overrides: Partial<Parameters<typeof ListAssigneeMenu>[0]> = {},
) {
  const onToggleAssignee = vi.fn();
  const onClose = vi.fn();
  render(
    <ListAssigneeMenu
      currentUserId="user-1"
      members={Array.from({ length: 20 }, (_, index) => member(index + 1))}
      onClearAssignees={vi.fn()}
      onClose={onClose}
      onToggleAssignee={onToggleAssignee}
      placement="below"
      selectedIds={new Set()}
      {...overrides}
    />,
  );
  return { onClose, onToggleAssignee };
}

describe('ListAssigneeMenu', () => {
  it('focuses the search box once the menu is visible so typing can start at once', async () => {
    renderMenu();

    await waitFor(() => {
      expect(document.activeElement).toBe(screen.getByRole('textbox'));
    });
  });

  it('lists every member instead of cutting the list off', () => {
    renderMenu();

    expect(screen.getAllByRole('checkbox')).toHaveLength(20);
  });

  it('assigns the first match on Enter and clears the query for the next name', () => {
    const { onToggleAssignee } = renderMenu();
    const input = screen.getByRole('textbox');

    fireEvent.change(input, { target: { value: 'user7' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(onToggleAssignee).toHaveBeenCalledTimes(1);
    expect(onToggleAssignee).toHaveBeenCalledWith('user-7');
    expect(input).toHaveProperty('value', '');
  });

  it('does not assign the pinned current user when the query matches nobody', () => {
    const { onToggleAssignee } = renderMenu();
    const input = screen.getByRole('textbox');

    fireEvent.keyDown(input, { key: 'Enter' });
    fireEvent.change(input, { target: { value: 'nobody' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(onToggleAssignee).not.toHaveBeenCalled();
  });

  it('moves the highlight with the arrow keys before Enter', () => {
    const { onToggleAssignee } = renderMenu();
    const input = screen.getByRole('textbox');

    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    // Index 0 is the pinned current user; two ArrowDowns land on the next one.
    expect(onToggleAssignee).toHaveBeenCalledWith('user-2');
  });

  it('closes on Escape', () => {
    const { onClose } = renderMenu();

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('puts the search box next to the task row when the menu opens above it', () => {
    renderMenu({ placement: 'above' });
    const menu = screen.getByTestId('list-assignee-menu');
    const input = screen.getByRole('textbox');
    const firstCheckbox = screen.getAllByRole('checkbox')[0];

    expect(menu.dataset.placement).toBe('above');
    expect(
      firstCheckbox.compareDocumentPosition(input) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

describe('resolveAssigneeHighlightIndex', () => {
  const candidates = [1, 2, 3].map((index) => ({
    ...member(index),
    id: `user-${index}`,
  }));

  it('prefers an explicit highlight and otherwise the first query match', () => {
    expect(
      resolveAssigneeHighlightIndex({
        candidates,
        highlightIndex: 2,
        query: '',
      }),
    ).toBe(2);
    expect(
      resolveAssigneeHighlightIndex({
        candidates,
        highlightIndex: null,
        query: 'User 2',
      }),
    ).toBe(1);
    expect(
      resolveAssigneeHighlightIndex({
        candidates,
        highlightIndex: null,
        query: '',
      }),
    ).toBe(-1);
    expect(
      resolveAssigneeHighlightIndex({
        candidates,
        highlightIndex: 9,
        query: 'User 3',
      }),
    ).toBe(2);
  });
});
