# 현재 플랫폼 서버 운영·연결 안내

- 정리일: 2026-11-15 · 실제 운영 확인의 최신 근거: [0.10.0 반영 기록](releases/pilot-0.10.0-2026-11-15-deployment.json)
- 역할: 현재 서버·복구·반영 절차. 전체 완료/진행/잔여와 착수 순서는 [PLAN](PLAN.md), CLI 후보·게시는 [CLI-RELEASE](CLI-RELEASE.md)가 관리한다.

이 프로젝트의 “운영” 주소는 아직 개발 서버다. 운영 반영은 이 서버의 변경분 배포를 뜻하며 제품 개발 완료나 정식 오픈 선언이 아니다. 이 checkout의 `.local`과 `web/platform/dist`는 실제 제공 상태이므로 검증 도구·빌드는 [AGENTS](../AGENTS.md)의 격리 경계를 지킨다.

현재 prod 포털·dev 포털·관리/로그인 콘솔은 같은 IP의 HTTPS 443을 공유하는 별도 호스트명이다. 업무 앱은 설치마다 `https://v2-app-<키>.dwdcc.kr` 프레임에서 실행한다. 포털은 자기 환경 앱만 실행하고 콘솔은 관리·device CLI 승인·Keycloak을 제공한다. 세션 용도·Origin·CSRF를 서버가 확인하며 같은 사이트의 쿠키 간섭·프로세스 분리는 보장하지 않는다. 공개 서버를 사내 신뢰 범위로 자동 인정하지 않는다([경계 설계](APP-BOUNDARIES.md), [앱 프레임](APP-FRAMES.md)).

포털 내부 앱 이동은 같은 탭, 포털↔콘솔·prod↔dev 포털 이동은 새 탭이다. 구 `https://v2.dwdcc.kr:9443`은 현행 콘솔의 같은 경로로 301 redirect한다. 도메인 없는 배치는 [단일 IP·포트 프로필](APP-BOUNDARIES.md#배치-프로필-도메인권장과-포트-분리지원)을 따른다.

이전 날짜별 배포 수치·실패·복구 명령은 [정리 전 전체 운영 안내](archive/results/SERVER-OPERATIONS-2026-11-14.md)와 연결된 실행별 JSON에 보존했다. 아래 현재 상태와 복구 경로를 우선 사용하며 과거 PASS/NOT_RUN을 새 소스로 이월하지 않는다.

## 1. 서버 주소와 실행 구성

| 항목 | 현재 구성 |
| --- | --- |
| 대표 주소 | `https://v2.dwdcc.kr` |
| 업무 웹·API·CLI 설치/다운로드 | prod 포털인 대표 주소, 호스트 Nginx에서 전달 |
| dev 업무 웹·API | `https://v2-dev.dwdcc.kr`, dev 앱 전용 포털 |
| 관리·CLI 승인·로그인 | `https://v2-console.dwdcc.kr`, 같은 IP·443의 별도 호스트명(옛 `:9443`은 redirect 전용 사이트) |
| 플랫폼 API/worker | systemd `miy-v2-serve`, `miy-v2-worker` |
| 서버 실행 파일 | `/projects/miy-v2/.local/bin/miy-server` |
| 포털 Core 내부 주소 | `127.0.0.1:18180` |
| 콘솔 Core 내부 주소 | `127.0.0.1:18182` |
| 프레임 Core·런타임 | `127.0.0.1:18184`, `.local/frame-runtime` |
| IdP | `https://v2-console.dwdcc.kr/identity/realms/miy`(realm `frontendUrl`), 내부 Keycloak `127.0.0.1:18181` |
| Keycloak 자원 | 컨테이너 RAM 2GiB, 추가 swap 0. JVM 초기 힙 1GiB·최대 약 1.4GiB([용량 기준](CAPACITY.md#인증-서버와-회귀-테스트의-현재-메모리-한도)) |
| PostgreSQL | 호스트 loopback `127.0.0.1:15432` |
| 상태·비밀 설정 | `/projects/miy-v2/.local/`, `server.json`, `runtime.env`, `credentials.json` |
| 직접 HTTP MCP | `https://v2.dwdcc.kr/mcp`, IdP OAuth 인증 |

대표 주소는 `MIY_ORIGIN`, dev 포털은 `MIY_DEV_PORTAL_ORIGIN`, 콘솔은 `MIY_CONSOLE_ORIGIN`, 로그인 issuer는 `MIY_ISSUER`로 관리한다. Core의 고정 목적 listener는 `MIY_BIND_ADDR`·`MIY_DEV_PORTAL_BIND_ADDR`·`MIY_CONSOLE_BIND_ADDR`이며 `server.json`의 `origin`·`devPortalOrigin`·`consoleOrigin`과 `bind`·`devPortalBind`·`consoleBind`에 맞춘다. Keycloak은 세 출처의 정확한 callback/logout URI만 허용하고 issuer는 콘솔에 둔다. MCP resource/audience와 CLI context·설치 안내의 서버 주소는 prod 포털을 유지한다. API와 DB의 내부 포트는 사용자 PC의 CLI 설정에 입력하지 않는다. 비밀 설정 파일이나 그 전체 내용을 문서·채팅·Git에 복사하지 않는다.

Keycloak 한도는 현재 컨테이너의 Docker 설정과 저장소 `compose.yaml`에 반영했다([적용·검사 기록](releases/resource-usage-2026-11-12-validation.json)). 그 적용에서 코어·UI의 배포 소스와 schema는 바뀌지 않았다. 한도 변경은 사용자가 요청했을 때 해당 인증 컨테이너만 정지 → `docker update --memory 2g --memory-swap 2g` → 시작하며, 전체 Compose 재생성이나 realm 초기화를 하지 않는다. 실제 JVM PID는 cgroup의 Java 프로세스로 찾고 PID 1로 고정하지 않는다. 부족할 경우 정지 후 두 상한을 함께 4GiB로 임시 확대하고 인증·OOM 상태를 확인한다. 이 Docker에서 `docker update --memory 0 --memory-swap 0`은 기존 값을 유지하므로 상한 해제 명령으로 사용하지 않는다.

서버 상태는 다음 읽기 명령으로 확인한다.

```sh
systemctl status miy-v2-serve miy-v2-worker --no-pager
curl --fail --silent --show-error https://v2.dwdcc.kr/api/health
curl --fail --silent --show-error https://v2-console.dwdcc.kr/api/health
curl --fail --silent --show-error https://v2.dwdcc.kr/api/v1/capabilities
sudo nginx -t
```

HTTP 주소 `http://v2.dwdcc.kr`는 현재 410을 반환한다. 사용자 접속·설치 안내에는 위 HTTPS 주소를 사용한다. 두 MIY 서비스는 부팅 시 시작하도록 enabled 상태다. 최근 실제 확인의 범위는 [작업본·운영 확인 기록](releases/worktree-sync-2026-11-14-validation.json)에 있으며, 문서 정리를 새 운영 검사로 간주하지 않는다.

### 앱 컨테이너의 네트워크

앱 컨테이너에서 밖으로 나가는 통신은 플랫폼이 제한하지 않는다([결정 8](PLAN.md#확정된-결정)). 차단이 필요하면 네트워크·호스트 방화벽에서 한다. 플랫폼 자체의 경계는 이 제한에 의존하지 않는다. Core listener(위 표의 `127.0.0.1` 주소), PostgreSQL 게시 포트와 앱 컨테이너 게시 포트가 모두 loopback에만 열려 있어 앱 컨테이너가 프록시를 거치지 않고 Core에 닿을 수 없다. PostgreSQL은 설치별 Docker 네트워크로 앱에 연결되며 앱은 자기 DB 역할만 받는다.

외곽 방화벽은 호스트 밖으로 나가는 통신만 본다. 앱 컨테이너에서 **같은 호스트의 다른 서비스**(모든 주소로 열린 포트)로 가는 통신은 호스트에서 막아야 한다. 실제 운영 서버는 다음 중 하나를 설치 조건으로 둔다.

- miy 전용 호스트로 두고 다른 서비스를 모든 주소로 열지 않는다.
- 호스트 방화벽에서 Docker bridge 대역이 호스트의 다른 포트로 들어오는 것을 막는다(프록시 443과 설치별 네트워크의 PostgreSQL은 허용).

현재 개발 호스트는 miy와 무관한 서비스 여러 개가 모든 주소로 열려 있고 위 규칙이 없다(2026-11-16 `ss -ltn` 확인). 개발 서버 범위에서 수용한 상태이며 운영 전환 때 위 조건을 적용한다.

## 2. 사용자 PC의 CLI와 MCP

Windows에서는 [설치 화면](https://v2.dwdcc.kr/cli/install)의 설치 파일을 사용하고 서버 주소에 `https://v2.dwdcc.kr`를 입력한다. 다른 OS도 같은 화면이 제공하는 해당 OS/CPU 배포물과 체크섬을 사용한다. 설치한 뒤 새 터미널에서 다음을 실행한다.

```sh
miy --version
miy context show
miy auth login
miy auth status
miy doctor --for operate
```

서버 주소를 등록하지 않았다면 `miy context init`으로 입력한다. 여러 서버를 사용한다면 명시적으로 추가·선택한다.

```sh
miy context add platform --url https://v2.dwdcc.kr
miy context use platform
miy --context platform auth login
```

서버 선택 순서는 `--context` → `MIY_CONTEXT` → 프로젝트 `.miy/context.json` → 전역 활성 context다. 프로젝트에 연결 이름이 있으면 `context use`로 바꾼 전역 기본값보다 우선한다. 프로젝트 설정에 적힌 연결 이름이 해당 PC에도 등록되어 있어야 한다.

Windows 설정 위치는 `%APPDATA%\miy\config.json`, Unix 기본 위치는 `~/.config/miy/config.json`이며 `XDG_CONFIG_HOME`과 `MIY_CONFIG_DIR`로 바꿀 수 있다. 서버 주소·CA 설정과 사용자 로그인 자격증명은 별도로 관리한다. 자격증명을 프로젝트나 AI 설정에 복사하지 않는다.

Codex·Claude의 생성된 프로젝트 MCP는 프로젝트의 `.miy-tools/miy[.exe] mcp`를 로컬 stdio로 실행한다. 이 프로세스가 CLI와 같은 context·사용자 세션으로 플랫폼의 계약·API·컴포넌트를 읽는다. 프로젝트 MCP 설정에 `/mcp` URL을 넣거나 별도의 토큰을 복사할 필요가 없다. 직접 HTTP MCP 연결을 사용하는 클라이언트는 `/mcp`의 별도 OAuth 흐름을 사용한다. MCP resource URL은 그대로지만 issuer가 콘솔로 바뀌었으므로 기존 외부 클라이언트의 새 issuer 발견·재로그인을 확인해야 한다. 해당 외부 클라이언트의 실제 전환 검증은 NOT_RUN이다.

## 3. 현재 배포 소스와 이 작업본의 차이

| 대상 | 현재 배포 소스·상태 | 근거 |
| --- | --- | --- |
| 서버·worker, 게시 CLI | `9133d700369c43b11abeee1738541e6abda5b4d6`, 서버·CLI 0.10.0 / SDK 번들 0.10.0 / Rust SDK 0.4.0 | [0.10.0 반영](releases/pilot-0.10.0-2026-11-15-deployment.json). 다운로드 서버 게시이며 정식 GitHub Release는 아님 |
| 프레임 런타임 | `9133d700369c43b11abeee1738541e6abda5b4d6` | 같은 기록 |
| 포털 UI | `1976005eebe1c765aae322e191d99f4410b4835c` | 같은 기록(사용자 등록 폼 보완을 뒤이어 반영) |
| 프록시 | 포털·dev 포털의 파일 전송 경로, 프레임의 `/_miy/files/`, 콘솔의 파일 경로 차단 | 같은 기록. `.local/nginx-miy.conf`가 설치된 사이트와 같다 |
| 공용 UI 컴포넌트 | `ui` 1.0.1, 1.1.0(새 프로젝트 기본) | 같은 기록. 1.1.0은 관리자 계정이 게시 |
| core DB | schema 23 | 같은 기록. 0023은 `app_files` 표와 색인 두 개 추가 |

2026-11-15에 공통 업무 기능과 파일럿 준비 보완을 반영했고, 게시 CLI로 새 앱 하나의 서버 빌드·dev 배포·프레임 표시를 인수했다(`pilot-8e4b32b9`, 중지 상태로 보존). `1976005`는 `9133d70`과 Rust·SDK·프레임 런타임·migration 입력이 같아 포털 UI만 그 커밋에서 반영했다. CLI 후보 고정·검증·게시 잠금을 포함한 `core-rollout.py apply`는 이 배포에서 실행했다. 실제 `rollback`과 후속 복구 도구의 운영 실행은 `NOT_RUN`이다.

**이 작업본의 다음 후보(미게시·미반영):** CLI/번들 0.11.0은 서버 배포를 서버 빌드 산출물로 한정한다([검증 기록](releases/server-build-only-2026-11-16-validation.json)). schema는 23 그대로다. 반영하면 서버·worker와 게시 CLI를 함께 갱신해야 하고, 사용자는 CLI를 갱신한 뒤 각 프로젝트에서 `miy app upgrade`를 실행한다(안내 문서만 바뀐다). 이전에 올린 이미지로 실행 중인 설치의 시작·중지·재시작·제거는 그대로 동작하지만, 그 릴리스를 prod로 승격하거나 같은 이미지를 다시 배포할 수는 없고 `miy app build`로 서버에서 다시 빌드해 배포해야 한다.

`main` 변경은 자동 배포되지 않는다. 서버를 갱신할 때에는 같은 버전 문자열이라도 대상 source commit·빌드 입력·설치된 SQL·게시물의 출처를 함께 확인한다. 검증한 소스의 별도 checkout에서 빌드하고 §7의 도구로 서버/worker·UI·게시 index를 같은 기록에 묶어 반영한다.

**현재 서버·CLI 복구:** 서버 바이너리 SHA-256은 `fc98d5abaf30e8ca8ed0cfcade52af0d5e5d1b5339c71690090b2f59fbbcae61`다. [0.10.0 반영 기록](releases/pilot-0.10.0-2026-11-15-deployment.json)의 순서(포털 UI → 프레임 런타임 → 프록시 → core)로 되돌린다. `.local/release-9133d70-source/scripts/core-rollout.py rollback --release-dir .local/release-9133d70-core`는 직전 서버 `467bb0d2…`(0.9.0)와 CLI·설치기 0.9.0 index를 함께 복원한다. schema 23·앱 데이터·게시한 `ui` 1.1.0·추가한 0.10.0 파일은 보존한다. 0.9.0 서버를 schema 23에서 실행하는 복구는 수행한 적이 없다(0023은 0.9.0이 읽지 않는 표만 추가). 이전 배포의 복구 명령은 현재 서버·CLI 동시 복구를 대신하지 않는다. 실행 당시 복구 도구 사본은 최신 도구로 덮어쓰지 않는다. Nginx 단계에는 아래의 후속 유지관리 도구를 사용하며 당시 백업·후보 파일을 그대로 읽는다.

현재 프레임 런타임·포털 UI·프록시의 복구 순서도 같은 기록에 있다. Nginx는 `python3 scripts/file-routes-rollout.py restore /projects/miy-v2/.local/release-9133d70-nginx`로 복구한다. 이 도구는 후속 작업본이며 실제 복구는 아직 실행하지 않았다([후속 검증](releases/pilot-review-fixes-2026-11-15-validation.json)). 원래 `.local` 도구는 재시도 결함이 있어 실행 이력으로만 보존한다. 게시 CLI source commit과 target별 hash는 `GET /api/v1/cli/install`로 확인한다. 서버는 default features 빌드이므로 클라이언트 전용 바이너리와 hash가 다르다.

## 4. 공유 개발 로그인과 운영 전환

현재 공개 서버의 **공유 개발 로그인은 꺼져 있다**. `MIY_DEV_LOGIN=0`·`sharedDevelopmentLogin=false`·`developmentLogin=false`이며 일반 OIDC로 로그인한다. 개발 계정을 선택해 관리 권한을 얻는 우회를 콘솔에 제공하지 않는다.

포털·dev 포털·콘솔은 목적별 세션을 사용한다. 브라우저 logout과 독립 CLI credential 회수는 별개이며 의심 device/legacy credential은 콘솔의 개인 CLI 목록에서 각각 회수한다. 기존 realm·사용자 sub·정상 CLI credential을 보존하는 현재 출처 변경 절차는 §7을 따른다. 운영 `.local`에 bootstrap·configure-identity를 다시 실행하지 않는다.

공개 접근 범위와 남은 credential/기기 점검은 [경계 설계](APP-BOUNDARIES.md#전환-전-임시-운영)와 [PLAN의 검증·결정 대기](PLAN.md#검증과-반영-대기)에서 추적한다. 일반 OIDC 성공이나 공유 개발 로그인 비활성화만으로 이 위험 검토를 완료 처리하지 않는다.

### 사용자 추가

실제 사용자는 두 단계로 추가한다. Core는 로그인 서버의 계정을 만들거나 읽지 않는다.

1. 로그인 서버: 서버 내부 Keycloak Admin API로 realm `miy`에 계정을 만들고 첫 로그인 때 바꿀 임시 암호를 정한다. [SETUP §8](SETUP.md#8-기동-확인과-첫-로그인)은 UUID 조회·암호 재설정·로그인 계정 중지도 다룬다. 생성한 UUID가 Core가 받는 subject이며 토큰·암호는 출력하지 않는다. 공개 프록시는 Keycloak master 관리 화면/API를 차단하므로 콘솔 `/identity`의 관리 화면을 안내하지 않는다.
2. Core: 관리자가 콘솔 **플랫폼 관리 → 사용자 계정**에서 그 ID와 표시 이름을 등록한다(`POST /api/v1/users`). 필요하면 `앱 개발 허용`·`관리자 지정`을 따로 바꾼다. 앱 사용·개발·배포 권한은 기존 권한 화면이나 `miy grant set`으로 부여한다.

등록하지 않은 계정의 로그인은 403이다. 사용을 막을 때는 로그인 서버가 아니라 Core의 `사용 중지`가 기준이다(세션·CLI·대기 작업 차단). 감사에는 `user.registered`·`user.admin.*`·`user.developer.*`·`user.renamed`가 남는다.

## 5. 실제 연결 확인

최근 실제 확인(2026-11-15, [0.10.0 반영 기록](releases/pilot-0.10.0-2026-11-15-deployment.json))에서는 세 출처 health, 서버/worker 실행 파일·schema, 게시 CLI 5종·helper 2종의 해시, 일반 OIDC/SSO·CLI·생성 stdio MCP 연결 22항목, 기존 앱 4개의 프레임 실행, 게시 CLI로 새 앱 하나의 생성·서버 빌드·dev 배포·프레임 표시 8항목을 검사했다. 같은 Linux 호스트에서 공개 HTTPS 주소로 실행한 결과이며 다른 PC·Windows·AI 에이전트 세션·직접 외부 OAuth MCP 인수와 구분한다.

검증할 CLI의 공개 checksum/provenance와 [CLI 릴리스](CLI-RELEASE.md)의 게시 출처를 먼저 대조하고 절대 경로를 전달한다. `pnpm` 의존성과 Chrome이 필요하며 출력 디렉터리는 새 경로여야 한다.

```sh
node scripts/check-platform-connection.mjs \
  --origin https://v2.dwdcc.kr \
  --cli /absolute/path/to/verified/miy \
  --credentials-file /projects/miy-v2/.local/credentials.json \
  --output /projects/miy-v2/.local/evidence/new-platform-connection-run
```

이 검사기는 비밀번호·쿠키·device code·토큰·원본 응답을 출력하지 않는다. 별도의 private CLI 설정과 브라우저 프로필로 실제 일반 OIDC 로그인을 한 번 시도하며, 프로젝트 파일은 출력 디렉터리에만 생성한다. 앱 등록·권한 변경·배포는 요청하지 않는다. 종료 시 자신이 만든 CLI·각 목적의 웹 세션만 로그아웃하고 MCP·브라우저 프로세스를 종료한다. 계정 전체 IdP 세션은 철회하지 않는다. 결과의 확인 항목·정리 실패와 소스·실행 환경을 기록하고 이전 실행의 개수를 새 검사 결과로 사용하지 않는다.

전체 다운로드만 확인하려면 아래 `CLI_CANDIDATE_40_HEX_COMMIT`을 릴리스 기록에서 확인한 현행 게시 소스로 바꾼다. 서버 자신의 안내만 믿지 않고 검증된 CI 후보의 출처와 대조한다.

```sh
python3 scripts/check-cli-downloads.py --origin https://v2.dwdcc.kr \
  --expected-commit CLI_CANDIDATE_40_HEX_COMMIT
```

`check-mcp.mjs`는 로컬 검증 계정의 전체 세션을 철회하므로 일반 플랫폼 연결 확인에 사용하지 않는다.

새 앱을 실제로 만들어 배포까지 확인할 때는 `check-pilot-flow.mjs`를 쓴다. 관리자·개발자가 일반 OIDC로 콘솔에 로그인해 CLI를 device 승인하고, 게시 CLI로 `app init` → `component sync` → 등록 → 관리자 승인 → 서버 빌드 → dev 배포 → dev 포털의 앱 프레임 표시까지 수행한다. **서버를 바꾼다:** 실행마다 `pilot-*` 앱 하나(등록·dev develop 권한·dev DB·릴리스·컨테이너)가 생기고, 정상 종료 때 앱을 중지한다(`--keep-running true`로 유지). 후속 검사기 v2는 배포 응답 유실 시 작업을 재제출하지 않고 appId/dev의 접수된 작업을 조회한다. 작업 종료를 확인한 뒤 중지를 요청하고, 끝내 상태를 확인할 수 없거나 중지하지 못하면 `cleanupFailures`를 기록해 실패로 끝낸다. 앱 행과 DB는 남는다. 검사 결과 v2의 `passes`는 고정 단계 `name`과 별도 `details`를 사용한다. 이 보완 검사기의 실서버 실행은 아직 `NOT_RUN`이며 위 8항목은 기존 v1 실행이다. `--publish-component DIR`을 주면 관리자가 그 공용 컴포넌트 버전을 먼저 게시한다.

```sh
node scripts/check-pilot-flow.mjs --origin https://v2.dwdcc.kr \
  --cli /absolute/path/to/verified/miy \
  --credentials-file /projects/miy-v2/.local/credentials.json \
  --output /projects/miy-v2/.local/evidence/new-pilot-flow-run
```

`check-server-development-flow.mjs --run`은 앱 프레임 도입 전의 계약(포털 쿠키로 앱 업무 API 호출)으로 작성된 이전 전체 인수 도구이며 현행 서버에 맞게 갱신하지 않았다. Windows·macOS 사용자 PC, 직접 HTTP MCP 인증, 업무 앱 작성/승격·DB 보존은 해당 범위의 별도 인수로 기록한다.

## 6. 변경 전 최소 백업과 복원 확인

core DB를 바꾸는 반영(migration, 권한·그룹 데이터 변경) 전에는 그 범위를 백업하고, 별도 영역에 복원해 확인한다. 이는 [완료한 최소 데이터 보호](PLAN.md#완료한-작업) 범위이며, 역할·앱 DB·파일·키까지 포함하는 전체 백업 제품(코어 4차)과 구분한다.

```sh
python3 scripts/core-backup.py create --out .local/backups/core-$(date -u +%Y%m%dT%H%M%SZ)
python3 scripts/core-backup.py verify .local/backups/core-<시각> --migrate-with <반영할 서버 바이너리>
python3 scripts/check-db-connections.py
```

- `create`: 운영 `miy_core`를 서비스 중지 없이 읽기만 한다. 덤프와 테이블별 행 수를 같은 REPEATABLE READ 스냅샷에서 얻어 `manifest.json`에 기록한다. 덤프와 manifest는 0600, 폴더는 0700이다.
- `verify`: 라벨을 붙인 일회용 PostgreSQL(tmpfs)에 복원해 migration 버전과 모든 테이블 행 수를 대조한다. `--migrate-with`를 주면 반영할 바이너리의 core migration을 복원본에 먼저 적용해 리허설한다. 소유 라벨과 전체 컨테이너 ID가 일치할 때만 지우고, 성공한 목록 조회로 부재를 확인해야 정리 완료로 보고한다. Docker 연결·조회 오류나 삭제 실패는 `FAIL`이며 `cleanupError`를 남긴다.
- 서비스를 멈춘 상태의 일관된 덤프가 필요한 실제 반영은 §7의 `core-rollout.py apply`가 코어를 멈춘 뒤 다시 백업하고, 별도 복원·migration 확인을 통과해야 설치한다.

`check-db-connections.py`는 공유 PostgreSQL의 최악의 연결 수(코어 풀·병렬 관리 연결·Keycloak 풀·설치 역할 한도·활성 개발 세션)를 사용 가능 연결과 비교한다. 현재 계산은 관리 연결 4개를 포함하며 [연결 예산](CAPACITY.md#연결-예산-현재-구현-2026-11-04)을 따른다. 이전 연결 예산 FAIL과 이후 반영·재검증은 [보관 이력](archive/results/SERVER-OPERATIONS-2026-11-14.md)에 남긴다.

새 코어는 앱 배포뿐 아니라 worker 시작 때도 기존 설치 역할 한도(runtime 6, migration 2)를 맞춘다. 코어가 생성하는 `miy_r_`·`miy_m_`와 32자리 소문자 16진수 ID 형식만 허용한다. 시작 시 적용이 실패하면 경고를 기록하고 다음 worker 시작 또는 해당 앱 배포 때 다시 적용한다. compose는 PostgreSQL `max_connections=200`, Keycloak `KC_DB_POOL_MAX_SIZE=10`을 설정한다. 소스 변경만으로 기존 운영 설정이나 역할 한도가 바뀌지 않으므로 운영 반영 후 예산을 다시 측정한다.

## 7. 배포 도구

같은 schema의 일반 서버·UI 반영은 저장소에서 계속 관리하는 두 도구로 한다. 2026-11-07의 schema19→20·설정·issuer 전환은 검증한 전용 조정 도구로 수행했으며 [배포 기록](releases/internal-minimum-security-2026-11-07-deployment.json)을 따른다. 현재 `core-rollout.py`에는 정확한 SHA CI gate와 고정된 migration 21·22·23의 한 단계 확장 예외가 구현되어 있다. 임의 migration·설정·issuer를 한꺼번에 전환하는 일반 도구는 미구현이다. 반영할 commit의 별도 clean checkout에 있는 사본을 실행하며, `scripts/test-core-ops.py`가 같은 파일의 실패 경계를 모의 시험한다. 배포별 증거 디렉터리의 이전 도구(`core-0.7.0-linux`, `core-0.7.0-7d3c66a-linux`, `core-review-20261104-linux`)는 실행 당시 기록이므로 고치거나 다시 쓰지 않는다.

**서버 `scripts/core-rollout.py`**

- **범위:** 기본은 core migration 집합이 운영 DB와 같은 소스의 서버 바이너리 교체다. 다른 schema는 `migration_rollout_not_supported`·`source_schema_older_than_live`로 거부한다. 예외인 `--widen-migration 21|22|23`(23은 `app_files` 표와 색인 두 개만 추가)은 소스에 고정한 SQL 해시가 일치하고 현재 schema보다 정확히 한 단계 높을 때만 허용한다. 적용 전 실제 덤프의 별도 복원·migration 검사를 거치며, 그 배포의 rollback은 확장한 schema를 유지하고 이전 바이너리만 복구한다. 임의 migration이나 DB 하향 복구를 지원하는 기능은 아니다. PostgreSQL 연결 한도와 Keycloak 풀은 기대값(기본 200·10)과 같은지 확인만 하고 바꾸지 않는다. 일반 migration·설정 전환은 별도 구현·검증 과제다.
- **CI gate(2026-11-09):** 사전 확인에서 `gh`로 대상 commit의 main push workflow(`web.yml`, `cli-foundation.yml`, `runtime-regression.yml`)의 최신 실행이 모두 completed/success이고 모든 job이 성공했는지 확인한다. 실패·누락·다른 branch면 `ci_not_successful`·`ci_jobs_not_successful`·`ci_source_mismatch`·`required_ci_missing`로 아무것도 바꾸지 않고 거부한다. 확인한 run ID·URL은 결과와 기록에 남긴다.
- **`inspect`(기본):** 바꾸지 않고 `apply`의 사전 확인만 한다. 소스 commit·clean 상태, 실행 중인 도구와 소스 사본의 일치, 후보 해시·버전, 설치 파일과 serve·worker 실제 실행 파일의 해시, schema, 연결 설정·예산, health·게시 CLI 안내, 대기 작업 0, 앱 컨테이너를 확인한다. 서비스의 `Id`·`Slice`·cgroup 경로와 cgroup v2의 읽기 가능 여부도 중지 전에 점검한다.
- **`apply`:** 같은 확인 후 코어를 멈추고 프로세스가 없음을 확인한 뒤 대기 작업을 다시 확인한다. 새 덤프를 일회용 PostgreSQL에 복원해 행 수와 후보의 migration(기본은 변경 없음, 위 확장 예외는 고정된 한 단계)을 확인해야 설치한다. 시작 후 health·게시 CLI 안내·실행 파일 해시·schema·주요 레코드 해시·앱 컨테이너·연결 예산을 확인한다.
- **실패 처리:** 후보 시작 전 실패도 실제 중지 검사를 통과해야 해시를 확인한 이전 바이너리를 다시 설치하고 코어를 시작한다(`ROLLED_BACK`). 중지 확인에 실패하면 `ROLLBACK_FAILED`로 수동 판단을 남기며, 잔류 프로세스를 강제 종료하거나 서비스를 시작하지 않는다. 시작 후 실패는 새로 받은 쓰기를 덮어쓰지 않도록 코어를 멈춘다(`POST_START_FAILED`). 운영자가 판단한 뒤 `rollback`으로 이전 바이너리를 다시 설치·시작한다. 보관한 바이너리 해시가 다르거나 설치 파일이 이 반영의 것이 아니면 시작하지 않는다. 시작 직후 사용자의 정상 쓰기도 레코드 해시 비교를 실패시킬 수 있으므로 이용이 적은 시간에 반영한다.
- **`rollback` 유휴 검사:** 실행 중일 수 있는 서비스(`active`·`activating` 등)가 하나라도 있으면 중지 전에 대기·실행 작업을 검사한다. serve가 실패하고 worker만 살아 있는 경우도 포함하며, 중지 직후 다시 검사한다. 후자의 검사에서 작업이 발견되면 바이너리를 교체하지 않고 코어를 중지 상태로 남겨 수동 판단한다. 모든 서비스가 `inactive` 또는 `failed`여도 아래 PID·cgroup 검사로 실제 프로세스가 없음을 먼저 확인해야 유휴 검사를 생략할 수 있다. 이 경우에만 운영자가 평가한 복구 경로로 취급해 남은 작업을 이전 바이너리가 이어받는다.
- **중지 확인:** `systemctl stop` 뒤 모든 서비스가 `inactive` 또는 `failed`이고, `MainPID`·`ControlPID`가 모두 0이며 서비스 cgroup에 프로세스가 없음을 확인한다. `cgroup.events`의 `populated=0`은 하위 cgroup까지 포함한다. 확인 후 `failed`로 남은 unit만 `systemctl reset-failed`로 정리하고, 모든 서비스의 `inactive`와 PID·cgroup을 다시 확인한다. 시작 제한 횟수도 초기화되며 실패 내용은 journal에 남는다. 프로세스 잔류, 읽기 실패, 상태 판정 불가 시 배포·복구를 중단하고 바이너리 교체·서비스 시작을 진행하지 않는다.
- **중지 판정의 호스트 조건:** 이 도구는 cgroup v2와 `system.slice`의 고정 서비스 경로를 사용한다. systemd `Id`가 `<서비스명>.service`, `Slice`가 `system.slice`인지 확인하고, `ControlGroup`은 `/system.slice/<Id>` 또는 빈 값만 허용한다. 값이 비어도 `/sys/fs/cgroup/system.slice/<Id>`를 계속 검사한다. cgroup v2 표식과 부모 `system.slice`를 읽을 수 있고 해당 서비스 경로가 실제로 사라진 경우에만 빈 것으로 인정한다. cgroup v1·별도 slice·예상과 다른 경로·존재하는 cgroup의 events 누락 등은 안전하게 판정할 수 없으므로 거부한다.
- **기록:** 시도마다 `.local` 아래 새 `--release-dir`(0700)에 이전·후보 바이너리와 덤프(`rollback/`), 명령·종료 코드만 담은 `deployment.json`(0600)을 남긴다. 출력과 기록에 비밀값·행 내용을 넣지 않는다.
- **CLI 게시 동시 반영(`--cli-downloads DIR --cli-installers DIR`):** 설치 안내는 서버 자신의 버전과 같은 CLI만 제공하므로 버전 변경은 이 옵션 없이 거부한다. 두 입력은 stage 도구로 운영 다운로드 경로와 분리해 준비한다. 코어 중지 전에 별도 후보로 복사해 5개 CLI·2개 helper, 체크섬·provenance·설치기 내장 ZIP과 두 index의 연결을 해당 소스의 패키지 검사기로 확인한다. `inspect`의 임시 복사본은 정리하고, `apply`는 `rollback/cli-candidate/`에 고정한 파일·해시만 게시한다. 원본 staging이 이후 바뀌어도 새 바이트를 채택하지 않으며, 후보가 바뀌면 거부한다. `apply`·`rollback`은 기존 stage 도구와 같은 두 `.publish.lock`을 고정 순서로 잡아 검사·백업·게시·복구 확인을 마칠 때까지 유지한다. 다른 게시 작업이 있으면 서비스 변경 전에 `cli_publication_busy`로 거부한다. 기존 index를 `rollback/`에 백업하고, core 중지 중 새 commit 디렉터리를 추가한 뒤 두 index를 차례로 원자 교체한다. index는 교체 전 임시 파일의 해시까지 검사한다. 시작 후 새 설치 안내를 확인하며, 후보 시작 전 실패와 `rollback`은 이전 index·바이너리·설치 안내를 복구한다. 추가한 commit 디렉터리는 불변 기록으로 남긴다. 이 후보 고정·검증·잠금 보완은 [2026-11-14 검증](releases/cli-rollout-review-fixes-2026-11-14-validation.json) 후 [0.10.0 apply](releases/pilot-0.10.0-2026-11-15-deployment.json)에서 사용했다. 실제 rollback은 NOT_RUN이다.

```sh
python3 <checkout>/scripts/core-rollout.py inspect --source <checkout> --commit <commit> \
  --candidate <checkout>/target/release/miy --candidate-sha256 <후보> --installed-sha256 <현재>
python3 <checkout>/scripts/core-rollout.py apply <같은 인수> --release-dir .local/release-<commit>-<시도>
python3 <checkout>/scripts/core-rollout.py rollback --release-dir <같은 경로>
```

**웹 화면 `scripts/ui-rollout.py`**

- **`build`:** checkout이 `--commit`에 있고 빌드 입력(루트 `package.json`·lock·workspace·`tsconfig.json`, `packages/`, `web/platform/`)이 깨끗할 때만 `<release>/dist`에 빌드한다. `VITE_*` 환경 변수와 루트·`web/platform`의 `.env`, `.env.local`, `.env.production`, `.env.production.local`을 추적 여부와 관계없이 거부한다. 현재 Vite의 기본 `envDir`·`publicDir`을 기준으로 하므로 이 설정을 바꾸면 검사 범위도 함께 바꾼다. public의 무시된 파일·비추적 파일·심볼릭 링크를 거부하고 입력 바이트를 빌드 전후 및 commit과 비교한다. 입력 파일·후보 파일·현재 운영 index의 해시를 `manifest.json`에 묶는다. 운영 dist에는 빌드하지 않는다.
- **`inspect`(기본):** 후보와 운영 dist를 비교하고, manifest의 입력 해시가 commit의 파일과 같은지 확인한다.
- **`apply --expected-commit`:** 운영 dist를 `<release>/production-backup`에 복사·검증한다. 같은 이름의 다른 파일이 있거나 빌드 뒤 운영 index가 바뀌었으면 아무것도 바꾸지 않고 거부한다. 새 파일만 추가한 뒤 `index.html`을 원자적으로 바꾼다.
- **`rollback`:** 백업의 `index.html`로 되돌린다. 두 세대의 자산은 모두 남긴다.
- **공통 잠금:** `apply`·`rollback`은 정규화한 live 경로를 기준으로 `.local/ui-rollout-locks/`의 같은 잠금을 사용한다. release가 달라도 같은 대상을 동시에 바꿀 수 없으며, 백업부터 index 교체·결과 기록까지 잠금을 유지한다. 이미 다른 작업이 잠금을 갖고 있으면 즉시 거부한다.

```sh
python3 scripts/ui-rollout.py build --release-dir .local/release-<commit>-ui --commit <commit>  # --source-root 기본값은 이 checkout(의존성 설치 필요)
python3 scripts/ui-rollout.py inspect --release-dir <같은 경로>
python3 scripts/ui-rollout.py apply --release-dir <같은 경로> --expected-commit <commit>
python3 scripts/ui-rollout.py rollback --release-dir <같은 경로>
```

**dev 포털 추가 전환(`scripts/dev-portal-settings.py`).** 이 호스트의 실제 반영은 [2026-11-09 배포 기록](releases/dev-portal-2026-11-09-deployment.json)에 있고 아래는 도구의 현재 절차다. 개발 단계에서는 짧은 중단·재로그인을 허용하므로 구 바이너리 호환이나 무중단 전환을 새 요구로 만들지 않는다. 운영 반영은 사용자 요청 범위에서 수행한다.

1. `python3 scripts/dev-portal-settings.py inspect --dev-portal-origin <주소>`: 주소·내부 bind·Keycloak `miy-web` 상태를 읽는다. 새 포트는 비어 있어야 하고 호스트명은 다른 활성 Nginx 사이트와 충돌하지 않아야 한다. 현재 소스는 따옴표·여러 줄·대소문자와 주석을 처리해 정확한 `server_name` 충돌을 검사한다. wildcard·정규식 이름 충돌 검사는 이 보완 범위가 아니다. 이 호스트의 dev 주소는 `https://v2-dev.dwdcc.kr`이다.
2. `... apply --dev-portal-origin <주소> --release-dir .local/release-<commit>-dev-portal`: 변경 전에 세 설정 파일의 원본 백업과 복구 manifest를 저장하고, 그 뒤 `server.json`·`runtime.env`·`compose.env`에 dev 키를 추가하며 Keycloak `miy-web`에 dev callback·logout URI를 더한다. 기존 callback/logout URI·web origins·secret·사용자는 보존한다. 현재 소스의 부분 실패 복원은 아래 보완 설명을 따른다.
3. `core-rollout.py apply --widen-migration 21 ...`: 정확한 SHA CI gate, 실제 dump의 복원·migration 리허설 뒤 서버를 바꾸고 고정된 `0021_dev_portal.sql`(제약 확장만)을 적용한다. 이 시점부터 기존 포털은 dev 앱을 거부한다.
4. `dev-portal-settings.py nginx-apply --release-dir ... --certificate ... --key ...`: 다시 생성한 설정이 현재 운영 파일과 dev 포털 server 블록 하나만 다를 때만 설치하고 `nginx -t` 후 reload한다.
5. `ui-rollout.py apply`: 해당 서버의 세 출처 설정에 맞는 UI를 반영하고 환경별 직접 링크·새 탭 이동·로그인을 확인한다. 게시 UI `3760ef0`의 구 서버 fallback은 당시 구현 사실이며 향후 유지 조건이 아니다.

되돌릴 때는 역순으로 `ui-rollout.py rollback`, `nginx-restore`, `core-rollout.py rollback`, `dev-portal-settings.py restore`를 실행한다. schema 21을 읽을 수 있는 이전 서버로 되돌리는 경우 DB는 되돌리지 않는다. 예상하지 못한 제3의 파일·설정 변경은 덮어쓰지 않고 거부한다. dev 설정의 `restore`는 저장한 변경 전·후 상태를 둘 다 허용하므로 적용·복원이 중간에 실패해도 같은 명령으로 다시 실행할 수 있다. 대상 IdP callback/logout URI가 원래부터 있었는지는 복구 manifest와 원본 IdP 백업(`settings-backup/identity.json`)을 함께 사용해 보존·복원한다. **이 부분 실패·재시도 보완은 현재 소스의 [검증 기록](releases/boundary-review-fixes-2026-11-09-validation.json)이며 실제 운영 재실행은 NOT_RUN이다.**

**파일 전송 프록시(`scripts/file-routes-rollout.py`).** 0.10.0의 포털·dev 포털 streaming location, 앱 프레임 `/_miy/files/`, 콘솔 거부 패턴만 다룬다. `prepare SOURCE COMMIT RELEASE_DIR CERTIFICATE KEY`는 기존 설정으로 정확히 되돌아가는 차이만 허용하고, `apply RELEASE_DIR`는 백업·후보 해시를 검사한 뒤 설치·검사·reload한다. `restore RELEASE_DIR`는 당시 배포의 `nginx-deployment.json`과 백업·후보를 읽고 복원한다.

복구 중 검사·reload·렌더 사본 갱신·기록 저장이 실패해도 각 파일이 기록된 이전/후속 바이트라면 같은 restore를 재실행할 수 있다. 모르는 제3의 바이트, 손상된 백업·후보와 심볼릭 링크는 거부한다. 종료된 복구도 안전하게 재확인할 수 있으며, 원래 `.local/release-9133d70-checks/nginx-file-routes-rollout.py`를 덮어쓰지 않는다. 실패 경계는 `scripts/test-file-routes-rollout.py`의 임시 파일·모의 명령 검사로 확인하고 실제 운영 복구 PASS로 확대하지 않는다.

**업무 앱 프레임(`scripts/app-frame-settings.py`, `ui-rollout.py --target frame-runtime`).** 2026-11-12 최초 도입은 [첫 배포 기록](releases/app-frames-2026-11-12-deployment.json), `7defdb0` 갱신은 [리뷰 보완 배포 기록](releases/app-frames-review-fixes-2026-11-12-deployment.json), 각 절차와 되돌리기는 [APP-FRAMES §9](APP-FRAMES.md#9-운영-반영-계획)를 따른다. `app-frame-settings.py`의 `inspect`·`apply`·`restore`는 프레임 출처 템플릿(`https://v2-app-{key}.dwdcc.kr`)·내부 bind(`127.0.0.1:18184`)·프레임 런타임 경로를 세 설정 파일에 줄 단위로 추가·복원하며 IdP는 바꾸지 않는다. `nginx-apply`는 다시 생성한 설정이 프레임 server 블록 하나와 포털·dev 포털의 `frame-src`·Referrer-Policy만 더할 때 설치한다. 기존 프레임 블록을 갱신한 2026-11-12 배포에는 재사용하지 않고 `error_log /dev/null;` 한 줄만 추가했다. 최초 도입 렌더러는 실행한 checkout의 정적 경로를 쓰므로 운영 checkout의 도구로 실행한다. `ui-rollout.py --target frame-runtime`은 `.local/frame-runtime`에 자산을 추가하고 `index.html`을 교체한다(첫 rollback은 index만 제거). Core가 프레임 문서마다 index를 읽으므로 재시작은 필요 없다.

**콘솔 출처 이동(`scripts/console-origin-move.py`).** 콘솔은 OIDC issuer(Keycloak `/identity`)도 함께 제공하고 core는 시작할 때 issuer의 discovery를 읽으므로, 짧은 점검 시간 한 번으로 옮긴다. `inspect`는 대상 주소, 현재 Nginx 파일, 새로 생성한 설정(콘솔 블록과 포털의 CLI 승인 redirect의 콘솔 주소만 바뀌는지), Keycloak `miy-web`, 유휴 core를 읽기만 한다. `apply`는 core를 멈춘 뒤 realm `frontendUrl`과 `miy-web`의 콘솔 callback·logout·web origin만 바꾸고(ids·secret·사용자 유지) 설정 파일을 줄 단위로 고친다. 이어서 Nginx에 새 콘솔과 옛 콘솔 주소의 301 redirect 사이트를 설치하고 core를 시작해 health·issuer·MCP 메타데이터를 확인한다. 어느 단계든 실패하면 Keycloak·파일·Nginx를 백업으로 되돌리고 옛 주소로 core를 시작한다. 나중의 되돌리기는 `restore`다. 모든 브라우저 사용자는 한 번 다시 로그인하고 CLI credential은 유지된다.

같은 schema·같은 인증 계약의 일반 변경은 서버를 반영하고 확인한 다음 웹 화면을 반영한다. 세션·issuer·schema가 함께 바뀌면 변경 범위의 백업·별도 복원 확인과 각 도구의 선행 조건을 지키며 검증한 순서로 수행한다. 짧은 중단·재로그인을 허용하며 유지보수 창·무중단 전환을 별도 요구로 추가하지 않는다. 반영 후 확인은 §5의 연결 검사와 별도 브라우저 확인으로 한다. 게시 CLI·Windows 설치 파일은 [CLI 릴리스](CLI-RELEASE.md) 절차로 staging을 준비하고, 서버 버전 변경 시 위 CLI 동시 반영 옵션으로 함께 게시한다. 웹 화면 반영 도구는 CLI를 바꾸지 않는다.

## 새 세 출처 설정과 전환 준비

**아무것도 없는 별도 서버는 [새 서버 설치 안내](SETUP.md)를 따른다.** DB·Keycloak·첫 관리자·TLS·프레임 런타임·Nginx/systemd와 첫 앱까지 한 문서에 있으며, 이 호스트의 고정 경로를 가진 배포 도구를 사용하지 않는다. 기존 서버에서 새 출처로 바꾸는 작업과 빈 DB의 첫 설치는 다르다.

기존 서버 전환에는 세 목적별 listener·인증서·프록시·`miy-web`의 정확한 callback/logout URI를 함께 맞춘다. 기존 realm·사용자 sub·secret을 재생성하지 않고 issuer는 콘솔 `/identity/realms/miy`에 둔다. CLI context는 prod 포털이고 MCP 클라이언트는 새 issuer 발견·필요한 재로그인을 확인한다. Core는 시작할 때 public issuer를 읽으므로 새 Nginx/Keycloak discovery가 준비된 뒤 시작한다. 위 §7의 해당 전환 도구와 되돌리기를 적용하고 새 서버 전체 설치 PASS로 옮기지 않는다.

DB·데이터를 변경하는 반영 전에는 변경 범위의 백업·별도 복원과 필요한 migration 리허설을 수행한다. 의심 자격증명은 콘솔의 개인 CLI 목록에서 device·legacy를 각각 회수한다. 개발 단계에서 CLI·SDK·설정 호환 보존은 필수가 아니지만 게시물 바이트·앱 데이터는 보존한다.

이미 앱 코드를 실행했던 포털의 브라우저별 개발자 도구에서 해당 origin의 Service Worker 등록을 해제하고 cache/storage를 정리한 뒤 새 로그인으로 확인한다. 콘솔은 앱 코드가 실행되지 않았던 새 origin을 쓴다. 일회성 서버 정리가 필요하면 **해당 origin에만** `Clear-Site-Data: "cache", "storage"`를 사용한다. `"cookies"`/`"*"`는 등록 도메인과 다른 포트 서비스에 영향을 주므로 금지한다. 알려진 구 `miy_session`·`miy_development`(/)·`miy_login`(/auth)만 만료시키고 새 목적별 쿠키나 IdP 쿠키를 일괄 지우지 않는다. 중복 쿠키/431이 남은 기기는 해당 호스트의 사이트 데이터 정리를 사용자에게 안내한다. 미접속 기기 정리 완료를 주장하지 않는다.

인증서를 교체할 때에는 새 인증서·키를 별도 파일에 두고 `check-server-tls.py`로 체인·IP SAN·만료·키 일치를 검사한다. 새 CA라면 단말/CLI 신뢰를 먼저 등록하고 원본 경로를 보존한 뒤 proxy의 설정 검사·재적재 절차를 따른다. 이후 같은 검사에 `--connect`를 붙여 세 HTTPS health를 확인한다. 실패하면 보존한 인증서·키·proxy 설정으로 되돌린다. 자동 CA 설치/갱신과 실제 Windows 검증은 이번 구현 범위 밖이다.
