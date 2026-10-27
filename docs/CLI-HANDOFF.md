# CLI 0.5 개발 인계

- 갱신: 2026-10-27
- 코드 버전: CLI **0.5.0** / 개발 번들 **0.6.2**
- 구현 기준: [구현 계획](CLI-IMPLEMENTATION-PLAN.md). 실행 증거는 [구현 결과](IMPLEMENTATION-RESULTS.md), 세부 요구·검사는 [검증 행렬](CLI-VALIDATION-MATRIX.md).

계획의 코드 구현과 명령·지침 이전을 완료하고 로컬 자동 검사·기본 CI·5개 배포 후보 CI를 통과했다. **이번 0.5의 실제 Windows 11 검증과 정식 릴리스 승인은 아직 완료하지 않았다. 기존 0.4.2의 실제 Windows 기반 검사 성공 이력은 문서 끝에 보존한다.** Windows 빌드·운영 검증은 사용자가 [전용 안내](WINDOWS-VALIDATION.md)에 따라 수행한 뒤 결과를 전달하면 문서를 갱신한다. 현재 개발 환경의 기존 플랫폼 서비스는 재배포하거나 migration하지 않았다.

검증 대상 소스는 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`다. clean 작업본의 실제 HTTP/PostgreSQL/Docker/CLI 통합52개, [기본 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40460546995), [5개 target 후보 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868)를 통과했다. [후보 artifact](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868/artifacts/12715067551)와 [파일별 hash](releases/cli-0.5.0-candidate.json)가 준비되어 있다. 검증 결과만 추가한 문서 commit과 이 소스 commit을 구분한다. 공개 릴리스는 발행하지 않았다.

## 현재 구현

| 범위 | 코드와 동작 |
| --- | --- |
| 명령·출력·발견 | `main.rs`, `cli_output.rs`, `cli_error.rs`, `cli_input.rs`, `cli_discovery.rs`: 루트 명령, JSON/JSONL, 종료 코드, fields, typed input, help/schema/completion |
| context·인증 | `cli_config.rs`, `cli_login.rs`, `credential_store.rs`, `auth_sessions.rs`: 사용자 전역 저장, device 승인, access 15분/refresh family 30일, 회전·철회·동시 갱신 |
| 안전한 실행 | `runtime_contract.rs`, `runtime_preflight.rs`, `cli_remote.rs`, `runtime.rs`: 공유 dry-run/expected-state, prod와 모든 remove의 명시 대상·확인, typed retry |
| 작업·업로드·로그 | `cli_journal.rs`, `cli_upload.rs`, `read_api.rs`, `job_api.rs`, `delivery.rs`, `runtime_logs.rs`: 요청별 복구, 페이지·이벤트, worker 유한 로그 스냅샷 |
| OS·프로젝트 갱신 | `platform.rs`, `owned_process.rs`, `file_transaction.rs`, `agent_tools.rs`, `upgrade.rs`: Windows 자식 소유, 파일 잠금, staging·backup·복구 |
| 배포 파일 | `scripts/*-cli.ps1`, `windows-cli-common.ps1`, `package-cli-release.py`, `.github/workflows/cli-release.yml`: 5개 OS/CPU candidate와 SHA-256, 별도 증거 기반 공개 |
| 웹·지침·스크립트 | device 승인 화면, 실행 전 확인 화면, 템플릿·두 에이전트 스킬·MCP·회귀 스크립트 새 명령 이전 |

서버 migration은 `0009_runtime_contract.sql`~`0012_runtime_logs.sql`을 추가했다. 기존 migration과 앱 SQL/SDK 런타임 코드는 변경하지 않는다. Core의 `users.enabled`가 CLI 사용자 활성 상태의 기준이다. Core 상태 변경은 발급·갱신·API·worker에서 DB 재검사하며 Keycloak 비활성화의 자동 전달을 제공한다고 주장하지 않는다.

대상 선택은 `--context` → `MIY_CONTEXT` → 프로젝트 바인딩 → active context다. 프로젝트에는 토큰을 저장하지 않는다. 갱신 응답이 유실되면 이전 refresh token을 자동 재전송하지 않고 재로그인을 요구한다. `job resume`은 원래 요청을 관찰/복구하고, `job retry`는 허용된 최종 실패에 대한 새 작업이다. 명시적인 새 lifecycle 명령은 새 preflight와 확인을 거친다.

## 다음 환경에서 실행할 순서

1. `git status --short --branch`, `git log -5 --oneline`으로 checkout과 미커밋 변경을 확인한다. Rust 1.99.1/pnpm 10.34.5를 준비한다.
2. 아래 로컬 정적·계약 검사를 실행한다. 이전 결과는 해당 커밋·바이너리에만 적용한다.
3. Docker가 있는 격리 Linux 환경에서 runtime runner를 실행한다. 기존 일반 플랫폼에 `scripts/check.sh`를 무심코 실행하지 않는다. 이 스크립트는 서버·worker·DB를 변경하는 전체 검사다.
4. 서버 migration/API/capability를 먼저 배포한 별도 검증 서버에서 원격 실증을 수행한다. 구 서버의 새 CLI 인증/운영은 `unsupported_capability`로 거부된다.
5. Windows 11/PowerShell 5.1·7/Docker Desktop과 독립 AI 평가를 수행하고 증거를 기록한다. 공개 릴리스는 candidate build와 구분한다.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
cargo test --workspace --locked --no-default-features
cargo build --locked --no-default-features
pnpm typecheck
python3 scripts/check-cli-output.py --cli target/debug/miy
python3 scripts/check-bundle-portability.py check --cli target/debug/miy --output .local/ci/bundle.json
node scripts/check-cli-guidance.mjs
python3 scripts/runtime-regression/test_cleanup.py
python3 scripts/runtime-regression/run.py
pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
```

0.6.1→0.6.2 보존 검사는 `MIY_PREVIOUS_BUNDLE_CLI`에 실제 이전 CLI 절대 경로를 지정해 `node scripts/check-cli-guidance.mjs`를 실행한다. 이전 바이너리가 없으면 해당 전환을 통과로 표시하지 않는다. `.local`, target, OS 보안 저장소, 증거·CA·DB·볼륨은 Git으로 전달되지 않는다.

## 아직 별도 실증이 필요한 항목

- 사용자 Windows 11의 PowerShell 5.1·7, Credential Manager/DPAPI·ACL, 콘솔 Ctrl+C, Docker Desktop, 실제 Codex/Claude 프로젝트 MCP.
- 별도 Linux 서버에서 실제 OIDC 로그인, 업로드·배포·운영 승격 및 실제 업무 UI/API. loopback 개발 로그인은 실제 IdP 로그인을 대체하지 않는다.
- Codex·Claude 각각 OS별 독립 새 세션 3회 평가. 기존 에이전트 구현 작업이나 MCP fixture 성공은 이 평가가 아니다.
- 전체 제품 V2~V7의 독립 설치·재시작·v1→v2 데이터 보존 인수는 [전체 검증 계획](VALIDATION.md)으로 계속 관리한다.

과거 기반/출력 커밋과 CI 기록은 [구현 결과](IMPLEMENTATION-RESULTS.md)에 이력으로 보존한다. 새 결과의 요구 ID·환경·명령·종료 코드·바이너리 hash·증거 위치를 남기고 미실행은 이유와 함께 `NOT_RUN`으로 기록한다.

## 2026-10-27 실제 Windows 기반 검증 결과

`fb42b384081afe8b77eab7b2af419f5c1e79c3ec`를 실제 Windows 11 Pro build 26200 x64 일반 사용자 환경에서 검사했다. PowerShell 7.6.5와 Windows PowerShell 5.1.26100.9711에서 각각 fmt·Windows MSVC check·Clippy 경고 금지·단위 검사 **37개**·release build·번들/Git 왕복·공통 출력 블랙박스 **9개**·오프라인 앱 생성/반복 setup·stdio MCP·ZIP/체크섬/압축 해제 실행을 통과했다. 추가 Node CLI 결과 파서 검사 4개도 통과했다. 두 셸은 같은 target을 순차 사용하며 PowerShell 5.1의 build는 최신 산출물을 재사용했다.

Rust 1.99.1/MSVC/SDK는 사용자 승인으로 준비했다. MSVC 설치기의 3010 재부팅 요청 후 이번 검사는 재부팅 없이 수행했다. [실행 기록](WINDOWS-VALIDATION.md#windows-실행-기록)에 환경·명령·종료 코드·로컬 증거 경로·exe/ZIP SHA-256과 미실행 사유가 있다. 원본 로그·바이너리·ZIP은 이 PC의 `.local/`에만 있고 Git에는 결과 문서만 포함한다.

실제 Codex/Claude MCP 승인·세션 유지, Windows 콘솔 Ctrl+C, Docker Desktop 전체 개발·배포, 실제 인증·원격 운영과 재부팅 후 검증은 남아 있다. 이번 commit의 다른 OS manifest가 없어 로컬 OS 간 비교는 수행하지 않았다. 전체 1A/5/6 단계나 정식 지원·릴리스 완료로 체크하지 않는다.
