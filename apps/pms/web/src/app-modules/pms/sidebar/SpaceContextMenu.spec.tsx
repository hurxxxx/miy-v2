import { fireEvent, render, screen } from '@testing-library/react';
import { createRef } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { SpaceContextMenu } from './SpaceContextMenu';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

function renderMenu(props: Partial<Parameters<typeof SpaceContextMenu>[0]>) {
  const anchorRef = createRef<HTMLButtonElement>();
  const onClose = vi.fn();
  render(
    <SpaceContextMenu
      anchorRef={anchorRef}
      canManage={false}
      onClose={onClose}
      onManageMembers={vi.fn()}
      open
      {...props}
    />,
  );
  return { onClose };
}

describe('SpaceContextMenu', () => {
  it('gives a plain member pin, hide and the member list but no management', () => {
    const onTogglePinned = vi.fn();
    const onToggleHidden = vi.fn();
    const { onClose } = renderMenu({ onToggleHidden, onTogglePinned });

    expect(screen.queryByText('common:actions.rename')).toBeNull();
    expect(screen.queryByText('common:actions.archive')).toBeNull();
    expect(screen.getByText('pms.spaceMembers.viewMembers')).not.toBeNull();

    fireEvent.click(screen.getByText('pms.sidebar.pinSpace'));
    fireEvent.click(screen.getByText('pms.sidebar.hideSpace'));

    expect(onTogglePinned).toHaveBeenCalledTimes(1);
    expect(onToggleHidden).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('offers to undo a pin or a hide', () => {
    renderMenu({
      hidden: true,
      onToggleHidden: vi.fn(),
      onTogglePinned: vi.fn(),
      pinned: true,
    });

    expect(screen.getByText('pms.sidebar.unpinSpace')).not.toBeNull();
    expect(screen.getByText('pms.sidebar.showSpace')).not.toBeNull();
  });
});
