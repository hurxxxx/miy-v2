import type { components, paths } from '../../api.generated';
/**
 * Spaces of the whiteboard app (AI-DO app spaces): list, create, delete and
 * the members editor's providers (SDK resource ACL). Same contract as the
 * board app's spaces (apps/board/web/src/platform/board/board-api.ts).
 */
import type {
  DirectoryOption,
  DirectoryProvider,
  ShareProvider,
  ShareSnapshot as UiShareSnapshot,
} from '@miy/ui';
import type { AppRuntime } from '@miy/sdk';

import { apiFetchJson } from '../api/client';
import type { AppApi } from '../runtime/runtime-context';

type SpaceOperations = paths['/v1/spaces'];
type SpacesResponse = SpaceOperations['get']['responses'][200]['content']['application/json'];
export type WhiteboardSpace = SpaceOperations['post']['responses'][201]['content']['application/json'];
type CreateSpaceInput = SpaceOperations['post']['requestBody']['content']['application/json'];
type ServerShareSnapshot = components['schemas']['SpaceShareSnapshot'];
type ReplaceSpaceSharingInput = components['schemas']['SharingRequest'];
type SharingResult = paths['/v1/spaces/{space_id}/sharing/requests/{request_id}']['get']['responses'][200]['content']['application/json'];

export async function listSpaces(
  api: AppApi | null | undefined,
): Promise<WhiteboardSpace[]> {
  const response = await apiFetchJson<SpacesResponse>(
    '/v1/spaces',
    api,
  );
  return response.spaces;
}

export function createSpace(
  api: AppApi | null | undefined,
  input: CreateSpaceInput,
): Promise<WhiteboardSpace> {
  return apiFetchJson<WhiteboardSpace>('/v1/spaces', api, {
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
      const payload: ReplaceSpaceSharingInput = {
        expectedRevision: Number(expectedRevision),
        requestId,
        entries: entries.map((entry) => ({
          subject: { kind: entry.subject.kind, id: entry.subject.id },
          role: entry.role,
        })),
      };
      const snapshot = await api<ServerShareSnapshot>(base, {
        method: 'PUT',
        signal,
        body: JSON.stringify(payload),
      });
      return toUiSnapshot(snapshot);
    },
    async lookupResult(requestId, { signal }) {
      const snapshot = await api<SharingResult>(
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
