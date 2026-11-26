import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { isApiError, request } from "@miy/sdk";
import { DirectoryPicker, type DirectoryOption, type DirectoryProvider } from "@miy/ui";
import { useAdminAppRead } from "../data/admin-apps";
import { fetchAdminUsers } from "../pages/admin-users-directory";
import { Button, Field, Loading } from "../ui";

type Assignment = {
  id: string;
  subjectName: string;
  subjectId: string;
  status: "pending" | "issued" | "applied" | "blocked";
  reason: string | null;
};

const users: DirectoryProvider = {
  async load({ query, cursor, limit, signal }) {
    const page = await fetchAdminUsers({ q: query, status: "enabled" }, { cursor, limit, signal });
    return {
      items: page.items.map((user) => ({ kind: "user", id: user.id, name: user.name, description: user.id })),
      nextCursor: page.nextCursor,
    };
  },
};

export function StewardAssignment({ installationId, csrf }: { installationId: string; csrf: string }) {
  const { t } = useTranslation("console");
  const path = `/api/v1/installations/${installationId}/steward-assignment`;
  const view = useAdminAppRead<{ approved: boolean; assignment: Assignment | null }>(path);
  const [selected, select] = useState<DirectoryOption[]>([]);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const assignment = view.data?.assignment;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !selected.length || !view.data?.approved) return;
    setBusy(true);
    setError("");
    try {
      await request(path, csrf, { method: "PUT", body: JSON.stringify({
        subjectId: selected[0].id, expectedAssignmentId: assignment?.id ?? null, reason: reason.trim() || null,
      }) });
      select([]);
      setReason("");
    } catch (caught) {
      setError(isApiError(caught) && caught.status === 409 ? "changed" : "failed");
    } finally {
      await view.refresh();
      setBusy(false);
    }
  }
  return (
    <section className="grid gap-2">
      <h3 className="font-semibold">{t("stewards.title")}</h3>
      <p>{t("stewards.policy")}</p>
      {view.error && <p role="alert">{t("stewards.loadFailed")}</p>}
      {error && <p role="alert">{t(`stewards.${error}`)}</p>}
      {!view.data && !view.error && <Loading />}
      {view.data && !assignment && <p>{t("stewards.empty")}</p>}
      {assignment && (
        <div role="status">
          <p>{t(`stewards.${assignment.status}`, { name: assignment.subjectName })}</p>
          {assignment.reason && <p>{t("stewards.recordedReason", { reason: assignment.reason })}</p>}
        </div>
      )}
      <Button size="sm" disabled={busy} onClick={() => void view.refresh()}>{t("stewards.refresh")}</Button>
      {view.data?.approved && !view.error && (
        <form className="grid gap-2" onSubmit={submit}>
          {assignment && <p>{t("stewards.recoveryPolicy")}</p>}
          <DirectoryPicker label={t("stewards.person")} provider={users} kinds={["user"]}
            value={selected} onChange={(values) => select(values.slice(-1))} disabled={busy}
            labels={{ users: t("groups.users"), searchPlaceholder: t("groups.searchUsers"), loading: t("groups.loadingUsers"),
              unavailable: t("groups.usersLoadFailed"), noMatches: t("common:empty.noResults"), retry: t("common:actions.retry"),
              loadMore: t("groups.moreUsers"), remove: (name) => t("groups.undoSelection", { name }) }} />
          {assignment && <Field label={t("stewards.reason")}>
            <input value={reason} onChange={(event) => setReason(event.target.value)} required maxLength={500} />
          </Field>}
          <Button type="submit" size="sm" disabled={busy || !selected.length || (!!assignment && !reason.trim())}>
            {t(assignment ? "stewards.recover" : "stewards.assign")}
          </Button>
        </form>
      )}
    </section>
  );
}
