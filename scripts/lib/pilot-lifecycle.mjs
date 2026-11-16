// Pilot runtime cleanup uses read-only job observation before a single stop.
// Never resume/retry a deployment: those CLI commands may submit remote work.
const uuid = value => typeof value === "string" && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(value)
  && value !== "00000000-0000-0000-0000-000000000000";
const terminal = job => ["succeeded", "failed"].includes(job.status);
const error = code => Object.assign(new Error(code), { safeCode: code });
const requireThat = (condition, code) => { if (!condition) throw error(code); };

export const pilotReceipt = (name, details = {}) => ({ name, details });

// Only public, validated receipt identities survive a CLI failure. Never retain
// the response body, recovery argv, diagnostic text or arbitrary error details.
export function pilotCliFailure(code, details) {
  const failure = error(typeof code === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(code) ? code : "unexpected_failure");
  for (const key of ["jobId", "requestId"]) if (uuid(details?.[key])) failure[key] = details[key];
  return failure;
}

export function finalizePilotResult(result) {
  if (result.cleanupFailures.length) result.status = "failed";
  return result.status === "passed" ? 0 : 1;
}

export function createPilotLifecycle({ cli, appId, keepRunning = false, timeoutMs = 600000,
  pollMs = 1000, now = Date.now, sleep = ms => new Promise(resolve => setTimeout(resolve, ms)) }) {
  const state = { appId, environment: "dev", deploymentAttempted: false, deployment: {}, stopAttempted: false, stop: {} };

  const bind = (target, value) => {
    if (uuid(value?.id)) target.jobId = value.id;
    if (uuid(value?.jobId)) target.jobId = value.jobId;
    if (uuid(value?.requestId)) target.requestId = value.requestId;
  };
  const checkedJob = (job, action) => {
    requireThat(uuid(job?.id) && uuid(job.requestId) && job.appId === appId && job.environment === "dev"
      && ["deploy", "promote", "start", "stop", "restart", "remove"].includes(job.action)
      && (!action || job.action === action) && ["queued", "running", "succeeded", "failed"].includes(job.status), "pilot_job_response_mismatch");
    return job;
  };
  const call = (args, deadline) => {
    requireThat(now() < deadline, "pilot_observation_timeout");
    return cli(["--app", appId, ...args], { timeout: Math.min(120000, deadline - now()) });
  };
  const pause = async (deadline, code) => {
    requireThat(now() < deadline, code);
    await sleep(Math.min(pollMs, deadline - now()));
    requireThat(now() < deadline, code);
  };
  const read = async (operation, deadline, code) => {
    for (;;) {
      try { return await operation(); }
      catch (failure) {
        // A malformed/wrong-target response is not a transient transport error.
        if (failure?.safeCode?.endsWith("_mismatch")) throw failure;
        await pause(deadline, code);
      }
    }
  };
  const jobs = async deadline => {
    const rows = [], cursors = new Set();
    let cursor;
    do {
      const page = await call(["--limit", "200", ...(cursor ? ["--cursor", cursor] : []), "job", "list", "--environment", "dev"], deadline);
      requireThat(Array.isArray(page?.jobs) && (page.nextCursor === null || typeof page.nextCursor === "string"), "pilot_job_page_mismatch");
      rows.push(...page.jobs.map(job => checkedJob(job)));
      cursor = page.nextCursor;
      requireThat(!cursor || (cursor.length <= 2048 && !cursors.has(cursor) && cursors.size < 20), "pilot_job_page_mismatch");
      cursors.add(cursor);
    } while (cursor);
    requireThat(new Set(rows.map(job => job.id)).size === rows.length, "pilot_job_page_mismatch");
    return rows;
  };
  const installation = async deadline => {
    const status = await call(["app", "status"], deadline);
    requireThat(status?.app?.id === appId && Array.isArray(status.installations), "pilot_app_status_mismatch");
    const matches = status.installations.filter(item => item.environment === "dev");
    requireThat(matches.length <= 1 && matches.every(item => item.appId === appId && uuid(item.id)
      && ["running", "stopped"].includes(item.desiredState)
      && ["not_deployed", "deploying", "starting", "running", "stopping", "stopped", "removed", "failed", "unreachable"].includes(item.runtimeStatus)), "pilot_app_status_mismatch");
    const item = matches[0] ?? null;
    state.observedInstallation = item ? { id: item.id, runtimeStatus: item.runtimeStatus, desiredState: item.desiredState } : null;
    return item;
  };
  const observe = async (target, action, deadline, excluded = new Set()) => {
    for (;;) {
      const rows = await read(() => jobs(deadline), deadline, "pilot_jobs_unavailable");
      const candidates = rows.filter(job => job.action === action && !excluded.has(job.id)
        && (!target.jobId || job.id === target.jobId) && (!target.requestId || job.requestId === target.requestId));
      requireThat(candidates.length <= 1, "pilot_job_identity_mismatch");
      const job = candidates[0];
      if (job) {
        bind(target, job);
        target.status = job.status;
        // Wait for every accepted action on this new pilot target before stop.
        // The CLI's checked preflight also rejects a concurrent later action.
        if (terminal(job) && rows.every(terminal)) return { job, rows };
      } else {
        // Absence of a receipt/installation after a lost acknowledgement does
        // not prove rejection: a server request may still be committing.
        await read(() => installation(deadline), deadline, "pilot_status_unavailable");
      }
      await pause(deadline, job ? "pilot_jobs_still_active" : `pilot_${action}_unconfirmed`);
    }
  };

  return {
    state,
    async deploy() {
      requireThat(!state.deploymentAttempted, "pilot_deployment_already_attempted");
      state.deploymentAttempted = true;
      let receipt;
      try {
        receipt = await cli(["--app", appId, "app", "deploy", "--environment", "dev", "--no-wait"], { timeout: 1800000 });
      } catch (failure) { bind(state.deployment, failure); throw failure; }
      bind(state.deployment, checkedJob(receipt, "deploy"));
      const deadline = now() + 1800000;
      for (;;) {
        const job = checkedJob(await call(["job", "get", state.deployment.jobId], deadline), "deploy");
        requireThat(job.id === state.deployment.jobId && job.requestId === state.deployment.requestId, "pilot_job_identity_mismatch");
        state.deployment.status = job.status;
        if (terminal(job)) {
          requireThat(job.status === "succeeded", "pilot_deployment_failed");
          break;
        }
        await pause(deadline, "pilot_deployment_observation_timeout");
      }
      const item = await installation(deadline);
      requireThat(item?.runtimeStatus === "running", "dev_installation_not_running");
      return item;
    },
    async cleanup() {
      if (!state.deploymentAttempted) return { status: "not_needed", reason: "deployment_not_attempted", stopped: false };
      if (keepRunning) return { status: "skipped", reason: "keep_running_requested", stopped: false };
      const deadline = now() + timeoutMs;
      try {
        const settled = await observe(state.deployment, "deploy", deadline);
        let item = await read(() => installation(deadline), deadline, "pilot_status_unavailable");
        if (!item) {
          requireThat(settled.job.status === "failed", "pilot_installation_unconfirmed");
          return { status: "passed", finalState: "not_installed", stopped: false };
        }
        const isStopped = value => value && ["stopped", "not_deployed", "removed"].includes(value.runtimeStatus) && value.desiredState === "stopped";
        if (isStopped(item)) return { status: "passed", finalState: item.runtimeStatus, stopped: true };

        requireThat(!state.stopAttempted, "pilot_stop_already_attempted");
        state.stopAttempted = true;
        try {
          const receipt = await call(["--yes", "app", "stop", "--environment", "dev", "--no-wait"], deadline);
          bind(state.stop, checkedJob(receipt, "stop"));
        } catch (failure) {
          bind(state.stop, failure);
          if (failure?.safeCode?.endsWith("_mismatch")) throw failure;
          // A failed local command is not cancellation. Observe its possible
          // receipt, without sending stop (or deploy) again.
        }
        const stopped = await observe(state.stop, "stop", deadline, new Set(settled.rows.map(job => job.id)));
        requireThat(stopped.job.status === "succeeded", "pilot_stop_failed");
        item = await read(() => installation(deadline), deadline, "pilot_status_unavailable");
        requireThat(isStopped(item), "pilot_stop_not_confirmed");
        return { status: "passed", finalState: item.runtimeStatus, stopped: true };
      } catch (failure) {
        return { status: "failed", code: pilotCliFailure(failure?.safeCode).safeCode, stopped: false };
      }
    },
  };
}
