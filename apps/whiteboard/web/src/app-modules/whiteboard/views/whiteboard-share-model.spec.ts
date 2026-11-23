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
    primary_group: null,
  };
}

function sharing(): WhiteboardSharingResponse {
  return {
    link_share: null,
    owner_id: 'owner',
    whiteboard_id: 'board',
    users: [
      {
        access_level: 'edit',
        user_id: 'user-2',
        full_name: 'Grace Hopper',
        email: '',
      },
    ],
  };
}

describe('whiteboard share model', () => {
  it('builds stable user rows', () => {
    const presenter = buildWhiteboardSharePresenter({
      sharing: sharing(),
      users: [user('user-1', 'Ada Lovelace'), user('user-2', 'Grace Hopper')],
    });

    expect(presenter.userRows.map((row) => row.user.id)).toEqual([
      'user-2',
      'user-1',
    ]);
    expect(presenter.userRows[1]).toMatchObject({
      currentShare: null,
      shared: false,
    });
    expect(presenter.userRows[0]).toMatchObject({
      currentShare: { access_level: 'edit', user_id: 'user-2' },
      shared: true,
    });
  });

  it('keeps existing grants manageable outside the candidate page', () => {
    const grants = sharing();
    grants.users.push({
      user_id: 'removed-user',
      access_level: 'read',
      full_name: '',
      email: '',
    });
    const presenter = buildWhiteboardSharePresenter({ sharing: grants, users: [] });

    expect(presenter.userRows.map(({ user, shared }) => [user.full_name, shared]))
      .toEqual([['Grace Hopper', true], ['removed-user', true]]);
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
