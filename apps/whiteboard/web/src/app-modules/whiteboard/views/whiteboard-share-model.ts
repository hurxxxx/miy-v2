import type {
  ShareableUserItem,
  WhiteboardSharingResponse,
  WhiteboardUserShareItem,
} from '../api/whiteboard-api';

export interface WhiteboardShareUserRow {
  user: ShareableUserItem;
  currentShare: WhiteboardUserShareItem | null;
  shared: boolean;
}

/**
 * AI-DO views/whiteboard-share-model.ts without the link share URL (link
 * shares are not offered in miy).
 */
export interface WhiteboardSharePresenter {
  userRows: WhiteboardShareUserRow[];
}

export function buildWhiteboardSharePresenter(options: {
  sharing: WhiteboardSharingResponse | null;
  users: ShareableUserItem[];
}): WhiteboardSharePresenter {
  const { sharing, users } = options;
  const sharesByUserId = new Map(
    (sharing?.users ?? []).map((share) => [share.user_id, share]),
  );
  const candidatesById = new Map(users.map((user) => [user.id, user]));
  const currentRows = [...sharesByUserId.values()].map((share) => ({
    user: candidatesById.get(share.user_id) ?? {
      id: share.user_id,
      full_name: share.full_name || share.email || share.user_id,
      email: share.email,
      primary_group: null,
    },
    currentShare: share,
    shared: true,
  }));

  return {
    userRows: [
      ...currentRows,
      ...users.filter((user) => !sharesByUserId.has(user.id)).map((user) => ({
        user,
        currentShare: null,
        shared: false,
      })),
    ],
  };
}
