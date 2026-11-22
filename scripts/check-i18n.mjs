#!/usr/bin/env node
// Static checks of the portal's translations (web/platform):
// 1. every namespace has the same keys in ko-KR and en-US (i18next plural
//    suffixes count as one key),
// 2. every literal translation key the code uses exists in ko-KR,
// 3. screen code has no Korean string literals outside the translation files,
//    except the files listed below that are still being translated.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "web/platform/src");
const locales = join(source, "platform/i18n/locales");
const LANGUAGES = ["ko-KR", "en-US"];

// Screens that still carry Korean literals. Remove a file from this list when
// it is translated; the check fails if a listed file has none left.
const UNTRANSLATED = new Set([
  "app/AppFrame.tsx",
  "app/AppView.tsx",
  "components/LoadState.tsx",
  "components/NewTabLink.tsx",
  "components/RuntimeControls.tsx",
  "components/RuntimeHealth.tsx",
  "data/installations.tsx",
  "pages/Admin.tsx",
  "pages/AdminAccess.tsx",
  "pages/AdminAudit.tsx",
  "pages/AdminReports.tsx",
  "pages/AdminUsers.tsx",
  "pages/CliAuthorize.tsx",
  "pages/CliInstall.tsx",
  "pages/CliSessions.tsx",
  "pages/Home.tsx",
  "pages/Join.tsx",
  "pages/Login.tsx",
  "pages/LoginRecovery.tsx",
  "pages/ManagedApps.tsx",
  "ui/index.tsx",
]);

const failures = [];
const plural = /_(zero|one|two|few|many|other)$/;
function flatten(value, prefix = "") {
  return Object.entries(value).flatMap(([key, child]) =>
    typeof child === "object" && child !== null ? flatten(child, `${prefix}${key}.`) : [`${prefix}${key}`],
  );
}
const resources = {};
for (const language of LANGUAGES) {
  resources[language] = {};
  for (const file of readdirSync(join(locales, language)).filter((name) => name.endsWith(".json"))) {
    const namespace = file.replace(/\.json$/, "");
    resources[language][namespace] = new Set(
      flatten(JSON.parse(readFileSync(join(locales, language, file), "utf8"))).map((key) => key.replace(plural, "")),
    );
  }
}
for (const namespace of new Set(LANGUAGES.flatMap((language) => Object.keys(resources[language])))) {
  const [ko, en] = LANGUAGES.map((language) => resources[language][namespace] ?? new Set());
  for (const key of ko) if (!en.has(key)) failures.push(`en-US ${namespace}: missing ${key}`);
  for (const key of en) if (!ko.has(key)) failures.push(`ko-KR ${namespace}: missing ${key}`);
}

function files(directory) {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) return name === "locales" ? [] : files(path);
    return /\.(ts|tsx)$/.test(name) ? [path] : [];
  });
}
const korean = /[ㄱ-ㆎ가-힣]/;
for (const path of files(source)) {
  const name = relative(source, path);
  const text = readFileSync(path, "utf8");
  // Keys: t("ns:key") anywhere, and t("key") in files with one default namespace.
  const defaults = [...text.matchAll(/useTranslation\(\s*["'](\w+)["']/g)].map((match) => match[1]);
  const fallback = defaults.length === 1 ? defaults[0] : text.includes("useTranslation(") ? null : "common";
  for (const match of text.matchAll(/\bt\(\s*["']([^"'`$]+)["']/g)) {
    const [namespace, key] = match[1].includes(":") ? match[1].split(":", 2) : [fallback, match[1]];
    if (!namespace) continue;
    if (!resources["ko-KR"][namespace]?.has(key)) failures.push(`${name}: unknown translation key ${namespace}:${key}`);
  }
  // Korean literals: comments and lines marked i18n-allow are fine.
  const code = text
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .filter((line) => !line.includes("i18n-allow"))
    .map((line) => line.replace(/\/\/.*$/, ""))
    .join("\n");
  const hasKorean = korean.test(code);
  if (hasKorean && !UNTRANSLATED.has(name)) failures.push(`${name}: Korean text outside the translation files`);
  if (!hasKorean && UNTRANSLATED.has(name)) failures.push(`${name}: translated; remove it from UNTRANSLATED`);
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log(`PASS i18n: ${LANGUAGES.join(", ")} keys match; used keys exist; ${UNTRANSLATED.size} screens still to translate`);
