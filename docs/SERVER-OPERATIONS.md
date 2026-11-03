# 현재 플랫폼 서버 운영·연결 안내

- 확인일: 2026-11-03
- 플랫폼 주소: **https://v2.dwdcc.kr**
- CLI 설치 안내: **https://v2.dwdcc.kr/cli/install**
- 실행 서버·게시 CLI: **0.7.0**, SDK 번들 **0.8.0** (2026-11-03 반영)
- 배포 소스: 서버·CLI `01e4d4924f8c8fc4e11720c1af81adf17ddb20bf`, 웹 화면 `98a31cc`(기본 UI 이식, `01e4d49`와 화면 소스 동일)

2026-11-03 [설치 승인·사용자 위임·앱 호출 계약](CORE-PERMISSIONS.md)을 포함한 **0.7.0 / 번들 0.8.0**을 운영에 반영했다([배포 기록](releases/core-0.7.0-deployment.json), [배포 스크립트](releases/evidence/core-0.7.0-linux/deploy.py)).

- 서버: 별도 checkout에서 `cargo build --release --locked`, SHA-256 `615f19a4f96b9b3adbdf393dd6c9cb8a7e52bda1dfd2d608ef5ac808da0bbe3d`. 이전 0.6.3 바이너리는 `4573260…`.
- DB: core migration 15 → 17. 적용 전 서비스 중지 상태에서 `miy_core`를 덤프하고 읽기 가능함을 확인했다. 사전 리허설로 실제 DB 덤프를 일회용 PostgreSQL에 복원해 같은 migration이 통과하고 설치 7건이 보존됨을 확인했다. 실제 설치 7건은 모두 `database`만 요청하므로 0016 이후 추가로 승인할 외부 연결은 없었다.
- 게시 CLI: GitHub Actions 후보 run `41613023015`(5개 target, 공개 Release 없음)와 Windows 설치 파일 run `41614954095`(`MiySetup-0.7.0-windows-x64.exe`)를 검증·staging한 뒤 게시했다.
- 확인: 앱 컨테이너·앱·릴리스·설치(릴리스·상태 포함)·권한·사용자 수가 전후 동일, health·설치 안내 0.7.0, 공개 다운로드 7개 해시 일치, 공유 개발 로그인 읽기 전용 브라우저 검사 오류 0건, 게시 0.7.0 Linux CLI로 아래 연결 검사 17개 PASS(정리 실패 0).
- 되돌리기: `.local/release-0.7.0/rollback/`에 이전 바이너리·CLI/설치 index·DB 덤프(0600)를 보존했다. 배포 스크립트의 `restore()`가 서비스 중지 → 파일 복원 → `pg_restore --clean --single-transaction` → 시작 순서로 되돌린다.
- 기존 사용자 PC의 0.6.3 CLI는 서버에서 차단되지 않지만 새 기능·번들 0.8.0은 [설치 안내](https://v2.dwdcc.kr/cli/install)의 0.7.0으로 갱신해야 한다. 기존 앱은 `miy app upgrade`로 번들을 전환한다. Windows 설치 파일의 실제 실행은 사용자 수행 범위로 남는다.

2026-11-03 웹 화면만 [기본 UI 이식](../UIUX_MIGRATION_PLAN.md) 결과(소스 `98a31cc`)로 교체했다. 이 시점의 서버·worker 실행 파일과 DB는 0.6.3(`8d7d042`)이었고, 같은 날 위 0.7.0 반영으로 교체했다. 새 화면이 쓰는 API 경로·SDK·공유 모듈 설정은 `8d7d042` 화면과 같음을 확인했다. 교체 전 `web/platform/dist`는 `.local/platform-dist-backups/dist-20261103T183736`(파일 해시 목록 `.sha256` 포함)에 보존했고, 열린 탭이 이전 파일을 계속 읽도록 이전 `assets/`와 `remoteEntry`도 새 산출물에 남겼다. 공개 주소에서 공유 개발 로그인으로 홈·앱 레일·기존 앱 화면·운영 영역·관리 화면·모바일 서랍·공개 CLI 설치 화면을 읽기만 하여 확인했고 페이지 오류는 없었다(검사 세션은 로그아웃). 되돌리려면 `mv web/platform/dist web/platform/dist.failed && cp -a .local/platform-dist-backups/dist-20261103T183736 web/platform/dist`를 실행한다. Nginx·서비스 재시작은 필요 없다.

현재 호스트가 플랫폼 서버다. 다른 PC의 브라우저·CLI·프로젝트 MCP는 위 HTTPS 주소를 사용한다. 주소·인증·서비스 설정은 이미 적용되어 있으며, 이 문서는 현재 배포와 저장소 작업본을 구분해 유지관리하는 절차다.

## 1. 서버 주소와 실행 구성

| 항목 | 현재 구성 |
| --- | --- |
| 대표 주소 | `https://v2.dwdcc.kr` |
| 웹·API·CLI 설치/다운로드 | 같은 대표 주소, 호스트 Nginx에서 전달 |
| 플랫폼 API/worker | systemd `miy-v2-serve`, `miy-v2-worker` |
| 서버 실행 파일 | `/projects/miy-v2/.local/bin/miy-server` |
| API 내부 주소 | `127.0.0.1:18180` |
| IdP | `/identity/realms/miy`, 내부 Keycloak `127.0.0.1:18181` |
| PostgreSQL | 호스트 loopback `127.0.0.1:15432` |
| 상태·비밀 설정 | `/projects/miy-v2/.local/`, `server.json`, `runtime.env`, `credentials.json` |
| 직접 HTTP MCP | `https://v2.dwdcc.kr/mcp`, IdP OAuth 인증 |

대표 주소는 서버의 `MIY_ORIGIN`, 로그인 issuer는 `MIY_ISSUER`로 관리한다. Nginx·Keycloak callback·MCP resource/audience·설치 화면의 서버 주소를 같은 origin에 맞춘다. API와 DB의 내부 포트는 사용자 PC의 CLI 설정에 입력하지 않는다. 비밀 설정 파일이나 그 전체 내용을 문서·채팅·Git에 복사하지 않는다.

서버 상태는 다음 읽기 명령으로 확인한다.

```sh
systemctl status miy-v2-serve miy-v2-worker --no-pager
curl --fail --silent --show-error https://v2.dwdcc.kr/api/health
curl --fail --silent --show-error https://v2.dwdcc.kr/api/v1/capabilities
sudo nginx -t
```

HTTP 주소 `http://v2.dwdcc.kr`는 현재 410을 반환한다. 사용자 접속·설치 안내에는 위 HTTPS 주소를 사용한다. 두 MIY 서비스는 부팅 시 시작하도록 enabled 상태다. 이번 확인에서 서버의 실제 TLS 인증서 검증과 Nginx 설정 검사를 통과했다.

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

Codex·Claude의 생성된 프로젝트 MCP는 프로젝트의 `.miy-tools/miy[.exe] mcp`를 로컬 stdio로 실행한다. 이 프로세스가 CLI와 같은 context·사용자 세션으로 플랫폼의 계약·API·컴포넌트를 읽는다. 프로젝트 MCP 설정에 `/mcp` URL을 넣거나 별도의 토큰을 복사할 필요가 없다. 직접 HTTP MCP 연결을 사용하는 클라이언트는 `/mcp`의 별도 OAuth 흐름을 사용한다.

## 3. 현재 배포 소스와 이 작업본의 차이

2026-11-02에 `main`에 운영 소스를 통합하여 CLI **0.6.3 / 번들 0.7.1**로 맞췄다. 이후 2026-11-03 코어 권한 소스는 0.7.0 / 0.8.0으로 진행했으며 이 운영 반영 이력과 구분한다. 실제 서버와 공개 CLI는 별도 checkout에서 검증한 `8d7d042` 배포물을 유지한다. 앞선 2026-11-02 연결 검사 시점의 `main`은 0.6.0이었으며 그 당시 source/hash 기록은 변경하지 않는다. 운영 소스는 `codex/cli-experience-improvements` 계열이며, 로컬 `codex/cli-experience-linux-validation`도 해당 후속 검증 기록을 포함한다.

따라서 서버를 갱신할 때에는 먼저 배포 대상 소스 commit과 현재 실행 파일을 확인한다. 같은 버전 문자열이라도 실제 source commit·빌드 경계·설치된 SQL과 게시된 배포물의 출처를 함께 확인한다. 새 배포는 검증한 소스의 별도 checkout에서 빌드하고, 서버/worker·UI·게시 index를 같은 검증 기록에 묶어 반영한다. 기존 비밀 설정과 DB 데이터를 사용하는 서버에 초기 `bootstrap`, `seed`, Keycloak 재설정을 갱신 절차 대신 실행하지 않는다.

현재 서버 바이너리 SHA-256은 `4573260257147c2563ba5ffae177b9ac6c6b0a331d2a0b1fef1cf6e60d10064e`다. 게시 CLI의 source commit과 target별 hash는 `GET /api/v1/cli/install`로 확인한다. 서버 바이너리는 default features 빌드이므로 클라이언트 전용 바이너리와 hash가 다르다.

배포와 이전 검증의 상세 이력은 [운영 소스의 0.6.3 반영 기록](https://github.com/hurxxxx/miy-v2/blob/7617246/docs/CLI-0.6.3-LINUX-VALIDATION.md)에 있다. 이 문서의 실제 연결 재검사 결과는 아래에 추가한다.

## 4. 공유 개발 로그인과 운영 전환

현재 서버에는 명시적으로 **공유 개발 로그인**이 켜져 있다. 접속자는 로그인 화면의 개발 계정 버튼으로 관리자·개발자·사용자 계정을 선택할 수 있다. 이는 이 서버의 개발·평가용 설정이며, 실제 IdP 로그인도 별도로 제공한다.

일반 사용자별 인증으로 전환할 때에는 같은 상태 디렉터리에서 `python3 scripts/bootstrap.py --no-shared-development-login`으로 설정을 바꾸고 `miy-v2-serve`에 반영한다. 반영할 실행 파일이 해당 옵션을 지원하는 소스인지 먼저 확인한다. 개발 로그인 버튼을 끈 사실만으로 사용자 계정·권한·업무 앱의 전체 인수가 완료되는 것은 아니다.

Windows·macOS의 실제 사용자 PC에서 설치·인증·개발·배포를 실행한 증거와 독립 AI 세션 평가는 해당 환경의 기록으로 관리한다. 같은 Linux 호스트에서 공개 HTTPS 주소로 접속한 검사를 다른 PC의 검증 결과로 기록하지 않는다.

## 5. 2026-11-02 실제 연결 재검증

2026-11-03의 [코어 안정화 1차](CORE-HARDENING.md)는 소스 `9967265`와 격리 회귀 결과다. 실행 중 서버·worker와 공개 배포물의 `8d7d042` 출처는 유지하며, 같은 0.6.3 버전 문자열만 보고 새 코어 수정이 운영에 반영됐다고 판단하지 않는다.

이 절은 앱 생성 없는 연결 재검사의 이력이다. 이후 main 통합과 별도 새 앱 두 개의 [독립 AI 개발 인수](INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md)를 완료했다. 후속 인수는 의도적으로 dev 앱·빌드·배포를 추가했으며 기존 서비스·컨테이너·installation의 보존과 검증용 세션 정리를 별도로 기록했다.

현재 Linux 호스트에서 공개 HTTPS 주소로 접속했다. [비밀값을 제외한 실행 기록](releases/platform-server-2026-11-02-validation.json)에 버전·소스·바이너리/검사 코드 SHA-256·통과 항목·보존 결과를 기록했다.

| 검사 | 실제 결과 |
| --- | --- |
| TLS·기본 서비스 | TLS 1.3 인증서 검증, health/capability 응답, Nginx 설정 검사 PASS |
| 서버 설치 구성 | 생성본과 설치된 Nginx/systemd unit 해시 일치, 두 MIY 서비스 active/enabled, 비밀 설정 0600·상태 디렉터리 0700 |
| 공개 배포물 | CLI 5종 + helper 2종, 총 249,147,486 bytes 실제 다운로드·SHA-256 일치 |
| Windows 설치 파일 | `MiySetup-0.6.3-windows-x64.exe`, 6,105,600 bytes 실제 다운로드·게시된 hash 일치. 이번 실행에서 Windows 설치 파일을 실행하지는 않음 |
| Unix 설치 | 게시 Linux CLI를 임시 prefix에 설치하고 명시한 서버 주소 등록·실행 파일 hash 보존 확인 |
| 로그인·CLI·프로젝트 MCP | **17개 확인 PASS**, 정리 실패0. 일반 IdP 개발자 로그인 1회, 같은 계정의 device 승인·CLI auth status·operate 진단, 로컬 프로젝트 생성·stdio 시작과 도구4종·자기 세션 정리 포함 |
| MCP 실제 응답 대조 | 보호 파일19개, API 목록55개, 컴포넌트1개와 선택한 명령 도움말이 인증된 HTTP API/CLI 결과와 일치 |
| 직접 HTTP MCP 경계 | resource/issuer metadata의 대표 주소 일치, 인증 없는 요청과 잘못된 bearer 각각401 및 OAuth challenge 확인 |
| 서비스·앱 보존 | 검사 전후 서비스 PID·시작 시각·재시작 횟수 동일, 기존 실행 컨테이너23개 동일. 앱3·릴리스5·설치5·runtime job13·image upload4·server build2·적용 migration15개 수 보존 |

17개는 서버 단위 테스트 수가 아니라 연결·도구 응답·정리 조건의 확인 수다. 기존 앱 등록·배포·DB 조작을 포함하는 전체 업무 인수와 구분한다. 이번에는 일반 IdP 로그인도 실제 성공했으며, 기존 기록의 `NOT_RUN`을 이 실행 범위에서 보완했다. 별도 PC 접속, 독립 AI 실행, 직접 HTTP MCP의 인증된 도구 호출, 정상 OAuth callback 재사용과 앱 재배포는 이번 실행 범위에 포함하지 않았다.

로그인·CLI·프로젝트 MCP의 17항목 재검사는 설치된 `pnpm` 의존성과 Chrome이 있는 서버에서 다음처럼 실행한다. 위 표의 전체 다운로드·서비스 구성·DB 건수 보존 검사는 별도로 수행했다. 검증할 CLI의 공개 checksum/provenance를 먼저 확인하고 그 절대 경로를 전달한다. 출력 디렉터리는 새 경로여야 한다.

```sh
node scripts/check-platform-connection.mjs \
  --origin https://v2.dwdcc.kr \
  --cli /absolute/path/to/verified/miy \
  --credentials-file /projects/miy-v2/.local/credentials.json \
  --output /projects/miy-v2/.local/evidence/new-platform-connection-run
```

이 검사기는 비밀번호·쿠키·device code·토큰·원본 응답을 출력하지 않는다. 별도의 private CLI 설정과 브라우저 프로필을 만들고 실제 일반 OIDC 로그인을 한 번 시도한다. 프로젝트 파일은 출력 디렉터리에만 생성하며 앱 등록·권한 변경·배포를 요청하지 않는다. 종료 시 이번 CLI 세션과 웹 플랫폼 세션을 로그아웃하고 MCP·브라우저 프로세스를 종료한다. 계정 전체 IdP 세션 철회는 수행하지 않는다.

전체 다운로드만 확인하려면 읽기 전용 명령을 사용한다.

```sh
python3 scripts/check-cli-downloads.py --origin https://v2.dwdcc.kr \
  --expected-commit 8d7d04271f9467d8bbdd16308e73715b33e4dc0c
```

기존 `scripts/check-mcp.mjs`는 로컬 검증 계정의 전체 세션을 철회하는 검사이며, 일반 플랫폼의 연결 확인용으로 실행하지 않는다. `check-server-development-flow.mjs --run`도 새 앱·권한·DB·배포를 만드는 전체 인수 도구이므로 위 연결 검사와 용도를 구분한다.
