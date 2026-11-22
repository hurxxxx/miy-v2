#!/usr/bin/env node
// miy-authored screens take colors from the --ui-* tokens (packages/tokens) so
// light and dark stay consistent. This rejects color literals (hex, rgb(),
// hsl()) in the portal, the frame runtime and the miy composite components of
// the common UI. Code copied from AI-DO keeps its original values and is not
// scanned (packages/ui/src/lib, styles.css). A line may carry `color-allow:`
// with a reason when a fixed color is intended.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const scanned = [
  "web/platform/src",
  "web/frame-runtime/src",
  "packages/ui/src/directory.tsx",
  "packages/ui/src/sharing.tsx",
  "packages/ui/src/files.tsx",
  "packages/ui/src/utilities.ts",
  "packages/ui/src/internal",
];
const literal = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(/;

function files(path) {
  if (!statSync(path).isDirectory()) return [path];
  return readdirSync(path).flatMap((name) => (name === "locales" ? [] : files(join(path, name))));
}
const failures = [];
let count = 0;
for (const path of scanned.flatMap((entry) => files(join(root, entry)))) {
  if (!/\.(css|ts|tsx)$/.test(path)) continue;
  count += 1;
  readFileSync(path, "utf8")
    .split("\n")
    .forEach((line, index) => {
      if (line.includes("color-allow:")) return;
      // Hex inside URLs or ids (e.g. "#platform-main") is not a color.
      const code = line.replace(/["']#[a-z][a-z0-9-]*["']/gi, "").replace(/href=\{?["'`][^"'`]*["'`]\}?/g, "");
      if (literal.test(code)) failures.push(`${relative(root, path)}:${index + 1}: ${line.trim()}`);
    });
}
if (failures.length) {
  console.error("Use a --ui-* token instead of a color literal:\n" + failures.join("\n"));
  process.exit(1);
}
console.log(`PASS hardcoded colors: ${count} miy-authored style and screen files use tokens`);
