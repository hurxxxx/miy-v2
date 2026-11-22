import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { request } from "@miy/sdk";
import { Button } from "../ui";

/** GET/PUT/DELETE /api/v1/installations/{id}/file-quota (docs/CORE-PERMISSIONS.md "대용량 전송"). */
type Quota = {
  installationId: string;
  custom: boolean;
  maxBytes: number;
  maxFiles: number;
  usedBytes: number;
  usedFiles: number;
};

const GIB = 1024 * 1024 * 1024;

function checked(value: unknown): Quota {
  const quota = value as Partial<Quota> | null;
  if (
    !quota ||
    typeof quota.maxBytes !== "number" ||
    typeof quota.maxFiles !== "number" ||
    typeof quota.usedBytes !== "number" ||
    typeof quota.usedFiles !== "number"
  )
    throw new Error("invalid_file_quota");
  return quota as Quota;
}

function gib(bytes: number) {
  return Math.round((bytes / GIB) * 100) / 100;
}

/**
 * An installation's file storage limits, read when opened. The administrator
 * sets them or returns to the defaults; existing files stay and only new
 * uploads above the limit are refused. The server decides and audits.
 */
export function FileQuota({ installationId, csrf }: { installationId: string; csrf: string }) {
  const { t } = useTranslation("console");
  const [quota, setQuota] = useState<Quota | null>(null);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [bytes, setBytes] = useState("");
  const [files, setFiles] = useState("");
  const path = `/api/v1/installations/${encodeURIComponent(installationId)}/file-quota`;

  const adopt = (value: unknown) => {
    const next = checked(value);
    setQuota(next);
    setBytes(String(gib(next.maxBytes)));
    setFiles(String(next.maxFiles));
  };
  async function load() {
    setError(undefined);
    try {
      adopt(await request(path, csrf));
    } catch {
      setError(t("fileQuota.loadFailed"));
    }
  }
  async function send(method: "PUT" | "DELETE", body?: unknown) {
    setBusy(true);
    setError(undefined);
    try {
      adopt(await request(path, csrf, { method, body: body === undefined ? undefined : JSON.stringify(body) }));
    } catch {
      setError(t("fileQuota.saveFailed"));
    } finally {
      setBusy(false);
    }
  }
  function save(event: FormEvent) {
    event.preventDefault();
    const maxBytes = Math.round(Number(bytes) * GIB);
    const maxFiles = Number(files);
    if (!Number.isFinite(maxBytes) || maxBytes < 0 || !Number.isInteger(maxFiles) || maxFiles < 0) {
      setError(t("fileQuota.invalid"));
      return;
    }
    void send("PUT", { maxBytes, maxFiles });
  }

  return (
    <details
      className="file-quota"
      onToggle={(event) => {
        if ((event.currentTarget as HTMLDetailsElement).open && !quota) void load();
      }}
    >
      <summary>{t("fileQuota.title")}</summary>
      {error && <p role="alert" className="file-quota-error">{error}</p>}
      {quota && (
        <form className="file-quota-form" onSubmit={save}>
          <p>
            {t("fileQuota.usage", {
              used: gib(quota.usedBytes),
              max: gib(quota.maxBytes),
              usedFiles: quota.usedFiles,
              maxFiles: quota.maxFiles,
            })}{" "}
            {!quota.custom && <span>{t("fileQuota.default")}</span>}
          </p>
          <label>
            {t("fileQuota.maxBytes")}
            <input className="app-field-input" inputMode="decimal" value={bytes} onChange={(e) => setBytes(e.target.value)} />
          </label>
          <label>
            {t("fileQuota.maxFiles")}
            <input className="app-field-input" inputMode="numeric" value={files} onChange={(e) => setFiles(e.target.value)} />
          </label>
          <div className="file-quota-actions">
            <Button type="submit" size="sm" variant="primary" disabled={busy}>
              {t("fileQuota.save")}
            </Button>
            {quota.custom && (
              <Button size="sm" disabled={busy} onClick={() => void send("DELETE")}>
                {t("fileQuota.reset")}
              </Button>
            )}
          </div>
        </form>
      )}
    </details>
  );
}
