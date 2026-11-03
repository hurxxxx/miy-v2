import { useCallback, useEffect, useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
import { Badge, Button, Field, Loading, Panel, useConfirm } from "../ui";

// Users and groups receive app permissions here; apps keep their internal data access.
// Group permissions are computed on every request, so nothing is copied to users.

export type DirectoryUser = { id: string; name: string };
type Group = {
  id: string;
  name: string;
  source: "hr" | "local";
  parentId: string | null;
  active: boolean;
  members: number;
};
type Member = { id: string; name: string; enabled: boolean };
type Environment = "dev" | "prod";
type GroupGrant = {
  groupId: string;
  environment: Environment;
  permission: "use" | "develop";
};
type UserGrant = {
  userId: string;
  environment: Environment;
  permission: "use" | "develop" | "deploy";
};
type Access = {
  appId: string;
  disabled: boolean;
  admission: Record<Environment, "selected" | "all">;
  userGrants: UserGrant[];
  groupGrants: GroupGrant[];
};

const sourceLabel = { hr: "조직(인사)", local: "수동 그룹" } as const;
const environmentLabel = { dev: "테스트", prod: "운영" } as const;
const permissionLabel = {
  use: "사용",
  develop: "개발",
  deploy: "배포",
} as const;

type Report = (message: string) => void;

// Bind each response to the selection that requested it. Refreshing after a
// mutation reloads the current selection, even if the user switched meanwhile.
function useSelectedResource<T>(id: string | null, path: string | null, report: Report, errorMessage: string) {
  const [result, setResult] = useState<{ id: string; data: T } | null>(null);
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    setResult((previous) => (previous?.id === id ? previous : null));
    if (!id || !path) return;
    let current = true;
    request<T>(path)
      .then((data) => {
        if (current) setResult({ id, data });
      })
      .catch((error) => {
        if (current) report(`${errorMessage}: ${String(error)}`);
      });
    return () => {
      current = false;
    };
  }, [id, path, revision, report, errorMessage]);
  return { data: result?.id === id ? result.data : null, refresh };
}

export function useGroups(report: Report) {
  const { data: groups, refresh } = useSelectedResource<Group[]>("groups", "/api/v1/groups", report, "그룹 목록을 불러오지 못했습니다");
  return { groups, refresh };
}

async function send(csrf: string, path: string, method: "POST" | "PUT" | "PATCH", body: unknown) {
  await request(path, csrf, { method, body: JSON.stringify(body) });
}

type AdminAccessProps = {
  csrf: string;
  users: DirectoryUser[] | null;
  report: Report;
  directory: ReturnType<typeof useGroups>;
};

export function GroupAdmin({ csrf, users, report, directory }: AdminAccessProps) {
  const { groups, refresh } = directory;
  const [selected, setSelected] = useState<string | null>(null);
  const { data: members, refresh: refreshMembers } = useSelectedResource<Member[]>(
    selected,
    selected ? `/api/v1/groups/${selected}/members` : null,
    report,
    "구성원을 불러오지 못했습니다",
  );
  const [source, setSource] = useState<"hr" | "local">("local");
  const group = groups?.find((g) => g.id === selected) ?? null;
  const hrGroups = (groups ?? []).filter((g) => g.source === "hr" && g.active);
  async function apply(path: string, method: "POST" | "PUT" | "PATCH", body: unknown, done: string) {
    try {
      await send(csrf, path, method, body);
      report(done);
      refresh();
      refreshMembers();
    } catch (error) {
      report(String(error));
    }
  }
  function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const parentId = source === "hr" ? String(form.get("parent") ?? "") : "";
    void apply("/api/v1/groups", "POST", { name: form.get("name"), source, parentId: parentId || null }, "그룹을 만들었습니다.");
    event.currentTarget.reset();
  }
  function addMember(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!group) return;
    const userId = String(new FormData(event.currentTarget).get("user") ?? "");
    if (group.source === "local")
      void apply(`/api/v1/groups/${group.id}/members`, "PUT", { userId, member: true }, "구성원을 추가했습니다.");
    else void apply(`/api/v1/users/${encodeURIComponent(userId)}/primary-group`, "PUT", { groupId: group.id }, "주 소속을 지정했습니다.");
  }
  function moveParent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!group) return;
    const parentId = String(new FormData(event.currentTarget).get("parent") ?? "");
    void apply(`/api/v1/groups/${group.id}/parent`, "PUT", { parentId: parentId || null }, "상위 조직을 바꿨습니다.");
  }

  return (
    <Panel title="그룹" actions={groups && <Badge>{groups.length}개</Badge>}>
      <form className="access-toolbar" onSubmit={create} aria-label="그룹 만들기">
        <Field label="이름">
          <input name="name" required maxLength={120} autoComplete="off" />
        </Field>
        <Field label="유형">
          <select name="source" value={source} onChange={(e) => setSource(e.target.value as "hr" | "local")}>
            <option value="local">{sourceLabel.local}</option>
            <option value="hr">{sourceLabel.hr}</option>
          </select>
        </Field>
        <Field label="상위 조직">
          <select name="parent" disabled={source !== "hr"}>
            <option value="">없음</option>
            {hrGroups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </Field>
        <Button type="submit" variant="primary">
          만들기
        </Button>
      </form>
      <p className="ui-empty-inline access-hint">
        조직(인사) 그룹은 주 소속이 정확히 그 조직인 사용자만, 수동 그룹은 직접 추가한 사용자만 포함합니다. 상위·하위 조직으로 권한이 이어지지 않습니다.
      </p>
      {groups === null ? (
        <Loading />
      ) : groups.length === 0 ? (
        <p className="ui-empty-inline">그룹이 없습니다.</p>
      ) : (
        <table className="access-table">
          <thead>
            <tr>
              <th scope="col">이름</th>
              <th scope="col">유형</th>
              <th scope="col">상위 조직</th>
              <th scope="col">구성원</th>
              <th scope="col">상태</th>
              <th scope="col">
                <span className="visually-hidden">동작</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {groups.map((g) => (
              <tr key={g.id} aria-current={g.id === selected ? "true" : undefined}>
                <td>{g.name}</td>
                <td>{sourceLabel[g.source]}</td>
                <td>{groups.find((p) => p.id === g.parentId)?.name ?? "—"}</td>
                <td>{g.members}</td>
                <td>{g.active ? "사용" : <Badge tone="warning">비활성</Badge>}</td>
                <td className="access-actions">
                  <Button size="sm" onClick={() => setSelected(g.id === selected ? null : g.id)} aria-expanded={g.id === selected}>
                    구성원
                  </Button>
                  <Button
                    size="sm"
                    onClick={() =>
                      void apply(`/api/v1/groups/${g.id}`, "PATCH", { active: !g.active }, g.active ? "그룹을 비활성화했습니다." : "그룹을 다시 사용합니다.")
                    }
                  >
                    {g.active ? "비활성화" : "다시 사용"}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {group && (
        <section className="access-detail" aria-label={`${group.name} 구성원`}>
          <h3>{group.name} 구성원</h3>
          <form className="access-toolbar" onSubmit={addMember}>
            <Field label={group.source === "local" ? "추가할 사용자" : "주 소속으로 지정할 사용자"}>
              <select name="user" disabled={users === null}>
                {(users ?? []).map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name}
                  </option>
                ))}
              </select>
            </Field>
            <Button type="submit">{group.source === "local" ? "추가" : "주 소속 지정"}</Button>
          </form>
          {group.source === "hr" && (
            <form key={`${group.id}:${group.parentId ?? ""}`} className="access-toolbar" onSubmit={moveParent}>
              <Field label="상위 조직">
                <select name="parent" defaultValue={group.parentId ?? ""}>
                  <option value="">없음</option>
                  {hrGroups
                    .filter((g) => g.id !== group.id)
                    .map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.name}
                      </option>
                    ))}
                </select>
              </Field>
              <Button type="submit">상위 조직 변경</Button>
            </form>
          )}
          {members === null ? (
            <Loading />
          ) : members.length === 0 ? (
            <p className="ui-empty-inline">구성원이 없습니다.</p>
          ) : (
            <ul className="access-members">
              {members.map((m) => (
                <li key={m.id}>
                  <span>
                    {m.name} {!m.enabled && <Badge tone="warning">사용 중지</Badge>}
                  </span>
                  <Button
                    size="sm"
                    onClick={() =>
                      group.source === "local"
                        ? void apply(`/api/v1/groups/${group.id}/members`, "PUT", { userId: m.id, member: false }, "구성원을 뺐습니다.")
                        : void apply(`/api/v1/users/${encodeURIComponent(m.id)}/primary-group`, "PUT", { groupId: null }, "주 소속을 해제했습니다.")
                    }
                  >
                    {group.source === "local" ? "빼기" : "주 소속 해제"}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </Panel>
  );
}

export function AppAccessAdmin({ csrf, users, report, directory }: AdminAccessProps) {
  const { groups } = directory;
  const [apps, setApps] = useState<{ id: string; name: string }[] | null>(null);
  const [appId, setAppId] = useState("");
  const { data: loadedAccess, refresh } = useSelectedResource<Access>(
    appId,
    appId ? `/api/v1/apps/${encodeURIComponent(appId)}/access` : null,
    report,
    "앱 접근 설정을 불러오지 못했습니다",
  );
  const access = loadedAccess?.appId === appId ? loadedAccess : null;
  const { confirm, dialog } = useConfirm();
  useEffect(() => {
    request<{ apps: { id: string; name: string }[] }>("/api/v1/apps?limit=200")
      .then((page) => setApps(page.apps))
      .catch((error) => report(`앱 목록을 불러오지 못했습니다: ${String(error)}`));
  }, [report]);
  async function apply(path: string, body: unknown, done: string) {
    try {
      await send(csrf, path, "PUT", body);
      report(done);
      refresh();
    } catch (error) {
      report(String(error));
    }
  }
  const base = `/api/v1/apps/${encodeURIComponent(appId)}`;
  async function toggleCompanySwitch() {
    if (!access) return;
    if (!access.disabled) {
      const ok = await confirm({
        title: "앱 사용을 중지할까요?",
        body: <p>관리자를 포함한 모든 사용자가 이 앱을 사용할 수 없습니다. 개발 권한이 있는 사용자는 테스트 환경에서 앱을 고칠 수 있습니다.</p>,
        confirmLabel: "사용 중지",
        tone: "danger",
      });
      if (!ok) return;
    }
    await apply(`${base}/control`, { disabled: !access.disabled }, access.disabled ? "앱 사용을 다시 허용했습니다." : "앱 사용을 중지했습니다.");
  }
  function addGroupGrant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    void apply(
      "/api/v1/group-grants",
      {
        groupId: form.get("group"),
        appId,
        environment: form.get("environment"),
        permission: form.get("permission"),
        enabled: true,
      },
      "그룹 권한을 부여했습니다.",
    );
  }
  const groupName = (id: string) => groups?.find((g) => g.id === id)?.name ?? id;
  const userName = (id: string) => users?.find((u) => u.id === id)?.name ?? id;

  return (
    <Panel title="앱 접근">
      {dialog}
      <Field label="앱">
        <select value={appId} onChange={(e) => setAppId(e.target.value)} disabled={apps === null}>
          <option value="">앱 선택</option>
          {(apps ?? []).map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} ({a.id})
            </option>
          ))}
        </select>
      </Field>
      {appId && access === null && <Loading />}
      {access && (
        <div className="access-detail">
          <div className="access-row">
            <span>
              회사 사용 상태: {access.disabled ? <Badge tone="danger">중지됨</Badge> : <Badge tone="success">사용 중</Badge>}
            </span>
            <Button size="sm" variant={access.disabled ? "primary" : "danger"} onClick={() => void toggleCompanySwitch()}>
              {access.disabled ? "사용 다시 허용" : "사용 중지"}
            </Button>
          </div>
          {(["dev", "prod"] as const).map((environment) => (
            <div key={environment} className="access-row">
              <span>{environmentLabel[environment]} 입장 대상</span>
              <select
                aria-label={`${environmentLabel[environment]} 입장 대상`}
                value={access.admission[environment]}
                onChange={(e) =>
                  void apply(`${base}/admission`, { environment, mode: e.target.value }, `${environmentLabel[environment]} 입장 대상을 바꿨습니다.`)
                }
              >
                <option value="selected">권한을 받은 사용자·그룹만</option>
                <option value="all">사용 중인 모든 사용자</option>
              </select>
            </div>
          ))}
          <h3>그룹 권한</h3>
          <form className="access-toolbar" onSubmit={addGroupGrant}>
            <Field label="그룹">
              <select name="group" disabled={!groups?.length}>
                {(groups ?? [])
                  .filter((g) => g.active)
                  .map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
              </select>
            </Field>
            <Field label="환경">
              <select name="environment">
                <option value="dev">{environmentLabel.dev}</option>
                <option value="prod">{environmentLabel.prod}</option>
              </select>
            </Field>
            <Field label="권한" hint="운영 배포 권한은 사람별로만 부여합니다.">
              <select name="permission">
                <option value="use">{permissionLabel.use}</option>
                <option value="develop">{permissionLabel.develop}</option>
              </select>
            </Field>
            <Button type="submit" disabled={!groups?.length}>
              부여
            </Button>
          </form>
          {access.groupGrants.length === 0 ? (
            <p className="ui-empty-inline">그룹 권한이 없습니다.</p>
          ) : (
            <ul className="access-members">
              {access.groupGrants.map((g) => (
                <li key={`${g.groupId}-${g.environment}-${g.permission}`}>
                  <span>
                    {groupName(g.groupId)} · {environmentLabel[g.environment]} · {permissionLabel[g.permission]}
                  </span>
                  <Button size="sm" onClick={() => void apply("/api/v1/group-grants", { ...g, appId, enabled: false }, "그룹 권한을 회수했습니다.")}>
                    회수
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <h3>사용자 권한</h3>
          {access.userGrants.length === 0 ? (
            <p className="ui-empty-inline">사용자에게 직접 준 권한이 없습니다.</p>
          ) : (
            <ul className="access-members">
              {access.userGrants.map((g) => (
                <li key={`${g.userId}-${g.environment}-${g.permission}`}>
                  <span>
                    {userName(g.userId)} · {environmentLabel[g.environment]} · {permissionLabel[g.permission]}
                  </span>
                  <Button
                    size="sm"
                    onClick={() =>
                      void apply(
                        "/api/v1/grants",
                        {
                          userId: g.userId,
                          appId,
                          environment: g.environment,
                          permission: g.permission,
                          enabled: false,
                        },
                        "권한을 회수했습니다.",
                      )
                    }
                  >
                    회수
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </Panel>
  );
}
