# 현재 플랫폼 서버 운영·연결 안내

- 확인일: 2026-11-04
- 플랫폼 주소: **https://v2.dwdcc.kr**
- CLI 설치 안내: **https://v2.dwdcc.kr/cli/install**
- 실행 서버·게시 CLI: **0.7.0**, SDK 번들 **0.8.0**
- 배포 소스: 서버·웹 화면 `3406d9f5b9c8afbc55a0e3e8752cb178c61db0de`(2026-11-04 08:30 KST 반영), 게시 CLI `01e4d4924f8c8fc4e11720c1af81adf17ddb20bf`

2026-11-04 사용자 요청으로 사용자·그룹 앱 권한, 감사 조회와 리뷰 보완을 운영에 반영했다([배포 결과](releases/core-review-2026-11-04-deployment.json), [배포 도구](releases/evidence/core-review-20261104-linux/deploy.py)). 아래 2026-11-03 기록은 이전 배포 이력이다.

- 별도 clean checkout에서 빌드한 서버 SHA-256: `59407ce6d8853c8176054fb36afb92fa97585502cea1b5b04483d8a82ce91feb`. serve·worker의 실제 실행 파일도 같은 해시다. DB migration은 17→19, 코어 서비스 중지 구간은 약 35초였다.
- 반영 전에 실제 운영 덤프를 격리 DB에 복원하고 비특권 소유자로 migration·신규 FK 데이터 작성·rollback을 수행했다. 35개 테이블의 행 수와 전체 행 내용 해시, 소유자·locale·ACL 복원이 일치했다. 서비스 중지 후 다시 받은 최종 백업은 218,503 bytes·1,444행이며, 별도 복원·migration 확인 후 반영했다.
- PostgreSQL 컨테이너와 앱 네트워크 9개를 유지하고 연결 한도를 200으로 반영했다. Keycloak 풀은 10, 기존 앱 역할 14개는 runtime 6·migration 2다. 연결 예산은 **90 / 197 PASS**이며 과거 FAIL 기록은 보존한다.
- 앱 5개·릴리스 9개·설치 7개와 주요 기존 레코드 내용, 앱 컨테이너 7개의 ID·이미지·상태·시작 시각을 보존했다. 배포 직전 기준 실행 6개·중지 1개이며, 실행 앱 6개는 배포 후 모두 healthy다. 초기 점검 뒤 발생한 앱 조작은 배포 직전 기준에 반영했다. 작업 중 첫 배포 시도는 대기 작업 감지로 서비스 중지 전에 거부됐고, 완료·유휴 확인 후 재시도했다.
- UI는 기존 자산을 유지하고 새 자산 3개를 추가한 뒤 `index.html`을 원자적으로 교체했다. 공개 index와 새 자산의 해시가 일치한다. 실제 IdP·게시 CLI·stdio MCP 연결 17항목, 읽기 위주 화면/API 12항목 PASS, 페이지 오류·세션 정리 실패 0건이다. 기존 그룹이 없어서 그룹 구성원 화면, 일반 사용자에게 보이는 실행 앱이 없어서 해당 앱 렌더링은 NOT_RUN이다.
- 복구 자료: `.local/release-review-20261104/core/rollback/`의 기존 서버·DB 덤프, `.local/release-review-20261104/ui/production-backup/`의 기존 화면. [UI 도구](releases/evidence/core-review-20261104-linux/ui-rollout.py)의 `rollback --backup <위 UI 경로>`로 이전 index를 복구할 수 있다. 서버 시작 전 실패는 검증한 덤프로 복구하지만, 시작 후 실패는 새로 받은 쓰기를 덮어쓰지 않도록 코어를 중지하고 별도 판단한다. 실제 운영 rollback은 사용하지 않았다. DB·Keycloak 연결 설정은 서버 rollback 시 유지된다.

게시 CLI와 Windows 설치 파일은 기존 0.7.0 배포물을 유지했다. 이번 소스의 CLI 파일 잠금 수정은 아직 사용자 PC 배포물에 포함되지 않는다. Windows 실행 인수는 [별도 절차](WINDOWS-ACCEPTANCE.md)로 수행한다.

2026-11-03 [설치 승인·사용자 위임·앱 호출 계약](CORE-PERMISSIONS.md)을 포함한 **0.7.0 / 번들 0.8.0**을 운영에 반영했다([배포 기록](releases/core-0.7.0-deployment.json), [배포 스크립트](releases/evidence/core-0.7.0-linux/deploy.py)).

- 서버: 별도 checkout에서 `cargo build --release --locked`, SHA-256 `615f19a4f96b9b3adbdf393dd6c9cb8a7e52bda1dfd2d608ef5ac808da0bbe3d`. 이전 0.6.3 바이너리는 `4573260…`.
- DB: core migration 15 → 17. 적용 전 서비스 중지 상태에서 `miy_core`를 덤프하고 읽기 가능함을 확인했다. 사전 리허설로 실제 DB 덤프를 일회용 PostgreSQL에 복원해 같은 migration이 통과하고 설치 7건이 보존됨을 확인했다. 실제 설치 7건은 모두 `database`만 요청하므로 0016 이후 추가로 승인할 외부 연결은 없었다.
- 게시 CLI: GitHub Actions 후보 run `41613023015`(5개 target, 공개 Release 없음)와 Windows 설치 파일 run `41614954095`(`MiySetup-0.7.0-windows-x64.exe`)를 검증·staging한 뒤 게시했다.
- 확인: 앱 컨테이너·앱·릴리스·설치(릴리스·상태 포함)·권한·사용자 수가 전후 동일, health·설치 안내 0.7.0, 공개 다운로드 7개 해시 일치, 공유 개발 로그인 읽기 전용 브라우저 검사 오류 0건, 게시 0.7.0 Linux CLI로 아래 연결 검사 17개 PASS(정리 실패 0).
- 되돌리기: `.local/release-0.7.0/rollback/`에 이전 바이너리·CLI/설치 index·DB 덤프(0600)를 보존했다. 배포 스크립트의 `restore()`가 서비스 중지 → 파일 복원 → `pg_restore --clean --single-transaction` → 시작 순서로 되돌린다.
- 기존 사용자 PC의 0.6.3 CLI는 서버에서 차단되지 않지만 새 기능·번들 0.8.0은 [설치 안내](https://v2.dwdcc.kr/cli/install)의 0.7.0으로 갱신해야 한다. 기존 앱은 `miy app upgrade`로 번들을 전환한다. Windows 설치 파일의 실제 실행은 사용자 수행 범위로 남는다.

2026-11-03 웹 화면만 [기본 UI 이식](UIUX-MIGRATION.md) 결과(소스 `98a31cc`)로 교체했다. 이 시점의 서버·worker 실행 파일과 DB는 0.6.3(`8d7d042`)이었고, 같은 날 위 0.7.0 반영으로 교체했다. 새 화면이 쓰는 API 경로·SDK·공유 모듈 설정은 `8d7d042` 화면과 같음을 확인했다. 교체 전 `web/platform/dist`는 `.local/platform-dist-backups/dist-20261103T183736`(파일 해시 목록 `.sha256` 포함)에 보존했고, 열린 탭이 이전 파일을 계속 읽도록 이전 `assets/`와 `remoteEntry`도 새 산출물에 남겼다. 공개 주소에서 공유 개발 로그인으로 홈·앱 레일·기존 앱 화면·운영 영역·관리 화면·모바일 서랍·공개 CLI 설치 화면을 읽기만 하여 확인했고 페이지 오류는 없었다(검사 세션은 로그아웃). 되돌리려면 `mv web/platform/dist web/platform/dist.failed && cp -a .local/platform-dist-backups/dist-20261103T183736 web/platform/dist`를 실행한다. Nginx·서비스 재시작은 필요 없다.

2026-11-03 21:43 KST(12:43 UTC) 웹 화면을 [UI 후속 리뷰](UIUX-MIGRATION.md#9-ui-이식-후속-리뷰-2026-11-03) 보완 소스 `3876d43`으로 다시 교체했다. 서버·worker·DB·게시 CLI는 0.7.0 그대로다. 교체 전에 격리 전체 스택(`scripts/runtime-regression/stack-ui.sh run`)을 이 소스로 다시 실행해 5건 PASS, 정리 후 남은 자원 0, 실행 전후 호스트 비교 차이 0을 확인했다.

- 빌드: `vite build --mode production`을 격리 경로 `.local/release-ui/3876d43/dist`에 실행했다. 운영 dist와 이름이 같은 파일 105개는 내용도 같았고, 새 파일은 3개였다.
- 교체: 현재 dist를 `.local/platform-dist-backups/dist-20261103T124335`(UTC 표기, `.sha256` 포함)에 백업했다. 새 파일은 기존 파일을 덮어쓰지 않고 추가해 이전 탭용 파일을 유지했고, 새 `index.html`이 참조하는 파일이 모두 있는지 확인한 뒤 `index.html`만 rename으로 바꿨다.
- 확인: 공개 주소가 새 `index-Cfb5OKoD.js`를 제공하고 health는 정상이다. 공유 개발 로그인으로 읽기만 하는 브라우저 검사에서 다음을 확인했고 페이지 오류는 0건이다(검사 세션은 로그아웃).
  - 앱 레일 6개, 기존 앱 화면 렌더링(오류 경계 0), 운영 앱 2개, 관리 화면
  - 모바일 서랍 포커스 복귀, 가로 넘침 없음
  - 공개 CLI 설치 화면
- 되돌리기: `mv web/platform/dist web/platform/dist.failed && cp -a .local/platform-dist-backups/dist-20261103T124335 web/platform/dist`. Nginx·서비스 재시작은 필요 없다.

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

실행 서버·worker·웹 화면은 `3406d9f5b9c8afbc55a0e3e8752cb178c61db0de`이고, 게시 CLI는 `01e4d4924f8c8fc4e11720c1af81adf17ddb20bf`다(모두 버전 0.7.0 / 번들 0.8.0). DB 역할 연결 한도·`application_name`·Docker 저장 공간 하한과 사용자·그룹 앱 권한·감사 조회 migration `0018`·`0019`는 2026-11-04 반영했다. 이후 `main` 변경은 자동 배포되지 않는다. `3406d9f` 이후 `main`에는 사용자 활성 상태 변경과 앱 등록 결정의 관리자 재확인·감사 기록(`user.enabled`·`user.disabled`, 등록 승인의 `grant.enabled`)이 추가됐고([검증 기록](releases/core-admin-recheck-2026-11-04-validation.json)) 운영에는 아직 반영하지 않았다. 운영 반영은 사용자 요청과 별도 기록으로 하며, 반영 전 §6의 최소 백업과 `--migrate-with` 리허설을 수행한다.

서버를 갱신할 때는 먼저 배포 대상 소스 commit과 현재 실행 파일을 확인한다. 같은 버전 문자열이라도 실제 source commit·빌드 경계·설치된 SQL과 게시된 배포물의 출처를 함께 확인한다. 새 배포는 검증한 소스의 별도 checkout에서 빌드하고, 서버/worker·UI·게시 index를 같은 검증 기록에 묶어 반영한다.

현재 서버 바이너리 SHA-256은 `59407ce6d8853c8176054fb36afb92fa97585502cea1b5b04483d8a82ce91feb`다(직전 서버는 `615f19a4…`). 게시 CLI의 source commit과 target별 hash는 `GET /api/v1/cli/install`로 확인한다. 서버 바이너리는 default features 빌드이므로 클라이언트 전용 바이너리와 hash가 다르다.

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

## 6. 변경 전 최소 백업과 복원 확인

core DB를 바꾸는 반영(migration, 권한·그룹 데이터 변경) 전에는 그 범위를 백업하고, 별도 영역에 복원해 확인한다. 이는 [추진 계획](PLAN.md#다음-우선-작업) 순서 1의 최소 보호이며, 역할·앱 DB·파일·키까지 포함하는 전체 백업 제품(코어 4차)과 구분한다.

```sh
python3 scripts/core-backup.py create --out .local/backups/core-$(date -u +%Y%m%dT%H%M%SZ)
python3 scripts/core-backup.py verify .local/backups/core-<시각> --migrate-with <반영할 서버 바이너리>
python3 scripts/check-db-connections.py
```

- `create`: 운영 `miy_core`를 서비스 중지 없이 읽기만 한다. 덤프와 테이블별 행 수를 같은 REPEATABLE READ 스냅샷에서 얻어 `manifest.json`에 기록한다. 덤프와 manifest는 0600, 폴더는 0700이다.
- `verify`: 라벨을 붙인 일회용 PostgreSQL(tmpfs)에 복원해 migration 버전과 모든 테이블 행 수를 대조한다. `--migrate-with`를 주면 반영할 바이너리의 core migration을 복원본에 먼저 적용해 리허설한다. 소유 라벨과 전체 컨테이너 ID가 일치할 때만 지우고, 성공한 목록 조회로 부재를 확인해야 정리 완료로 보고한다. Docker 연결·조회 오류나 삭제 실패는 `FAIL`이며 `cleanupError`를 남긴다.
- 첫 실행(2026-11-03 21:57 KST): 덤프 214,693 bytes, migration 17, 테이블 35개·행 1,365개. 복원 대조 PASS, 현재 소스 바이너리로 migration 리허설 종료코드 0, 일회용 컨테이너 삭제. 행 수 하나를 바꾼 manifest는 FAIL로 거부됐다.
- 서비스를 멈춘 상태의 일관된 덤프가 필요한 실제 반영은 [0.7.0 배포 스크립트](releases/evidence/core-0.7.0-linux/deploy.py)처럼 반영 절차 안에서 다시 백업한다.

`check-db-connections.py`는 공유 PostgreSQL의 최악의 연결 수(코어 풀·병렬 관리 연결·Keycloak 풀·설치 역할 한도·활성 개발 세션)를 사용 가능 연결과 비교한다. 2026-11-03 운영 결과는 FAIL이었다. 당시 설치 역할 14개에 연결 한도가 없고, Keycloak 풀이 기본값 100이라 당시 계산의 최악 122가 사용 가능한 97을 넘었다. 현재 계산은 관리 연결 4개를 포함하며 [연결 예산](CAPACITY.md#연결-예산-현재-구현-2026-11-04)을 따른다.

새 코어는 앱 배포뿐 아니라 worker 시작 때도 기존 설치 역할 한도(runtime 6, migration 2)를 맞춘다. 코어가 생성하는 `miy_r_`·`miy_m_`와 32자리 소문자 16진수 ID 형식만 허용한다. 시작 시 적용이 실패하면 경고를 기록하고 다음 worker 시작 또는 해당 앱 배포 때 다시 적용한다. compose는 PostgreSQL `max_connections=200`, Keycloak `KC_DB_POOL_MAX_SIZE=10`을 설정한다. 소스 변경만으로 기존 운영 설정이나 역할 한도가 바뀌지 않으므로 운영 반영 후 예산을 다시 측정한다.

## 7. 7d3c66a 전용 배포·복구 도구의 범위

[배포 도구](releases/evidence/core-0.7.0-7d3c66a-linux/deploy.py)는 해당 소스의 0.7.0 서버와 core migration 17→19를 위한 특정 호스트 도구다. 이후 소스의 배포 검증이나 범용 updater로 취급하지 않는다. 2026-11-04 리뷰 보완은 이 도구를 실행해 운영에 반영한 기록이 아니다.

- 단일 실행 잠금과 새로운 복구 디렉터리를 요구하며, 기존 `rollback`이 있으면 덮어쓰지 않는다. 후보 바이너리를 복사해 검증·migration·설치가 같은 바이트를 사용한다.
- 서비스 중지 전 DB 소유자·UTF8·locale·libc provider·PUBLIC 권한 회수가 복원 계약과 같은지 확인한다. serve·worker 각각의 중지와 시작 상태를 확인한다.
- 코어 중지 후 `rollback/core/miy_core.dump`를 만들고, 일회용 PostgreSQL에서 실제 복원·migration 검증과 정리를 통과한 뒤 운영 설정·schema 변경을 시작한다.
- 복구 시 현재 DB의 재접속을 막고 고유 이름으로 보존한 뒤 빈 DB에 덤프를 복원한다. 덤프·기존 바이너리 해시와 복원 성공을 확인하기 전 서비스를 다시 시작하지 않는다. 복구에 실패하면 `ROLLBACK_FAILED`를 기록하고 수동 복구가 필요하다.
- PostgreSQL 설정 변경은 기존 컨테이너를 재시작해 앱별 네트워크 연결을 보존한다. 실행 중 앱의 DB 연결에는 영향이 있으므로 별도 운영 반영 절차로 수행한다. 전원 장애·SIGKILL 이후 자동 복구는 구현 범위에 없다.

리뷰 당시 모의 시험과 합성 DB 복원 결과는 [리뷰 후 검증 기록](releases/core-review-2026-11-04-validation.json)에 있다. 이후 `3406d9f`의 실제 운영 덤프 리허설·배포 결과는 [별도 기록](releases/core-review-2026-11-04-deployment.json)으로 관리한다. 실제 Windows 인수는 별도다.
