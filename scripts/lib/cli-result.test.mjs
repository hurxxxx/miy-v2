import assert from "node:assert/strict";
import test from "node:test";
import { CLI_EXIT, assertCliFailure, cliData, cliResult } from "./cli-result.mjs";

const success = {
  schemaVersion: 1, command: "app status", ok: true,
  data: [{ id: "fixture" }], error: null, meta: {},
};
const failure = {
  schemaVersion: 1, command: "app deploy", ok: false, data: null,
  error: { code: "forbidden", message: "Access denied", retryable: false, details: {}, recovery: null },
  meta: {},
};

test("unwraps a successful CLI envelope and checks command identity", () => {
  assert.deepEqual(cliData(JSON.stringify(success), "app status"), success.data);
  assert.throws(() => cliData(JSON.stringify(success), "app deploy"));
});

test("rejects legacy raw JSON and stdout contaminated by extra output", () => {
  assert.throws(() => cliData(JSON.stringify(success.data)));
  assert.throws(() => cliData(JSON.stringify(success) + "\n" + JSON.stringify(success)));
  assert.throws(() => cliData("build output\n" + JSON.stringify(success)));
});

test("checks the exact failure category and structured stdout, not stderr", () => {
  const result = { status: CLI_EXIT.forbidden, stdout: JSON.stringify(failure), stderr: "diagnostic" };
  assert.equal(assertCliFailure(result, CLI_EXIT.forbidden, /forbidden/, "app deploy").code, "forbidden");
  assert.throws(() => assertCliFailure(result, CLI_EXIT.internal, /forbidden/));
  assert.throws(() => assertCliFailure({ ...result, status: null }, CLI_EXIT.forbidden));
  assert.throws(() => assertCliFailure({ ...result, stdout: "", stderr: JSON.stringify(failure) }, CLI_EXIT.forbidden));
});

test("rejects contradictory envelope state and missing typed error fields", () => {
  assert.throws(() => cliData(JSON.stringify({ ...success, error: failure.error })));
  assert.throws(() => cliResult(JSON.stringify({ ...failure, data: [] }), { exitCode: 4, expectedExit: 4 }));
  const { retryable, ...incomplete } = failure.error;
  assert.throws(() => cliResult(JSON.stringify({ ...failure, error: incomplete }), { exitCode: 4, expectedExit: 4 }));
});
