type Account = { id: string; name: string; admin: boolean; canDevelop: boolean; enabled: boolean; primaryGroupId: string | null };
type Organization = { id: string; name: string; parentId: string | null; source: "hr" | "local" };

export function mockAdminUserPage(accounts: Account[], groups: Organization[], params: URLSearchParams) {
  const search = params.get("q")?.toLowerCase() ?? "";
  const organization = params.get("organization");
  const descendants = params.get("descendants") === "true";
  const role = params.get("role");
  const status = params.get("status");
  const limit = Number(params.get("limit") ?? "25");
  const offset = Number(params.get("cursor") ?? "0");
  const matching = accounts.filter((account) => {
    if (search && !account.name.toLowerCase().includes(search) && !account.id.toLowerCase().includes(search)) return false;
    if (role === "admin" && !account.admin) return false;
    if (role === "developer" && !account.canDevelop) return false;
    if (role === "member" && (account.admin || account.canDevelop)) return false;
    if (status && (status === "enabled") !== account.enabled) return false;
    if (!organization) return true;
    if (!account.primaryGroupId) return organization === "unassigned";
    let candidate: string | null = account.primaryGroupId;
    const visited = new Set<string>();
    while (candidate && !visited.has(candidate)) {
      if (candidate === organization) return true;
      if (!descendants) break;
      visited.add(candidate);
      candidate = groups.find((group) => group.id === candidate)?.parentId ?? null;
    }
    return false;
  });
  const items = matching.slice(offset, offset + limit).map((account) => ({
    ...account,
    primaryGroupName: groups.find((group) => group.id === account.primaryGroupId)?.name ?? null,
  }));
  return { items, nextCursor: offset + limit < matching.length ? String(offset + limit) : null };
}
