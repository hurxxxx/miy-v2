#!/usr/bin/env node
// WCAG contrast of the design token pairs that carry text or focus, in light and
// dark (packages/tokens/tokens.css). Text needs 4.5:1, the focus ring and large
// or non-text indicators 3:1. Input and panel borders keep AI-DO's values by
// decision (docs/COMMON-FEATURES-PLAN.md §7) and are not checked here.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const css = readFileSync(join(root, "packages/tokens/tokens.css"), "utf8");

function block(selector) {
  const values = {};
  for (const match of css.matchAll(new RegExp(`${selector}\\s*\\{([^}]*)\\}`, "g")))
    for (const [, name, value] of match[1].matchAll(/(--ui-color-[a-z0-9-]+):\s*([^;]+);/g)) values[name] = value.trim();
  return values;
}
const light = block(":root");
const dark = { ...light, ...block('\\.dark,\\s*\\[data-miy-theme="dark"\\]') };

function rgb(value) {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (!hex) throw new Error(`Not a solid hex color: ${value}`);
  return [0, 2, 4].map((offset) => parseInt(hex[1].slice(offset, offset + 2), 16) / 255);
}
function luminance(value) {
  const [r, g, b] = rgb(value).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a, b) {
  const [x, y] = [luminance(a), luminance(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
}

// [foreground, background, minimum]
const TEXT = 4.5;
const UI = 3;
const pairs = [
  ["ink", "bg", TEXT],
  ["ink", "surface", TEXT],
  ["ink", "surface-subtle", TEXT],
  ["ink", "surface-hover", TEXT],
  ["ink-muted", "bg", TEXT],
  ["ink-muted", "surface", TEXT],
  ["ink-muted", "surface-subtle", TEXT],
  ["ink-subtle", "bg", TEXT],
  ["ink-subtle", "surface", TEXT],
  ["accent-fg", "accent", TEXT],
  ["accent-fg", "accent-hover", TEXT],
  ["accent", "bg", UI],
  ["brand", "bg", TEXT],
  ["brand", "surface", TEXT],
  ["info-text", "info-bg", TEXT],
  ["success-text", "success-bg", TEXT],
  ["warning-text", "warning-bg", TEXT],
  ["danger-text", "danger-bg", TEXT],
  ["danger", "bg", UI],
];

const failures = [];
let checked = 0;
for (const [theme, values] of [["light", light], ["dark", dark]]) {
  for (const [foreground, background, minimum] of pairs) {
    const ratio = contrast(values[`--ui-color-${foreground}`], values[`--ui-color-${background}`]);
    checked += 1;
    if (ratio < minimum) failures.push(`${theme}: ${foreground} on ${background} ${ratio.toFixed(2)}:1 < ${minimum}:1`);
  }
  // The focus ring is the brand color at 70% over the surface it sits on.
  for (const background of ["bg", "surface"]) {
    const [f, b] = [rgb(values["--ui-color-brand"]), rgb(values[`--ui-color-${background}`])];
    const mixed = "#" + f.map((c, i) => Math.round((0.7 * c + 0.3 * b[i]) * 255).toString(16).padStart(2, "0")).join("");
    const ratio = contrast(mixed, values[`--ui-color-${background}`]);
    checked += 1;
    if (ratio < UI) failures.push(`${theme}: focus ring on ${background} ${ratio.toFixed(2)}:1 < ${UI}:1`);
  }
}
if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log(`PASS token contrast: ${checked} light and dark pairs meet WCAG AA`);
