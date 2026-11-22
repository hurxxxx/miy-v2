#!/usr/bin/env node
// The workspace and generated apps use the same maintained UI source. Versioned
// registry files are immutable: --write only creates an untracked candidate.
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = "2.0.0";
const bundleVersion = "0.12.0";
const source = join(root, "packages/ui/src");
const destination = join(root, "registry/components/ui", version);

// The npm packages an app must list to build this component. Names and
// versions have one source, packages/ui/package.json: every runtime dependency
// except the ones each app already carries, plus the Tailwind build tools.
const suppliedByApp = ["react"];
const buildTools = ["tailwindcss", "@tailwindcss/vite"];
const manifest = JSON.parse(readFileSync(join(root, "packages/ui/package.json"), "utf8"));
const dependencies = {};
for (const [name, requirement] of Object.entries(manifest.dependencies))
  if (!suppliedByApp.includes(name)) dependencies[name] = requirement;
for (const name of buildTools) {
  if (!manifest.devDependencies?.[name]) throw new Error(`UI build tool is not declared: ${name}`);
  dependencies[name] = manifest.devDependencies[name];
}
// The same rule as the component contract (crates/miy/src/component_contract.rs).
for (const [name, requirement] of Object.entries(dependencies))
  if (!/^(?:@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]*$/.test(name) || !/^[a-zA-Z0-9.^~+-]{1,64}$/.test(requirement))
    throw new Error(`Unsupported UI dependency: ${name}@${requirement}`);
const sortedDependencies = Object.fromEntries(Object.entries(dependencies).sort(([a], [b]) => (a < b ? -1 : 1)));

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
const entrypoints = ["index.tsx", "index.ts"].filter(path => files.has(path));
if (entrypoints.length !== 1) throw new Error("UI package needs exactly one entrypoint: index.tsx or index.ts");
if (files.size + 2 > 128) throw new Error("UI package exceeds the component file limit");
for (const [path, value] of files)
  if (Buffer.byteLength(value) > 512 * 1024) throw new Error(`UI source file exceeds the component file size limit: ${path}`);
files.set("README.md", `# miy common UI ${version}

Generated from the maintained packages/ui source. Use SDK bundle ${bundleVersion}.
Install through miy component add ui --version ${version}; do not edit the
installed component or use platform administrator APIs as a data provider.

This version is the AI-DO UI package, moved as it is: lib/ holds AI-DO's
primitives, layout, overlay, data display and feedback components (Tailwind
CSS 4, Radix UI, lucide-react). The block editor is not included. The miy
composite components remain: DirectoryPicker, ShareEditor, FileTransferPanel
and FileImagePreview, with formatByteSize, requestId and moveItem. The 1.x
Field, Loading and classNames are gone (use Input, ContentState and cn), and
Button, IconButton, Badge, Panel, ContentState, Dialog, useConfirm and
usePrompt are now the AI-DO ones with AI-DO's props.

## npm packages

A component carries source only. The app lists these packages in its
package.json and installs them; miy component sync and miy app check fail with
component_dependencies_missing until it does. React and React DOM come from the
app as before.

${Object.entries(sortedDependencies).map(([name, requirement]) => `- ${name} ${requirement}`).join("\n")}

## Tailwind

The components are styled with Tailwind utility classes, so the app builds
them. Add the @tailwindcss/vite plugin to the Vite config and give the app one
Tailwind entry CSS that imports this component's theme.css and app.css after
Tailwind and scans the installed component sources:

    @import "tailwindcss";
    @import "../../platform-components/ui/theme.css";
    @import "../../platform-components/ui/app.css";
    @source ".";
    @source "../../platform-components";

The paths are for an entry CSS in web/src. Without the @source line Tailwind
does not see the component classes and the components render unstyled.
theme.css registers the color and font names the classes use (bg-ui-*,
text-ui-*, border-ui-*, bg-app-*) and the class based dark variant. app.css
adds AI-DO's app-level classes (app-text-*, app-control*, app-field-input*,
app-menu-*, card, sidebar-*), its base rules and the light-mode legibility
floor, so screens ported from AI-DO keep their look.

Import components from the installed index. It loads styles.css, so the app
does not import that file. The workspace equivalent is @miy/ui.

## Tokens and dark mode

The package declares no design token values. The --ui-* tokens, the Pretendard
font and the theme are supplied by the platform: the frame runtime delivers
them to every app frame and sets the dark class and data-miy-theme="dark" on
the document root. Dark mode follows the document root, not a nested element.

## Data providers

Directory and other data providers return only the app's permitted public DTOs.
Selection and disabled controls do not grant authority: the app server checks
current identity, membership and resource ACL again for each operation.
`);
const metadata = {
  protocol: "miy.component/v1",
  name: "ui",
  version,
  description: "AI-DO UI package (Tailwind CSS 4, Radix UI) with directory selection, resource sharing and small attachments",
  uiContract: "miy.ui/v1alpha1",
  bundleVersions: [bundleVersion],
  dependencies: sortedDependencies,
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
