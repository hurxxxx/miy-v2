import { Trans, useTranslation } from "react-i18next";
import { Notice, Panel } from "../ui";
import { useUser } from "../data/session";
import { CliDeviceApproval } from "./LoginRecovery";

/**
 * The pre-0.5 browser-approval CLI login is retired: a single click here could
 * hand a CLI credential to whoever started the request. Supported CLIs use the
 * device login, which needs the code shown in the user's own terminal.
 */
export function CliAuthorize() {
  const { t } = useTranslation("portal");
  return (
    <Panel title={t("cliAuthorize.title")}>
      <Notice tone="warning" role="alert">
        {t("cliAuthorize.retired")}
      </Notice>
      <p>
        <Trans t={t} i18nKey="cliAuthorize.upgrade" components={{ code: <code /> }} />
      </p>
    </Panel>
  );
}

export function CliDeviceApprove() {
  return <CliDeviceApproval user={useUser()} />;
}
