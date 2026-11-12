# 변경별 검사 지도

- 작성일: 2026-11-03
- 역할: 무엇을 바꿨을 때 어떤 검사를 최소로 실행하는지 정한다. 검사 결과의 기록 방식은 [문서 지도](README.md#문서-작성-규칙)와 루트 `AGENTS.md`를 따른다.

검사는 필요한 환경에 따라 네 층으로 나뉜다. 위층일수록 가볍고 안전하다.

| 층 | 명령 | 필요한 것 | 안전 범위 |
| --- | --- | --- | --- |
| 정적·설정 | `bash scripts/check-static.sh [all\|rust\|web\|scripts]` | Rust 1.99.1, Node 22·pnpm, Python 3, Playwright Chromium. `scripts`의 격리 프록시 검사는 Docker·PostgreSQL/Caddy 이미지·Nginx 필요 | CI와 운영 호스트에서도 실행 가능. 빌드는 `.local/evidence/check-static`에만 쓰고 모의 API를 쓴다. 프록시 검사는 임시 경로·전용 포트만 사용 |
| 격리 Docker | `python3 scripts/runtime-regression/run.py`, `server-ui.config.ts`, `scripts/runtime-regression/stack-ui.sh run` | Docker와 고정 이미지 | 자체 라벨·포트·상태만 만들고 정리한다. 운영 컨테이너·DB를 건드리지 않는다 |
| 로컬 개발 스택 | `bash scripts/check.sh` | `scripts/up.sh`로 띄운 loopback 스택 | `.local`을 읽고 worker를 재시작한다. origin이 loopback이 아니면 거부한다 |
| 운영 | [서버 운영 안내](SERVER-OPERATIONS.md)의 반영 절차와 연결 검사 | 사용자의 반영 요청 | 반영 전 백업·복원 확인, 반영 후 읽기 위주 확인 |

GitHub Actions는 `cli-foundation.yml`(Linux 서버·Linux/macOS/Windows 클라이언트의 fmt·Clippy·test·CLI 검사)과 `web.yml`(정적 `scripts`·`web`)을 실행한다. 각 workflow가 실제 실행한 검사 범위를 구분하고, 이를 로컬 전체 스택·운영 검사의 통과로 옮기지 않는다.

`runtime-regression.yml`은 main push·main 대상 PR에서 격리 Docker 회귀를 실행한다. 현재 기대값은 runtime 46·operations 6·CLI 6·auth 8·permissions 24·delegation 10·boundary 12·migration 3·frames 14, 총 129개 관찰이다. 권한 스냅샷 변경 중 실제 HTTP 목록·worker 재확인과 위임 발급·사용자 비활성화의 HTTP 경합도 포함한다. 프레임 인증 뒤 본문 수집 중 릴리스가 바뀌는 handler 검사와 발급된 후속 호출의 원 프레임 결합 검사도 포함하며, 브라우저가 GET 본문을 보낸다는 전제는 쓰지 않는다. 로컬 실행의 PASS와 해당 commit의 GitHub 실행 결과는 따로 기록한다. CLI 응답 타입을 바꿀 때는 정적 검사 외에 실제 클라이언트 빌드로 출력·복구 검사까지 실행한다.

`scripts/test-core-ops.py`는 백업 정리·연결 예산과 유지 관리하는 배포 도구(`scripts/core-rollout.py`, `scripts/ui-rollout.py`)의 실패 경계를 모든 외부 명령을 대체한 모의 시험으로 검사한다. 부분 서비스 장애 중 rollback 유휴 검사, 서로 다른 release의 같은 UI 대상 잠금, 무시된 public/환경 입력·심볼릭 링크와 빌드 중 입력 변경 거부, `ui-rollout.py --target frame-runtime`의 첫 설치·rollback 뒤 새 빌드, 중단 뒤 index 없이 남은 자산의 보존·충돌 검사도 포함한다. `scripts/test-app-frame-settings.py`는 앱 프레임 설정 도구(`scripts/app-frame-settings.py`)의 설정 패치·복원, 백업/복구 기록 실패와 부분 적용·부분 복원 뒤 재시도, Nginx 차이 검사를 실제 렌더러 출력까지 포함해 검사한다. 프록시 검사에는 upstream 장애 시 앱 자산 능력값이 access/error log에 남지 않는지도 포함한다. `test-frame-diagnostics.mjs`는 브라우저 오류 메시지를 증거로 기록하기 전에 자산 능력값·자격증명을 가리는지 검사한다.

코어 중지 검사는 `failed`·`inactive` 표시와 실제 프로세스 부재를 구분한다. `reset-failed` 전후의 `MainPID`·`ControlPID`, 하위 cgroup을 포함하는 cgroup v2 `populated` 값, 고정 서비스 경로를 확인한다. 잔류 프로세스·읽기 실패·판정 불가 시 배포·복구가 중단되고, 이미 중지된 코어의 rollback 유휴 검사 생략도 실제로 비어 있을 때만 허용되는지 검사한다. 범위와 실행 결과는 [서버 운영 안내](SERVER-OPERATIONS.md#7-배포-도구)와 [중지 확인 검증](releases/core-stop-quiescence-2026-11-05-validation.json)에 기록한다.

모의 시험은 운영 배포를 실행하지 않는다. 실제 Vite 빌드와 실제 HTTP·PostgreSQL 위임 검증은 별도 실행으로 기록한다. `docs/releases/evidence/`의 배포별 도구 사본은 당시 기록이며 검사 대상이 아니다. 격리 runtime 도구는 CLI와 테스트 실행 파일을 검증 디렉터리에 복사해 고정하며, worker 자식도 같은 테스트 실행 파일을 사용한다. 다른 Cargo 빌드가 공유 `target`을 바꿔도 실행 중인 회귀의 자식 파일 경로가 사라지지 않게 한다.

## 변경 영역별 최소 검사

| 바꾼 것 | 최소 검사 | 해당할 때 추가 |
| --- | --- | --- |
| 문서만(`docs/`, `README.md`) | `python3 scripts/check-doc-links.py` | 상태·버전·commit을 바꿨다면 실제 증거와 대조. 검사기는 tracked 문서만 순회하므로 신규 untracked 문서의 링크·앵커도 별도 확인 |
| 코어 서버 Rust(`crates/miy/src`의 서버 기능) | `check-static.sh rust` | 배포·이미지 적재·권한·위임: `python3 scripts/runtime-regression/run.py`. 개발 세션 DB: `python3 scripts/check-development-database.py`. 서버 빌드: `python3 scripts/check-server-builds.py`(셋 다 일회용 PostgreSQL·Docker). 실제 DB 복구 테스트는 로컬 개발 스택의 `check.sh` |
| core migration(`migrations/core`) | `check-static.sh rust`, `run.py` | 이미 적용한 파일은 고치지 않고 새 번호로 추가(sqlx가 checksum 불일치를 거부). 운영 반영 전 `python3 scripts/core-backup.py create`로 대상 DB를 백업하고 `verify --migrate-with <서버 바이너리>`로 일회용 PostgreSQL에서 복원·migration 리허설 |
| CLI 클라이언트(`--no-default-features` 경로) | `check-static.sh rust` | `cargo build --locked --no-default-features` 후 `python3 scripts/check-cli-output.py --cli target/debug/miy`, `check-cli-install.py`, `check-cli-recovery.py`. macOS·Windows는 CI 매트릭스 |
| 개발 번들·템플릿·CLI 지침(`templates/`, `crates/miy/guidance/`, `registry/`) | `check-static.sh rust`(번들 바이트 검사 포함) | 지침·업그레이드: 이전 기준 CLI를 `MIY_PREVIOUS_BUNDLE_CLI`로 지정하고 `node scripts/check-cli-guidance.mjs`. 번들 버전 변경은 [CLI 릴리스](CLI-RELEASE.md) 절차 |
| 플랫폼 웹(`web/platform`) | `check-static.sh web` | 업무 앱 프레임: `stack-ui.sh up` 후 `pnpm exec playwright test -c scripts/runtime-regression/app-frames.config.ts`(브라우저는 `MIY_FRAME_BROWSERS`). 실제 core 연동: `cargo build` 후 `pnpm exec playwright test --config scripts/runtime-regression/server-ui.config.ts`. 인증·앱 배포 전체: `stack-ui.sh run` |
| SDK·공용 UI(`packages/sdk`, `registry/components`) | `check-static.sh web` | 게시된 불변 버전 디렉터리는 수정하지 않고 새 버전을 추가. 컴포넌트 설치·갱신 흐름: 로컬 개발 스택에서 `node scripts/check-components.mjs` |
| 검증·정리 스크립트(`scripts/`) | `check-static.sh scripts` | 자원을 지우는 스크립트는 소유 표식·기존 자원 보존을 stub 테스트로 확인 |
| 릴리스·설치 파일(`scripts/package-*`, `stage-*`, 릴리스 workflow) | `check-static.sh scripts` | GitHub Actions 후보 실행(`publish=false`) 후 `package-cli-release.py verify`, `package-windows-installer.py verify`. Windows 11 실제 설치는 [Windows 인수](WINDOWS-ACCEPTANCE.md)로 사용자가 수행 |
| 서버 구성 생성(`bootstrap.py`, `render-server-config.py`, `configure-identity.py`) | `check-static.sh scripts`(`check-server-config.py` 포함) | 운영 `.local`에 bootstrap·seed·Keycloak 재설정을 실행하지 않는다 |
| 운영 반영(v2.dwdcc.kr) | 위 해당 검사 전부, [배포 도구](SERVER-OPERATIONS.md#7-배포-도구)의 `inspect`(백업·별도 복원 확인과 연결 예산은 `core-rollout.py apply`가 수행) | health·설치 안내 버전, 게시 CLI로 `node scripts/check-platform-connection.mjs`, 읽기 위주 브라우저 확인, 운영 기록 갱신 |

최소 보안 구현은 [경계 인수](APP-BOUNDARIES.md#경계-인수)의 필수 행을 검사한다. 업무 포털(prod)·dev 포털(dev)·콘솔+Keycloak 세 출처, portal/dev_portal/console 용도·환경별 Use 판정·기존 SDK/CLI 호환이 기준이다. 후속 BV-13·외부 앱 격리·별도 IdP·MFA 의무화·자동 CA는 현재 기능 개발의 선행 검사가 아니다. 같은 BV ID의 이전 iframe 결과는 이월하지 않으며 문서 PASS로 제품 NOT_RUN을 바꾸지 않는다.

실제 Core·브라우저로 포털 세션의 관리 변경·device 승인 거부와 콘솔의 정상 승인, device 시작·poll/refresh·상대 verificationPath redirect, legacy start/approve/exchange의 410 종료를 확인한다. 같은 호스트 다른 출처의 쿠키 인증 요청(GET·탐색 포함)이 거부되는지도 확인한다. realm/sub·callback·MCP 호환, device와 legacy 회수, 사용자/그룹 권한 회수, API sandbox/nosniff·정상 JSON/MF/preview·Nginx anti-frame을 검사한다. 관리 계정의 포털 사용·dev 개발 편의가 유지되는지도 확인한다. 전면 CLI 중지나 개발자 앱 사용 금지는 기본 인수가 아니며 구 위험 경로를 임시 중지할 때만 영향·복구를 별도 시험한다.

단일 IP HTTPS는 인증서 반입·CA 신뢰/IP SAN·만료 거부·수동 교체, 실제 OIDC/CLI, 포트 공유 쿠키·잘못된 목적/중복 인증 쿠키 거부·기본 복구를 확인한다. 쿠키 가용성 잔여를 기록하고 자동 CA·전 기기 스트레스는 후속으로 둔다. [공통 ACL·PLM 연계](CORE-PERMISSIONS.md#대표-인수와-검토할-결정)는 G4·G5·G6·Invoke·서비스·이벤트·파일의 해당 기능을 구현할 때 검사하며 모두를 최소 경계의 선행 조건으로 묶지 않는다. 기존 HTTP IP 결과로 새 HTTPS 인수를 대신하지 않는다.

## 루트 Playwright 설정

루트 `playwright.config.ts`(`pnpm test:e2e`)는 명시적인 격리 상태·DB·origin·빌드 경로(`MIY_STATE_DIR`, `MIY_DATABASE_CONTAINER`, `MIY_TEST_ORIGIN`, `MIY_TEST_WEB_DIST`, `MIY_FIXTURE_DIST`, `MIY_BROWSER_CDP`)를 요구하는 `validation.spec.ts`, `development-login.spec.ts`만 실행한다. 운영 `.local`·DB와 `web/platform/dist`는 거부한다. 이 호스트에서는 `check.sh` 대신 정적/격리 도구를 사용한다. `validation.spec.ts`는 `stack-ui.sh up` 뒤 그 상태(`MIY_STATE_DIR=.local/stack-ui`, `MIY_DATABASE_CONTAINER=miy-stack-ui-postgres-1`, `MIY_TEST_ORIGIN=http://localhost:18300`, `MIY_TEST_WEB_DIST=.local/stack-ui/site`), 별도 fixture 빌드와 CDP로 연 Chromium으로도 실행할 수 있다. 다른 spec은 자기 설정과 서버를 함께 쓴다.

| spec | 설정 | 서버 |
| --- | --- | --- |
| `runtime-health`, `platform-shell`, `runtime-confirmation` | `scripts/runtime-regression/ui.config.ts` | 격리 빌드 preview + 모의 API·모의 앱 프레임(`tests/frame-mock.ts`) |
| `cli-experience`, `cli-install` | `tests/cli-experience.config.ts` | Vite dev 서버 + 모의 API |
| `surface-boundaries` | `tests/surface-boundaries.config.ts` | 세 목적의 UI·환경별 이동 계약과 콘솔의 MF 미적재를 검사하는 모의 서버 |
| `http-ip` | `scripts/runtime-regression/http-ui.config.ts` | 비 loopback IPv4의 격리 preview |
| `server-ui` | `scripts/runtime-regression/server-ui.config.ts` | 일회용 PostgreSQL과 이 checkout의 core |
| `stack-ui` | `scripts/runtime-regression/stack-ui.config.ts` | `stack-ui.sh`의 일회용 전체 스택 |
