#!/usr/bin/env node
// The workspace and generated apps use the same maintained UI source. Versioned
// registry files are immutable: --write only creates an untracked candidate.
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = "1.1.0";
const bundleVersion = "0.10.0";
const source = join(root, "packages/ui/src");
const destination = join(root, "registry/components/ui", version);

function filesAt(directory, base = directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory).sort().flatMap(name => {
    const path = join(directory, name);
    const info = lstatSync(path);
    if (info.isSymbolicLink()) throw new Error("UI package cannot contain symlinks");
    if (info.isDirectory()) return filesAt(path, base);
    if (!info.isFile()) throw new Error("UI package must contain regular files");
    return [relative(base, path).replaceAll("\\", "/")];
  });
}

const files = new Map();
for (const path of filesAt(source)) {
  if (!/\.(?:ts|tsx|css|svg|json)$/.test(path))
    throw new Error(`Unsupported UI source file: ${path}`);
  if (!path.split("/").every(part => /^[a-zA-Z0-9_-][a-zA-Z0-9_.-]*$/.test(part)))
    throw new Error(`Invalid UI source path: ${path}`);
  const value = readFileSync(join(source, path), "utf8");
  if (value.includes("\r")) throw new Error(`UI source must use LF: ${path}`);
  files.set(path, value);
}
if (!files.has("index.tsx")) throw new Error("UI entrypoint is missing");
files.set("README.md", `# miy common UI ${version}

Generated from the maintained packages/ui source. Use SDK bundle ${bundleVersion}.
Install through miy component add ui --version ${version}; do not edit the
installed component or use platform administrator APIs as a data provider.

Import components from the installed index and include its styles.css once in
the app entrypoint. The workspace equivalent is @miy/ui and @miy/ui/styles.css.
React is supplied by the app bundle; this component installs no npm packages.

Directory and other data providers return only the app's permitted public DTOs.
Selection and disabled controls do not grant authority: the app server checks
current identity, membership and resource ACL again for each operation.
`);
const metadata = {
  protocol: "miy.component/v1",
  name: "ui",
  version,
  description: "AI-DO common primitives, directory selection, resource sharing and small attachments",
  uiContract: "miy.ui/v1alpha1",
  bundleVersions: [bundleVersion],
  files: [...files.keys()].sort(),
};
files.set("component.json", JSON.stringify(metadata, null, 2) + "\n");
if ([...files.values()].reduce((sum, value) => sum + Buffer.byteLength(value), 0) > 2 * 1024 * 1024)
  throw new Error("UI package exceeds the component size limit");

if (process.argv.includes("--check")) {
  const actual = filesAt(destination);
  const expected = [...files.keys()].sort();
  if (JSON.stringify(actual.sort()) !== JSON.stringify(expected))
    throw new Error("UI registry candidate file inventory differs from maintained source");
  for (const [path, value] of files)
    if (readFileSync(join(destination, path), "utf8") !== value)
      throw new Error(`UI registry candidate is stale: ${path}`);
  console.log(`PASS UI ${version}: ${files.size - 1} package files match maintained source`);
} else if (process.argv.includes("--write")) {
  const tracked = execFileSync("git", ["ls-files", "--", relative(root, destination)], { cwd: root, encoding: "utf8" }).trim();
  if (tracked) throw new Error("Refusing to overwrite a tracked UI version; choose a new version");
  for (const path of filesAt(destination))
    if (!files.has(path)) throw new Error(`Refusing to delete an unexpected candidate file: ${path}`);
  for (const [path, value] of files) {
    const output = join(destination, path);
    mkdirSync(dirname(output), { recursive: true });
    writeFileSync(output, value);
  }
  console.log(`Wrote untracked UI ${version} candidate with ${files.size - 1} package files`);
} else {
  throw new Error("Use --check, or --write to create the new untracked version");
}
