import { fireEvent, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import { FeedbackProvider } from '@miy/ui';

import { renderWithRuntime, TEST_USER } from '../../../test-utils/runtime';
import { addSpaceMember, createSpace, listPmsUsers } from '../api/pms-api';
import { CreateSpaceModal } from './CreateSpaceModal';

const { translate } = vi.hoisted(() => ({
  translate: (key: string, values?: { failures?: string }) =>
    values?.failures ? `${key} ${values.failures}` : key,
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: translate }),
}));

// miy: the current user comes from the frame's `/v1/pms/me` context, and there
// is no app-first runtime (any PMS user may create a space). AI-DO also
// refreshed the portal's app/space catalog after creating a space
// (`reloadGlobalApps`); miy has no such catalog in the app, so those
// assertions are gone.
const ME = { ...TEST_USER, id: 'me' };

vi.mock('../api/pms-api', () => ({
  addSpaceMember: vi.fn(),
  createSpace: vi.fn(),
  listPmsUsers: vi.fn(),
}));

const space = {
  id: 'new-space-id',
  workspace_id: 'new-space-id',
  workspace_key: 'new-space',
  key: 'new-space',
  name: 'Project',
  description: '',
  member_count: 1,
  current_user_role: 'owner',
  created_at: '',
  updated_at: '',
};

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(createSpace).mockResolvedValue(space);
  vi.mocked(listPmsUsers).mockResolvedValue([
    { id: 'member', full_name: 'Project Member', email: 'member@example.com' },
  ]);
});

async function submitWithInvite() {
  fireEvent.change(
    screen.getByLabelText('apps:pms.spaceName', { exact: false }),
    {
      target: { value: 'Project' },
    },
  );
  fireEvent.focus(screen.getByPlaceholderText('apps:pms.searchUser'));
  fireEvent.click(
    await screen.findByRole('button', { name: /Project Member/ }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'apps:pms.createSpace' }));
}

it('closes the created space form and retains the failed invite warning after navigation', async () => {
  vi.mocked(addSpaceMember).mockRejectedValue(new Error('Access revoked'));
  const onCreated = vi.fn();
  const onClose = vi.fn();

  function WorkspaceView() {
    const [created, setCreated] = useState(false);
    return created ? (
      <p>New workspace</p>
    ) : (
      <CreateSpaceModal
        isOpen
        workspaceSlug="current-space"
        onClose={onClose}
        onCreated={(result) => {
          onCreated(result);
          setCreated(true);
        }}
      />
    );
  }

  const { runtime } = renderWithRuntime(
    <FeedbackProvider
      labels={{ close: 'Close', item: 'Notice', region: 'Notices' }}
    >
      <WorkspaceView />
    </FeedbackProvider>,
    { user: ME },
  );

  await submitWithInvite();

  expect(await screen.findByText('New workspace')).toBeTruthy();
  expect(
    screen.getByText(
      'apps:pms.createSpacePartialFailure Project Member: Access revoked',
    ),
  ).toBeTruthy();
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(onClose).toHaveBeenCalledOnce();
  expect(onCreated).toHaveBeenCalledExactlyOnceWith(space);
  expect(createSpace).toHaveBeenCalledOnce();
  expect(addSpaceMember).toHaveBeenCalledExactlyOnceWith(
    runtime.api,
    space.id,
    { user_id: 'member', role: 'member' },
    'current-space',
  );
});

it('allows dismissal during a delayed invite and blocks duplicate creation after reopening', async () => {
  let rejectInvite!: (reason: Error) => void;
  vi.mocked(addSpaceMember).mockReturnValue(
    new Promise((_, reject) => {
      rejectInvite = reject;
    }),
  );
  const onCreated = vi.fn();

  function WorkspaceView() {
    const [open, setOpen] = useState(true);
    return (
      <>
        <button onClick={() => setOpen(true)}>Open creation</button>
        <CreateSpaceModal
          isOpen={open}
          workspaceSlug="current-space"
          onClose={() => setOpen(false)}
          onCreated={onCreated}
        />
      </>
    );
  }

  renderWithRuntime(
    <FeedbackProvider
      labels={{ close: 'Close', item: 'Notice', region: 'Notices' }}
    >
      <WorkspaceView />
    </FeedbackProvider>,
    { user: ME },
  );

  await submitWithInvite();
  await waitFor(() => expect(addSpaceMember).toHaveBeenCalledOnce());
  fireEvent.click(
    screen.getByRole('button', { name: 'common:actions.cancel' }),
  );
  expect(screen.queryByRole('dialog')).toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Open creation' }));
  const name = screen.getByLabelText('apps:pms.spaceName', { exact: false });
  fireEvent.change(name, { target: { value: 'Another project' } });
  expect(
    screen.getByRole('button', { name: 'apps:pms.creating' }),
  ).toHaveProperty('disabled', true);
  fireEvent.keyDown(name, { key: 'Enter' });
  expect(createSpace).toHaveBeenCalledOnce();

  rejectInvite(new Error('Access revoked'));
  expect(
    await screen.findByText(
      'apps:pms.createSpacePartialFailure Project Member: Access revoked',
    ),
  ).toBeTruthy();
  expect(onCreated).not.toHaveBeenCalled();
  expect(name).toHaveProperty('value', 'Another project');
  expect(
    screen.getByRole('button', { name: 'apps:pms.createSpace' }),
  ).toHaveProperty('disabled', false);
});
