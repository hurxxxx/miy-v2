# App API contract

- Register app API handlers only through `OpenApiRouter::routes(routes!(...))`. Use `#[utoipa::path]`, typed Rust inputs and outputs, app-relative paths, unique stable `operationId`s and local references. The same registration builds the live route and OpenAPI 3.1 operation.
- Match the wire contract. Cover parameters, omitted versus null fields, success and error responses, and media types. Record file headers and limits. For WebSockets, record the upgrade and message protocol. Version breaking operations.
- Use generated TypeScript types with `runtime.api`, `runtime.fileApi` or `runtime.live`. Use platform-verified `Identity`, the app's ACL and its injected DB. Keep reads free of business writes. Approved cross-app calls use `runtime.appApi` or Rust `Identity::app_api`.

New apps can follow `GET /ping` in `src/api.rs` and `web/src/api.ts`. Existing apps can follow a registered operation and its generated types.

1. Install dependencies with `pnpm install --frozen-lockfile`.
2. Run `pnpm api:generate` and commit `docs/openapi.json` and `web/src/api.generated.ts` with the handler and tests.
3. Run `miy app check`. The quick `--harness-only` check validates file structure. Full checks and server builds compare the generated files when `apiContract` is `openapi-v1`.

## Local preview authentication

Run `miy app dev` and open the loopback URL. The CLI authorizes the dev runner and database. The app receives the identity of the person signed in through the dev portal in that browser. The same SDK handlers run locally and in deployed frames. Keep business rules on the backend and use the verified Identity on each request. The gateway routes backend platform calls using a short-lived proof for that browser user.

For a two-person approval flow, open the same URL in two separate browser profiles. Give both accounts dev app access and sign in as the requester in one and the approver in the other. Before the first dev deployment, each tester needs develop access. User search and files also need the current dev release's approved permissions. Signing out in the dev portal ends that profile's preview authorization.
