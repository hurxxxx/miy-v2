// Stateful CLI protocol fixtures only: no process, network, files or platform.
import assert from "node:assert/strict";
import { createPilotLifecycle, finalizePilotResult, pilotCliFailure, pilotReceipt } from "./lib/pilot-lifecycle.mjs";

const appId = "pilot-fixture";
const id = number => `00000000-0000-4000-8000-${String(number).padStart(12, "0")}`;
const clone = value => JSON.parse(JSON.stringify(value));
const passed = [];
async function check(name, action) { await action(); passed.push(name); console.log("PASS " + name); }

function fixture(options = {}) {
  let clock = 0, sequence = 0, pendingDeploy, installation = null;
  const jobs = [], finish = new Map(), events = [];
  const counts = { deploy: 0, stop: 0, status: 0, list: 0, get: 0 };
  const installed = runtimeStatus => ({ id: id(99), appId, environment: "dev", runtimeStatus,
    desiredState: runtimeStatus === "stopped" ? "stopped" : "running" });
  const newJob = action => {
    const job = { id: id(++sequence), requestId: id(100 + sequence), appId, environment: "dev", action, status: "queued" };
    jobs.push(job);
    finish.set(job.id, clock + (action === "deploy" && options.activeForever ? Infinity : 3));
    if (action === "deploy" && options.earlyRunning) installation = installed("running");
    return job;
  };
  if (options.paginate) {
    const previous = newJob("start");
    previous.status = "succeeded";
    finish.delete(previous.id);
  }
  const advance = () => {
    if (pendingDeploy && clock >= pendingDeploy.at) { newJob("deploy"); pendingDeploy = null; }
    for (const job of jobs) if (finish.has(job.id) && clock >= finish.get(job.id)) {
      job.status = (job.action === "deploy" && options.deploymentFailed) || (job.action === "stop" && options.stopFailed) ? "failed" : "succeeded";
      finish.delete(job.id);
      events.push(`${job.action}:${job.status}`);
      if (job.action === "deploy" && (!options.deploymentFailed || options.partialDeployment)) installation = installed("running");
      if (job.action === "deploy" && options.otherAcceptedAction) newJob("restart");
      if (job.action === "stop" && job.status === "succeeded" && !options.stopLeavesRunning) installation = installed("stopped");
    }
  };
  const cli = async args => {
    advance();
    assert.equal(args[0], "--app");
    assert.equal(args[1], appId);
    assert(!args.includes("resume") && !args.includes("retry") && !args.includes("prod"), "only observation and one explicit dev stop are allowed");
    const command = args.includes("job") ? "job" : "app";
    const action = args[args.indexOf(command) + 1];
    events.push(`${command} ${action}`);
    if (command === "app" && ["deploy", "stop"].includes(action)) {
      assert(args.includes("--no-wait"), "observe accepted work separately from submission");
      assert.equal(args[args.indexOf("--environment") + 1], "dev");
      counts[action]++;
      assert.equal(counts[action], 1, "a mutation must never be retransmitted");
      if (action === "stop") {
        assert(jobs.every(job => ["succeeded", "failed"].includes(job.status)), "stop must not race accepted work");
        assert(args.includes("--yes"));
        if (options.stopRejected) throw pilotCliFailure("forbidden");
      }
      if (action === "deploy" && options.neverAccepted) throw pilotCliFailure("command_timeout");
      let job;
      if (action === "deploy" && options.delayedAcceptance) pendingDeploy = { at: clock + 2 };
      else job = newJob(action);
      if (action === "deploy" && options.lostDeploymentResponse) {
        throw pilotCliFailure(options.lostDeploymentResponse, options.errorHasReceipt ? { jobId: job.id, requestId: job.requestId } : undefined);
      }
      if (action === "stop" && options.lostStopResponse) throw pilotCliFailure("command_timeout", { jobId: job.id, requestId: job.requestId });
      return clone(job);
    }
    if (command === "job" && action === "get") {
      counts.get++;
      if (options.getFailsOnce && counts.get === 1) throw pilotCliFailure("job_watch_timeout");
      return clone(jobs.find(job => job.id === args[args.indexOf("get") + 1]));
    }
    if (command === "job" && action === "list") {
      counts.list++;
      if (options.listFailsOnce && counts.list === 1) throw pilotCliFailure("network_error");
      const offset = args.includes("--cursor") ? Number(args[args.indexOf("--cursor") + 1]) : 0;
      const size = options.paginate ? 1 : 200;
      const rows = clone(jobs.slice(offset, offset + size));
      if (options.wrongTarget && rows.length) rows[0].appId = "unrelated-app";
      return { jobs: rows, nextCursor: offset + size < jobs.length ? String(offset + size) : null };
    }
    if (command === "app" && action === "status") {
      counts.status++;
      if ((options.statusFailsOnce && counts.status === 1) || options.statusAlwaysFails) throw pilotCliFailure("network_error");
      if (options.alreadyStopped && counts.status > 1) installation = installed("stopped");
      return { app: { id: appId }, installations: installation ? [clone(installation)] : [] };
    }
    assert.fail("unexpected mock CLI command");
  };
  return { cli, counts, events, get installation() { return installation; }, now: () => clock,
    sleep: async ms => { clock += ms; advance(); } };
}

async function exercise(options = {}, keepRunning = false) {
  const server = fixture(options);
  const lifecycle = createPilotLifecycle({ cli: server.cli, appId, keepRunning, timeoutMs: 15, pollMs: 1, now: server.now, sleep: server.sleep });
  const result = { status: "running", passes: [], cleanupFailures: [], runtime: lifecycle.state };
  try {
    const installation = await lifecycle.deploy();
    result.passes.push(pilotReceipt("dev_deployment_running", { installationId: installation.id }));
    result.status = "passed";
  } catch (failure) { result.status = "failed"; result.failure = failure.safeCode; }
  result.pilotCleanup = await lifecycle.cleanup();
  result.stopped = result.pilotCleanup.stopped;
  if (result.pilotCleanup.status === "failed") result.cleanupFailures.push("pilot_app:" + result.pilotCleanup.code);
  return { server, result, exitCode: finalizePilotResult(result) };
}

await check("a successful pilot observes deployment and stop receipts and confirms the stopped installation", async () => {
  const { server, result, exitCode } = await exercise();
  assert.equal(exitCode, 0);
  assert.equal(result.status, "passed");
  assert.equal(result.stopped, true);
  assert.equal(server.counts.deploy, 1);
  assert.equal(server.counts.stop, 1);
  assert(server.events.indexOf("deploy:succeeded") < server.events.indexOf("app stop"));
});

await check("cleanup waits for another accepted runtime action and skips an already stopped installation", async () => {
  const active = await exercise({ otherAcceptedAction: true });
  assert.equal(active.exitCode, 0);
  assert(active.server.events.indexOf("restart:succeeded") < active.server.events.indexOf("app stop"));
  const stopped = await exercise({ alreadyStopped: true });
  assert.equal(stopped.exitCode, 0);
  assert.equal(stopped.result.stopped, true);
  assert.equal(stopped.server.counts.stop, 0);
});

for (const [name, options] of [
  ["lost acknowledgement with delayed acceptance", { lostDeploymentResponse: "network_error", delayedAcceptance: true, earlyRunning: true }],
  ["CLI timeout with a receipt identity", { lostDeploymentResponse: "command_timeout", errorHasReceipt: true }],
  ["job observation failure", { getFailsOnce: true }],
  ["installation status failure", { statusFailsOnce: true }],
]) await check(`${name} preserves failure while finding and stopping accepted work`, async () => {
  const { server, result, exitCode } = await exercise(options);
  assert.equal(exitCode, 1);
  assert.equal(result.status, "failed");
  assert.equal(result.stopped, true);
  assert.equal(result.pilotCleanup.status, "passed");
  assert.deepEqual(result.cleanupFailures, []);
  assert.equal(server.counts.deploy, 1);
  assert.equal(server.counts.stop, 1);
  assert(server.events.indexOf("deploy:succeeded") < server.events.indexOf("app stop"));
});

await check("terminal deployment failure still stops its partially created installation", async () => {
  const { server, result } = await exercise({ deploymentFailed: true, partialDeployment: true });
  assert.equal(result.status, "failed");
  assert.equal(result.stopped, true);
  assert.equal(server.counts.stop, 1);
});

await check("a failed deployment with no installation is confirmed absent without issuing stop", async () => {
  const { server, result } = await exercise({ deploymentFailed: true });
  assert.equal(result.pilotCleanup.finalState, "not_installed");
  assert.equal(result.pilotCleanup.status, "passed");
  assert.equal(result.stopped, false);
  assert.equal(server.counts.stop, 0);
});

for (const [options, code] of [
  [{ neverAccepted: true }, "pilot_deploy_unconfirmed"],
  [{ lostDeploymentResponse: "command_timeout", activeForever: true, earlyRunning: true }, "pilot_jobs_still_active"],
  [{ statusAlwaysFails: true }, "pilot_status_unavailable"],
  [{ wrongTarget: true }, "pilot_job_response_mismatch"],
]) await check(`${code} is an explicit cleanup failure with no competing stop`, async () => {
  const { server, result, exitCode } = await exercise(options);
  assert.equal(exitCode, 1);
  assert.equal(result.status, "failed");
  assert.equal(result.stopped, false);
  assert.equal(result.pilotCleanup.code, code);
  assert.equal(result.cleanupFailures.length, 1);
  assert.equal(server.counts.stop, 0);
});

for (const [options, code] of [
  [{ stopFailed: true }, "pilot_stop_failed"],
  [{ stopRejected: true }, "pilot_stop_unconfirmed"],
  [{ stopLeavesRunning: true }, "pilot_stop_not_confirmed"],
]) await check(`${code} changes an otherwise passed run to failed and never retransmits stop`, async () => {
  const { server, result, exitCode } = await exercise(options);
  assert.equal(result.passes.length, 1);
  assert.equal(exitCode, 1);
  assert.equal(result.status, "failed");
  assert.equal(result.stopped, false);
  assert.equal(result.pilotCleanup.code, code);
  assert.equal(server.counts.stop, 1);
});

await check("lost stop acknowledgements and transient reads are recovered only by observing receipts", async () => {
  const { server, result, exitCode } = await exercise({ lostStopResponse: true, listFailsOnce: true, paginate: true });
  assert.equal(exitCode, 0);
  assert.equal(result.stopped, true);
  assert.equal(server.counts.stop, 1);
  assert(server.counts.list > 3, "paginated job reads must be followed");
});

await check("keep-running explicitly skips cleanup and never claims the app was stopped", async () => {
  const { server, result, exitCode } = await exercise({}, true);
  assert.equal(exitCode, 0);
  assert.equal(result.pilotCleanup.status, "skipped");
  assert.equal(result.pilotCleanup.reason, "keep_running_requested");
  assert.equal(result.stopped, false);
  assert.equal(server.installation.runtimeStatus, "running");
  assert.equal(server.counts.stop, 0);
});

await check("no deployment attempt needs no remote cleanup", async () => {
  const lifecycle = createPilotLifecycle({ appId, cli: async () => assert.fail("must not query or mutate") });
  assert.equal((await lifecycle.cleanup()).status, "not_needed");
});

await check("receipt names and safe failure identities cannot be overwritten by detail fields", async () => {
  const receipt = pilotReceipt("administrator_published_the_shared_component", { name: "ui", version: "1.1.0" });
  assert.equal(receipt.name, "administrator_published_the_shared_component");
  assert.deepEqual(receipt.details, { name: "ui", version: "1.1.0" });
  const failure = pilotCliFailure("unsafe/code", { jobId: id(1), requestId: "PRIVATE-TOKEN", body: "PRIVATE-TOKEN" });
  assert.equal(failure.safeCode, "unexpected_failure");
  assert.equal(failure.jobId, id(1));
  assert(!JSON.stringify(failure).includes("PRIVATE-TOKEN"));
});

console.log(JSON.stringify({ passed: passed.length, scope: "pilot runtime CLI state machine and evidence", liveServicesUsed: false, filesWritten: false }));
