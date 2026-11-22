import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppRuntime } from '@miy/sdk';

import {
  createTestRuntime,
  renderWithRuntime,
  TEST_USER,
} from '../../../test-utils/runtime';
import { SpaceOverviewView } from './SpaceOverviewView';
import { PMS_SPACE_CHANGED_EVENT } from './pms-events';

// miy: no app-first runtime, so a removed space returns to the app root `/`
// (AI-DO: `/pms` in the app-first runtime, `/w/<slug>/pms` on workspace routes).
const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  deleteSpace: vi.fn(),
  translate: (key: string) => key,
  preferences: new Map(),
}));
vi.mock('../../../platform/router', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../platform/router')>()),
  useNavigate: () => mocks.navigate,
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: mocks.translate, i18n: { language: 'en-US' } }),
}));
vi.mock('@miy/ui', async (original) => ({
  ...(await original<typeof import('@miy/ui')>()),
  useFeedback: () => ({ error: vi.fn() }),
  useConfirm: () => ({ confirm: async () => true, confirmDialog: null }),
  usePrompt: () => ({ prompt: vi.fn(), promptDialog: null }),
}));
vi.mock('../api/pms-api', () => ({
  deleteSpace: mocks.deleteSpace,
  listAllPmsTaskLists: async () => ({ items: [] }),
  listFolders: async () => ({ items: [] }),
  listSpaceMembers: async () => ({ items: [] }),
  listSpaces: async () => [
    { id: 'space-one', name: 'Space title', current_user_role: 'owner' },
  ],
}));
vi.mock('./usePmsSpacePreferences', () => ({
  usePmsSpacePreferences: () => ({
    preferences: mocks.preferences,
    setSpacePreference: vi.fn(),
  }),
}));
vi.mock('./PmsCenteredStateBlock', () => ({
  PmsCenteredLoadingState: () => null,
}));
vi.mock('./PmsSpaceToolTabs', () => ({ PmsSpaceToolTabs: () => null }));
vi.mock('./CreateTaskListModal', () => ({ CreateTaskListModal: () => null }));
vi.mock('./SpaceMembersModal', () => ({ SpaceMembersModal: () => null }));
vi.mock('../sidebar/SpaceOrderEditorModal', () => ({
  SpaceOrderEditorModal: () => null,
}));
vi.mock('../sidebar/SpaceContextMenu', () => ({
  SpaceContextMenu: ({ onDelete }: { onDelete?: () => void }) => (
    <button onClick={onDelete}>Delete space</button>
  ),
}));
let runtime: AppRuntime;

function renderOverview(workspaceSlug: string) {
  return renderWithRuntime(
    <SpaceOverviewView spaceId="space-one" workspaceSlug={workspaceSlug} />,
    { runtime, user: { ...TEST_USER, id: 'actor' } },
  );
}

beforeEach(() => {
  mocks.navigate.mockReset();
  mocks.deleteSpace.mockReset().mockResolvedValue(undefined);
  runtime = createTestRuntime({ path: '/spaces/space-one' });
});
afterEach(cleanup);

describe('PMS removed-space navigation', () => {
  it.each(['deleted', 'left'])(
    'returns to the app global entry after the current space is %s',
    async (type) => {
      renderOverview('pms-source');
      await screen.findByRole('heading', { name: 'Space title' });
      act(() =>
        window.dispatchEvent(
          new CustomEvent(PMS_SPACE_CHANGED_EVENT, {
            detail: { type, spaceId: 'space-one' },
          }),
        ),
      );
      expect(mocks.navigate).toHaveBeenCalledWith('/');
    },
  );

  it('deletes with the source scope then navigates outside that deleted scope', async () => {
    renderOverview('pms-source');
    await screen.findByRole('heading', { name: 'Space title' });
    fireEvent.click(screen.getByRole('button', { name: 'Delete space' }));
    await waitFor(() =>
      expect(mocks.deleteSpace).toHaveBeenCalledWith(
        runtime.api,
        'space-one',
        'pms-source',
      ),
    );
    await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/'));
  });
});
