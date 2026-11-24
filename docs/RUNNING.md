# CLI와 독립 앱 개발·운영 실행 안내

현행 CLI의 설치·인증·앱 개발·운영 사용법을 모은다. 설치된 실행 파일의 `miy --version`, `miy describe`와 서버 capability가 실제 사용 가능한 범위를 결정한다. 게시 CLI와 SDK 번들의 버전은 서버마다 다르며 [CLI-RELEASE](CLI-RELEASE.md)가 관리한다. 현행 게시물은 서버 빌드 전용 배포·한 줄 설치·비로그인 신고를 제공한다. 연결할 서버의 설치 안내에서 해당 게시물을 받는다. 게시 출처와 남은 조건은 [CLI-RELEASE](CLI-RELEASE.md), 실제 서버 상태는 [SERVER-OPERATIONS](SERVER-OPERATIONS.md)를 따른다.

검사 선택은 [CHECKS](CHECKS.md), 사용자 Windows 설치·인수는 [WINDOWS-ACCEPTANCE](WINDOWS-ACCEPTANCE.md)에 모은다. 명령 구현·특정 환경 PASS·전체 V7 인수는 구분한다. 현재 [실행·인증 계약](APP-BOUNDARIES.md#확정-결정)은 prod 포털·dev 포털·관리/CLI 승인/Keycloak 콘솔 세 출처다. 원격 업무 앱은 설치별 출처 프레임에서 실행하며 단일 IP HTTPS도 지원한다. CLI의 대표 서버 주소는 prod 포털이고 브라우저 승인만 콘솔에서 처리한다.

## 설치와 발견

처음 접하는 코딩 에이전트에는 업무 포털의 `/api/v1/cli/agent-setup` URL 하나를 제공한다. 이 공개 텍스트는 설정된 서버의 설치 주소, 계정 승인, 앱 초기화, 프로젝트 MCP 검증을 안내한다.

배포받은 클라이언트로 원격 운영만 수행할 때는 Rust·Node·Docker가 필요하지 않다. CLI를 소스에서 빌드할 때는 Rust 도구체인과 OS의 네이티브 링크 도구가 필요하다. 앱 개발 도구는 아래 [개발 방식별 조건](#로컬-개발-빌드와-dev-배포)을 따른다. 사용자 전용 설치 경로를 사용하고 시스템 전역 PATH·인코딩·TLS 신뢰 설정을 임의로 변경하지 않는다.

서버의 **통합 설치** 화면(`/cli/install`)에서 운영체제에 맞는 명령을 복사한다. 계정이 처음이면 화면의 **회원가입 신청**을 이용한다. 콘솔 관리자가 승인한 링크로 비밀번호를 설정한 뒤 CLI에 로그인한다. 브라우저에서 패키지를 먼저 내려받거나 압축을 풀 필요가 없다. 예를 들어 서버가 `https://PLATFORM`이라면 다음과 같다(실제 주소로 바꾼다).

```sh
curl -fsSL 'https://PLATFORM/api/v1/cli/install.sh' | bash
```

Windows x64 PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod -Uri 'https://PLATFORM/api/v1/cli/install.ps1')))
```

온라인 설치기는 OS/CPU에 맞는 게시 패키지를 다운로드하고 SHA-256과 바이너리 해시·버전을 확인한 뒤 **그 패키지에 포함된 설치기**를 실행한다. 서버 주소는 `MIY_ORIGIN`에서 주입하며 도메인을 내장하지 않는다. HTTP IP·포트와 HTTPS 도메인을 모두 지원한다. 서버에 전체 CLI 패키지가 게시되지 않았거나 검증에 실패하면 실행 스크립트를 제공하지 않는다(503). 사설 CA는 먼저 PC의 curl/PowerShell과 CLI에서 신뢰하도록 등록하고 TLS 검증을 끄지 않는다.

Unix는 `~/.local/bin`과 사용자 셸 PATH를 설정한다. 새 터미널에서 `miy auth login`을 실행하거나 설치기가 출력하는 절대 경로로 바로 로그인한다. 경로 변경은 `curl … | bash -s -- --prefix /absolute/path`, 셸 파일을 수정하지 않으려면 `--no-modify-path`를 사용한다. Windows는 사용자 설치 폴더·PATH를 설정하고 현재 PowerShell에서도 명령을 사용할 수 있게 한다. 기존 연결·인증 정보는 보존하며 다른 서버를 사용 중이었다면 `miy context list`로 확인하고 새 연결을 명시적으로 추가·선택한다.

오프라인 패키지 또는 소스 설치는 아래 방식도 지원한다. 이 오프라인 Unix 설치기는 셸 설정을 수정하지 않는다.

```sh
bash scripts/install-cli.sh --from /path/to/miy --server https://PLATFORM
# PATH에 없으면 현재 셸에만 추가
export PATH="$HOME/.local/bin:$PATH"
miy --version
miy auth login
miy describe
miy describe --search deploy
miy describe --command "app deploy"
```

Unix 배포물은 해당 OS/CPU의 tar.gz와 게시 SHA-256을 확인한 뒤 새 디렉터리에 푼다. 그 안에서 `bash ./install-cli.sh --prefix "$HOME/.local/bin" --server https://PLATFORM`을 실행하면 동봉된 `release.json`과 바이너리를 사용하고 Rust 빌드를 하지 않는다. 별도 실행 파일은 `--from /absolute/path/to/miy`로 지정한다. 소스 checkout의 `bash scripts/install-cli.sh`만 클라이언트를 빌드한다. shell profile/PATH는 자동 변경하지 않는다. Windows의 GUI·수동 ZIP 설치와 PowerShell 조건은 [Windows 안내](WINDOWS-ACCEPTANCE.md)를 따른다. 프로젝트 실행 파일은 Unix에서 `.miy-tools/miy`, Windows에서 `.miy-tools/miy.exe`다.

플랫폼의 **통합 설치** 화면(`/cli/install`)은 서버 `MIY_ORIGIN`에서 읽은 주소를 Unix의 `--server` 또는 Windows의 `-Server`에 넣은 명령을 제공한다. 검증한 배포물을 풀어 그 디렉터리에서 명령을 실행한다. 공통 실행 파일에 특정 운영 도메인을 내장하지 않으므로 같은 CLI를 다른 서버에도 사용할 수 있다. 설치 안내 API는 `GET /api/v1/cli/install`이며 요청의 Host나 브라우저 주소를 서버 설정 대신 사용하지 않는다.

CLI에는 기본 IP·도메인이 없다. 설치 명령의 서버 인자 또는 사용자가 직접 입력한 URL만 저장한다. 주소를 생략한 대화형 설치는 주소 입력을 요청하고, 입력 취소·비대화형 설치는 실행 파일만 설치한 뒤 서버 미등록 상태와 `miy context init`을 안내한다. 설치 후 해당 명령을 실행하거나 서버 미등록 상태에서 대화형 로그인을 시작하면 주소를 입력할 수 있다. URL에는 `http://` 또는 `https://`를 명시한다. 기존 설정과 자격증명은 보존하며, 전달한 주소가 기존 활성 연결과 다르면 다른 서버 추가·선택 절차를 안내한다. context 이름 `default`는 저장 이름이며 미리 정해진 서버 주소를 뜻하지 않는다. 이전 CLI 0.5 후보에는 이 흐름이 포함되어 있지 않다.

`describe` 기본 결과는 짧은 루트 목록이다. `--command`는 선택한 명령의 인수·enum·필수값·기본값·충돌·스키마·권한·부작용·오류·예제를, `--all`은 전체 참조를 반환한다. 도움말과 셸 완성은 같은 Clap 트리에서 생성한다.

```sh
miy completion bash
miy completion zsh
miy completion fish
miy completion powershell
```

## CLI 업데이트와 MCP 갱신

전역 CLI 설치, 프로젝트에 복사된 CLI, 실행 중인 MCP는 각각 갱신해야 한다. CLI 0.12.1부터 `miy describe --topic updates`와 프로젝트 MCP의 `get_cli_help({"topic":"updates"})`로 로그인·네트워크 없이 이 절차를 읽을 수 있다. 원격 HTTP MCP에는 `get_cli_update_guide`가 있으며 해당 HTTP 연결의 인증은 필요하다.

1. `miy context show`의 업무 포털 origin을 확인하고 그 서버의 `/cli/install`을 연다. 도메인 또는 IP·포트를 그대로 사용한다. 설정을 읽지 못하면 관리자에게 공개 포털 주소를 확인한다.
2. 로컬 미리보기와 CLI를 사용하는 Codex·Claude Code를 종료한다. 특히 Windows는 실행 중인 파일을 교체할 수 없다. 같은 OS 사용자의 일반 터미널에서 **기존 설치 명령을 다시 실행**한다. 기존 context·자격증명은 보존된다. 사용자 지정 설치 경로를 사용했다면 같은 경로를 지정한다.

```sh
# https://PLATFORM을 선택한 서버 origin으로 바꾼다.
curl -fsSL 'https://PLATFORM/api/v1/cli/install.sh' | bash
miy --version
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod -Uri 'https://PLATFORM/api/v1/cli/install.ps1')))
miy --version
```

설치 화면의 버전과 일치하는지 확인한다. 이전 버전이 나오면 새 터미널을 열고 `command -v miy` 또는 PowerShell의 `Get-Command miy`로 실제 실행 경로를 확인한다. 설치기가 출력한 새 실행 파일의 절대 경로도 사용할 수 있다. 오프라인 환경은 검증한 새 패키지에 동봉된 설치기를 사용한다. 사설 HTTPS는 기존 CA 신뢰를 사용하며 TLS 검증을 끄지 않는다.

3. **각 앱 프로젝트 폴더에서 새 전역 CLI로** 아래 명령을 실행한다. `app setup` 자체는 새 CLI를 다운로드하지 않으며, 실행한 바이너리를 프로젝트에 복사한다. 기존 `.miy-tools/miy app setup`을 실행하면 이전 버전을 다시 복사하므로 전역 설치와 구분한다.

```sh
miy app setup
```

SDK 번들이 같으면 이것으로 충분하다. 번들이 다르면 변경을 검토하고 깨끗한 커밋을 만든 뒤 명시적으로 `miy app upgrade`를 사용한다. 게시 SDK 파일을 직접 고치지 않는다.

4. **Codex·Claude Code를 재시작**하여 프로젝트 MCP와 가이드를 다시 불러온다. 같은 OS 사용자·context로 프로젝트 실행 파일을 확인한다.

```sh
./.miy-tools/miy --version
./.miy-tools/miy auth status --output json
```

Windows는 `.\.miy-tools\miy.exe`를 사용한다. 업데이트만으로 재로그인을 요구하지 않는다. `login_required`·`reauthentication_required` 등 인증이 필요하다는 결과일 때 로그인한다. 인증 저장소 오류라면 브라우저 승인을 반복하지 말고 안전한 진단 필드를 확인한 뒤 같은 실행 파일의 `report send --message "인증 저장소 오류"`로 [비로그인 신고](SUPPORT-REPORTS.md)를 보낸다.

위 프로젝트 갱신은 로컬 stdio MCP에 해당한다. 업무 포털 `/mcp`에 직접 연결한 HTTP MCP는 서버에서 실행되고 인증도 별도다. 로컬 CLI 업데이트가 HTTP MCP 서버나 OAuth 세션을 갱신하지는 않는다.

## 서버 context와 인증

주소 고정과 이전 시의 context·CLI 세션·HTTP MCP OAuth 영향은 [서버 주소 설정](SERVER-ADDRESSES.md)을 참고한다. 프로젝트의 `miy mcp` stdio와 `<업무 포털>/mcp` HTTP 연결은 구분한다.

설치 명령에 서버 주소가 포함되어 있었다면 `miy context show`로 명시적으로 등록된 연결을 확인하고 로그인한다. 주소를 등록하지 않았다면 먼저 `miy context init`으로 입력한다. URL을 별도로 등록하거나 다른 서버를 추가할 때에는 아래 명령을 사용한다.

```sh
miy context add work --url https://PLATFORM
# 사설 CA라면 context add에 --ca-file /path/to/root.crt 추가
miy context use work
miy context list
miy context show
miy auth login
# pending이면 표시된 URL에서 로그인하고 userCode를 대조·승인한 뒤:
miy auth wait LOGIN_UUID
miy auth status
miy doctor --for operate
```

선택 순서는 `--context`, `MIY_CONTEXT`, 프로젝트 binding, 활성 context다. 잘못된 명시적 context를 다른 서버로 대체하지 않는다. `--project PATH`는 작업 프로젝트를 지정한다. 운영 명령은 로컬 소스 없이 `--app APP_ID`로 실행할 수 있다. `doctor --for operate`는 인증·연결·서버 기능을 검사하며 개발 도구 설치를 요구하지 않는다.

설치기는 `miy --output json context init --url URL`을 사용한다. 이 명령은 네트워크 접속 없이 명시된 URL을 검증하고 사용자 설정이 없는 경우에만 연결을 생성한다. `--url`을 생략하면 대화형 터미널에서 주소를 받으며 비대화형 실행은 등록 필요 오류를 반환한다. `--context`, `MIY_CONTEXT`, 프로젝트 binding은 초기화 대상에 영향을 주지 않는다. 기존 파일이 잘못된 형식이면 덮어쓰지 않고 실패한다. 연결 초기화가 실패하면 설치기는 비정상 종료와 함께 실행 파일은 이미 설치되었음을 구분해 알린다. 사설 CA가 필요한 수동 초기화에는 `context init --url URL --ca-file PATH`를 사용할 수 있다.

로그인 요청은 5분 동안 유효하다. 공개 출력은 URL·userCode·login ID뿐이며 device secret·access/refresh token은 출력하지 않는다. 비TTY, `--no-input`, `--no-wait` 로그인은 pending을 즉시 반환한다. 대화형 기본은 브라우저를 열고 기다릴 수 있고, `--device-code`·`--no-open`은 자동 브라우저 실행을 생략한다. `auth wait`는 서버의 poll 간격과 slow-down을 따르며 Ctrl-C 시 남은 요청을 보존한다.

설정 루트는 `MIY_CONFIG_DIR`를 우선하고, Unix는 `$XDG_CONFIG_HOME/miy` 또는 `~/.config/miy`, Windows는 `%APPDATA%\miy`를 사용한다. Windows와 WSL의 자격증명을 자동 공유하지 않는다. 프로젝트에는 연결 이름만 기록하고 토큰을 두지 않는다.

Access token은 최대 15분, 회전하는 refresh 세션은 최대 30일이다. 원본 origin/user에 묶인 자격증명을 OS 저장소와 보호된 fallback에 보관한다. 프로젝트 소스나 일반 출력에 저장하지 않는다. 프로세스 간 잠금으로 갱신을 직렬화하며 갱신 응답이 유실되면 사용한 refresh token을 반복 제출하지 않고 다시 로그인한다. 서버의 Core `users.enabled` 검사는 상위 IdP 비활성화가 이미 전파됐다는 증거가 아니다. 실제 원천과 전파 지연은 별도 검증한다.

```sh
miy auth sessions list --limit 20
miy --yes auth sessions revoke SESSION_UUID
miy auth logout
# 원격 철회를 시도하지 않고 로컬 저장만 제거할 때에만:
miy auth logout --local-only
miy --yes context remove work
```

로그아웃은 원격 철회 확인과 로컬 삭제 결과를 구분한다. `context remove`는 로컬 연결만 제거하고 원격 세션이나 앱 데이터를 삭제하지 않는다. 원격 철회가 필요하면 context 제거 전에 logout 또는 sessions revoke를 실행한다.

CLI context·API의 대표 주소는 prod 포털이며 device의 브라우저 승인만 콘솔 세션으로 처리한다. 게시 CLI가 받는 상대 `verificationPath`는 서버가 설정된 콘솔의 승인 페이지로 고정 리디렉션한다. 임의 목적지나 토큰을 URL에 넣지 않는다. device 시작·poll·refresh는 비브라우저 경로로 제공한다. 0.5 이전 CLI의 legacy 로그인은 종료했으므로(410 `cli_upgrade_required`) 업그레이드 후 `miy auth login`을 다시 실행한다. `miy-web` OIDC client에는 prod/dev/console 세 출처의 정확한 callback/logout URI를 등록한다. 개발 단계의 후속 변경에 구 형식 호환을 요구하지 않는다. 게시 버전과 후보 출처는 [CLI 릴리스](CLI-RELEASE.md)를 따른다.

## 새 앱 준비와 등록

```sh
miy --context work app init ../miy-apps/my-app --id my-app --name '내 앱'
cd ../miy-apps/my-app
.miy-tools/miy component sync
.miy-tools/miy doctor --for develop
.miy-tools/miy app register
# 관리자 context에서:
miy registration list
miy registration approve REGISTRATION_UUID
miy user list
miy --app my-app grant set --user USER_ID --permission use --environment dev
```

사람은 Core에 등록된 뒤에만 로그인할 수 있다. 관리자가 콘솔 **플랫폼 관리 → 사용자 초대**에서 초대 링크를 만들어 전달하면 받은 사람이 직접 계정을 만들고 그 순간 등록된다. 비밀번호를 잊으면 같은 화면에서 재설정 링크를 만든다([계정 발급](ACCOUNTS.md)). 계정 링크가 설정되지 않은 서버에서는 운영자가 [SETUP §8의 내부 Keycloak API 절차](SETUP.md#8-기동-확인과-첫-로그인)로 계정을 만들고 관리자가 **로그인 서버 계정 직접 등록**에 그 UUID와 표시 이름을 입력한다. 공개 `/identity`는 로그인 경로만 허용하며 Keycloak master 관리 화면은 열지 않는다. 같은 miy 화면에서 관리자 지정, 앱 등록 신청 자격(`앱 개발 허용`), 사용 중지를 바꾼다. 등록만으로는 앱 권한이 생기지 않는다. API는 `POST /api/v1/users`, `PATCH /api/v1/users/{id}`, `GET·POST /api/v1/account-links`이며 CLI 명령은 `user list`만 있다.

선택한 context가 있으면 init이 프로젝트에 그 연결 이름을 기록한다. context 없이 로컬 프로젝트를 생성할 수 있지만 원격 작업 전에는 context를 추가·선택하고 로그인해야 한다. 등록 요청만으로 권한이 생기지 않는다. 관리자의 승인은 요청자에게 dev/develop 권한만 부여하며 prod/deploy는 별도다.

`doctor --for develop`의 기본 runtime은 native다. manifest·번들·CLI 지침·컴포넌트 잠금, Rust/Node/pnpm/Git, 실제 Rust 링크와 프로젝트 MCP 시작을 검사한다. Docker 버전·Linux 모드·엔진/서버 아키텍처 검사는 `--runtime docker`를 명시했을 때 추가한다. 필수 검사 실패는 exit 7과 개별 결과로 반환한다. 새 앱은 로그인 후 component sync를 먼저 해야 컴포넌트 검사를 통과할 수 있다.

생성기는 SDK·지침·스킬과 프로젝트 Codex/Claude MCP 설정을 제공한다. 호환 CLI로 `app setup`을 실행하면 프로젝트 실행 파일·설정·CLI 지침을 함께 갱신한다. 일반 `codex` 또는 `claude`로 앱 폴더를 열고 각 클라이언트의 프로젝트 신뢰/MCP 승인을 정상적으로 진행한다. 전역 모델·권한 설정을 우회하지 않는다. 루트 `miy mcp` stdio 서버는 선택한 CLI context/session으로 계약과 허용된 API·컴포넌트 목록을 읽는다. API 사용 지침과 실제 배포 실행을 구분한다.

## 공식 앱 공급과 설치

이 절은 Core·CLI **0.13.0**, core schema 25부터 적용한다. 카탈로그에 등록한 제품과 실제 배포한 릴리스는 따로 확인한다. `miy app list`와 `app status`의 `app.distribution`은 제품 분류이고, 설치 행의 `distribution`은 서버가 검증한 해당 릴리스의 분류다. 값은 `default_official`(기본 제공), `optional_official`(공식 업무), `organization`(조직 제작)이다. `kind`는 실행 형식이다. 조회는 기존 사용자·환경 권한을 따른다.

1. **공급자**는 독립 앱 Git 저장소에서 릴리스할 버전을 커밋한다. 해당 앱 폴더에서 `miy --output json app source-pin`을 실행하면 실제 서버 빌드와 같은 방식으로 계산한 `data.appId`, `version`, `sourceCommit`, `archiveSha256`을 얻는다. 같은 ID의 `releases`에 뒤의 세 필드를 추가해 검토한 카탈로그를 공급한다. 앱의 manifest나 빌드 요청자가 공식 지위를 부여할 수 없다.
2. **서버 운영자**는 신뢰한 카탈로그를 선택해 설치 호스트의 `MIY_STATE_DIR`을 지정한 셸에서 `MIY_BINARY="$MIY_STATE_DIR/bin/miy-server" bash scripts/run-service.sh catalog-sync --file /path/to/catalog.json`을 실행한다. 서버 실행 파일의 설치 경로가 다르면 해당 절대 경로를 지정한다. `scripts/up.sh`는 서버를 시작한 뒤 `registry/apps/catalog.json`을 동기화하며, 운영자가 `MIY_OFFICIAL_APP_CATALOG`로 다른 파일을 지정할 수 있다. 동기화는 제품·버전을 누적 반영하고 같은 버전의 소스 변경이나 기존 조직 앱·등록·권한 충돌은 전체 거절한다. 직원 권한, 설치 데이터, 실행 상태와 현장에서 수정한 제품 이름은 보존한다.
3. **설치 담당자**는 같은 카탈로그와 그 커밋으로 체크아웃한 소스를 준비한다. 아래는 저장소 루트에서 실행하는 예다. `--sources` 아래 폴더 이름은 앱 ID이고, 단일 앱은 `--app planner --source /path/to/worktree`도 사용할 수 있다. 모노레포의 서로 다른 커밋이 필요하면 앱별 Git worktree를 준비한다. 게시된 버전이 없는 제품은 `plan`에서 `unreleased`로 표시된다.

```bash
python3 scripts/official-apps.py plan --catalog /path/to/catalog.json
python3 scripts/official-apps.py install --context work --catalog /path/to/catalog.json --sources /path/to/official-apps
python3 scripts/official-apps.py install --context work --catalog /path/to/catalog.json --app pms --source /path/to/pms
python3 scripts/official-apps.py update --context work --catalog /path/to/catalog.json --app planner --source /path/to/planner --version 1.1.0
```

`install`은 기본 제품 또는 `--app`으로 명시한 제품을 dev에 준비한다. 한 회사에서만 PMS를 기본 구성에 포함하려면 `--app planner --app messenger --app pms`처럼 원하는 목록을 지정한다. 기존 설치를 다시 시작하거나 갱신하지 않는다. `update`는 실제로 실행 중인 dev 설치를 명시적으로 갱신한다. 개발자는 허용된 계정으로 dev에서 동작과 데이터 보존을 시험하고, 해당 릴리스 UUID와 검사 결과가 든 [인수 보고서](#인수-기록-prod-승격과-수명주기)를 제출한다. 운영 배포 담당자는 검증한 동일 UUID를 선택한다.

```bash
miy --context work --app planner app accept --note '실제로 확인한 결과 요약' --report /path/to/acceptance.json
python3 scripts/official-apps.py promote --context work --catalog /path/to/catalog.json --app planner --release RELEASE_UUID --confirm-prod
```

권한이 다른 개발·운영 담당자가 인수와 승격을 나눠 수행할 수 있다. 회사에서 중지한 앱과 정지·제거된 설치는 기본 설치·업데이트로 되살리지 않는다. 확인 중에 상태가 바뀌면 새 상태를 읽고 다시 판단한다. 공식 릴리스의 DB 변경은 검증된 dev 릴리스 배포로 적용한다. 같은 스키마의 공유 dev DB 개발, 또는 `miy app dev --runtime docker --database local`을 사용할 수 있다. 직원 입장과 연결 승인은 [코어 권한 계약](CORE-PERMISSIONS.md#확정-권한-행렬)을 따른다.

첫 설치가 도중에 실패해 설치 행만 남았으면 `install` 재실행도 실패 상태를 보고한다. 작업 중이면 `inspect`에 표시된 `job watch`로 완료를 기다린다. 실패한 작업은 출력의 `inspect`로 원인을 확인하고, 재시도할 수 있으면 `reviewRetry`의 `--dry-run`으로 영향을 확인한다. 이 과정에서 도구가 배포나 재시작을 자동으로 요청하지 않는다.

공식 dev 배포가 릴리스 등록 후 실패하면 후임 개발자도 해당 앱의 dev 배포 권한으로 같은 이미지를 다시 사용할 수 있다. 해당 앱·dev 환경에 `read`와 `deploy`를 위임받은 에이전트도 자신의 CLI context에서 같은 절차를 따른다. `job get`의 `releaseId` 또는 설치 도구의 `attemptedReleaseId`를 확인한다. 다음 두 명령은 앱 소스와 최초 담당자의 로컬 빌드 영수증 없이 실행할 수 있다.

```bash
miy --context work --app planner app deploy --environment dev --release RELEASE_UUID --dry-run
miy --context work --app planner app deploy --environment dev --release RELEASE_UUID
```

설치 도구의 JSON은 UTF-8이다. CLI 빌드가 끝난 뒤 로컬 영수증 저장에 실패하면 오류의 `recoveryCommand`에 담긴 `job resume`으로 영수증을 복원한다. 자동화에서는 이 객체의 실행 파일과 인수 배열을 사용한다.

## 공용 컴포넌트 설치와 업데이트

```sh
miy component list
miy component show ui --version 1.0.1
miy component sync
miy component add COMPONENT --version X.Y.Z
miy component update COMPONENT --version X.Y.Z
miy component check
```

Sync는 고정 버전을 복원하며 자동 업그레이드하지 않는다. Add/update는 명시적 변경이다. `platform-components/`와 잠금을 앱 저장소에 포함하고 업무 확장은 `web/src` 등에 둔다. `@miy/components/NAME`으로 import하며 기본 UI 별칭은 `@miy/ui`다. 관리자 `component publish PATH`는 새 불변 패키지를 게시할 뿐 실행 앱을 갱신하지 않는다. 컴포넌트 변경도 검사·버전·커밋·빌드·배포를 거쳐 반영한다.

## 로컬 개발, 빌드와 dev 배포

| 방식 | 개발 PC에 필요한 도구 | 실행·DB 위치 |
| --- | --- | --- |
| 기본 `app dev` | Rust와 네이티브 링크 도구, Node, 프로젝트 지정 pnpm, Git. Windows는 MSVC·Windows SDK | 로컬 Rust·React preview, 서버의 해당 앱 dev DB. Docker Desktop·WSL·로컬 PostgreSQL 불필요 |
| `app dev --runtime docker` | 위 개발 도구와 Linux 컨테이너용 Docker | 로컬 컨테이너, 기본은 서버 dev DB. 별도 DB는 `--database local`을 명시 |
| `app build` | CLI·Git·서버 로그인, 서버의 `app.builds.v1` | 서버 Docker에서 검사·빌드. PC의 Docker·Rust·Node·SSH 불필요 |

서버는 **자신이 빌드한 이미지만 실행한다.** 로컬에서 배포용 이미지를 만들거나 이미지를 올리는 경로는 없다(CLI 0.11.0부터). 로컬 테스트는 `app dev`로 하고, 서버에 올릴 것은 커밋한 소스를 `app build`로 서버에서 빌드한다. 로컬 preview(native·Docker)의 결과물은 배포할 수 없다.

```sh
miy app fmt
miy app check
miy app dev --no-open
# 작업 종료 시 별도 터미널에서:
miy app dev-stop
# 변경된 앱 버전을 manifest에 반영하고 소스를 커밋한 다음:
miy app build --no-wait
miy job watch BUILD_JOB_UUID --timeout 1800
miy job logs BUILD_JOB_UUID --after 0
miy app deploy --environment dev --dry-run
miy app deploy --environment dev --no-wait
miy job watch DEPLOY_JOB_UUID --timeout 600
miy app status
```

Check는 프로젝트 Rust 테스트와 프런트 도구를 실행하고 의존성·빌드 산출물을 쓴다. 정적 검사 통과가 실제 브라우저·DB 인수를 대신하지 않는다. 기본 Dev는 작업 트리를 빌드하여 네이티브 백엔드·게이트웨이·DB 터널을 loopback에서 실행하고 서버의 해당 앱 개발 DB에 연결한다. 변경 파일을 자동 재빌드하는 HMR은 제공하지 않으므로 수정 후 중지·재실행한다. 같은 플랫폼 주소의 WS/WSS 터널을 사용하며 로컬 SQL에 운영 DB·관리자 권한을 제공하지 않는다. 앱당 한 개발 세션만 허용하며, `app dev-stop`으로 연결을 해제한 후 개발 배포하면 같은 DB와 데이터를 사용한다. 연결 실패 시 로컬 DB로 자동 전환하지 않는다. 별도 로컬 DB가 필요한 경우에만 `app dev --runtime docker --database local`을 명시한다. 종료는 소유 프로세스·컨테이너를 정리하고 DB·기존 로컬 볼륨은 보존한다. 스키마 변경은 서버가 적용하며 변경된 이전 SQL 이력을 거부한다. 준비/종료 대기는 성공 완료와 구분한다.

Docker runtime의 서버 DB 연결에는 컨테이너에서 접근 가능한 서버 IP·도메인이 필요하다. 이 조합에서는 `localhost`, 루프백·미지정 IP를 빌드·DB 요청 전에 거부한다. 기본 native runtime에는 이 Docker 주소 제한을 적용하지 않는다. PostgreSQL 쿼리 취소 패킷은 지원하지 않으며 연결 종료 또는 `app dev-stop`을 사용한다. 이미 진행 중인 장시간 서버 마이그레이션은 중지 요청·CLI 대기 시간과 별개로 완료를 기다릴 수 있다. 임시 접속 권한의 만료와 마이그레이션 작업 취소는 구분한다.

로컬 preview는 앱 하나의 `AppRoot`와 AppRuntime을 같은 loopback 문서에서 실행하며 원격 자격증명을 앱 코드에 전달하지 않는다. 원격 dev/prod의 설치별 출처 iframe 브리지를 재현하는 검사가 아니다. preview의 UI·API 성공과 원격 프레임의 DOM·자격증명·메시지 경계 인수를 구분한다.

`app build`는 깨끗이 커밋한 소스를 선택한 서버로 보내고, 서버의 Docker Engine이 서버 소유 빌드 절차로 검사·빌드한다. 잠긴 의존성만 받은 뒤 네트워크를 끊고 Rust 포맷 검사·테스트와 React 타입 검사·빌드를 실행하며, SDK·공용 컴포넌트 원본과 적용 SQL 이력을 확인한다. 이미지는 서버의 `runtimePlatform`용으로 만든다. 로컬 개발 세션을 종료하고 소스를 커밋한 뒤 실행한다. 기본은 완료까지 기다리고(`--timeout`, 기본 1800초) `--no-wait`는 접수한 빌드 작업을 바로 반환한다.

배포는 manifest·커밋·이미지·번들과 **서버 빌드 영수증**을 확인한다. 영수증이 없는 프로젝트(이전 CLI가 로컬에서 만든 이미지 포함)의 `app deploy`는 서버에 요청하기 전에 `server_build_required`로 끝나고 `miy app build`를 안내한다. 서버도 배포(dev)와 승격(prod) 접수·사전 점검·worker 실행에서 그 앱의 성공한 서버 빌드가 만든 이미지인지 다시 확인하며, 아니면 `server_build_required`로 거부한다.

빌드는 깨끗한 Git HEAD의 허용된 업무·SDK·migration·잠금 파일만 올린다. 인증 설정·`.git`·`.miy`·`.miy-tools`·빌드 산출물·`.env`·개인 키 파일을 제외하고 심볼릭 링크와 하위 저장소를 거부한다. 커밋했지만 이 범위 밖이라 보내지 않는 파일은 CLI가 올리기 전에 stderr로 이름을 알린다. 임의 업무 파일에 하드코딩한 비밀을 모두 찾는 기능은 아니다. 서버는 소스 크기·SHA-256·개발 권한·SDK/컴포넌트 원본·적용 SQL 이력을 검사한다. 빌드 자체는 개발 DB를 변경하지 않는다. 커밋 ID는 CLI가 제공한 출처이며 서명된 증명이 아니고, 실제 전송 바이트는 SHA-256으로 식별한다.

빌드 영수증은 서버·사용자·앱·커밋·소스 해시·불변 이미지에 묶인다. 연결이 끊기면 `job resume REQUEST_OR_JOB_UUID --environment dev`로 원래 작업을 조회한다. 같은 소스·커밋·플랫폼의 진행/성공 작업은 재사용한다. 실패한 빌드의 resume은 원래 실패를 반환하며, 원인을 해결하고 커밋한 뒤 `app build`를 명시적으로 다시 실행해야 새 시도를 요청한다.

Dry-run은 예상 상태·계획된 변경·worker에서 남은 검사를 반환하고 runtime job을 생성하지 않는다. 실제 실행은 context/origin/user/app, 요청 키, payload, expected-state와 서버 빌드 영수증을 먼저 journal에 기록한다. 이미지의 `/miy`(manifest·build.json·migrations·ui)는 디렉터리와 일반 파일만 허용하며 10,000개·파일당 64MiB·합계 256MiB를 넘거나 링크·장치·파이프가 있으면 `invalid_image_contract`로 거부한다. 배포의 migration은 기본 잠금 대기 30초로 시작하고 서버의 전체 한도(기본 10분, `MIY_MIGRATION_TIMEOUT_SECONDS`)를 넘으면 중단·롤백되어 `migration_failed`가 된다. runtime 요청의 접수 여부 조회부터 재제출까지가 복구 대상이다.

현행 runtime 명령은 `runtime.preflight.v2`를 요구한다. 이 계약은 실행 앱 상태에 더해 독립적인 DB 식별자와 스키마 변경 이력을 비교한다. 이전 CLI나 서버 조합은 capability 오류로 업그레이드를 안내하며, 알 수 없는 상태를 무시하고 배포하지 않는다.

빌드 큐가 가득 찬 경우처럼 서버가 접수를 거절했고 해당 요청에 서버 작업이 없다고 확인되면, 원인을 해결한 뒤 같은 커밋에서 `app build`를 다시 실행한다. 응답이 유실되거나 서버가 소스를 수신 중인 경우에는 `job resume`으로 원래 요청을 이어간다. CLI는 전송에 필요한 동안에만 소스 tar를 보관하고 서버의 수신 또는 확정 거절을 확인하면 이를 정리한다.

### 릴리스 소스 다운로드와 복구

`app.release-source.v1`을 제공하는 서버와 해당 명령이 있는 CLI를 사용한다. 서버는 성공한 빌드의 소스 tar를 앱·SHA-256별로 보관하며, 릴리스의 빌드 영수증으로 다운로드할 바이트를 찾는다. dev와 prod에서 같은 릴리스를 사용하는 경우 소스도 같다. 소스 접근은 해당 앱의 dev `develop` 권한과 위임되지 않은 로그인으로 제한된다.

```sh
miy --context CONTEXT --app APP --project /path/to/original/app app source upload --release RELEASE_ID
miy --context CONTEXT --app APP app source upload --release RELEASE_ID --archive original-build.tar
miy --context CONTEXT --app APP app source download --release RELEASE_ID --out release-source.tar
```

`--project`는 깨끗한 Git 작업본의 HEAD를 기존 빌드와 같은 방식으로 패키징한다. 생략하면 현재 앱 프로젝트를 사용한다. `--archive`에는 당시와 바이트가 같은 빌드 tar를 지정한다. 서버는 크기와 SHA-256이 영수증과 일치하는 복구만 받는다. `release_source_missing`이거나 저장된 tar의 크기 또는 해시가 달라 `release_source_bytes_invalid`가 발생하면 작업본이나 백업에서 원본을 올려 복구한다. 서버는 손상이 확인된 tar를 검증한 원본으로 원자 교체한다. 당시 빌드 영수증까지 없으면 `release_source_receipt_unavailable`이다. 수정한 소스는 `app build`와 새 릴리스로 배포한다. 다운로드는 서버 해시를 확인한 뒤 새 파일을 쓰므로 기존 파일은 보존된다.

```sh
miy job list --limit 20
miy job resume REQUEST_OR_JOB_UUID
miy --output jsonl job watch JOB_UUID --after EVENT_CURSOR
miy job get FAILED_JOB_UUID
miy job retry FAILED_JOB_UUID
```

응답 유실은 원래 요청을 먼저 조회한다. `job resume`만 조회 후 미접수인 원본 요청을 같은 키로 명시적으로 다시 보낼 수 있으며, 다시 보내기 전에 서버 빌드 영수증을 서버에서 재확인한다. 새 deploy를 반복해 불명확한 작업을 만들지 않는다. Watch의 Ctrl-C/timeout은 로컬 관찰만 끝내며 원격 작업을 취소하지 않는다. 터미널 runtime 실패는 타입이 허용한 transient 분류만 fresh preflight 후 `job retry`로 재시도한다. 빌드가 실패하면 runtime job이 없으므로 `job logs BUILD_JOB_UUID`로 원인을 확인하고 소스를 고쳐 커밋한 뒤 `app build`를 다시 실행한다.

상태의 release/image/version과 실제 배포 URL을 검사한다. 이전 릴리스가 실행 중이거나 job 접수만 성공한 상태를 새 배포 성공으로 보고하지 않는다. UI 변경 반영을 위해 플랫폼 코어를 재시작하거나 플랫폼 UI를 다시 빌드하지 않는다.

## 인수 기록, prod 승격과 수명주기

기존 운영 설치는 새 버전의 상태 검사가 통과한 뒤 활성 릴리스를 변경한다. 실패하면 관리자가 `app status`의 설치 `releaseId`와 마지막 작업의 `lastOperation.releaseId`를 비교해 마지막 정상 버전과 배포 시도를 구분할 수 있다. DB가 이전 버전과 호환된다고 확인되면 기존 컨테이너를 복구한다. 확인할 수 없으면 `recovery_required`로 중지한다. 이때는 실패 작업과 DB 변경 상태를 점검한 뒤 호환되는 릴리스를 dev에서 검수해 prod로 승격한다.

```sh
miy app accept --note '실제로 수행한 검사' --report acceptance.json
# 또는 strict {releaseId,note,checks}:
miy app accept --input @acceptance-input.json
miy --context work --app my-app app promote --environment prod --release RELEASE_UUID --dry-run
miy --context work --app my-app --yes app promote --environment prod --release RELEASE_UUID
miy --context work --app my-app --yes app restart --environment prod
miy --context work --app my-app --yes app remove --environment dev
miy --context work --app my-app --yes job retry FAILED_JOB_UUID --environment prod
```

[인수 기록 형식](../templates/rust-react/docs/ACCEPTANCE.md)은 정확한 dev release에 실제 scope/actor/result/evidence를 남긴다. 미검사는 이유와 `not_run`으로 기록한다. 사용자 증언과 runtime health가 업무 품질을 자동 검증한 결과는 아니다. 승격은 인수된 동일 이미지를 별도 prod 설치·DB에서 실행하며 dev 데이터를 복사하지 않는다.

Prod 변경과 모든 환경의 app remove는 `--context`, `--app`, `--environment`를 명시해야 한다. Dry-run도 대상을 명시하지만 확인은 생략한다. 실행·retry·미접수 원본 재전송은 대화형 확인 또는 `--yes`가 필요하고, 서버는 권한·릴리스·상태를 재검사한다. 이미 접수된 job의 조회·관찰은 새 실행 승인을 요구하지 않는다. App remove는 DB를 보존한다. Context remove와는 다른 작업이다.

```sh
miy --app my-app app logs --environment dev --tail 100 --since 2026-10-27T00:00:00Z
miy --app my-app app logs --environment dev --request LOG_REQUEST_UUID
miy app rename --name '관리 화면 표시 이름'
```

로그는 기본 100줄, 요청당 최대 1,000줄/1 MiB의 제한된 스냅샷이다. 해당 앱·환경의 관리 권한을 확인하고 만료되는 결과를 반환한다. Truncation과 회전에 따른 누락 가능성을 표시하며 연속 follow나 영구 보관을 제공하지 않는다. Timeout/중단 시 알려진 요청 ID로 같은 로그 결과를 다시 관찰한다. Rename은 관리자 표시 이름만 바꾸며 소스 manifest·앱 ID를 바꾸지 않는다.

## 자동화 계약

일반 명령은 `--output auto|text|json|jsonl`을 공유한다. Auto는 stdout TTY에서 text, 비TTY에서 JSON이다. JSON은 `schemaVersion:1,command,ok,data,error,meta`인 UTF-8 한 응답이다. 성공 값은 data, 오류는 code/message/retryable/details와 선택적 recovery 실행 파일·인수 배열이다. 진행·하위 도구 출력은 stderr로 보낸다. JSONL은 job watch의 이벤트 스트림만 지원한다. Help/version·completion·MCP는 고유 형식을 유지한다.

```sh
miy --output json --fields apps.id,apps.name,nextCursor app list --limit 20
miy app list --cursor NEXT_CURSOR
miy --app my-app grant set --input @grant.json
```

원격 업무 목록 cursor는 서버의 사용자·필터 범위에 묶인다. `context list`도 `--limit/--cursor`를 지원하고 설정 변경 시 첫 페이지부터 다시 읽는다. `component list`는 `{components,nextCursor}`를 반환한다. 컴포넌트 카탈로그는 서버에서 전체 조회한 뒤 CLI가 페이지를 나누므로 서버 전송량 제한은 아니다. Read 명령의 fields 선택으로 권한을 우회하지 않는다. `--input @file` 또는 `--input -`는 크기 제한과 unknown-field 거부를 적용하며 충돌하는 플래그와 함께 사용하지 않는다. Grant 입력은 `{userId,appId?,environment,permission}`, acceptance는 `{releaseId,note,checks}`다. JSON appId와 `--app`의 중복도 거부한다.

종료 코드는 0 성공/접수, 1 내부 오류, 2 인수/입력, 3 인증, 4 권한, 5 대상 없음, 6 충돌, 7 환경/검증, 8 네트워크/일시 장애/관찰 timeout, 9 접수된 원격 작업의 최종 실패, 130 로컬 중단이다. Error의 retryable은 새로운 job 생성 승인이 아니다. Recovery 인수 배열을 검토하여 원래 요청을 복구한다.

## 호환 도구 갱신과 이전 이름

호환 번들은 의도한 새 global CLI의 `miy app setup`, 다른 지원 번들은 깨끗한 커밋 후 `miy app upgrade`로 전환한다. 이전 번들 스냅샷을 사용하며 업무 파일과 적용 SQL 바이트를 보존한다. 변경 검토·검사·커밋 후 필요한 경우에만 재배포한다. 새 skill을 읽도록 에이전트 세션을 다시 연다. 관리 파일은 LF, SQL/컴포넌트는 원래 바이트 계약을 따른다.

구 `app login/logout/doctor/describe/components/mcp/registrations/approve/reject/users/grant`는 해당 루트 명령으로 이동했다. `app resume` 대신 `job resume ID`, `app control` 대신 start/stop/restart/remove를 쓴다. `app setup-codex`·`app codex`는 제거됐으며 app setup 후 일반 클라이언트를 실행한다. 제거된 명령은 조용히 다른 일을 하지 않고 usage 오류와 대체 명령을 반환한다. Deploy `--retry`·`--environment prod`도 사용할 수 없다.

## 현행 서버 구성: HTTPS 도메인과 사내 HTTP IP

빈 서버의 설치 명령은 **[새 서버 설치 안내](SETUP.md)** 한 곳에서 관리한다. Ubuntu 도구 설치·Git clone 이후의 PostgreSQL/Keycloak 초기화·첫 관리자·TLS·Core/worker·포털/프레임 빌드·Nginx/systemd·재부팅·첫 앱까지 순서대로 따른다. 기존 개발 호스트를 갱신할 때는 [서버 운영 안내](SERVER-OPERATIONS.md)를 사용한다. 기존 배포 도구를 새 서버 설치기로 실행하지 않는다.

새 사내 서버는 HTTPS이며 도메인 또는 단일 IP+포트 풀을 선택할 수 있다. IdP는 콘솔 `/identity/realms/miy`, CLI context의 대표 주소는 prod 포털이다. Core·Keycloak·PostgreSQL은 loopback에 두며 정확한 목적별 프록시로 연결한다. 사설 CA의 서버/브라우저/CLI 신뢰는 각각 설정하고 TLS 검증을 끄지 않는다. HTTP는 명시적 로컬/호환 검증용이며 새 서버 기본 경로가 아니다.

사설 CA를 배포 앱 컨테이너/SDK에 전달하는 기능은 아직 없어 앱 백엔드의 공통 API까지 쓸 때는 보완이 필요하다. 신규 서버에서 공통 기능을 바로 사용하는 경로는 공개 CA 도메인 배치이며, 사설 CA의 실제 앱 통신은 별도 인수 대상이다([SETUP](SETUP.md)).

`--state-dir`/`MIY_STATE_DIR`는 private 상태 경로, `--binary`/`MIY_BINARY`는 서버 실행 파일이다. 프레임 런타임의 `MIY_FRAME_RUNTIME_DIR`은 `<state>/frame-runtime`이며 포털 빌드와 별도로 설치한다. bootstrap 재실행은 기존 비밀을 재사용하지만 DB/Keycloak의 기존 암호를 바꾸는 도구가 아니다. 서버 빌드는 커밋된 소스를 받아 worker의 Docker에서 검사·빌드한다. Docker Engine과 CLI는 `network connect --gw-priority`를 지원하는 28 이상이어야 한다.

설치 절차·설정 검사·개발 도구의 PASS와 **별도 빈 서버의 전체 설치/재부팅 인수**를 구분한다. 후자는 [PLAN](PLAN.md#검증과-반영-대기)에서 추적하며, 새 서버 실행 결과를 받기 전에는 NOT_RUN이다.

## 공유 개발 서버의 계정 버튼

운영자가 공유 개발 로그인을 켜면 설정된 서버 주소의 로그인 화면에 `admin`, `developer`, `alice`, `bob` 버튼이 나타난다. 해당 서버에 접속한 사람은 비밀번호 입력 없이 선택한 개발 계정으로 로그인할 수 있다. 계정 비밀번호를 화면이나 문서에 공개하지 않으며 기존 OAuth 로그인도 계속 사용할 수 있다. 공유 개발 로그인은 새 서버에서 기본으로 꺼져 있고, 도메인이나 접속 위치만으로 활성화되지 않는다.

이미 **세 출처 계약으로 구성된 별도 개발 서버**에서만 같은 명시적 상태 경로에 적용한다. 이 checkout의 운영 `.local`에 bootstrap을 실행하지 않는다. 구 단일 출처 설정은 아래 명령과 serve 재시작만으로 전환할 수 없으며 [서버 전환 절차](SERVER-OPERATIONS.md#새-세-출처-설정과-전환-준비)를 따른다.

```sh
python3 scripts/bootstrap.py --state-dir /srv/miy-dev --shared-development-login
# 해제
python3 scripts/bootstrap.py --state-dir /srv/miy-dev --no-shared-development-login
```

각 변경 후에는 해당 개발 서버의 서비스 재시작 절차를 따른다. 초기 구성에는 정확한 `--origin`·`--console-origin`을 함께 지정한다. 환경변수 `MIY_SHARED_DEVELOPMENT_LOGIN=1` 또는 `0`으로도 설정할 수 있으며 명령줄 옵션이 우선한다. 선택은 `server.json`의 `sharedDevelopmentLogin`에 저장되어 같은 대표 주소로 재실행하면 유지된다. 대표 주소를 바꾸면 공유 로그인이 꺼지므로 새 주소에서도 사용할 때는 옵션을 다시 명시한다. 활성화 시 런타임에는 `MIY_DEV_LOGIN=shared`, `MIY_DEV_ORIGIN=MIY_ORIGIN`이 기록된다.

CLI에서는 평소처럼 `miy auth login`을 실행한다. 열린 브라우저에서 개발 계정 버튼을 누르고 CLI 승인 화면의 코드를 대조·승인하면 해당 계정으로 인증된다. 이후 `miy auth status`로 확인한다. 실제 OAuth·기업 SSO 인수는 해당 로그인 경로로 별도 확인한다.

## 로컬 플랫폼 보조 도구와 검증

저장소의 `bash scripts/up.sh --profile local`는 로컬 PostgreSQL·Keycloak·Caddy와 코어/worker/UI 검증 기반을 준비한다. 이 로컬 검증 프로필은 포털 `https://localhost:18443`, dev 포털 `https://localhost:18445`, 콘솔 `https://localhost:18444`를 제공하며 클라이언트의 서버 기본값과 관계없다. HTTP 검증에는 `--origin http://IP:18000 --allow-http --proxy-bind 0.0.0.0`을 명시한다. 외부 Nginx를 구성한 서버에서는 `--profile server --origin URL`을 사용하며 로컬 Caddy·브라우저 컨테이너는 시작하지 않는다. `bash scripts/down.sh`는 저장 데이터를 보존하며 해당 기반을 중지한다. 이 도구의 기존 환경 기록을 모든 OS의 지원 증거로 확대하지 않는다. `.local/credentials.json`과 `.local/runtime.env`는 로컬 비밀 설정이므로 출력·게시하지 않는다.

`up.sh`는 루트 빌드 뒤 프레임 런타임도 `<state>/frame-runtime`에 빌드한다. pnpm의 패키지 작업 경로와 혼동되지 않도록 `--state-dir`/`MIY_STATE_DIR`는 **절대 경로**를 사용한다. 이 개발 보조 도구는 TLS 반입·외부 Nginx·systemd 설치를 대신하지 않으므로 새 서버는 [SETUP](SETUP.md)을 따른다. 플랫폼 작업의 실제 프레임 검증에는 [검사 지도](CHECKS.md)의 격리 `stack-ui.sh`와 앱 프레임 브라우저 설정을 사용한다.

명시적으로 켠 loopback 개발 프로필은 설정된 포털/콘솔의 합성 계정 버튼과 `miy auth login --account developer`를 제공한다. 실제 OAuth 승인/기업 SSO 검증과 구분한다. 사설 CA의 CLI 설정과 브라우저 신뢰는 별개이며 TLS 검증을 끄지 않는다.

전체 검사와 격리 회귀의 준비·정리 경계는 [검사 지도](CHECKS.md) 및 [런타임 회귀 안내](../scripts/runtime-regression/README.md)를 따른다. 과거 실행은 [보관 결과](archive/README.md)에 소스·환경별로 남긴다. 이 안내를 갱신한 사실만으로 별도 Linux 서버·실제 Windows 11·독립 AI·실제 IdP 전파 검사를 완료한 것으로 기록하지 않는다.
