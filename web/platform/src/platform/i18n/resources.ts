// The common, auth and shell namespaces are AI-DO's resources
// (apps/web/src/platform/i18n/resources.ts at 0002af062) unchanged. portal holds
// the strings of miy's own screens (app environments, CLI); console holds the
// administration screens of the management console. launcher holds AI-DO's
// app bar category admin strings and miy's own launcher messages.
import koCommon from "./locales/ko-KR/common.json";
import koAuth from "./locales/ko-KR/auth.json";
import koShell from "./locales/ko-KR/shell.json";
import koPortal from "./locales/ko-KR/portal.json";
import koConsole from "./locales/ko-KR/console.json";
import koLauncher from "./locales/ko-KR/launcher.json";
import enCommon from "./locales/en-US/common.json";
import enAuth from "./locales/en-US/auth.json";
import enShell from "./locales/en-US/shell.json";
import enPortal from "./locales/en-US/portal.json";
import enConsole from "./locales/en-US/console.json";
import enLauncher from "./locales/en-US/launcher.json";

export const resources = {
  "ko-KR": { common: koCommon, auth: koAuth, shell: koShell, portal: koPortal, console: koConsole, launcher: koLauncher },
  "en-US": { common: enCommon, auth: enAuth, shell: enShell, portal: enPortal, console: enConsole, launcher: enLauncher },
} as const;
