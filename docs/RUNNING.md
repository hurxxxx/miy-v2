# 로컬 플랫폼과 독립 앱 개발·배포 실행 안내

현재 CLI는 0.4.2, 새 개발 번들은 0.6.1이다. 로컬 앱 실행, HTTP 이미지 업로드, 테스트 배포, 승인한 릴리스의 운영 승격을 제공한다. V0~V1 검증용 UI fixture의 `runtimeStatus: not_deployed`는 실제 Docker 앱과 구분한다.

추가로 CLI와 실제 Codex 개발·Docker 배포를 연결한 [휴가 관리 대장 실증](LEAVE-LEDGER-RESULTS.md)을 제공한다. 기존 V0~V1 fixture와 실제 Docker 앱을 구분한다.

2026-10-22 사용자 요청으로 기존 사용자 앱과 검증용 fixture를 정리했다. 이후 사용자가 새로 만든 `my-cli-test`는 메모 0.1.1에서 할 일을 추가한 0.2.0으로 dev 업데이트됐고 prod 목록은 비어 있다. [독립 Codex 세션 재검토](IMPLEMENTATION-RESULTS.md#독립-codex-세션-재검토-2026-10-22)와 [할 일 추가 검토](IMPLEMENTATION-RESULTS.md#할-일-추가-요청과-업데이트-배포-검토)에 실행 상태와 보완점을 기록했다. 휴가·계산기 등 정리 전 앱 주소는 다시 등록·배포하기 전에는 사용할 수 없다. 인수 검사를 다시 실행하면 검증용 앱이 생성된다.

## 준비

현재 지원·검증 환경은 Apple Silicon macOS와 Docker Desktop의 Linux 엔진이다. Rust 1.99.1, pnpm 10.34.5, Python 3, Docker Compose, Codex CLI 0.163.0을 사용한다. 프런트 의존성은 `pnpm-lock.yaml`, Rust 의존성은 `Cargo.lock`, 기반 이미지는 Compose의 digest로 고정한다.

Rust는 rustup으로 설치하고 `~/.cargo/bin`을 사용한다. 최초 실행에는 패키지·이미지를 내려받을 네트워크가 필요하다. `18000`은 개발 화면, `18443`은 OAuth·MCP용 HTTPS 진입점, `15432`는 로컬 PostgreSQL, `18080`은 Rust 코어, `19222`는 검증용 브라우저 연결에 사용한다.

```sh
bash scripts/up.sh
```

PostgreSQL·Keycloak·Caddy와 CA를 준비하고, Rust 빌드·코어 migration·검증 계정·정적 UI 빌드·DB 실행기·검증 브라우저를 기동한다. 최초 앱 등록은 인수 검사에서 수행한다. 코어·DB 실행기는 이 단계에서 호스트의 Rust 프로세스이며 Compose 기반 서비스와 연결한다.

자격증명은 최초 실행 시 생성한 `.local/credentials.json`, 서버 환경은 `.local/runtime.env`에 보관한다. `.local`은 Git에서 제외한다. `admin`, `developer`, `alice`, `bob`은 합성 검증 계정이다. 초기화 SQL과 realm에는 이 로컬 설정의 자격증명만 사용한다.

기동 도구는 코어 프로세스의 환경에서 DB 생성 자격증명을 제거하고, DB 실행기 환경에서 OAuth 클라이언트 비밀값을 제거한다. 로컬의 동일 OS 계정에서 실행하는 프로세스 분리를 제품의 OS 계정·컨테이너 격리 검증으로 해석하지 않는다.

## 브라우저에서 개발 화면 열기

[http://localhost:18000](http://localhost:18000)에 접속해 **관리자로 시작**을 누르면 바로 로그인한다. 비밀번호 입력이나 인증서 설정은 필요 없다. **개발자**, **사용자 A**, **사용자 B** 버튼으로 권한이 다른 계정도 확인할 수 있으며, 로그아웃 후 다른 계정을 선택한다.

로컬 기동 스크립트가 `MIY_DEV_LOGIN=1`과 `MIY_DEV_ORIGIN=http://localhost:18000`을 설정한다. 이 기능은 두 서비스 주소가 모두 localhost 또는 loopback이고 명시적으로 활성화한 경우에만 동작한다. 설정이 없으면 계정 버튼과 개발 로그인 API는 비활성화된다. 세션은 서버 DB에 저장하고 기존 앱·환경 권한 검사를 그대로 적용한다.

실제 OAuth 로그인은 화면의 **OAuth로 로그인** 또는 `https://localhost:18443/auth/login`을 사용한다. 일반 브라우저에서 이 HTTPS 경로를 사용하려면 `.local/caddy/caddy/pki/authorities/local/root.crt`를 신뢰해야 한다. 자동 OAuth·MCP 검증은 별도 브라우저 컨테이너의 신뢰 저장소에 CA를 등록한다. 호스트의 시스템 신뢰 설정을 자동 변경하거나 TLS 검증을 끄지 않는다. 개발 계정 버튼의 성공은 실제 OAuth·MCP 인증 검증 결과에 포함하지 않는다.

## 전체 검사

기존 Codex의 모델 공급자 인증이 연결되어 있어야 한다. 플랫폼의 MCP OAuth는 별도로 검증 계정으로 로그인한다. 모델 공급자 계정의 로그인·로그아웃 설정은 바꾸지 않는다.

```sh
bash scripts/check.sh
```

검사는 Rust 단위 검사·프런트 타입 검사, Playwright 통합 인수, 실제 DB 격리·migration·실행기 강제 종료, 실제 Codex MCP 호출과 갱신, 독립 OAuth·MCP 클라이언트 검사 순서다. DB 장애 검사 중에는 일반 실행기를 중지하고 검사 전용 실행기를 사용하며 종료 후 일반 실행기를 복구한다.

Codex 검사는 명령의 프로세스별 설정으로 `miy_validation` 서버만 구성하고 읽기 전용 `get_contract`만 허용한다. 로그인·호출은 실제 Codex 클라이언트를 사용하며 별도 개발 에이전트를 제작하지 않는다. 로컬 CA는 기존 CA에 추가한 검사 전용 묶음으로 제공한다. 개발 계정의 명시적인 `offline_access` grant와 scope로 Codex 갱신을 검사한다.

실행 증거는 `.local/evidence/`에 저장한다. `runtime.json`, `database.json`, `codex.json`, `mcp.json`을 모아 `summary.json`을 생성하며, 빠진 결과는 성공으로 처리하지 않는다. 실행 로그와 Playwright trace는 로컬에만 두고 외부에 게시하지 않는다.

## CLI 설치

저장소에서 다음 한 명령으로 사용자 전용 디렉터리에 설치한다. sudo와 셸 설정 변경은 하지 않는다. 소스 설치에는 rustup이 필요하며, 개발 자체에는 Rust·pnpm·Docker가 필요하다.

```sh
bash scripts/install-cli.sh
export PATH="$HOME/.local/bin:$PATH"  # PATH에 아직 없는 경우 현재 터미널에 적용
miy --version
```

이미 배포받은 실행 파일이 있으면 `bash scripts/install-cli.sh --from /path/to/miy`로 Rust 컴파일 없이 설치한다. `--prefix /custom/bin`도 지원한다. 현재 GitHub Release에서 OS별 바이너리를 자동 배포하는 서비스는 제공하지 않는다. 소스 설치와 기존 바이너리 설치를 각각 실제로 검사했다. macOS ARM64에서 검증했으며 다른 OS/CPU의 설치 검증은 별도다.

## 새 앱 개발 환경 (bundle 0.6.1)

```sh
miy app init ../miy-apps/my-app --id my-app --name '내 앱' --url http://localhost:18000
cd ../miy-apps/my-app
.miy-tools/miy app doctor
.miy-tools/miy app login
.miy-tools/miy app components sync
.miy-tools/miy app register
```

`app login`은 브라우저를 열고, 웹에서 로그인한 사용자가 **터미널과 같은 요청 ID**를 확인해 승인하도록 한다. 실제 플랫폼 로그인과 연결된 8시간 유효 CLI 자격증명을 `.miy/session.json`에 권한 600으로 저장한다. 로그인 요청은 5분 뒤 만료되고 verifier 검증 후 한 번만 교환된다. `app logout`으로 CLI 자격증명을 즉시 철회한다. 이 방식은 플랫폼의 브라우저 승인 흐름이며 RFC 8628 기기 인증 구현을 의미하지 않는다. 로컬 계정 버튼을 쓰려면 `app login --account developer --url http://localhost:18000`도 가능하다. 이 단축 경로는 명시적으로 활성화한 loopback 개발 프로필 전용이다.

공인 HTTPS 플랫폼은 `--url https://PLATFORM`으로 지정한다. 사설 CA는 init/login의 `--ca-file /path/to/root.crt`로 지정하며 TLS 검증을 끄지 않는다. CLI에 CA를 지정하는 것과 브라우저가 해당 CA를 신뢰하는 것은 별개다. 로컬 개발에서는 HTTP 18000을 사용하면 인증서 설정 없이 시작할 수 있다.

앱 생성 시 SDK·공통 컴포넌트의 기본 선언·AGENTS.md·CLAUDE.md·개발 스킬·CLI 지침과 프로젝트 MCP 설정을 함께 만든다. Codex에는 `.codex/config.toml`, Claude Code에는 `.mcp.json`을 제공한다. 일반 `codex` 또는 `claude`로 앱 폴더를 열면 된다. **첫 실행의 프로젝트 신뢰/MCP 승인 절차는 해당 클라이언트에서 정상적으로 승인해야 한다.** 생성기는 이를 자동 우회하거나 전역 모델·권한 설정을 변경하지 않는다. 경로가 바뀌거나 Git에서 앱을 다시 받은 경우 호환 CLI로 `miy app setup`을 실행한다. 새 체크아웃에는 무시된 CLI 바이너리도 설치한다.

두 클라이언트는 `.miy-tools/miy app mcp` stdio 서버에서 설치된 SDK 번들의 계약과 로그인된 플랫폼의 현재 API 목록을 읽는다. 최신 서버 계약을 기존 앱의 SDK 계약으로 오인하지 않도록 설치 번들·플랫폼 번들 버전을 구분한다. `get_cli_help`와 `app describe`는 설치된 CLI에서 명령 설명을 직접 생성한다. MCP는 계약 조회와 검증만 제공하며 배포는 명시적인 CLI 작업으로 수행한다. Codex용 `.agents/skills/miy-app`과 Claude Code용 `.claude/skills/miy-app`은 같은 개발 흐름을 안내한다. 지침의 작성·형식 검증을 완료했으며, 이번 구조 검증에서 새로운 LLM 업무 앱 생성까지 반복한 것은 아니다.

프로젝트 MCP와 스킬 구성은 [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp), [Codex skills](https://learn.chatgpt.com/docs/build-skills), [Claude Code MCP](https://code.claude.com/docs/en/mcp), [Claude Code skills](https://code.claude.com/docs/en/skills)의 공식 안내를 참고했다.

직접 cargo가 PATH에 없어도 `app fmt`, `app check`, `app build`가 rustup 설치 경로를 찾는다. `.miy-tools/miy app codex` 실행기는 필요한 경우 부모 Codex 프로세스에 Rust PATH도 전달한다. legacy `setup-codex`는 직접 HTTP OAuth MCP를 사용하는 이전 번들의 별도 경로다. 새 앱은 위 프로젝트 stdio MCP와 CLI 로그인을 사용한다.

등록 신청만으로 권한이 생기지는 않는다. 관리자가 웹의 **플랫폼 관리** 또는 `app approve REQUEST_ID`로 승인하면 신청자에게 해당 앱의 `dev/develop` 권한이 부여된다. 현재 로컬 개발자는 사전 준비된 developer 계정이다. 기업 SSO 사용자 신규 등록·수명주기 전체를 제공하는 제품 단계는 아니다.

## 공용 컴포넌트 설치와 업데이트

이 기능을 처음 사용하려면 현재 CLI 0.4.2를 한 번 설치한다. 이후 개별 컴포넌트 추가·갱신 때문에 CLI를 다시 설치하지 않는다. 기존 앱의 기반 번들은 아래 `app upgrade` 절차로 명시적으로 전환한다.

컴포넌트 구현 코드는 CLI에 내장하지 않는다. 플랫폼의 인증된 `/api/v1/components` 저장소가 버전·소스·사용 예제·호환 정보를 제공한다. 플랫폼의 `Button`·`Panel`과 배포용 `ui 1.0.0`은 `registry/components/ui/1.0.0/`의 같은 원본을 사용한다. 새 컴포넌트를 게시할 때 CLI 업데이트나 플랫폼 재시작이 필요하지 않다. 플랫폼 자체 화면은 자신의 빌드에서 정한 UI 버전을 사용하며 공용 UI 게시만으로 바뀌지 않는다.

```sh
.miy-tools/miy app components list
.miy-tools/miy app components show ui --version 1.0.1
.miy-tools/miy app components sync
.miy-tools/miy app components add COMPONENT --version X.Y.Z
.miy-tools/miy app components update COMPONENT --version X.Y.Z
.miy-tools/miy app components check
```

새 템플릿의 `miy.components.json`은 기본 `ui` 버전을 선언한다. 로그인 후 `sync`가 처음 설치하며, 이미 설치된 경우에는 잠금 파일의 정확한 버전을 복원한다. `sync`는 새 버전을 자동 선택하지 않는다. `add`/`update`에서 버전을 생략하면 현재 SDK 번들과 호환되는 가장 높은 안정 버전을 선택한다. 현재 버전 형식은 `숫자.숫자.숫자`이며 prerelease와 버전 범위는 지원하지 않는다.

설치 코드는 `platform-components/NAME/`에 놓고, 잠금 파일은 `platform-components/miy.components.lock.json`이다. 플랫폼 주소, 컴포넌트 버전, 패키지 SHA-256, 파일별 해시·호환 정보를 기록한다. 이 디렉터리 전체를 앱 Git에 커밋한다. 이후 검사·빌드는 컴포넌트 다운로드 없이 가능하다. Cargo·프런트의 외부 의존성 캐시까지 자동 준비한다는 뜻은 아니다.

```tsx
import { Button, Panel } from "@miy/ui";
// 추가 컴포넌트는 해당 패키지 README에 나온 export 사용
// import { SomeComponent } from "@miy/components/COMPONENT";
```

컴포넌트 파일을 직접 편집하거나 사용자 파일을 섞으면 갱신을 거부한다. 수정이 필요하면 업무 코드에서 감싸거나 플랫폼 관리자가 새 버전을 게시한다. 누락 파일은 `sync`로 복원한다. 같은 프로젝트에서 동시 설치는 거부하고 디렉터리 교체 중단 흔적은 다음 설치에서 복구한다. 다른 플랫폼 주소의 패키지를 기존 잠금에 조용히 섞지 않는다.

MCP `list_components`와 `get_component`는 같은 저장소의 최신 목록·버전별 README·설치 명령을 제공한다. 설치나 배포를 실행하지 않는다. 에이전트 지침에도 이 조회·설치 흐름을 포함했다.

플랫폼 관리자는 자신의 CLI 로그인으로 다음을 실행한다. 컴포넌트 게시 권한은 앱 개발·운영 배포 권한과 별개인 플랫폼 관리자 권한이다.

```sh
miy app components publish /path/to/component-directory
```

작성 형식은 `registry/components/ui/1.0.0/component.json`과 소스 파일을 참고한다. `protocol`, `name`, `version`, `description`, `uiContract`, `bundleVersions`, 명시적인 `files` 목록이 필요하다. `index.ts` 또는 `index.tsx`, `README.md`를 포함한다. 같은 이름·버전의 다른 내용은 거부하며 동일 내용 재전송은 허용한다. 최초 기동의 seed만 저장소에 기본 UI를 게시하고, 이후 추가 게시에는 API를 사용한다.

현재 패키지는 UTF-8 TS/TSX·CSS·SVG·JSON·Markdown 소스이며 최대 2 MiB, 파일당 512 KiB·128개다. 설치 hook, 임의 셸 명령, 외부 의존성 자동 설치, 컴포넌트 간 의존성 해석은 제공하지 않는다. 필요한 추가 의존성이 있으면 검토해 앱의 기존 의존성 관리로 추가한다. 게시된 코드는 신뢰된 코드로 취급하며 해시 검사가 악성 코드 심사를 대체하지 않는다. 별도 서명 키·폐쇄망 패키지 미러는 후속 범위다.

앱의 컴포넌트를 갱신해도 실행 중인 앱은 바뀌지 않는다. 앱 버전 증가 → 커밋 → 검사·빌드 → 테스트 배포 → 운영 승격을 그대로 따른다. SDK 기반 계약 자체의 변경은 `app upgrade` 대상이며, 개별 컴포넌트 릴리스와 구분한다. 공통 로그인 컴포넌트와 사용자별 로컬 로그인은 이번 배포 체계 분리와 별도로 아직 구현하지 않았다.

## 로컬에서 직접 테스트

등록 승인과 로그인 후 실행한다.

```sh
.miy-tools/miy app fmt
.miy-tools/miy app check
.miy-tools/miy app dev --port 18100
```

`app dev`는 커밋 전 작업 트리도 빌드한다. 앱 폴더별 로컬 PostgreSQL과 Rust 컨테이너를 만들고 React 미리보기를 연다. 데이터베이스 소유·migration·실행 역할을 나누며 플랫폼 앱 DB에는 직접 연결하지 않는다. 표시된 미리보기 URL을 이용한다. 접근 토큰·플랫폼 세션은 로컬 Rust 게이트웨이가 보관하고, 브라우저에는 전달하지 않는다. 서명된 사용자 정보로 backend API를 호출하며 요청 시 CLI 사용자의 인증 상태를 다시 확인한다.

공통 사용자 목록은 `platform:directory:read` 선언 후 `runtime.platformApi("directory")`, 다른 앱은 `app:TARGET:read` 또는 `app:TARGET:write` 선언 후 `runtime.appApi(TARGET,"/path",options)`로 호출한다. 서버는 상대 앱 사용 권한도 검사하고, 상대 backend가 레코드 소유권 등 업무 권한을 판단한다. 로컬 미리보기는 서버 **dev** 환경만 연결한다. 범용 데이터소스·파일 저장소 전체가 구현됐다는 의미는 아니다.

미리보기는 빌드 후 실행 방식이며 HMR은 아니다. 변경 후 Ctrl-C로 종료하고 다시 실행한다. Ctrl-C는 게이트웨이와 컨테이너를 종료하며 로컬 DB 볼륨을 보존한다. `app dev-stop`도 해당 프로젝트의 게이트웨이·컨테이너·포트·잠금을 함께 정리한다. 빌드·migration 진행 중에는 현재 작업 완료 후 종료하며, 45초 내 완료하지 못하면 성공 대신 종료 대기로 알린다. 구버전 실행기는 먼저 해당 터미널에서 Ctrl-C로 종료해야 한다. 같은 프로젝트의 동시 실행은 거부한다. 다른 앱은 다른 `--port`로 실행한다.

## 테스트 배포와 운영 승격

```sh
git init -b main  # 아직 저장소가 없는 새 앱에서만
git add .
git commit -m 'feat: implement app'
.miy-tools/miy app build
.miy-tools/miy app deploy
.miy-tools/miy app status
```

build는 전체 검사 후 한 Docker 이미지에 Rust·React·SQL·manifest·소스 커밋을 묶는다. deploy는 archive를 인증된 HTTP API로 업로드하고 SHA-256·단일 이미지·앱 식별자를 검증한 뒤 worker가 import한다. CLI와 서버가 같은 Docker 이미지 저장소를 쓸 필요가 없다. archive 한도는 1 GiB이며 현재 빌드 이미지와 서버 Docker의 CPU 아키텍처가 맞아야 한다. 별도 Linux 서버·서로 다른 CPU·대용량 전송 재개까지 검증한 것은 아니다.

앱마다 별도 Docker 네트워크를 만들고 DB 서비스만 연결한다. 컨테이너는 비루트, 읽기 전용 파일시스템, capability 제거, CPU·메모리·PID 제한을 적용한다. 기존 0.2/0.3 배포 컨테이너의 네트워크는 자동 교체하지 않는다. 다음 재배포부터 새 네트워크를 사용한다. 인터넷 egress 정책과 자원 예약·총량 스케줄러는 후속 범위다.

테스트 URL은 `/environments/dev/apps/APP_ID`다. 브라우저/API로 실제 확인한 뒤 다음을 실행한다.

```sh
.miy-tools/miy app accept --note '직접 확인한 화면·API·데이터 보존 검사'
.miy-tools/miy app promote --release RETURNED_RELEASE_ID
.miy-tools/miy app status
```

accept는 현재 정상 실행 중인 dev 릴리스에 사용자 테스트 확인을 기록한다. 자동 업무 검사를 대신하지 않는다. promote는 **그 릴리스의 동일 이미지**를 별도 prod 컨테이너·DB에 배포한다. dev 데이터를 복사하지 않는다. 운영 주소는 `/environments/prod/apps/APP_ID`다. 기존 `/apps/APP_ID` 주소는 dev 별칭으로 유지한다.

운영 승격과 운영 시작·중지·제거에는 관리자 또는 해당 앱의 **prod/deploy** 권한이 필요하다. prod/develop은 운영 배포 권한이 아니다. 요청 시와 worker 실행 시 권한을 모두 검사한다. 일반 deploy나 이전 fixture 설치 API로 운영 승격을 우회할 수 없다.

```sh
# 관리자가 자신의 CLI 세션으로 실행
miy app users
miy app grant --user USER_ID --permission deploy --environment prod
miy app grant --user USER_ID --permission use --environment prod
```

관리자는 웹 `/admin`에서 등록 승인/거절, 앱·환경 권한 부여/회수, 전체 앱의 시작/중지/재시작/컨테이너 제거를 수행한다. CLI도 `app control stop --environment prod` 같은 명령을 제공한다. 제거는 컨테이너만 제거하고 DB를 보존한다.

새 버전은 manifest version 증가 → 커밋 → build → dev deploy → 실제 테스트 → accept → promote 순서다. 같은 버전의 다른 이미지는 거부하고 이전 배포를 유지한다. DB migration 실패 후 자동 롤백은 하지 않는다. 적용한 SQL은 수정하지 말고 새 릴리스로 전진 복구한다. `app status`의 활성 버전, `localBuild.matchesRunningImage`, 마지막 작업을 함께 확인한다. `running`만으로 새 배포의 성공을 판단하지 않는다.

`--no-wait`와 `app resume`은 제출한 배포 작업을 이어받는다. 이미지 archive의 부분 전송 재개는 지원하지 않는다. 실패 원인을 해결한 뒤에만 명시적 `--retry`를 사용한다. 배포·업데이트 때문에 플랫폼 서버를 재시작하거나 플랫폼 UI를 다시 빌드하지 않는다.

## 기존 0.3·0.4 앱 개발 번들 전환

0.3·0.4 앱은 기존 컨테이너와 동봉된 CLI로 계속 동작한다. 새 개발 기능을 사용하려면 최신 CLI로 앱 폴더 안에서 실행한다.

```sh
miy app login  # 기존 연결이 유효하면 생략
miy app upgrade
.miy-tools/miy app check
git diff
```

깨끗한 Git 작업 트리와 알려진 0.3 또는 0.4 보호 파일 해시, 플랫폼 CLI 로그인이 필요하다. 기본 UI 패키지를 먼저 다운로드·검증한 후 0.6으로 전환한다. SDK·개발 지침·미리보기 도구를 교체하며 이전 파일은 `.miy/upgrade-backup-*`에 보관한다. 업무 Rust/React와 기존 migration은 보존한다. 검토 후 manifest 버전을 올려 커밋·배포한다. 0.2 번들의 자동 전환은 제공하지 않는다. 기존 사용자 앱을 자동 전환하지 않는다. `platform-components/`는 새로 설치하고 업무 Rust/React와 SQL은 그대로 보존한다. 커스텀 Vite 설정이 자동 변환 조건과 다르면 변경 전에 중단한다.

## 구조 검증 실행

`node scripts/check-operations.mjs`는 현재 CLI로 별도 임시 앱의 preview 종료·재실행, DB 보존, 실제 브라우저 이동 보호, 표시 이름 권한, 구조화된 인수 기록과 dev 배포를 검사한 뒤 등록·설치·DB를 정리한다. 플랫폼·worker·호스트 Chrome·검증 브라우저가 실행 중이어야 한다. 0.5 → 0.6 전환 검사도 하려면 `MIY_PREVIOUS_CLI=/path/to/bundle-0.5/miy`를 지정한다. 기존 사용자 앱을 전환하지 않고 그 실행 파일로 별도의 템플릿을 생성한다. 결과는 `.local/evidence/operations/`에 보관한다.

`node scripts/check-components.mjs`는 별도 합성 앱·컴포넌트를 만들고 게시·설치·업데이트·Docker 배포를 실제 수행한다. CLI 파일 해시·core PID·플랫폼 UI 해시·기존 앱 릴리스가 변하지 않는지 확인하고 0.3/0.4 앱의 복제 저장소에서 전환을 검사한다. 기존 실증 앱 `my-local-test`, `arch-mvix06lk-ui`가 있는 로컬 검증 환경을 전제로 한다. 결과는 `.local/evidence/component-registry/`에 보관한다. OAuth 세션 철회 검사와 웹 세션 갱신 검사는 같은 계정을 쓰므로 순차 실행한다.

`scripts/check-architecture.mjs`는 새 합성 앱을 만들고 별도 작업공간에 커밋·배포한다. 실제 사용자 앱에 실패를 주입하지 않는다. 한 실행 세트는 `prepare`, `auth`, `deploy`, `preview`, `promote`, `update`, `boundaries`, `auth-https` 단계다. `auth` 전 최초 Codex 프로젝트 신뢰 승인이 필요하다. 단계들은 `.local/evidence/structural-audit/apps.json`을 공유하므로 순서를 지켜 실행한다. 기존 증거가 있으면 prepare는 덮어쓰지 않는다. preview 단계는 호스트 Chrome, 플랫폼 화면 검사는 전용 검증 브라우저를 사용한다.

배포 이후 `node scripts/check-architecture.mjs routes`로 정확한 환경 URL과 잘못된 환경의 차단을 별도 검사할 수 있다.

`develop-app.mjs APP_PATH PROMPT_FILE LABEL`은 선택적인 Codex 실행 기록기다. 실제 에이전트 호출을 요청할 때만 사용한다. 기본 모델·인증·sandbox·승인 정책을 그대로 사용하며 `--dry-run`으로 연결 준비만 검사할 수 있다. 예전 평가 전용 OAuth·OS sandbox 조합을 일반 사용자 개발 경로로 강제하지 않는다.

## 중지와 재실행

```sh
bash scripts/down.sh
bash scripts/up.sh
```

중지는 DB 볼륨·개발 자산·자격증명을 보존한다. `up.sh`는 저장된 자격증명을 재사용하며 사용자 DB를 초기화하지 않는다. UI 검증은 매 실행마다 새 앱 ID와 설치를 사용해 플랫폼의 사전 하드코딩에 의존하지 않는다.

직접 Rust 검사를 실행하려면 `.local/runtime.env`를 현재 프로세스에 읽어야 한다. DB 장애 검사에는 실제 기반 서비스가 필요하며 일반 DB 실행기가 중지된 상태에서만 실행한다. 평상시 `cargo test`에는 해당 검사가 명시적으로 `ignored`로 표시되고, 전체 검사 스크립트가 이를 따로 실행한다.

## 현재 계약의 범위

- 기존 V0~V1 릴리스는 해시로 검증한 정적 UI·manifest·SQL fixture다. 현재 앱 CLI는 인증된 archive 업로드 후 불변 Docker 이미지 ID로 배포한다. 서버 캐시에 이미지가 없는 조건에서 전송을 검사했으며, 별도 서버 실증은 아직 하지 않았다.
- Docker 앱의 backend API는 권한·CSRF·릴리스 검사와 서명된 사용자 정보를 통해 프록시한다. 기존 미배포 fixture에서 SDK의 `api` 호출은 `app_runtime_not_deployed`로 거부한다.
- migration은 트랜잭션 가능한 SQL만 지원한다. 적용한 파일의 변경·알 수 없는 이력·비트랜잭션 표시는 거부한다.
- 통합 UI는 신뢰된 코드다. 브라우저 보안 격리를 제공하는 것으로 해석하지 않는다.
- Keycloak의 `resource-indicators`는 실험 기능이다. 이번 호환성 결과를 기업 SSO·다른 클라이언트 전체의 지원으로 확대하지 않는다.
- 기동·검사 스크립트의 Python과 브라우저 검증 도구는 개발 보조 도구다. 플랫폼 코어와 앱 백엔드의 실행 프로필은 Rust다.

## CLI 0.4 / bundle 0.6 운영 보완

기존 앱은 자동 변경하지 않는다. SDK 버전 변경이므로 기존 0.5 앱에서 적용하려면:

```sh
.miy-tools/miy app components update ui --version 1.0.1
# 다른 공용 컴포넌트도 bundle 0.6 호환 버전인지 확인
git add .
git commit -m 'chore: prepare compatible components'
miy app upgrade       # 새로 설치한 전역 CLI 사용
.miy-tools/miy app check
```

upgrade는 깨끗한 Git 상태와 보호 파일·컴포넌트 호환성을 검사하고, 의존성 잠금을 사전 계산한 뒤 새 SDK·지침·프로젝트 CLI를 설치한다. 업무 코드와 기존 migration은 보존한다. 변경 검토·커밋·앱 버전 증가·빌드·재배포는 별도다. 구버전 실행 중 preview는 먼저 Ctrl-C로 닫는다.

미저장 입력은 앱이 `runtime.registerNavigationGuard`에 상태를 등록하면 플랫폼 메뉴·앱 이동·뒤로/앞으로·새로고침에서 확인한다. 로컬 preview도 같은 계약을 지원한다. 기존 앱에 이 등록을 자동 삽입하지 않으며, 브라우저 새로고침 경고는 브라우저 정책을 따른다. 등록 예제는 [번들 계약](../templates/rust-react/docs/CONTRACT.md#navigation-and-application-lifecycle)을 참고한다.

플랫폼 표시 이름은 최초 등록 후 관리자가 `miy app rename --name '표시 이름'`으로 관리한다. manifest 이름은 릴리스 정보로 유지하고 배포 때 플랫폼 이름을 덮어쓰지 않는다. `app status`가 두 이름과 차이를 보여준다.

`miy app accept --note '실제 검사 요약' --report acceptance.json`은 정확한 releaseId와 항목별 scope/actor/result/evidence를 기록한다. authenticated recorder는 서버가 정한다. result는 passed 또는 not_run이며 미검증 이유를 적는다. 실패 항목이나 다른 릴리스의 보고서는 승인할 수 없다. 기존 note-only CLI는 계속 지원하되 unstructured로 구분한다. 서버가 업무 기능의 정확성이나 증거 파일을 자동 검증하는 것은 아니다. 형식과 체크리스트는 [ACCEPTANCE.md](../templates/rust-react/docs/ACCEPTANCE.md)를 따른다.

공용 ui 1.0.1은 0.5·0.6 둘 다 지원한다. ui 1.0.0의 소스·메타데이터는 변경하지 않았으며 기존 앱에서 계속 사용할 수 있다. 신규 프로젝트 기본값은 1.0.1이다.

## CLI 사용법 스킬과 매뉴얼 동기화

CLI 0.4.1부터 `app init`은 다음 파일을 함께 설치한다. CLI 0.4.2 / bundle 0.6.1은 `AGENTS.md`와 `CLAUDE.md`에서 해당 스킬·매뉴얼의 경로와 갱신 절차를 직접 안내한다. 따라서 MCP 연결·로그인 전에도 에이전트가 로컬 CLI 사용법을 찾을 수 있다.

| 생성 파일 | 역할 |
| --- | --- |
| `.agents/skills/miy-cli/SKILL.md` | Codex의 CLI 운영 스킬 |
| `.claude/skills/miy-cli/SKILL.md` | 같은 원본에서 생성한 Claude Code 스킬 |
| 각 스킬의 `references/workflow.md` | 초기화·로그인·로컬 실행·등록·컴포넌트·배포·승격·종료·업데이트 절차 |
| 각 스킬의 `references/commands.md` | 실제 명령 정의에서 자동 생성한 전체 명령·옵션·기본값 |
| `docs/CLI-REFERENCE.md` | 같은 작업 절차와 명령 참조를 담은 사용자용 매뉴얼 |
| `miy.cli.json` | CLI 버전·SDK 번들 버전·관리 파일별 해시 |

`miy-app` 스킬은 업무 앱 구현과 SDK 계약을 계속 담당한다. `docs/CLI.md`는 기존 SDK 번들에 포함된 빠른 안내로 보존한다. 설치된 CLI의 전체 사용법은 새 `miy-cli` 스킬·`docs/CLI-REFERENCE.md`가 기준이다.

명령과 옵션은 Rust Clap 정의에서 생성하므로 새 명령이 추가되면 JSON 도움말·Markdown 매뉴얼·양쪽 클라이언트 스킬의 참조가 함께 바뀐다. 작업 순서와 주의할 동작은 단일 [스킬 원본](../crates/miy/guidance/miy-cli/SKILL.md) 및 [workflow 원본](../crates/miy/guidance/miy-cli/references/workflow.md)에서 관리한다. `app describe`와 MCP `get_cli_help`도 같은 CLI 버전의 정보를 제공한다. 사람용 전체 출력은 다음과 같다.

```sh
.miy-tools/miy app describe --format markdown
```

전역 CLI를 설치·업데이트한 뒤 **같은 SDK 번들을 사용하는 프로젝트 안에서** 다음을 실행한다.

```sh
miy app setup
.miy-tools/miy --version
.miy-tools/miy app check
```

setup은 프로젝트 실행 파일·CLI 스킬·매뉴얼·MCP 설정을 함께 갱신한다. SDK나 업무 코드를 교체하지 않고, 변경 중인 업무 파일도 보존한다. 새 안내 파일과 miy.cli.json은 앱 저장소에 커밋한다. 실행 파일과 인증 정보는 기존과 같이 Git에서 제외한다. 에이전트가 예전 지침/MCP 프로세스를 계속 쓰지 않도록 갱신 후 에이전트 세션을 다시 연다.

SDK 번들이 다르면 `app setup`이 버전 불일치로 중단한다. 위의 `app upgrade` 절차를 먼저 따른다. `app upgrade`도 CLI·스킬·매뉴얼을 함께 갱신하며, SDK가 이미 현재 버전이어도 CLI 갱신을 생략하지 않는다. 전역 CLI 설치만으로 모든 기존 프로젝트를 자동 변경하지 않는다.

0.6.0 → 0.6.1은 에이전트 진입 안내만 바꾸는 문서 패치다. 작업 내용을 커밋한 뒤 새 전역 CLI로 `miy app upgrade`를 실행한다. 이 경로는 로그인·컴포넌트 다운로드·의존성 재해석 없이 두 안내 파일·번들 잠금과 CLI 도구/안내를 갱신하며 업무 코드·SDK 실행 코드·마이그레이션·기존 공용 컴포넌트를 보존한다. 0.6.0을 지원하는 컴포넌트는 0.6.1에서도 호환되며, 기존 배포본의 소스·메타데이터는 변경하지 않는다.

`app check`는 안내 파일이 실행 중인 CLI와 일치하는지 검사하고 `app doctor`도 일치 상태를 표시한다. 관리 파일을 직접 고친 경우 setup이 덮어쓰지 않고 알려주며, 누락된 관리 파일은 setup으로 복구한다. 프로젝트 고유 운영 지침은 별도 파일에 작성한다. 이 잠금은 버전·편집 충돌을 찾기 위한 것으로 악성 앱 코드를 격리하는 장치는 아니다.

검증 명령은 `node scripts/check-cli-guidance.mjs`다. 기본 실행은 서버 등록·배포 없이 초기화·도움말·MCP·무결성·복구를 검사한다. 같은 SDK를 사용하는 구버전 실행 파일을 `MIY_PREVIOUS_CLI=/path/to/miy`로 지정하면 업데이트 보존 검사를 추가한다. `MIY_PREVIOUS_BUNDLE_CLI=/path/to/miy`에 bundle 0.6.0의 CLI를 지정하면 문서 패치 업그레이드·보존·충돌 거부를 검사한다. `MIY_GUIDANCE_LIVE_CHECK=1`은 실행 중인 로컬 플랫폼의 개발 로그인·컴포넌트 설치와 전체 app check까지 수행한다. 사용자 앱을 수정하거나 배포하지 않는다.
