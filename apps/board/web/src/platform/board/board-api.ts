import type { components, paths } from '../../api.generated';
import type {
  DirectoryOption,
  DirectoryProvider,
  ShareProvider,
  ShareSnapshot as UiShareSnapshot,
} from '@miy/ui';
import type { AppRuntime } from '@miy/sdk';

import { apiFetchJson } from '../api/client';
import type { AppApi } from '../runtime/runtime-context';

/** Board manager state (`GET /v1/board/me`). */
export type BoardMe = paths['/v1/board/me']['get']['responses'][200]['content']['application/json'];

export type BoardManager = components['schemas']['ManagerOut'];
export type BoardSpace = components['schemas']['SpaceOut'];

export function getBoardMe(api: AppApi | null | undefined): Promise<BoardMe> {
  return apiFetchJson<BoardMe>('/v1/board/me', api);
}

export function bootstrapBoardManager(
  api: AppApi | null | undefined,
): Promise<BoardMe> {
  return apiFetchJson<BoardMe>('/v1/board/managers/bootstrap', api, {
    method: 'POST',
  });
}

export async function listBoardManagers(
  api: AppApi | null | undefined,
): Promise<BoardManager[]> {
  const response = await apiFetchJson<components['schemas']['ManagersResponse']>(
    '/v1/board/managers',
    api,
  );
  return response.items;
}

export function addBoardManager(
  api: AppApi | null | undefined,
  userId: string,
): Promise<void> {
  return apiFetchJson<void>('/v1/board/managers', api, {
    method: 'POST',
    body: JSON.stringify({ userId } satisfies components['schemas']['ManagerAddRequest']),
  });
}

export function removeBoardManager(
  api: AppApi | null | undefined,
  userId: string,
): Promise<void> {
  return apiFetchJson<void>(
    `/v1/board/managers/${encodeURIComponent(userId)}`,
    api,
    { method: 'DELETE' },
  );
}

export async function listSpaces(
  api: AppApi | null | undefined,
): Promise<BoardSpace[]> {
  const response = await apiFetchJson<components['schemas']['SpacesResponse']>(
    '/v1/spaces',
    api,
  );
  return response.spaces;
}

export function createSpace(
  api: AppApi | null | undefined,
  input: components['schemas']['SpaceRequest'],
): Promise<BoardSpace> {
  return apiFetchJson<BoardSpace>('/v1/spaces', api, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function deleteSpace(
  api: AppApi | null | undefined,
  spaceId: string,
): Promise<void> {
  return apiFetchJson<void>(`/v1/spaces/${encodeURIComponent(spaceId)}`, api, {
    method: 'DELETE',
  });
}

/** The SDK ACL snapshot as the server sends it. */
type ServerShareSnapshot = components['schemas']['SharingSnapshot'];

function toUiSnapshot(snapshot: ServerShareSnapshot): UiShareSnapshot {
  return {
    revision: String(snapshot.revision),
    entries: snapshot.entries.map((entry) => ({
      subject: {
        kind: entry.subject.kind,
        id: entry.subject.id,
        name: entry.subject.name,
      },
      role: entry.role,
      locked: entry.protected,
    })),
    roles: snapshot.roles,
    defaultRole: snapshot.defaultRole ?? null,
    canManage: snapshot.canManage,
  };
}

/** ShareEditor provider for a space's members (SDK resource ACL). */
export function spaceShareProvider(
  api: AppRuntime['api'],
  spaceId: string,
): ShareProvider {
  const base = `/v1/spaces/${encodeURIComponent(spaceId)}/sharing`;
  return {
    async load({ signal }) {
      return toUiSnapshot(await api<ServerShareSnapshot>(base, { signal }));
    },
    async replace({ requestId, expectedRevision, entries }, { signal }) {
      const snapshot = await api<ServerShareSnapshot>(base, {
        method: 'PUT',
        signal,
        body: JSON.stringify({
          expectedRevision: Number(expectedRevision),
          requestId,
          entries: entries.map((entry) => ({
            subject: { kind: entry.subject.kind, id: entry.subject.id },
            role: entry.role,
          })),
        } satisfies components['schemas']['SharingRequest']),
      });
      return toUiSnapshot(snapshot);
    },
    async lookupResult(requestId, { signal }) {
      const snapshot = await api<ServerShareSnapshot | null>(
        `${base}/requests/${encodeURIComponent(requestId)}`,
        { signal },
      );
      return snapshot ? toUiSnapshot(snapshot) : null;
    },
  };
}

/** DirectoryPicker provider over `runtime.directory` (platform:directory:read). */
export function directoryProvider(
  directory: AppRuntime['directory'],
): DirectoryProvider {
  return {
    async load({ kind, query, cursor, limit, signal }) {
      const page = await directory(
        {
          kind: kind === 'user' ? 'users' : 'groups',
          q: query || undefined,
          limit,
          cursor,
        },
        { signal },
      );
      const items: DirectoryOption[] =
        kind === 'user'
          ? page.users.map((user) => ({
              kind: 'user',
              id: user.id,
              name: user.name,
              description: user.primaryGroup?.name,
            }))
          : page.groups.map((group) => ({
              kind: 'group',
              id: group.id,
              name: group.name,
            }));
      return { items, nextCursor: page.nextCursor ?? null };
    },
  };
}
