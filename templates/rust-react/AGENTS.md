# miy application development

This is an independent Rust + React application. Work only in this directory.
Read docs/CONTRACT.md and query the miy_app MCP get_contract and search_api tools before coding.
Backend business code is src/; React entry is web/src/AppRoot.tsx; SQL migrations are migrations/.
The local sdk/ directory is a versioned platform development bundle. Do not modify SDKs, the Dockerfile, CLI executable, or contract version to bypass checks.
Use `miy_app_sdk::Identity` from Axum Extension for the verified caller; never trust a user ID or admin flag from a request body.
Use `runtime.api()` and `runtime.navigate()` in the UI. No iframe, hardcoded host URLs, or credentials in code.
Use the app DB only. Do not connect to the core DB or another app DB. Never run migrations in the application process; the platform runner applies them.
Do not inspect the platform repository, user secrets, Docker socket, or other projects. Do not deploy from the development agent; the evaluator builds and deploys your completed app.
Run `cargo fmt --package miy-app` to format only your application, then `.miy-tools/miy app check` before declaring completion. The CLI checks bundle integrity, SQL migration rules, Rust formatting/tests, frozen dependencies, TypeScript, the UI build and Federation assets. Do not weaken checks or modify miy.bundle.json. If a check fails in a protected file, report the bundle defect instead of changing the SDK.
The CLI check does not prove live DB migration correctness or browser integration; those require platform acceptance tests. Record exact commands and failures in your completion report.
Preserve existing migrations; add new numbered transactional SQL files. Document endpoints and behavior in README.md.
