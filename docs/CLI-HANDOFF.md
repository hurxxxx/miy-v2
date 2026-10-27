# CLI 개선 개발 인계

- 작성일: 2026-10-27
- 대상: 다른 PC/OS에서 miy CLI 0.5 개발을 이어갈 개발자 또는 AI 에이전트
- 기준 코드: `ea61ad5ca83a724833dd3bcdb5ed1de5b61bd9c6`

**계획 검토 후 사용자 승인으로 첫 기반 구현을 시작했다. 전체 CLI 0.5와 Windows 지원은 아직 완료하지 않았다.** 기준 코드는 CLI 0.4.2 / bundle 0.6.1이다. [CLI 0.5 구현 계획](CLI-IMPLEMENTATION-PLAN.md)이 이번 작업의 범위·인터페이스·완료 기준이며, 기존 장기 설계 문서의 예시와 충돌할 때 이번 범위에는 이 계획을 우선 적용한다. 진행 중인 변경과 실제 검사·배포 결과를 구분하고, 아래 미완료 항목을 구현 사실만으로 완료 처리하지 않는다.

## 시작 순서

1. `https://github.com/hurxxxx/miy-v2.git`의 최신 `main`을 checkout하고 `git status --short --branch`, `git log -5 --oneline`으로 작업 상태를 확인한다. 기존 작업본에 변경이 있으면 보존한다.
2. [구현 계획](CLI-IMPLEMENTATION-PLAN.md), 이 인계 문서, [실행 안내](RUNNING.md)를 읽는다. `README.md`는 문서 진입점이다.
3. `rust-toolchain.toml`의 Rust 1.99.1과 `package.json`의 pnpm 10.34.5를 기준으로 의존성을 준비한다. Node 등 실제 설치 조건은 실행 안내를 따른다.
4. 기준 코드의 정적·단위·격리 런타임·브라우저 검사를 재현하고 실제 서버 코드·migration 버전을 별도로 확인한다. 별도 Linux 검증 서버의 로그인·이미지 전송·dev 배포·prod 승격 실증도 초기에 수행한다. 장비가 없는 항목은 미실행으로 남기고 독립적인 기반 작업을 진행한다.
5. CLI 명령·공통 출력·preflight/expected-state/재시도 계약과 OS 경계부터 구현한다. **Windows MSVC 빌드 CI를 마지막 포장 작업으로 미루지 않는다.** 서버의 Unix 코드는 클라이언트 빌드에서 제외하며, CI 설정 추가나 OS 모듈 타입검사를 Windows 네이티브 실행 검증으로 간주하면 안 된다.
6. 아래 체크리스트를 순서대로 진행하면서 실제 결과만 완료로 표시한다. 코드 수정 뒤 관련 검사를 실행하고 서버/Windows 검증 결과를 구분해서 기록한다.

## 합의 사항

- 여러 서버·앱·작업·로그를 다루는 전체 운영 CLI를 만든다. 명령/출력의 변경은 허용하며 스크립트·문서·에이전트 지침을 함께 이전한다.
- Windows 11 x64 네이티브 `miy.exe`와 PowerShell 7/5.1은 정식 지원 대상이다. WSL만으로 대체하지 않는다. Windows ARM64/10은 이번 범위 밖이다.
- 브라우저 또는 링크+인증 코드 로그인, 안전한 사용자 세션 저장·갱신·철회를 구현한다. AI는 현재 사용자의 세션/권한을 사용한다. 별도 CI 계정은 후속 범위다.
- prod 변경과 모든 환경의 `app remove`는 명시적인 context/app/environment·영향 확인이 필요하며 `job retry`에도 같은 조건을 적용한다. `context remove`는 로컬 context 이름과 설정·자격증명 연결의 영향을 별도 확인하고 앱·환경을 요구하지 않는다. 비대화형 자동화는 명시적 대상과 `--yes`로 확인한다. `--yes`가 권한을 추가하지 않는다.
- 원격 관리에는 개발 SDK와 Docker를 요구하지 않는다. Windows 로컬 앱 개발은 Docker Desktop Linux 컨테이너를 사용하며 서버/앱 이미지는 Linux를 유지한다.
- CLI 목표는 0.5.0, 번들 목표는 0.6.2다. 버전은 실제 구현·릴리스 시 변경한다. 기존 앱의 코드·SQL 이력·컴포넌트·데이터는 보존한다.

## 코드 위치

| 구현 대상 | 먼저 읽을 파일 |
| --- | --- |
| 진입점·Clap·명령 실행 | [`main.rs`](../crates/miy/src/main.rs), [`app_cli.rs`](../crates/miy/src/app_cli.rs) |
| HTTP 클라이언트·현재 로그인·서버 CLI 인증 | [`cli_client.rs`](../crates/miy/src/cli_client.rs), [`cli_auth.rs`](../crates/miy/src/cli_auth.rs), [`http.rs`](../crates/miy/src/http.rs) |
| 브라우저 승인 화면 | [`web/platform/src/main.tsx`](../web/platform/src/main.tsx)의 `CliAuthorize` |
| OS·빌드 경계 | [`platform.rs`](../crates/miy/src/platform.rs), `Cargo.toml`의 `server` feature, [`CLI 기반 CI`](../.github/workflows/cli-foundation.yml) |
| 공유 계약·로컬 실행 유틸리티 | `api_error.rs`, `identity.rs`, `component_contract.rs`, `image_contract.rs`, `docker.rs`, `app_health.rs` — 서버 DB·라우터와 분리 |
| 도구 진단·실행 탐색 | [`dev_tools.rs`](../crates/miy/src/dev_tools.rs) |
| 개발 프로세스·Docker preview | [`preview.rs`](../crates/miy/src/preview.rs) |
| 작업 접수·권한·실행·health | [`runtime.rs`](../crates/miy/src/runtime.rs), [`domain.rs`](../crates/miy/src/domain.rs), [`worker.rs`](../crates/miy/src/worker.rs), [`runtime_health.rs`](../crates/miy/src/runtime_health.rs) |
| 산출물·파일 권한·업로드 | [`delivery.rs`](../crates/miy/src/delivery.rs) |
| MCP·스킬·매뉴얼·CLI 설치 | [`agent_tools.rs`](../crates/miy/src/agent_tools.rs), [`cli_guidance.rs`](../crates/miy/src/cli_guidance.rs), [`guidance/miy-cli/SKILL.md`](../crates/miy/guidance/miy-cli/SKILL.md) |
| 컴포넌트 | [`component_cli.rs`](../crates/miy/src/component_cli.rs), [`component_registry.rs`](../crates/miy/src/component_registry.rs) |
| 템플릿 포함·보호·업그레이드 | [`build.rs`](../crates/miy/build.rs), [`bundle.rs`](../crates/miy/src/bundle.rs), [`upgrade.rs`](../crates/miy/src/upgrade.rs), [`templates/rust-react/`](../templates/rust-react/), [`crates/miy/bundles/`](../crates/miy/bundles/) |
| DB migration | [`migrations/core/`](../migrations/core/) — 현재 마지막 파일은 `0008_runtime_health.sql` |
| 기존 설치·실행 | [`scripts/install-cli.sh`](../scripts/install-cli.sh), [`scripts/up.sh`](../scripts/up.sh), [`compose.yaml`](../compose.yaml) |
| 회귀 검사 | [`scripts/runtime-regression/README.md`](../scripts/runtime-regression/README.md), [`scripts/check-cli-guidance.mjs`](../scripts/check-cli-guidance.mjs), [`scripts/check-developer-workflow.mjs`](../scripts/check-developer-workflow.mjs), [`scripts/check-workflow-deployment.mjs`](../scripts/check-workflow-deployment.mjs) |

새 OS 계층과 출력·명령 카탈로그 모듈의 파일 이름은 구현 시 정하되, 기존 대형 `app_cli.rs`에 모든 역할을 계속 추가하지 않는다. `templates/rust-react/`를 수정하기 전에는 그 안의 `AGENTS.md`도 읽는다.

## 놓치기 쉬운 기존 동작

현재 runtime 요청은 네트워크 전송 전에 멱등성 키를 저장한다. 이 동작을 유지하면서 마지막 작업 파일 하나에 대한 의존을 제거한다. 응답 유실 복구는 runtime 앞의 이미지 업로드 접수·완료도 포함해야 한다. 접수 전 요청 ID·archive 해시·대상을 journal에 저장하고 upload ID·서버 상태를 재조회하여 중복 업로드/import/배포를 방지한다. 대용량 byte-range 전송 재개와는 별도 범위다. 원격 관찰의 Ctrl+C를 원격 작업 취소로 바꾸면 안 된다.

재시도 가능성을 문자열 오류 메시지로 추측하지 않는다. 타입이 있는 실패 분류와 공유 preflight·expected-state 계약을 1B에서 먼저 고정하고 retry·일반 실행에 함께 적용한다. prod 변경·앱 제거의 이전 승인은 재시도에 자동 재사용하지 않는다.

CLI 토큰 경로와 기존 OIDC introspection 경로는 다르다. 사용자 비활성화의 기준 원천·전파 방식·최대 지연·원천 조회 실패 정책을 인증 구현 전에 확정하고, 발급·갱신·API·대기 작업 실행에 실제 반영되는지 검사한다. 원천에서 비활성화한 증거 없이 즉시 차단을 지원한다고 기록하지 않는다.

`app deploy`는 Clap에 prod 값이 보이지만 실행 단계에서 거부한다. 기존 prod 경로는 검증한 릴리스의 `accept → promote`다. `describe`는 타입 스키마가 아니라 전체 help 묶음이고, `doctor`의 Docker 준비 판정은 현재 daemon 가동 여부를 보장하지 않는다.

Windows 장애 지점은 `std::os::unix::fs`, `.miy-tools/miy` 고정 경로, `open`/`xdg-open`만 사용하는 브라우저 실행, 실행 파일 교체, `.cmd` 실행, preview 자식 프로세스 소유, junction/reparse point, `build.rs`의 역슬래시 경로와 CRLF다. Windows 패키지만 만드는 것으로 해결되지 않는다.

프로젝트 설정·스킬·매뉴얼은 보호 해시와 CLI/bundle 잠금으로 관리한다. 번들 0.6.1은 0.6.0 공용 컴포넌트와 호환되며 업그레이드에 사용자 파일 보존 검사가 있다. 새 CLI 명령으로 생성 문서를 바꿀 때는 이 보호·호환·이전 번들 스냅샷을 함께 다룬다. LF 정책은 플랫폼 저장소뿐 아니라 생성·upgrade한 앱의 관리 파일에도 전달하고 사용자 `.gitattributes` 충돌을 검사한다. 적용된 SQL을 줄바꿈 정규화 대상으로 삼지 않는다. 파일 잠금·부분 교체 후 복구·이전 버전 보존은 1A에서 공통 절차를 정하고 설치기와 setup/upgrade에 적용한다.

Worker의 제어 큐와 health 관찰은 분리되어 있다. 로그 수집 때문에 두 경로를 다시 직렬화하거나 코어에 Docker 접근을 부여하지 않는다. Preview 정리는 run ID와 소유 리소스를 기준으로 하며 기존 볼륨과 다른 앱을 보존한다.

## 작업 체크리스트

첫 기반 구현에 착수했지만 아래 전체 단계는 아직 완료하지 않았다. 실제 검사 결과가 있어야 완료로 바꾸며, 부분 구현·CI 정의 추가·실제 환경 통과를 구분한다.

- [ ] **0 기준선과 Linux 실증**: 현재 환경의 정적·단위·격리 런타임·브라우저 회귀, 서버 코드/migration 확인, 별도 Linux 서버 로그인·업로드·dev 배포·동일 릴리스 prod 승격·UI/API·데이터 보존.
- [ ] **1A OS와 빌드 — 진행 중**: 서버/클라이언트 경계, Windows MSVC 빌드 CI, 실행 파일·프로세스·경로·권한 계층, 생성/upgrade 앱까지 번들 경로·LF 정책, OS 간 해시 검사, 파일 교체의 staging·중단 복구 설계.
- [ ] **1B 명령 계약 — 진행 중**: 기존 앱 명령의 공통 출력·오류·종료 코드를 구현 중이다. 새 명령 체계, JSON 입력/필드/페이지 계약, describe/completion 생성, develop/operate 진단, 공유 preflight·expected-state·타입별 실패/재시도 계약은 남아 있다.
- [ ] **2 인증**: Context 선택·저장, capability 확인, 브라우저/device-code UI/API, 갱신·철회·보안 저장, 사용자 비활성화 원천/전파/최대 지연 확정과 검사, 동시 갱신 및 다중 서버 검사.
- [ ] **3 운영**: 앱/작업 목록, 이미지 업로드 접수부터 runtime까지 요청 journal·멱등 재조회·응답 유실 복구, transaction 이벤트, watch/resume/retry, 권한이 있는 제한된 로그 조회와 worker 진단 경로.
- [ ] **4 실행 전 검증**: 공유 preflight, dry-run, 대상/영향 확인, 비TTY 승인 조건, 상태·권한 재검사와 부작용 0건 증명, prod/앱 제거 retry에 같은 확인 정책 적용.
- [ ] **5 Windows 배포·개발**: ZIP/PowerShell 설치·갱신·제거, 실행 파일 잠금 보존, 프로젝트 upgrade 복구, native preview와 Docker Desktop, Codex/Claude MCP.
- [ ] **6 이전과 릴리스**: 번들 0.6.2/CLI 0.5.0, 기존 앱 보존, 서버 선배포, 문서/스크립트 이전, OS/CPU CI, 실제 Windows E2E, AI별 3개 독립 세션, 다섯 바이너리와 체크섬 공개.

각 단계의 완료 기록에는 commit, OS/CPU/PowerShell 버전, 실행 명령, 성공·실패 결과, 증거 경로와 남은 항목을 남긴다. Windows 실제 검증 장비가 없으면 빌드와 단위 검사는 진행하되 Windows E2E와 릴리스 완료를 체크하지 않는다.

CLI 0.5의 AI별 3개 세션 검증은 [전체 검증 계획](VALIDATION.md)의 V7 완료와 별도다. v1→v2 데이터 변경·보존, 독립 설치, 코어/runner/호스트 재시작을 포함한 제품 전체 반복 인수는 기존 계획으로 계속 추적한다.

## 기존 검증과 새 환경에서의 재검증

2026-10-27 인계 당시 상태 확인에서 기준 코드의 Rust 검사는 **19개 통과/3개 ignored**, fmt·Clippy 경고 금지·TypeScript 검사는 통과했다. 격리 브라우저 검사는 **4개 통과**, 정리 실패 주입 검사는 **1개 테스트 안의 3개 실패 시나리오 통과**였다. 이는 이전 환경의 결과이며 Windows 검증 결과가 아니다. 계획을 저장한 `9757cc8`에서는 애플리케이션 검사를 다시 실행하지 않았다. 후속 기반 구현의 검사 결과는 아래 과거 기록을 대체하지 않고 별도로 추가한다.

실제 Docker/PostgreSQL 격리 회귀 **29개 통과**는 2026-10-24 구현 결과와 보관 증거로 확인한 이력이다. 2026-10-27 상태 확인 당시 Docker daemon에 연결되지 않았고 `localhost:18000/api/health`도 응답하지 않아 전체 런타임 회귀는 재실행하지 않았다. 최신 런타임 수정은 기존 실행 서버에 재배포하지 않았으며 migration 0008도 그 서버에 적용하지 않았다는 기록이 있다. 새 환경에서는 대상 서버의 상태를 별도로 확인한다.

기존 macOS/Linux 기준 검사는 저장소 루트에서 다음과 같이 실행한다. 새 환경에는 의존성을 먼저 설치한다.

```sh
pnpm install --frozen-lockfile
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
pnpm typecheck
python3 scripts/runtime-regression/test_cleanup.py
pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
```

Playwright 검사는 해당 버전 브라우저 설치가 필요하다. 설치 조건과 격리 서버 포트는 [회귀 검사 안내](../scripts/runtime-regression/README.md)를 따른다. Docker가 준비되면 별도 격리 회귀를 실행한다.

```sh
python3 scripts/runtime-regression/run.py
```

현재의 `bash scripts/check.sh`는 일반 상태 조회가 아니다. 실행 서버·인증·DB를 사용하고 worker를 중단·재시작하는 전체 검사다. [실행 안내](RUNNING.md)의 준비 조건을 갖춘 개발/검증 환경에서 실행한다. Windows 네이티브 검증은 [전용 안내](WINDOWS-VALIDATION.md)를 따르며 CI 정의는 [.github/workflows/cli-foundation.yml](../.github/workflows/cli-foundation.yml)에 있다.

## 환경 간 인계 경계

Git으로 전달되는 것은 코드·문서·lockfile·migration·스크립트다. `.local/`, `target/`, `node_modules/`, 로컬 증거·백업·CA·DB·Docker 볼륨·실행 중 서비스는 전달되지 않는다. 새 환경에서 서비스를 준비하고 로그인하며 비밀 파일을 커밋하지 않는다. 이전 `.local/evidence/`의 성공 기록이 새 checkout에 존재한다고 가정하지 않는다.

기존 [구현 결과](IMPLEMENTATION-RESULTS.md)는 날짜별 이력이다. 새 작업 결과를 추가할 때는 계획·로컬 검사·실제 서버 적용·Windows 검증을 구분한다. 현재 코드 버전과 서비스에 배포된 버전도 별도로 기록한다.


## 2026-10-27 첫 기반 구현 결과

문서 검토 보완과 1A의 빌드 경계·OS 파일/도구 계층·줄바꿈 정책을 반영했다. CLI 0.4.2 / bundle 0.6.1을 유지하며 0.5 명령·인증은 아직 제공하지 않는다. 상세 구현·검사·미검증 범위는 [구현 결과](IMPLEMENTATION-RESULTS.md#cli-05-기반-구현-착수-2026-10-27)를 따른다.

기본 빌드는 기존 서버 명령을 유지한다. 서버 없는 클라이언트는 다음으로 검사한다. 로컬 preview가 필요로 하는 Axum/SQLx는 남지만 코어 `State`·인증·DB 작업·서버 라우터 모듈과 `serve/worker/migrate/seed`는 제외된다.

```sh
cargo build --locked --no-default-features
cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
cargo test --workspace --locked --no-default-features
python3 scripts/check-bundle-portability.py check --cli target/debug/miy --output .local/ci/bundle.json
```

Windows CI는 `--no-default-features --target x86_64-pc-windows-msvc`로 빌드·검사한다. Linux에서 OS 모듈의 Windows 타입검사는 통과했지만 전체 교차 검사는 `ring`의 MSVC `lib.exe` 부재로 실행되지 못했다. 이후 `fbbd087`의 [네이티브 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40446385352)는 Linux 서버/클라이언트, macOS, Windows MSVC와 OS 간 번들 비교를 모두 통과했다. 실제 사용자 Windows 11·PowerShell·Docker Desktop E2E는 미검증이며 1A 전체 완료 체크를 하지 않는다.

Windows 빌드·검증·로컬 ZIP 생성과 결과 기록은 [별도 문서](WINDOWS-VALIDATION.md)로 관리한다. 사용자가 Windows 환경에서 실행한 뒤 결과 업데이트를 요청하면 commit·환경·명령·증거에 맞춰 기록한다. 결과가 오기 전에는 미검증 상태를 유지하며, 독립적인 1B 공통 출력/오류·preflight·expected-state/재시도 타입 구현은 진행한다. Windows/macOS 네이티브 CI, 여러 파일 복구·소유 프로세스 종료 경계와 별도 원격 Linux 서버의 인증·전송·UI 실증도 남아 있다. 이 환경의 격리 Linux 런타임 회귀를 원격 서버 실증으로 대신하지 않는다.


## 2026-10-27 공통 출력·오류 계약 첫 구현

기반 commit `fbbd087` 이후 기존 `miy app ...` 명령에 공통 출력 계층을 연결했다. `--output auto|text|json`을 지원하고, 자동 모드는 stdout TTY에서 text·비TTY에서 JSON을 선택한다. JSON 성공 값은 `data`로 이동했으며 실패도 stdout의 단일 `schemaVersion: 1` envelope다. 오류 분류·종료 코드는 구현 계획의 표를 따르고, 임의 오류는 내부 오류 1로 남긴다. 스크립트는 stdout과 실제 종료 코드를 함께 확인한다. 기존 바이너리와 달라진 출력 계약이므로 버전 문자열만으로 구분하지 말고 commit도 확인한다.

help/version, Markdown describe, stdio MCP, legacy Codex 자식 스트림은 원래 형식을 유지한다. JSONL은 이벤트 명령이 생기기 전까지 실행 전에 거부한다. 서버의 serve/worker/migrate/seed는 기존 출력 경로이며 명시적인 `--output`을 거부한다. preview readiness는 stderr와 프로젝트 상태 파일에 기록한다. 원격 작업 관찰의 Ctrl+C는 작업을 취소하지 않고 job ID·복구 argv와 종료 코드 130을 반환한다.

검증 진입점은 `python3 scripts/check-cli-output.py --cli target/debug/miy`, `node --test scripts/lib/cli-result.test.mjs`, `node scripts/check-cli-guidance.mjs`다. 첫 Python 검사는 실제 플랫폼 대신 임시 loopback fixture를 사용하며 CI의 Linux/macOS/Windows 클라이언트에서 실행한다. Windows 콘솔 Ctrl+C는 별도 사용자 검증이다. 실행 증거와 미실행 범위는 [구현 결과](IMPLEMENTATION-RESULTS.md#cli-공통-출력과-오류-계약-2026-10-27)에 기록한다.

다음 구현은 서버/CLI가 공유할 preflight·expected-state와 타입별 원격 실패/재시도 계약이다. 현재의 `retryable`은 전송·관찰 오류에 대한 정보이며 원격 작업의 새 retry API나 실행 승인을 제공하지 않는다. 새 루트 명령, JSON 입력/fields/pagination, context/auth, job 이벤트·로그와 dry-run은 아직 남아 있다. 1B 전체를 완료로 표시하지 않는다.
