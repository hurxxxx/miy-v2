# 변경별 검사 지도

- 작성일: 2026-11-03
- 역할: 무엇을 바꿨을 때 어떤 검사를 최소로 실행하는지 정한다. 검사 결과의 기록 방식은 [문서 지도](README.md#문서-작성-규칙)와 루트 `AGENTS.md`를 따른다.

검사는 필요한 환경에 따라 네 층으로 나뉜다. 위층일수록 가볍고 안전하다.

| 층 | 명령 | 필요한 것 | 안전 범위 |
| --- | --- | --- | --- |
| 정적 | `bash scripts/check-static.sh [all\|rust\|web\|scripts]` | Rust 1.99.1, Node 22·pnpm, Python 3, Playwright Chromium | CI와 운영 호스트에서도 실행 가능. 빌드는 `.local/evidence/check-static`에만 쓰고 모의 API를 쓴다 |
| 격리 Docker | `python3 scripts/runtime-regression/run.py`, `server-ui.config.ts`, `scripts/runtime-regression/stack-ui.sh run` | Docker와 고정 이미지 | 자체 라벨·포트·상태만 만들고 정리한다. 운영 컨테이너·DB를 건드리지 않는다 |
| 로컬 개발 스택 | `bash scripts/check.sh` | `scripts/up.sh`로 띄운 loopback 스택 | `.local`을 읽고 worker를 재시작한다. origin이 loopback이 아니면 거부한다 |
| 운영 | [서버 운영 안내](SERVER-OPERATIONS.md)의 반영 절차와 연결 검사 | 사용자의 반영 요청 | 반영 전 백업·복원 확인, 반영 후 읽기 위주 확인 |

GitHub Actions는 `cli-foundation.yml`(Linux 서버·Linux/macOS/Windows 클라이언트의 fmt·Clippy·test·CLI 검사)과 `web.yml`(정적 `scripts`·`web`)을 실행한다. CI 통과를 격리 Docker·로컬 스택·운영 검사의 통과로 옮기지 않는다.

`scripts/test-core-ops.py`는 백업 정리·연결 예산과 유지 관리하는 배포 도구(`scripts/core-rollout.py`, `scripts/ui-rollout.py`)의 실패 경계를 모든 외부 명령을 대체한 모의 시험으로 검사한다. 운영 배포를 실행하지 않는다. `docs/releases/evidence/`의 배포별 도구 사본은 당시 기록이며 검사 대상이 아니다. 격리 runtime 도구는 CLI와 테스트 실행 파일을 검증 디렉터리에 복사해 고정하며, worker 자식도 같은 테스트 실행 파일을 사용한다. 다른 Cargo 빌드가 공유 `target`을 바꿔도 실행 중인 회귀의 자식 파일 경로가 사라지지 않게 한다.

## 변경 영역별 최소 검사

| 바꾼 것 | 최소 검사 | 해당할 때 추가 |
| --- | --- | --- |
| 문서만(`docs/`, `README.md`) | `python3 scripts/check-doc-links.py` | 상태·버전·commit을 바꿨다면 실제 증거와 대조 |
| 코어 서버 Rust(`crates/miy/src`의 서버 기능) | `check-static.sh rust` | 배포·이미지 적재·권한·위임: `python3 scripts/runtime-regression/run.py`. 개발 세션 DB: `python3 scripts/check-development-database.py`. 서버 빌드: `python3 scripts/check-server-builds.py`(셋 다 일회용 PostgreSQL·Docker). 실제 DB 복구 테스트는 로컬 개발 스택의 `check.sh` |
| core migration(`migrations/core`) | `check-static.sh rust`, `run.py` | 이미 적용한 파일은 고치지 않고 새 번호로 추가(sqlx가 checksum 불일치를 거부). 운영 반영 전 `python3 scripts/core-backup.py create`로 대상 DB를 백업하고 `verify --migrate-with <서버 바이너리>`로 일회용 PostgreSQL에서 복원·migration 리허설 |
| CLI 클라이언트(`--no-default-features` 경로) | `check-static.sh rust` | `cargo build --locked --no-default-features` 후 `python3 scripts/check-cli-output.py --cli target/debug/miy`, `check-cli-install.py`, `check-cli-recovery.py`. macOS·Windows는 CI 매트릭스 |
| 개발 번들·템플릿·CLI 지침(`templates/`, `crates/miy/guidance/`, `registry/`) | `check-static.sh rust`(번들 바이트 검사 포함) | 지침·업그레이드: 이전 기준 CLI를 `MIY_PREVIOUS_BUNDLE_CLI`로 지정하고 `node scripts/check-cli-guidance.mjs`. 번들 버전 변경은 [CLI 릴리스](CLI-RELEASE.md) 절차 |
| 플랫폼 웹(`web/platform`) | `check-static.sh web` | 셸 CSS: `node scripts/runtime-regression/app-css-baseline.mjs <기준 commit>`. 실제 core 연동: `cargo build` 후 `pnpm exec playwright test --config scripts/runtime-regression/server-ui.config.ts`. 인증·앱 배포 전체: `stack-ui.sh run` |
| SDK·공용 UI(`packages/sdk`, `registry/components`) | `check-static.sh web` | 게시된 불변 버전 디렉터리는 수정하지 않고 새 버전을 추가. 컴포넌트 설치·갱신 흐름: 로컬 개발 스택에서 `node scripts/check-components.mjs` |
| 검증·정리 스크립트(`scripts/`) | `check-static.sh scripts` | 자원을 지우는 스크립트는 소유 표식·기존 자원 보존을 stub 테스트로 확인 |
| 릴리스·설치 파일(`scripts/package-*`, `stage-*`, 릴리스 workflow) | `check-static.sh scripts` | GitHub Actions 후보 실행(`publish=false`) 후 `package-cli-release.py verify`, `package-windows-installer.py verify`. Windows 11 실제 설치는 [Windows 인수](WINDOWS-ACCEPTANCE.md)로 사용자가 수행 |
| 서버 구성 생성(`bootstrap.py`, `render-server-config.py`, `configure-identity.py`) | `check-static.sh scripts`(`check-server-config.py` 포함) | 운영 `.local`에 bootstrap·seed·Keycloak 재설정을 실행하지 않는다 |
| 운영 반영(v2.dwdcc.kr) | 위 해당 검사 전부, [배포 도구](SERVER-OPERATIONS.md#7-배포-도구)의 `inspect`(백업·별도 복원 확인과 연결 예산은 `core-rollout.py apply`가 수행) | health·설치 안내 버전, 게시 CLI로 `node scripts/check-platform-connection.mjs`, 읽기 위주 브라우저 확인, 운영 기록 갱신 |

## 루트 Playwright 설정

루트 `playwright.config.ts`(`pnpm test:e2e`)는 실행 중인 로컬 개발 스택을 쓰는 `validation.spec.ts`, `development-login.spec.ts`만 실행한다. 다른 spec은 자기 설정과 서버를 함께 쓴다.

| spec | 설정 | 서버 |
| --- | --- | --- |
| `runtime-health`, `platform-shell`, `runtime-confirmation`, `platform-css-isolation` | `scripts/runtime-regression/ui.config.ts` | 격리 빌드 preview + 모의 API |
| `cli-experience`, `cli-install` | `tests/cli-experience.config.ts` | Vite dev 서버 + 모의 API |
| `http-ip` | `scripts/runtime-regression/http-ui.config.ts` | 비 loopback IPv4의 격리 preview |
| `server-ui` | `scripts/runtime-regression/server-ui.config.ts` | 일회용 PostgreSQL과 이 checkout의 core |
| `stack-ui` | `scripts/runtime-regression/stack-ui.config.ts` | `stack-ui.sh`의 일회용 전체 스택 |
