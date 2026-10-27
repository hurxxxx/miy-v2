// Parse only miy CLI responses. API, MCP and child-tool output use their own contracts.
import assert from "node:assert/strict";

export const CLI_EXIT = Object.freeze({
  success: 0,
  internal: 1,
  usage: 2,
  auth: 3,
  forbidden: 4,
  notFound: 5,
  conflict: 6,
  environment: 7,
  network: 8,
  remoteFailure: 9,
  interrupted: 130,
});

export function cliResult(stdout, { exitCode = 0, expectedExit = 0, command } = {}) {
  assert(Object.values(CLI_EXIT).includes(expectedExit), "Explicit CLI exit category required");
  assert.equal(exitCode, expectedExit, "Unexpected miy CLI exit category");
  let value;
  try {
    value = JSON.parse(String(stdout));
  } catch {
    throw Error("miy stdout must contain exactly one JSON envelope");
  }
  assert(value && typeof value === "object" && !Array.isArray(value), "CLI envelope required");
  for (const key of ["schemaVersion", "command", "ok", "data", "error", "meta"])
    assert(Object.hasOwn(value, key), `Missing CLI envelope field: ${key}`);
  assert.equal(value.schemaVersion, 1, "Unsupported CLI response schema");
  assert.equal(typeof value.command, "string");
  assert(value.command.length > 0, "CLI command required");
  if (command !== undefined) assert.equal(value.command, command);
  assert(value.meta && typeof value.meta === "object" && !Array.isArray(value.meta));
  assert.equal(value.ok, expectedExit === CLI_EXIT.success);
  if (value.ok) {
    assert.equal(value.error, null, "Successful CLI response contains an error");
  } else {
    assert.equal(value.data, null, "Failed CLI response contains success data");
    assert(value.error && typeof value.error === "object" && !Array.isArray(value.error));
    assert.equal(typeof value.error.code, "string");
    assert(value.error.code.length > 0);
    assert.equal(typeof value.error.message, "string");
    assert.equal(typeof value.error.retryable, "boolean");
    assert(Object.hasOwn(value.error, "details"), "CLI error details required");
  }
  return value;
}

export function cliData(stdout, command) {
  return cliResult(stdout, { command }).data;
}

export function cliErrorText(error) {
  return `${error.code}: ${error.message}\n${JSON.stringify(error.details)}`;
}

// execFileSync exceptions carry the exit status and stdout; stderr is diagnostics only.
export function assertCliFailure(failure, expectedExit, pattern, command) {
  assert(failure, "miy command should fail");
  assert.notEqual(expectedExit, CLI_EXIT.success, "Failure must name a nonzero exit category");
  const value = cliResult(failure.stdout, {
    exitCode: failure.status,
    expectedExit,
    command,
  });
  if (pattern !== undefined) assert.match(cliErrorText(value.error), pattern);
  return value.error;
}
