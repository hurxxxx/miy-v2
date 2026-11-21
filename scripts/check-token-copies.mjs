// The design tokens have one source, packages/tokens/tokens.css. Places that
// cannot import the workspace package carry a byte-identical copy; this check
// keeps those copies from drifting. --write refreshes them from the source.
import { readFileSync, writeFileSync } from "node:fs";
const source = new URL("../packages/tokens/tokens.css", import.meta.url);
const copies = ["../templates/rust-react/sdk/preview/tokens.css"];
const write = process.argv.includes("--write");
const expected = readFileSync(source, "utf8");
let stale = 0;
for (const path of copies) {
  const url = new URL(path, import.meta.url);
  let actual = "";
  try {
    actual = readFileSync(url, "utf8");
  } catch {
    /* a missing copy is stale */
  }
  if (actual === expected) continue;
  if (write) writeFileSync(url, expected);
  else {
    stale += 1;
    console.error(`FAIL ${path.slice(3)} differs from packages/tokens/tokens.css; run node scripts/check-token-copies.mjs --write`);
  }
}
if (stale) process.exit(1);
console.log(write ? "wrote token copies" : "PASS token copies match packages/tokens/tokens.css");
