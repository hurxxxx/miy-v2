import { useEffect, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { request } from "@miy/sdk";
import { Button, Panel } from "../ui";

type InstallGuide = {
  schemaVersion: number;
  serverUrl: string;
  cliVersion: string;
  commands: { unix: string; windows: string; login: string };
  downloads?: { target: string; file: string; sha256: string; url: string; checksumUrl: string }[];
  sourceCommit?: string;
  windowsInstaller?: { file: string; sha256: string; url: string; checksumUrl: string };
};

// Inline markup of the translated paragraphs.
const markup = { code: <code /> };

function Command({ label, command }: { label: string; command: string }) {
  const { t } = useTranslation("portal");
  const [message, setMessage] = useState("");
  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setMessage(t("cliInstall.copied"));
    } catch {
      setMessage(t("cliInstall.copyManually"));
    }
  }
  return (
    <div className="cli-install-command">
      <pre aria-label={label}><code>{command}</code></pre>
      <Button type="button" aria-label={t("cliInstall.copyLabel", { label })} onClick={copy}>{t("cliInstall.copy")}</Button>
      {message && <p role="status">{message}</p>}
    </div>
  );
}

export function CliInstall() {
  const { t } = useTranslation("portal");
  const [guide, setGuide] = useState<InstallGuide>();
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true;
    request<InstallGuide>("/api/v1/cli/install")
      .then((result) => { if (active) setGuide(result); })
      .catch(() => { if (active) setError(true); });
    return () => { active = false; };
  }, []);
  const agentUrl = guide && new URL("/api/v1/cli/agent-setup", guide.serverUrl).href;
  return (
    <section className="cli-install" aria-labelledby="cli-install-title">
      <h1 id="cli-install-title">{t("cliInstall.title")}</h1>
      <p>{t("cliInstall.lead")}</p>
      {error && <p role="alert">{t("cliInstall.loadFailed")}</p>}
      {!guide && !error && <p role="status">{t("cliInstall.loading")}</p>}
      {guide && <>
        <p>{t("cliInstall.server")} <strong data-testid="cli-install-server">{guide.serverUrl}</strong> · CLI {guide.cliVersion}</p>
        <nav className="setup-sections" aria-label={t("cliInstall.sections")}>
          <a href="#cli-setup">{t("cliInstall.cliSection")}</a>
          <a href="#agent-setup">{t("cliInstall.agentSection")}</a>
        </nav>
        <section id="cli-setup" aria-label={t("cliInstall.cliSection")}>
          <Panel title={t("cliInstall.oneLineTitle")}>
            <p>{t("cliInstall.oneLineLead")}</p>
            <div className="cli-install-platforms">
              <Panel title="Linux · macOS">
                {guide.downloads?.some((item) => item.target !== "x86_64-pc-windows-msvc") ? <>
                  <Command label={t("cliInstall.unixCommand")} command={guide.commands.unix} />
                  <p><Trans t={t} i18nKey="cliInstall.unixNote" components={markup} /></p>
                </> : <p>{t("cliInstall.unixMissing")}</p>}
              </Panel>
              <Panel title="Windows">
                {guide.downloads?.some((item) => item.target === "x86_64-pc-windows-msvc") ? <>
                  <Command label={t("cliInstall.windowsCommand")} command={guide.commands.windows} />
                  <p>{t("cliInstall.windowsNote")}</p>
                </> : <p>{t("cliInstall.windowsMissing")}</p>}
              </Panel>
            </div>
          </Panel>
          <details>
            <summary>{t("cliInstall.directDownload")}</summary>
            {guide.downloads?.length ? <ul>{guide.downloads.map((download) => <li key={download.target}>
              <a href={download.url}>{download.file}</a>{" · "}<a href={download.checksumUrl}>SHA-256</a>
            </li>)}</ul> : <p>{t("cliInstall.noPackages")}</p>}
            {guide.sourceCommit && <p>{t("cliInstall.packageSource")} <code>{guide.sourceCommit}</code></p>}
            {guide.windowsInstaller && <>
              <p><a className="cli-install-download" href={guide.windowsInstaller.url} download={guide.windowsInstaller.file}>{t("cliInstall.windowsInstaller")}</a>{" · "}<a href={guide.windowsInstaller.checksumUrl}>{t("cliInstall.windowsInstallerChecksum")}</a></p>
              <p>{t("cliInstall.guiServer")}</p>
              <Command label={t("cliInstall.guiServerCommand")} command={guide.serverUrl} />
            </>}
          </details>
          <Panel title={t("cliInstall.loginTitle")}>
            <p>{t("cliInstall.loginRegisters")}</p>
            {guide.serverUrl.startsWith("http://") && <p>{t("cliInstall.httpWarning")}</p>}
            <p>{t("cliInstall.loginWhen")}</p>
            <Command label={t("cliInstall.loginCommand")} command={guide.commands.login} />
            <p><Trans t={t} i18nKey="cliInstall.contextList" components={markup} /></p>
          </Panel>
        </section>
        <section id="agent-setup" aria-label={t("cliInstall.agentSection")}>
          <Panel title={t("cliInstall.agentSection")}>
            <p>{t("cliInstall.agentIntro")}</p>
            <p><a href={agentUrl}>{t("cliInstall.agentGuide")}</a></p>
            {(["Codex", "Claude Code"] as const).map((agent) => (
              <Panel key={agent} title={agent}>
                <p><Trans t={t} i18nKey={agent === "Codex" ? "cliInstall.codexSteps" : "cliInstall.claudeSteps"} components={markup} /></p>
                <Command label={t("cliInstall.agentRequest", { agent })}
                  command={t("cliInstall.agentPrompt", { agent, url: agentUrl, interpolation: { escapeValue: false } })} />
              </Panel>
            ))}
            <p><Trans t={t} i18nKey="cliInstall.verifyAgent" components={markup} /></p>
          </Panel>
        </section>
        <Panel title={t("cliInstall.updateTitle")}>
          <p>{t("cliInstall.updateStop")}</p>
          <Command label={t("cliInstall.versionCommand")} command="miy --version" />
          <p>{t("cliInstall.updateProjects")}</p>
          <Command label={t("cliInstall.setupCommand")} command="miy app setup" />
          <p><Trans t={t} i18nKey="cliInstall.reopenAgents" components={markup} /></p>
          <p><Trans t={t} i18nKey="cliInstall.projectBinary" components={markup} /></p>
        </Panel>
      </>}
    </section>
  );
}
