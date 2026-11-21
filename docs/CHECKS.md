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

GitHub Actions는 `cli-foundation.yml`(Linux 서버·Linux/macOS/Windows 클라이언트의 fmt·Clippy·test·CLI 검사)과 `web.yml`(정적 `scripts`·`web`)을 실행한다. 빌드 요청 복구 검사인 `check-cli-build.py`는 세 클라이언트 OS의 네이티브 바이너리에서 실행한다. 각 workflow가 실제 실행한 검사 범위를 구분하고, 이를 로컬 전체 스택·운영 검사의 통과로 옮기지 않는다.

`runtime-regression.yml`은 main push·main 대상 PR에서 격리 Docker 회귀와 별도 서버 빌드·개발 DB 검사를 실행한다. 현재 기대값은 runtime 53·operations 5·CLI 7·auth 8·accounts 11·permissions 26·delegation 10·boundary 12·migration 3·frames 14, 총 149개 관찰이다. CLI에는 다른 프로세스가 로그인 갱신 중일 때 동시 명령이 재로그인을 요구하지 않고 그 결과를 기다려 쓰는지, runtime에는 재생성된 DB 컨테이너를 다른 런타임 작업이 대기하는 동안 실행 중인 앱의 설치 네트워크에 재연결하는지가 포함된다. accounts는 계정 링크의 발급 권한·1회 사용·취소/만료/시도 소진·발급자 권한 재확인·등록 실패 시 로그인 계정 삭제·재설정 결과 유실·세션 종료 재시도·취소 가능 링크 목록을 실제 PostgreSQL과 HTTP로 검사하며 로그인 서버 관리 호출은 기록용 대역이다. 런타임에는 실패한 승격의 기존 릴리스 복원, DB 변경 후 명시적 복구, 활성화와 상태 기록의 원자성, 재개 시 공식 앱의 회사 중지 재확인이 포함된다. 권한 스냅샷 변경 중 실제 HTTP 목록·worker 재확인과 위임 발급·사용자 비활성화의 HTTP 경합도 포함한다. 프레임 인증 뒤 본문 수집 중 릴리스가 바뀌는 handler 검사와 발급된 후속 호출의 원 프레임 결합 검사도 포함하며, 브라우저가 GET 본문을 보낸다는 전제는 쓰지 않는다. 로컬 실행의 PASS와 해당 commit의 GitHub 실행 결과는 따로 기록한다. CLI 응답 타입을 바꿀 때는 정적 검사 외에 실제 클라이언트 빌드로 출력·복구 검사까지 실행한다.

`scripts/test-core-ops.py`는 백업 정리·연결 예산과 유지 관리하는 배포 도구(`scripts/core-rollout.py`, `scripts/ui-rollout.py`)의 실패 경계를 모든 외부 명령을 대체한 모의 시험으로 검사한다. 부분 서비스 장애 중 rollback 유휴 검사, 서로 다른 release의 같은 UI 대상 잠금, 무시된 public/환경 입력·심볼릭 링크와 빌드 중 입력 변경 거부, `core-rollout.py`의 CLI 게시 동시 반영(실제 패키지 검사기를 통과하는 5개 CLI·2개 helper·설치기 fixture, 불완전한 자료·체크섬 거부, 후보 복사·검증 뒤 변경 거부, 기존 게시 도구와 공유하는 잠금 및 실패 시 해제, index 교체 중 실패·시작 후 실패의 복원), `ui-rollout.py --target frame-runtime`의 첫 설치·rollback 뒤 새 빌드, 중단 뒤 index 없이 남은 자산의 보존·충돌 검사도 포함한다. `scripts/test-app-frame-settings.py`는 앱 프레임 설정 도구(`scripts/app-frame-settings.py`)의 설정 패치·복원, 백업/복구 기록 실패와 부분 적용·부분 복원 뒤 재시도, Nginx 차이 검사를 실제 렌더러 출력까지 포함해 검사한다. 프록시 검사에는 upstream 장애 시 앱 자산 능력값이 access/error log에 남지 않는지도 포함한다. `test-frame-diagnostics.mjs`는 브라우저 오류 메시지를 증거로 기록하기 전에 자산 능력값·자격증명을 가리는지 검사한다.

코어 중지 검사는 `failed`·`inactive` 표시와 실제 프로세스 부재를 구분한다. `reset-failed` 전후의 `MainPID`·`ControlPID`, 하위 cgroup을 포함하는 cgroup v2 `populated` 값, 고정 서비스 경로를 확인한다. 잔류 프로세스·읽기 실패·판정 불가 시 배포·복구가 중단되고, 이미 중지된 코어의 rollback 유휴 검사 생략도 실제로 비어 있을 때만 허용되는지 검사한다. 범위와 실행 결과는 [서버 운영 안내](SERVER-OPERATIONS.md#7-배포-도구)와 [중지 확인 검증](releases/core-stop-quiescence-2026-11-05-validation.json)에 기록한다.

모의 시험은 운영 배포를 실행하지 않는다. 실제 Vite 빌드와 실제 HTTP·PostgreSQL 위임 검증은 별도 실행으로 기록한다. `docs/releases/evidence/`의 배포별 도구 사본은 당시 기록이며 검사 대상이 아니다. 격리 runtime 도구는 CLI와 테스트 실행 파일을 검증 디렉터리에 복사해 고정하며, worker 자식도 같은 테스트 실행 파일을 사용한다. 다른 Cargo 빌드가 공유 `target`을 바꿔도 실행 중인 회귀의 자식 파일 경로가 사라지지 않게 한다.

`scripts/test-file-routes-rollout.py`는 유지관리 프록시 도구의 검사·reload·사본 갱신·기록 저장 실패와 부분 복원 재시도를 임시 파일·모의 외부 명령으로 확인한다. 당시 `.local` 배포 도구와 백업은 수정하지 않는다. `node scripts/test-pilot-flow.mjs`는 배포 응답 유실·상태 조회 실패·진행 중 작업·중지 실패와 결과 식별자를 모의 CLI 상태로 확인하며 서버에 앱을 만들지 않는다. 둘 다 `check-static.sh scripts`에 포함한다.

새 서버 안내는 [SETUP](SETUP.md)을 따른다. 별도 VM에서 설치 도중 재접속과 변수/배열 복구, DB/IdP 준비·재부팅, 직원의 첫 암호 변경·중지, 앱 컨테이너의 공통 API 연결, 인증서 갱신, 백업·복원·버전 갱신을 인수한다. 문서 명령/격리 설정 검사를 실제 새 호스트 설치·재부팅 PASS로 확대하지 않는다. 2026-11-16 후속 보완은 사용자가 VM 검증을 맡기로 하여 문서만 반영하며 추가 실행 검사는 NOT_RUN이다.

앱 DB 로그인 경계를 바꾸면 정적 Rust 검사 외에 `scripts/check-development-database.py`를 실행한다. 실제 `session_user`·DB 결합, `RESET ROLE` 뒤에도 비관리자인지, 쿼리의 사용자/암호/DB 이름이 앱 계정을 덮지 못하는지 확인한다. 최초 준비의 관리자 연결도 지정된 앱 DB만 설정하며 관리용 DB의 스키마·기존 권한을 보존하는지 검사한다. 현재 역할만 확인하는 시험으로는 관리자 로그인 후 `SET ROLE`을 구분할 수 없다.

앱 응답의 4MiB 한도·64개 슬롯·Bytes clone/slice 수명·오류/취소 반환은 모든 OS의 `app_response` 단위 검사에서 확인한다. 작은 TCP 송수신 버퍼로 느린 클라이언트를 만드는 추가 시험은 Unix에서만 실행한다. Windows에서는 이 설정에도 OS가 본문 전체를 받아갈 수 있어 애플리케이션 버퍼가 반드시 남는다는 전제가 성립하지 않았다(커밋 `7952c8e`의 CI 실패). Windows의 실제 전송 지연·전체 RSS 검증은 이 시험의 PASS로 대신하지 않는다.

`scripts/test-server-addresses.py`는 도메인 없는 IPv4/IPv6·포트와 도메인 입력, 충돌 거부, 읽기 전용 검사·후보 설정의 원본 보존, 임시 PostgreSQL에서 실제 프레임 이전 SQL의 트랜잭션·기존 주소 예약·세션 회수·스냅샷 변경 거부를 검사한다. `check-server-config.py`는 Keycloak frontendUrl·MCP client ID 보존과 TLS symlink 경로도 확인한다. 실제 호스트의 주소 전환이나 외부 PC 재로그인은 이 격리 검사와 별도다.

`test-online-cli-install.py`는 임시 홈·HTTP 서버에서 실제 패키지 설치 스크립트를 호출해 다운로드·해시 거부·기존 파일 보존·서버 등록 인자·PATH 중복 방지와 OS 선택을 검사한다. CLI 실행 파일은 fixture이며 실제 공개 패키지 설치와 Windows/macOS 실기 인수를 대신하지 않는다. Core Rust 검사는 원본 Host 대신 설정한 도메인/IPv4/IPv6·포트 사용과 템플릿 렌더링·미게시 503을, 웹 검사는 명령 복사·미게시 상태·직접 다운로드의 접힌 구성을 확인한다.

## 변경 영역별 최소 검사

| 바꾼 것 | 최소 검사 | 해당할 때 추가 |
| --- | --- | --- |
| 문서만(`docs/`, `README.md`) | `python3 scripts/check-doc-links.py` | 상태·버전·commit을 바꿨다면 실제 증거와 대조. 검사기는 tracked 문서만 순회하므로 신규 untracked 문서의 링크·앵커도 별도 확인 |
| 코어 서버 Rust(`crates/miy/src`의 서버 기능) | `check-static.sh rust` | 배포(서버 빌드 산출물만 접수)·권한·위임: `python3 scripts/runtime-regression/run.py`. 개발 세션 DB: `python3 scripts/check-development-database.py`. 서버 빌드: `python3 scripts/check-server-builds.py`(셋 다 일회용 PostgreSQL·Docker). 실제 DB 복구 테스트는 로컬 개발 스택의 `check.sh` |
| 계정 링크·로그인 서버 연동(`accounts.rs`, `configure-identity.py`의 서비스 클라이언트·보호 설정) | `check-static.sh rust`, `run.py`, `python3 scripts/check-server-config.py` | 실제 Keycloak 동작은 `stack-ui.sh up` 뒤 `node scripts/check-account-links.mjs --origin http://localhost:18300 --credentials-file "$PWD/.local/stack-ui/credentials.json" --output <새 파일> --lockout true`, 끝나면 `stack-ui.sh down`. 실서버 인수는 같은 스크립트가 사람 한 명을 등록하고 끝에 중지한다([계정 발급](ACCOUNTS.md#검사)) |
| core migration(`migrations/core`) | `check-static.sh rust`, `run.py` | 이미 적용한 파일은 고치지 않고 새 번호로 추가(sqlx가 checksum 불일치를 거부). 운영 반영 전 `python3 scripts/core-backup.py create`로 대상 DB를 백업하고 `verify --migrate-with <서버 바이너리>`로 일회용 PostgreSQL에서 복원·migration 리허설 |
| 공식 앱 카탈로그·설치 정책 | `python3 scripts/test-official-apps.py`, `python3 scripts/check-server-builds.py`, `python3 scripts/check-development-database.py` | `bash scripts/runtime-regression/stack-ui.sh up`으로 만든 스택에서 `node scripts/runtime-regression/check-official-apps.mjs` 실행 후 `bash scripts/runtime-regression/stack-ui.sh down`. 시험용 플래너 두 버전의 서버 빌드·승격, 후임 개발자와 dev 배포를 위임받은 에이전트의 같은 릴리스 재배포, DB 보존, 사용 권한 부여·회수, 설치 도구를 다시 실행한 뒤에도 중지 상태가 유지되는지 확인한다. 빌드 큐 거절과 소스 tar 수명은 `python3 scripts/check-cli-build.py --cli target/debug/miy`로 확인한다. |
| CLI 클라이언트(`--no-default-features` 경로) | `check-static.sh rust` | `cargo build --locked --no-default-features` 후 `python3 scripts/check-cli-output.py --cli target/debug/miy`, `check-cli-install.py`, `check-cli-build.py`, `check-cli-recovery.py`. macOS·Windows는 CI 매트릭스 |
| 개발 번들·템플릿·CLI 지침(`templates/`, `crates/miy/guidance/`, `registry/`) | `check-static.sh rust`(번들 바이트 검사 포함) | 지침·업그레이드: 이전 기준 CLI를 `MIY_PREVIOUS_BUNDLE_CLI`로 지정하고 `node scripts/check-cli-guidance.mjs`. 번들 버전 변경은 [CLI 릴리스](CLI-RELEASE.md) 절차 |
| 플랫폼 웹(`web/platform`) | `check-static.sh web` | 업무 앱 프레임: `stack-ui.sh up` 후 `pnpm exec playwright test -c scripts/runtime-regression/app-frames.config.ts`(브라우저는 `MIY_FRAME_BROWSERS`). 실제 core 연동: `cargo build` 후 `pnpm exec playwright test --config scripts/runtime-regression/server-ui.config.ts`. 인증·앱 배포 전체: `stack-ui.sh run` |
| SDK·공용 UI(`packages/sdk`, `packages/ui`, `registry/components`, template SDK) | `check-static.sh web`, `node scripts/build-ui-component.mjs --check`, `node scripts/test-sdk-transport.mjs` | ACL SQL: `python3 scripts/test-sdk-acl.py`. 대표 업무 handler: `python3 scripts/check-common-work.py`(둘 다 일회용 PostgreSQL). 새 UI는 유지관리 소스에서 새 불변 버전으로 생성. 기존 바이트를 덮어쓰지 않음 |
| 검증·정리 스크립트(`scripts/`) | `check-static.sh scripts` | 자원을 지우는 스크립트는 소유 표식·기존 자원 보존을 stub 테스트로 확인 |
| 릴리스·설치 파일(`scripts/package-*`, `stage-*`, 릴리스 workflow) | `check-static.sh scripts` | GitHub Actions 후보 실행(`publish=false`) 후 `package-cli-release.py verify`, `package-windows-installer.py verify`. Windows 11 실제 설치는 [Windows 인수](WINDOWS-ACCEPTANCE.md)로 사용자가 수행 |
| 서버 구성 생성(`bootstrap.py`, `render-server-config.py`, `configure-identity.py`) | `check-static.sh scripts`(`check-server-config.py` 포함) | 운영 `.local`에 bootstrap·seed·Keycloak 재설정을 실행하지 않는다 |
| 운영 반영(dwdcc.kr 호스트) | 위 해당 검사 전부, [배포 도구](SERVER-OPERATIONS.md#7-배포-도구)의 `inspect`(백업·별도 복원 확인과 연결 예산은 `core-rollout.py apply`가 수행) | health·설치 안내 버전, 게시 CLI로 `node scripts/check-platform-connection.mjs`, 읽기 위주 브라우저 확인, 운영 기록 갱신. 새 앱 생성·빌드·dev 배포·프레임까지의 실서버 인수가 필요하면 `node scripts/check-pilot-flow.mjs`(서버에 `pilot-*` 앱 하나를 만든다) |

최소 보안 구현은 [경계 인수](APP-BOUNDARIES.md#경계-인수)의 필수 행을 검사한다. 업무 포털(prod)·dev 포털(dev)·콘솔+Keycloak 세 출처, portal/dev_portal/console 용도·환경별 Use 판정·기존 SDK/CLI 호환이 기준이다. 후속 BV-13·외부 앱 격리·별도 IdP·MFA 의무화·자동 CA는 현재 기능 개발의 선행 검사가 아니다. 같은 BV ID의 이전 iframe 결과는 이월하지 않으며 문서 PASS로 제품 NOT_RUN을 바꾸지 않는다.

실제 Core·브라우저로 포털 세션의 관리 변경·device 승인 거부와 콘솔의 정상 승인, device 시작·poll/refresh·상대 verificationPath redirect, legacy start/approve/exchange의 410 종료를 확인한다. 같은 호스트 다른 출처의 쿠키 인증 요청(GET·탐색 포함)이 거부되는지도 확인한다. realm/sub·callback·MCP 호환, device와 legacy 회수, 사용자/그룹 권한 회수, API sandbox/nosniff·정상 JSON/MF/preview·Nginx anti-frame을 검사한다. 관리 계정의 포털 사용·dev 개발 편의가 유지되는지도 확인한다. 전면 CLI 중지나 개발자 앱 사용 금지는 기본 인수가 아니며 구 위험 경로를 임시 중지할 때만 영향·복구를 별도 시험한다.

단일 IP HTTPS는 인증서 반입·CA 신뢰/IP SAN·만료 거부·수동 교체, 실제 OIDC/CLI, 포트 공유 쿠키·잘못된 목적/중복 인증 쿠키 거부·기본 복구를 확인한다. 쿠키 가용성 잔여를 기록하고 자동 CA·전 기기 스트레스는 후속으로 둔다. [공통 ACL·PLM 연계](CORE-PERMISSIONS.md#대표-인수와-검토할-결정)는 G4·G5·G6·Invoke·서비스·이벤트·파일의 해당 기능을 구현할 때 검사하며 모두를 최소 경계의 선행 조건으로 묶지 않는다. 기존 HTTP IP 결과로 새 HTTPS 인수를 대신하지 않는다.

## CLI·앱 개발 인수의 구분

고장 신고·인증 저장소 변경은 `python3 scripts/check-cli-reports.py --cli target/debug/miy`(비로그인 전송·저장소 장애·응답 유실·재전송·크기 제한)와 `check-cli-output.py`의 교환 후 저장소 읽기 실패/새 세션 회수 경로를 추가한다. 실제 DB·관리자 조회·CSRF·중복·접수량·화면 처리는 `server-ui.config.ts`의 신고 인수에서 검사한다. 이 검사들은 Mac의 Keychain/Background 복구를 대신하지 않는다.

CLI 0.5 계획·과거 검증표의 실행 이력은 [보관 결과](archive/results/CLI-VALIDATION-MATRIX.md)에 둔다. 새 변경은 위 최소 검사를 선택하고 아래 실제 사용 경계를 해당 후보의 소스·바이너리·환경별로 확인한다.

| 범위 | 별도로 확인할 결과 |
| --- | --- |
| CLI 자동화 계약 | help·describe·입력 타입과 실제 명령 일치, UTF-8 JSON stdout/stderr 분리·종료 코드, pagination·오류 복구 인수. 빌드·작업 응답 유실·resume·retry에서 중복 변경 없음 |
| 인증·권한·운영 | 실제 OIDC/device 승인·갱신·철회, context 혼동과 비밀 출력 거부, 최소 권한 개발자와 관리자 결과 구분. dry-run·권한 거부·확인 누락의 변경 없음과 접수/worker 재검사. Core 사용자 비활성화와 상위 IdP의 실제 전파 증거는 구분 |
| 네이티브 개발과 원격 배포 | 로컬 `app dev`의 실제 CRUD·인증·종료, 원격 build·dev 재배포 뒤 같은 DB UUID와 레코드 보존, 새 migration. 로컬 preview와 원격 설치별 프레임 브라우저 인수는 각각 실행. 명시적 Docker runtime은 별도 경로 |
| 설치·OS | 같은 후보의 네이티브 바이너리·번들 해시, 경로/인코딩·프로세스 종료·자격증명 보호·파일 잠금/복구·설정 보존. Windows Server CI는 실제 Windows 11 PowerShell 5.1/7 인수를 대신하지 않음 |
| 독립 AI 앱 개발 | 도구별 새 폴더·앱 ID·대화에서 v1 개발, 자신의 대화로 v2 변경, 실제 호출·개입·소스/build/job/release/image와 데이터 보존을 별도 평가. 기존 대화를 다른 도구/반복의 새 세션으로 재집계하지 않음 |

사용자 Windows 실행은 [WINDOWS-ACCEPTANCE](WINDOWS-ACCEPTANCE.md)로 인계하며, 새 후보를 만들었다는 이유로 남은 `NOT_RUN`을 지우지 않는다. OS·독립 AI 반복·별도 서버 등 정식 공개에 필요한 전체 조건은 [CLI-RELEASE의 공개 게이트](CLI-RELEASE.md#공개-릴리스-게이트)를 따른다. 독립 AI의 앱 개발 평가는 플랫폼 Hermes 실행 기능이나 전체 V7 인수의 완료 증거가 아니다. 새 서버는 프레임 런타임 설치를 포함한 전체 경로로 인수해야 하며 `up.sh`·플랫폼 빌드 성공만으로 완료 처리하지 않는다([실행 안내](RUNNING.md#현행-서버-구성-https-도메인과-사내-http-ip)).

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
