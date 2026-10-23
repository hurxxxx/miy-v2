# Acceptance evidence

After testing, copy the exact dev releaseId from `app status` into a JSON report.
Record observed results, not planned tests. Keep credentials and sensitive data out
of evidence. Commit reproducible test code where appropriate; artifact references
are user attestations, not uploaded/verified by the server.

```json
{
  "releaseId": "EXACT-DEV-RELEASE-UUID",
  "checks": [
    {"scope":"browser navigation", "actor":"developer", "result":"passed", "evidence":"App and shell links, back/forward: cancel preserves draft; accept navigates; saved state does not prompt"},
    {"scope":"cross-user authorization", "actor":"two ordinary users", "result":"not_run", "evidence":"Not tested yet; no claim of user data isolation"}
  ]
}
```

Run `app accept --note 'actual summary' --report acceptance.json` only after the
listed checks. The report must have at least one passed result. Unresolved failed
checks cannot be approved. `app status` exposes the recorder, time, exact release,
checks and legacy/unstructured status to app managers.

Consider these integration boundaries for the app's actual changes:

- Local preview: login, real local DB and authorized platform/other-app API calls.
- Preview lifecycle: dev → dev-stop → dev; port/lock released and local data retained.
- Identity: ordinary users, another owner, anonymous and denied users; verify server results.
- Navigation: app links, platform menu, back/forward, refresh; cancel and allow with dirty and saved state.
- Update: existing records and migration history survive a new dev release; verify version/image.
- Delivery: status and deployed browser route, not job submission alone. Production is a separate authorized action.

Use not_run and a reason when a boundary was not exercised; do not invent a pass.
The checklist is development guidance. `app check` cannot replace browser/DB tests.
