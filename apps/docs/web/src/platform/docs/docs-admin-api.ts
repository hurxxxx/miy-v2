import type {
  DirectoryOption,
  DirectoryProvider,
  ShareProvider,
  ShareSnapshot as UiShareSnapshot,
} from '@miy/ui';
import type { AppRuntime } from '@miy/sdk';

import { apiFetchJson } from '../api/client';
import type { AppApi } from '../runtime/runtime-context';

/** Docs manager state (`GET /v1/docs/me`). */
export interface DocsMe {
  userId: string;
  isManager: boolean;
  canBootstrap: boolean;
}

export interface DocsManager {
  userId: string;
  name: string | null;
  grantedBy: string;
  createdAt: string;
}

/** A docs space (spaces.rs); `role` is the caller's role there. */
export interface DocsSpace {
  id: string;
  key: string;
  kind: 'app_global' | 'app_space';
  role: 'viewer' | 'member' | 'admin';
  name: string;
  description: string;
  createdBy: string | null;
  canManage: boolean;
  createdAt: string;
  updatedAt: string;
}

export function getDocsMe(api: AppApi | null | undefined): Promise<DocsMe> {
  return apiFetchJson<DocsMe>('/v1/docs/me', api);
}

export function bootstrapDocsManager(
  api: AppApi | null | undefined,
): Promise<DocsMe> {
  return apiFetchJson<DocsMe>('/v1/docs/managers/bootstrap', api, {
    method: 'POST',
  });
}

export async function listDocsManagers(
  api: AppApi | null | undefined,
): Promise<DocsManager[]> {
  const response = await apiFetchJson<{ items: DocsManager[] }>(
    '/v1/docs/managers',
    api,
  );
  return response.items;
}

export function addDocsManager(
  api: AppApi | null | undefined,
  userId: string,
): Promise<void> {
  return apiFetchJson<void>('/v1/docs/managers', api, {
    method: 'POST',
    body: JSON.stringify({ userId }),
  });
}

export function removeDocsManager(
  api: AppApi | null | undefined,
  userId: string,
): Promise<void> {
  return apiFetchJson<void>(
    `/v1/docs/managers/${encodeURIComponent(userId)}`,
    api,
    { method: 'DELETE' },
  );
}

export async function listSpaces(
  api: AppApi | null | undefined,
): Promise<DocsSpace[]> {
  const response = await apiFetchJson<{ spaces: DocsSpace[] }>(
    '/v1/spaces',
    api,
  );
  return response.spaces;
}

export function createSpace(
  api: AppApi | null | undefined,
  input: { name: string; description?: string },
): Promise<DocsSpace> {
  return apiFetchJson<DocsSpace>('/v1/spaces', api, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function updateSpace(
  api: AppApi | null | undefined,
  spaceId: string,
  input: { name: string; description?: string },
): Promise<DocsSpace> {
  return apiFetchJson<DocsSpace>(`/v1/spaces/${encodeURIComponent(spaceId)}`, api, {
    method: 'PATCH',
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
interface ServerShareSnapshot {
  revision: number;
  entries: Array<{
    subject: { kind: 'user' | 'group'; id: string; name: string };
    role: string;
    protected: boolean;
  }>;
  roles: Array<{ id: string; label: string }>;
  defaultRole?: string | null;
  canManage: boolean;
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
        }),
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
