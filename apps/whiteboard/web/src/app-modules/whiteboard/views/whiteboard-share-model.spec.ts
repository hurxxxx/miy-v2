import { describe, expect, it } from 'vitest';

import type {
  ShareableUserItem,
  WhiteboardSharingResponse,
} from '../api/whiteboard-api';
import { buildWhiteboardSharePresenter } from './whiteboard-share-model';

function user(id: string, fullName: string): ShareableUserItem {
  return {
    id,
    email: `${id}@ai-do.local`,
    full_name: fullName,
  } as ShareableUserItem;
}

function sharing(): WhiteboardSharingResponse {
  return {
    link_share: null,
    users: [
      {
        access_level: 'edit',
        user_id: 'user-2',
      },
    ],
  } as unknown as WhiteboardSharingResponse;
}

describe('whiteboard share model', () => {
  it('builds stable user rows', () => {
    const presenter = buildWhiteboardSharePresenter({
      sharing: sharing(),
      users: [user('user-1', 'Ada Lovelace'), user('user-2', 'Grace Hopper')],
    });

    expect(presenter.userRows.map((row) => row.user.id)).toEqual([
      'user-1',
      'user-2',
    ]);
    expect(presenter.userRows[0]).toMatchObject({
      currentShare: null,
      shared: false,
    });
    expect(presenter.userRows[1]).toMatchObject({
      currentShare: { access_level: 'edit', user_id: 'user-2' },
      shared: true,
    });
  });

  it('returns empty sharing state before the response loads', () => {
    const presenter = buildWhiteboardSharePresenter({
      sharing: null,
      users: [user('user-1', 'Ada Lovelace')],
    });

    expect(presenter.userRows).toEqual([
      {
        user: user('user-1', 'Ada Lovelace'),
        currentShare: null,
        shared: false,
      },
    ]);
  });
});
