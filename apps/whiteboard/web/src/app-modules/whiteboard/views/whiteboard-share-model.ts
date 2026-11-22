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

  return {
    userRows: users.map((user) => {
      const currentShare = sharesByUserId.get(user.id) ?? null;
      return {
        user,
        currentShare,
        shared: currentShare !== null,
      };
    }),
  };
}
