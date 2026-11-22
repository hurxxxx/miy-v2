# App integration checks

Before implementing a feature, read `docs/CONTRACT.md` or MCP `get_contract`.
After `component sync`, inspect the locked UI and call `list_components` and
`get_component` when you need the catalog or an example. Start with the existing
`DirectoryPicker` for people or organizations, `ShareEditor` for share grants,
and `FileTransferPanel` for small transfers. Import from `@miy/components/ui`.
Keep business role choices, revocation checks and storage references in the app
API. Record a concrete reason when a custom flow needs a custom component.

Use the platform-supplied `Identity` in the server and `runtime.api`,
`runtime.fileApi` and `runtime.navigate` in the browser. Report unsaved state via
`runtime.setNavigationState`. Use the UI `requestId()` utility for browser-generated
IDs. It works in internal IP-based HTTP frames as well as HTTPS.

## File integration

- Use the owning app's file route with `runtime.fileApi('/records/42/content')`.
  The SDK adds `/api/files`, so that example reaches `/api/files/records/42/content`.
  Check the resulting route against the app's server handler.
- Save the original request ID and reservation intent in the app DB before
  reserving in Core. When the reply is lost, look up the same request or file
  before attempting another write. Continue an incomplete upload on its existing
  reservation. If the bytes are already ready, finish the app reference and attach
  instead of uploading them again.
- Commit the app reference before attaching. For deletion, commit the removal of
  the reference and retain the cleanup intent until Core confirms detach. Report a
  pending cleanup or failure when the result is unknown. Retry with the same file
  ID and reference. Use the cancellation path for explicitly abandoned unattached
  objects.
- Recheck the current app ACL on both metadata and download routes. Verify that
  direct file requests are denied after access is revoked. Apply business filters
  before page limits or carry the next cursor until the list is complete.

## Verify before completing the change

Run `miy app check --harness-only` for a quick offline source review. The full
`miy app check` includes the same findings alongside build and integrity results.
Findings are advisory source patterns and review prompts. Inspect the reported
locations and use the relevant acceptance tests to establish behavior.

- For browser UI changes, record which shared components are used and test the
  supported HTTP and HTTPS modes relevant to the change.
- For file transfer changes, record tests for a lost reservation, upload or attach
  response, recovery using the same request ID, and a failed detach followed by
  successful cleanup. Test access denial and revocation with ordinary users.
- Record what actually ran in `docs/ACCEPTANCE.md`'s report format. Mark omitted
  scenarios `not_run` with a reason. Passing a build or a source review does not
  establish browser, database or permission behavior.
