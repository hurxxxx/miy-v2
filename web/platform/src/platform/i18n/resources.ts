// The common, auth and shell namespaces are AI-DO's resources
// (apps/web/src/platform/i18n/resources.ts at 0002af062) unchanged. portal holds
// the strings of miy's own screens: app environments, the console and CLI.
import koCommon from "./locales/ko-KR/common.json";
import koAuth from "./locales/ko-KR/auth.json";
import koShell from "./locales/ko-KR/shell.json";
import koPortal from "./locales/ko-KR/portal.json";
import enCommon from "./locales/en-US/common.json";
import enAuth from "./locales/en-US/auth.json";
import enShell from "./locales/en-US/shell.json";
import enPortal from "./locales/en-US/portal.json";

export const resources = {
  "ko-KR": { common: koCommon, auth: koAuth, shell: koShell, portal: koPortal },
  "en-US": { common: enCommon, auth: enAuth, shell: enShell, portal: enPortal },
} as const;
