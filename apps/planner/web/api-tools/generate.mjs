import { readFileSync, writeFileSync } from "node:fs";
import openapiTS, { astToString } from "openapi-typescript";

const spec = new URL("../../docs/openapi.json", import.meta.url);
const output = new URL("../src/api.generated.ts", import.meta.url);
const header = "// Generated from docs/openapi.json. Run pnpm api:generate.\n\n";
const generated = header + astToString(await openapiTS(spec, { defaultNonNullable: false }));

if (process.argv.includes("--check")) {
  const current = readFileSync(output, "utf8").replaceAll("\r\n", "\n");
  if (current !== generated) {
    console.error("web/src/api.generated.ts is out of date. Run pnpm api:generate.");
    process.exitCode = 1;
  }
} else {
  writeFileSync(output, generated);
}
