import { useEffect, useState } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import { Trans, useTranslation } from "react-i18next";
import { request } from "@miy/sdk";
import { Button } from "../ui";

type InstallGuide = {
  schemaVersion: number;
  serverUrl: string;
  cliVersion: string;
  commands: { unix: string; windows: string; login: string };
  downloads?: { target: string; file: string; sha256: string; url: string; checksumUrl: string }[];
  sourceCommit?: string;
  windowsInstaller?: { file: string; sha256: string; url: string; checksumUrl: string };
};

const chapters = [
  ["agent-setup", "agentSection"],
  ["cli-setup", "cliSection"],
  ["cli-usage", "usageTitle"],
  ["cli-reference", "referenceTitle"],
  ["mcp-setup", "mcpTitle"],
  ["maintenance", "maintenanceTitle"],
] as const;
type ChapterId = typeof chapters[number][0];

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

function GuideLink({ chapter }: { chapter: ChapterId }) {
  const { t } = useTranslation("portal");
  const entry = chapters.find(([id]) => id === chapter)!;
  return <Link to="/cli/install" hash={chapter} activeOptions={{ includeHash: true }} resetScroll={false}>{t(`cliInstall.${entry[1]}`)}</Link>;
}

function Step({ title, body, command }: { title: string; body: string; command: string }) {
  const { t } = useTranslation("portal");
  return <section className="setup-step">
    <h3>{t(`cliInstall.${title}`)}</h3>
    <p><Trans t={t} i18nKey={`cliInstall.${body}`} components={markup} /></p>
    <Command label={t(`cliInstall.${title}`)} command={command} />
  </section>;
}

function AgentSetup({ guide }: { guide: InstallGuide }) {
  const { t } = useTranslation("portal");
  const [agent, setAgent] = useState<"Codex" | "Claude Code">("Codex");
  const agentUrl = new URL("/api/v1/cli/agent-setup", guide.serverUrl).href;
  const codex = agent === "Codex";
  return <>
    <p>{t("cliInstall.agentIntro")}</p>
    <div className="setup-agent-picker" role="group" aria-label={t("cliInstall.agentChoice")}>
      {(["Codex", "Claude Code"] as const).map((name) => <Button key={name} aria-pressed={agent === name}
        variant={agent === name ? "primary" : "secondary"} onClick={() => setAgent(name)}>{name}</Button>)}
    </div>
    <h3>{t("cliInstall.agentOpen", { agent })}</h3>
    <p><Trans t={t} i18nKey={codex ? "cliInstall.codexSteps" : "cliInstall.claudeSteps"} components={markup} />{" "}
      <a href={codex ? "https://developers.openai.com/codex/quickstart/" : "https://code.claude.com/docs/en/quickstart"}>{t("cliInstall.agentInstall", { agent })}</a>
    </p>
    <h3>{t("cliInstall.agentRequest", { agent })}</h3>
    <Command key={agent} label={t("cliInstall.agentRequest", { agent })}
      command={t("cliInstall.agentPrompt", { agent, url: agentUrl, interpolation: { escapeValue: false } })} />
    <p><a href={agentUrl}>{t("cliInstall.agentGuide")}</a></p>
    <h3>{t("cliInstall.agentConfirm")}</h3>
    <p><Trans t={t} i18nKey="cliInstall.verifyAgent" components={markup} /></p>
    <p>{t("cliInstall.agentNext")} <GuideLink chapter="mcp-setup" /></p>
    <p>{t("cliInstall.signupLead")} <a href={new URL("/signup", guide.serverUrl).href}>{t("signup.title")}</a></p>
  </>;
}

function Install({ guide }: { guide: InstallGuide }) {
  const { t } = useTranslation("portal");
  return <>
    <section>
      <h3>{t("cliInstall.oneLineTitle")}</h3>
      <p>{t("cliInstall.oneLineLead")}</p>
      <div className="cli-install-platforms">
        <section>
          <h4>Linux · macOS</h4>
          {guide.downloads?.some((item) => item.target !== "x86_64-pc-windows-msvc") ? <>
            <Command label={t("cliInstall.unixCommand")} command={guide.commands.unix} />
            <p><Trans t={t} i18nKey="cliInstall.unixNote" components={markup} /></p>
          </> : <p>{t("cliInstall.unixMissing")}</p>}
        </section>
        <section>
          <h4>Windows</h4>
          {guide.downloads?.some((item) => item.target === "x86_64-pc-windows-msvc") ? <>
            <Command label={t("cliInstall.windowsCommand")} command={guide.commands.windows} />
            <p>{t("cliInstall.windowsNote")}</p>
          </> : <p>{t("cliInstall.windowsMissing")}</p>}
        </section>
      </div>
    </section>
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
    <section>
      <h3>{t("cliInstall.loginTitle")}</h3>
      <p>{t("cliInstall.signupLead")} <a href={new URL("/signup", guide.serverUrl).href}>{t("signup.title")}</a></p>
      <p>{t("cliInstall.loginRegisters")}</p>
      {guide.serverUrl.startsWith("http://") && <p>{t("cliInstall.httpWarning")}</p>}
      <p>{t("cliInstall.loginWhen")}</p>
      <Command label={t("cliInstall.loginCommand")} command={guide.commands.login} />
      <p><Trans t={t} i18nKey="cliInstall.contextList" components={markup} /></p>
    </section>

    <Step title="installCheck" body="installCheckBody" command={"miy --version\nmiy auth status"} />
    <GuideLink chapter="cli-usage" />
  </>;
}

const initCommand = 'miy app init ./my-app --id my-app --name "My App"';

function Usage() {
  return <>
    <Step title="usageContext" body="usageContextBody" command={"miy context list\nmiy context show\nmiy auth status"} />
    <Step title="usageExisting" body="usageExistingBody" command="miy app setup" />
    <Step title="usageNew" body="usageNewBody" command={initCommand + "\ncd my-app"} />
    <Step title="usageCheck" body="usageCheckBody" command={"miy component sync\nmiy app check --harness-only\nmiy doctor --for develop"} />
    <Step title="usagePreview" body="usagePreviewBody" command="miy app dev" />
    <Step title="usageRelease" body="usageReleaseBody" command={'miy app status\nmiy describe --command "app build"'} />
    <GuideLink chapter="cli-reference" />
  </>;
}

const appCommands = [
  ["miy app list", "refList"],
  ["miy app register", "refRegister"],
  ["miy app check", "refCheck"],
  ["miy app build", "refBuild"],
  ["miy app deploy --environment dev --dry-run", "refDryRun"],
  ["miy app status", "refStatus"],
] as const;

const reference = [
  ["refHelp", "miy --help\nmiy app --help"],
  ["refSearch", "miy describe --search deploy"],
  ["refCommand", 'miy describe --command "app build"'],
  ["refApi", "miy describe --topic api"],
  ["refHarness", "miy describe --topic harness"],
  ["refUpdates", "miy describe --topic updates"],
  ["refMachine", "miy --output json --no-input context show"],
] as const;

function Reference() {
  const { t } = useTranslation("portal");
  return <>
    <p><Trans t={t} i18nKey="cliInstall.refIntro" components={markup} /></p>
    <h3>{t("cliInstall.refCommon")}</h3>
    <dl className="setup-command-index">{appCommands.map(([command, key]) => <div key={command}>
      <dt><code>{command}</code></dt><dd>{t(`cliInstall.${key}`)}</dd>
    </div>)}</dl>
    <h3>{t("cliInstall.refLookup")}</h3>
    <dl className="setup-reference">{reference.map(([key, command]) => <div key={key}>
      <dt>{t(`cliInstall.${key}`)}</dt>
      <dd><Command label={t(`cliInstall.${key}`)} command={command} /></dd>
    </div>)}</dl>
  </>;
}

function McpSetup({ guide }: { guide: InstallGuide }) {
  const { t } = useTranslation("portal");
  return <>
    <p><Trans t={t} i18nKey="cliInstall.mcpIntro" components={markup} /></p>
    <Step title="mcpLocal" body="mcpLocalBody" command="miy app setup" />
    <p>{t("cliInstall.mcpNew")} <GuideLink chapter="cli-usage" /></p>
    <Step title="mcpCodex" body="mcpCodexBody" command="codex mcp get miy_app" />
    <Step title="mcpClaude" body="mcpClaudeBody" command="claude mcp get miy_app" />
    <Step title="mcpTools" body="mcpToolsBody" command={'get_cli_help({"topic":"api"})\nget_contract({})'} />
    <Step title="mcpRemote" body="mcpRemoteBody" command={new URL("/mcp", guide.serverUrl).href} />
    <p><Trans t={t} i18nKey="cliInstall.mcpRemoteVerify" components={markup} /></p>
    <GuideLink chapter="maintenance" />
  </>;
}

function Maintenance({ guide }: { guide: InstallGuide }) {
  const { t } = useTranslation("portal");
  return <>
    <h3>{t("cliInstall.updateTitle")}</h3>
    <p>{t("cliInstall.updateStop")} <GuideLink chapter="cli-setup" /></p>
    <Command label={t("cliInstall.versionCommand")} command="miy --version" />
    <p>{t("cliInstall.updateProjects")}</p>
    <Command label={t("cliInstall.setupCommand")} command="miy app setup" />
    <p><Trans t={t} i18nKey="cliInstall.reopenAgents" components={markup} /></p>
    <p><Trans t={t} i18nKey="cliInstall.projectBinary" components={markup} /></p>
    <Step title="manageSessions" body="manageSessionsBody" command="miy auth sessions list" />
    <p><a href={new URL("/cli/sessions", guide.serverUrl).href}>{t("cliInstall.manageSessionsLink")}</a></p>
    <Step title="manageLogin" body="manageLoginBody" command={"miy context show\nmiy auth status\nmiy doctor --for operate"} />
    <Step title="manageTools" body="manageToolsBody" command="miy doctor --for develop" />
  </>;
}

export function CliInstall() {
  const { t } = useTranslation("portal");
  const hash = useLocation({ select: (location) => location.hash });
  const [chapter, titleKey] = chapters.find(([id]) => id === hash) ?? chapters[0];
  const [guide, setGuide] = useState<InstallGuide>();
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true;
    request<InstallGuide>("/api/v1/cli/install")
      .then((result) => { if (active) setGuide(result); })
      .catch(() => { if (active) setError(true); });
    return () => { active = false; };
  }, []);
  return <section className="cli-install" aria-labelledby="cli-install-title">
    <h1 id="cli-install-title">{t("cliInstall.title")}</h1>
    <p>{t("cliInstall.lead")}</p>
    {error && <p role="alert">{t("cliInstall.loadFailed")}</p>}
    {!guide && !error && <p role="status">{t("cliInstall.loading")}</p>}
    {guide && <div className="setup-layout">
      <nav className="setup-sections" aria-label={t("cliInstall.sections")}>
        {chapters.map(([id, key]) => <Link key={id} to="/cli/install" hash={id} activeOptions={{ includeHash: true }}
          resetScroll={false} aria-current={id === chapter ? "page" : undefined}>{t(`cliInstall.${key}`)}</Link>)}
      </nav>
      <article className="setup-article" id={chapter} key={chapter} aria-labelledby="setup-chapter-title">
        <h2 id="setup-chapter-title">{t(`cliInstall.${titleKey}`)}</h2>
        <p className="setup-server">{t("cliInstall.server")} <strong data-testid="cli-install-server">{guide.serverUrl}</strong> · CLI {guide.cliVersion}</p>
        {chapter === "agent-setup" && <AgentSetup guide={guide} />}
        {chapter === "cli-setup" && <Install guide={guide} />}
        {chapter === "cli-usage" && <Usage />}
        {chapter === "cli-reference" && <Reference />}
        {chapter === "mcp-setup" && <McpSetup guide={guide} />}
        {chapter === "maintenance" && <Maintenance guide={guide} />}
      </article>
    </div>}
  </section>;
}
