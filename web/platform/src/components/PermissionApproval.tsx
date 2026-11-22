import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ApiError, request } from "@miy/sdk";
import { Badge, Button } from "../ui";

/** GET /api/v1/installations/{id}/permissions for the current release. */
type Approval = {
  installationId: string;
  releaseId: string;
  requestedPermissions: string[];
  approvedPermissions: string[];
  revision: number;
  approvedAt: string | null;
};

// The database is the app's own; only platform permissions need approval
// (crates/miy/src/permissions.rs `external`).
const external = (permission: string) => permission !== "database";

const LABELS: Record<string, string> = {
  "platform:directory:read": "directoryRead",
  "platform:files:read": "filesRead",
  "platform:files:write": "filesWrite",
};

const strings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string");

/** An answer of another shape (an older server, a proxy page) is a load failure, never a crash. */
function checked(value: unknown): Approval {
  const approval = value as Partial<Approval> | null;
  if (
    !approval ||
    !strings(approval.requestedPermissions) ||
    !strings(approval.approvedPermissions) ||
    typeof approval.revision !== "number"
  )
    throw new Error("invalid_permission_approval");
  return approval as Approval;
}

/** Requested platform permissions not yet approved for the installation's release. */
export function pendingPermissions(approval: Approval) {
  return approval.requestedPermissions.filter(
    (permission) => external(permission) && !approval.approvedPermissions.includes(permission),
  );
}

/**
 * The release's requested platform permissions against what an administrator
 * approved, with approval in place. The server decides who may approve
 * (an undelegated administrator) and rejects a stale revision.
 */
export function PermissionApproval({
  installationId,
  releaseId,
  csrf,
  onChange,
}: {
  installationId: string;
  releaseId: string;
  csrf: string;
  onChange?: (pending: number) => void;
}) {
  const { t } = useTranslation("console");
  const [approval, setApproval] = useState<Approval | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const adopt = useCallback(
    (next: Approval) => {
      setApproval(next);
      setSelected(next.approvedPermissions.filter(external));
      onChange?.(pendingPermissions(next).length);
    },
    [onChange],
  );
  // After a refused save the reload keeps the reason on screen.
  const load = useCallback(async (keepError = false) => {
    try {
      adopt(
        checked(
          await request<unknown>(
            `/api/v1/installations/${installationId}/permissions?releaseId=${encodeURIComponent(releaseId)}`,
          ),
        ),
      );
      if (!keepError) setError(null);
    } catch {
      setError(t("permissionApproval.loadFailed"));
    }
  }, [adopt, installationId, releaseId, t]);
  useEffect(() => {
    void load();
  }, [load]);

  if (!approval) return error ? <p className="ui-empty-inline">{error}</p> : null;
  const requested = approval.requestedPermissions.filter(external);
  if (requested.length === 0)
    return <p className="ui-empty-inline">{t("permissionApproval.none")}</p>;
  const pending = pendingPermissions(approval);
  const changed =
    selected.length !== approval.approvedPermissions.filter(external).length ||
    selected.some((permission) => !approval.approvedPermissions.includes(permission));

  async function save(permissions: string[]) {
    if (!approval) return;
    setSaving(true);
    try {
      adopt(
        checked(
          await request<unknown>(`/api/v1/installations/${installationId}/permissions`, csrf, {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ releaseId, expectedRevision: approval.revision, permissions }),
          }),
        ),
      );
      setError(null);
    } catch (caught) {
      setError(
        caught instanceof ApiError && caught.code === "permission_revision_changed"
          ? t("permissionApproval.changedElsewhere")
          : t("permissionApproval.saveFailed"),
      );
      if (caught instanceof ApiError && caught.code === "permission_revision_changed") void load(true);
    } finally {
      setSaving(false);
    }
  }

  return (
    <fieldset className="permission-approval">
      <legend>
        {t("permissionApproval.title")}{" "}
        {pending.length > 0 ? (
          <Badge tone="warning">{t("permissionApproval.pending", { count: pending.length })}</Badge>
        ) : (
          <Badge tone="success">{t("permissionApproval.approved")}</Badge>
        )}
      </legend>
      {requested.map((permission) => (
        <label key={permission} className="permission-approval-row">
          <input
            type="checkbox"
            checked={selected.includes(permission)}
            disabled={saving}
            onChange={(event) =>
              setSelected((current) =>
                event.target.checked ? [...current, permission] : current.filter((value) => value !== permission),
              )
            }
          />
          <span>
            {LABELS[permission] ? t(`permissionApproval.labels.${LABELS[permission]}`) : permission}
            <code>{permission}</code>
          </span>
        </label>
      ))}
      {error && (
        <p role="alert" className="ui-empty-inline">
          {error}
        </p>
      )}
      <div className="permission-approval-actions">
        {pending.length > 0 && (
          <Button size="sm" variant="primary" disabled={saving} onClick={() => void save(requested)}>
            {t("permissionApproval.approveAll")}
          </Button>
        )}
        <Button size="sm" disabled={saving || !changed} onClick={() => void save(selected)}>
          {t("permissionApproval.save")}
        </Button>
      </div>
    </fieldset>
  );
}
