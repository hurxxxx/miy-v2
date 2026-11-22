import type { PmsCurrentUser } from './current-user';

/**
 * AI-DO apps/web/src/platform/auth/auth-api.ts, reduced to the helpers the
 * PMS screens use. `teamRoleAllows` is verbatim. AI-DO's
 * `hasWorkspaceMembership` checked the signed-in user's workspace list; in
 * miy a person who can open the PMS frame is a PMS user (the platform granted
 * `use`), and that is what "member of the PMS workspace" meant. Space and list
 * roles are still decided by the server per request.
 */
export type AuthUser = PmsCurrentUser;

const TEAM_ROLE_RANK: Record<string, number> = {
  viewer: 10,
  member: 20,
  admin: 30,
  owner: 40,
};

export function teamRoleAllows(
  role: string | null | undefined,
  minRole: keyof typeof TEAM_ROLE_RANK,
): boolean {
  return roleRankAllows(TEAM_ROLE_RANK, role, minRole);
}

function roleRankAllows<TRole extends string>(
  ranks: Record<TRole, number>,
  role: string | null | undefined,
  minRole: TRole,
): boolean {
  if (!role) {
    return false;
  }

  return ((ranks as Record<string, number>)[role] ?? -1) >= ranks[minRole];
}

export function hasWorkspaceMembership(
  user: PmsCurrentUser | null | undefined,
  _workspaceSlug?: string | null,
): boolean {
  return Boolean(user);
}
