# V0~V1 로컬 실행과 검증

이 단계는 UI 모듈 통합·OAuth·앱 DB 준비를 검증한다. 앱 컨테이너 설치와 업무 앱 개발 평가는 후속 단계이며, 검증용 UI의 `runtimeStatus`는 `not_deployed`로 표시한다.

추가로 CLI와 실제 Codex 개발·Docker 배포를 연결한 [휴가 관리 대장 실증](LEAVE-LEDGER-RESULTS.md)을 제공한다. 기존 V0~V1 fixture와 실제 Docker 앱을 구분하며, 로컬에 배포한 휴가 앱은 `/apps/leave-ledger`에서 사용한다.

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

## 앱 CLI 개발·등록·배포 (bundle 0.3.0)

새 앱은 독립 폴더에 생성한다. CLI 실행 파일, SDK, 공통 UI, 개발 지침, 도구체인과 `miy.bundle.json`을 함께 제공한다. 개발 지침은 MCP `get_contract`와 같은 원본에서 제공한다. 이전 bundle 0.2.x 앱은 기존 CLI를 유지하며 자동으로 새 SDK로 바뀌지 않는다.

`app init`만으로 일반 Codex 세션에 MCP가 연결되지는 않는다. `app login`은 플랫폼 개발 세션을 만들며 MCP OAuth와 별개다. Rust 설치 경로가 터미널이나 Codex의 PATH에서 빠질 수도 있으므로 개발을 시작하기 전에 아래 환경 설정을 수행한다.

```sh
target/debug/miy app init ../miy-apps/my-app --id my-app --name '내 앱'
cd ../miy-apps/my-app
.miy-tools/miy app doctor
.miy-tools/miy app check
.miy-tools/miy app login --account developer
.miy-tools/miy app register
```

`register`는 신청 ID와 `pending`을 반환한다. 신청만으로 배포 권한을 주지 않는다. 현재 검증 계정 중 `developer`와 관리자만 신청 가능하다. 관리자는 다음과 같이 승인하며, 신청자에게 이 앱의 `dev/develop` 권한만 부여한다. 아래 로그인 전환은 로컬 검증 예시다. 실제 운영에서는 관리자가 자신의 계정으로 승인해야 한다.

```sh
.miy-tools/miy app login --account admin
.miy-tools/miy app registrations
.miy-tools/miy app approve REQUEST_ID
.miy-tools/miy app users
.miy-tools/miy app grant --user USER_ID --permission use --environment dev
.miy-tools/miy app login --account developer
```

거절은 `app reject REQUEST_ID`, 권한 회수는 `app grant --user USER_ID --permission use --revoke`다. 승인·거절은 같은 결정을 반복해도 결과가 같으며, 결정 후 반대 결정으로 변경할 수 없다. 승인 후 접근 변경은 grant로 처리한다. 일반 사용자는 신청·승인·사용자 목록 조회를 할 수 없다.

Codex에는 앱 폴더, 해당 폴더의 지침·SDK와 MCP 연결을 제공한다. `cargo fmt --package miy-app`으로 앱만 포맷하고 `app check`를 실행한다. check는 보호된 번들 파일의 SHA-256, migration 이름·중복 번호·명시적 트랜잭션 금지, Rust 포맷·테스트, frozen 의존성 설치, TypeScript, UI 빌드·Federation 진입점과 자산을 검사한다. 실제 DB SQL 실행과 브라우저 동작까지 검사하는 명령은 아니다. 보호 파일을 수정하여 검사를 우회하면 안 된다.

로컬 플랫폼의 CA를 지정하여 프로젝트 전용 Codex 설정을 만든다. 공인 인증서를 사용하는 서버에서는 `--ca-file`을 생략할 수 있다. 설정과 CA 묶음은 Git에서 제외된 `.miy/`에 저장하며 전역 Codex 설정·모델·시스템 인증서 저장소는 변경하지 않는다. 로컬 검증 realm의 OAuth client는 `miy-codex`, callback은 `http://127.0.0.1:18555/callback`이다.

```sh
.miy-tools/miy app setup-codex \
  --url https://localhost:18443/mcp \
  --ca-file ~/projects/miy-v2/.local/caddy/caddy/pki/authorities/local/root.crt

# MCP 로그인이 없거나 만료된 경우에만 실행
.miy-tools/miy app codex -- mcp login miy_app

# 새 대화 또는 기존 대화 재개
.miy-tools/miy app codex
.miy-tools/miy app codex -- resume SESSION_ID
```

이 실행 명령은 `miy_app` HTTP MCP·OAuth 설정, 인증서 묶음과 Rust PATH를 해당 Codex 프로세스에 전달한다. 모델과 기존 다른 MCP 설정을 덮어쓰지 않으며, miy MCP 초기화에 실패하면 필수 연결 오류를 표시한다. **그냥 `codex resume`으로 실행하면 이 프로젝트 실행기의 설정이 적용되지 않는다.** HTTP MCP 설정·OAuth client/callback은 [공식 Codex MCP 문서](https://learn.chatgpt.com/docs/extend/mcp)를 따른다. 현재 로컬 개발 CA는 Codex 프로세스용 신뢰 묶음에 추가되며 일반 브라우저 인증서 신뢰 설정과는 별개다.

새 CLI는 `app check/build`에서 PATH에 cargo가 없어도 `CARGO_HOME/bin` 또는 `~/.cargo/bin`을 찾는다. 직접 shell에서 `cargo`를 실행하려면 `export PATH="$HOME/.cargo/bin:$PATH"`를 사용한다. `app doctor`의 MCP `configured`는 설정 파일 존재를 의미하며 OAuth 로그인 성공을 보장하지 않는다. 기존 bundle 0.3.0 앱에 이 명령이 없다면 동일 bundle을 제공하는 최신 CLI 실행 파일만 교체한다. SDK·보호 파일을 수동 수정하지 않는다.

```sh
git init -b main
git add .
git commit -m 'feat: implement app'
.miy-tools/miy app build
.miy-tools/miy app deploy --no-wait
.miy-tools/miy app resume
.miy-tools/miy app status
.miy-tools/miy app control stop
.miy-tools/miy app control start
```

build는 전체 check 후 한 Docker 이미지를 만들고 소스 커밋과 이미지 ID를 기록한다. deploy는 소스가 변경되거나 manifest 버전이 달라진 오래된 빌드를 거부한다. 서버는 이미지 안의 계약·UI 자산·migration을 먼저 확인한 뒤 기존 컨테이너 중지 → DB 준비·migration → 새 컨테이너 시작을 수행한다. migration 실패는 DB와 앱 상태에 표시하고 앱을 정지 상태로 둔다. 자동으로 이전 버전을 복구하지 않는다.

**같은 버전으로 다른 이미지를 배포할 수 없다.** 수정 후에는 `manifest.json`의 `version`을 `0.1.0` → `0.1.1`처럼 올리고 커밋·build·deploy를 다시 실행한다. 버전을 그대로 두면 `immutable_release_conflict`로 새 배포가 실패하고 기존 배포가 유지된다. `--retry`로 버전 충돌을 해결할 수는 없다. `app status`의 `version`, `localBuild.matchesRunningImage`, `lastOperation.status/errorCode/hint`를 함께 확인한다. `runtimeStatus: running`만으로 방금 만든 이미지가 배포됐다고 판단하면 안 된다.

CLI는 요청을 보내기 전에 멱등성 키를 `.miy/jobs/`에 저장한다. 응답 유실·CLI 종료 후 같은 명령을 다시 실행하면 진행 중인 같은 요청을 이어받는다. `app resume`은 마지막 제출 작업을 재조회한다. 실패 작업은 자동 재제출하지 않으며 같은 입력을 다시 실행하려면 `--retry`를 명시한다. 성공 후 새 명령 실행은 새 작업이다. stdout은 결과 JSON, 하위 빌드 출력과 오류 JSON은 stderr에 출력한다.

CLI 로그인은 **localhost 개발 프로필 전용**이다. 정식 CLI OAuth, 원격 레지스트리·서버 전송, 자동 롤백은 아직 구현하지 않았다. 현재 build와 runner는 같은 Docker Engine의 이미지 저장소를 사용한다.

개발 과정 검증 스크립트는 `node scripts/check-developer-workflow.mjs`다. 새 템플릿 3개와 검증 앱 하나를 생성하고 CLI 등록·승인·권한 검사를 수행한다. `.local/workflow-app.json`에 생성 경로를 기록한다. 실제 Codex를 호출하는 `scripts/develop-app.mjs APP_PATH PROMPT_FILE EVIDENCE_LABEL`은 읽기 전용 MCP와 외부 OS 정책을 사용한다. 평가자는 Codex 완료 후 `node scripts/check-workflow-deployment.mjs v1`, 스키마 변경 개발 후 `v2`, 의도적 migration 실패·새 릴리스 복구 검사에는 `failure`를 실행한다. 실패 주입은 이 스크립트가 만든 합성 검증 앱에만 사용한다. 이 검사는 기본 `check.sh`와 별도로 실행한다.

## 중지와 재실행

```sh
bash scripts/down.sh
bash scripts/up.sh
```

중지는 DB 볼륨·개발 자산·자격증명을 보존한다. `up.sh`는 저장된 자격증명을 재사용하며 사용자 DB를 초기화하지 않는다. UI 검증은 매 실행마다 새 앱 ID와 설치를 사용해 플랫폼의 사전 하드코딩에 의존하지 않는다.

직접 Rust 검사를 실행하려면 `.local/runtime.env`를 현재 프로세스에 읽어야 한다. DB 장애 검사에는 실제 기반 서비스가 필요하며 일반 DB 실행기가 중지된 상태에서만 실행한다. 평상시 `cargo test`에는 해당 검사가 명시적으로 `ignored`로 표시되고, 전체 검사 스크립트가 이를 따로 실행한다.

## 현재 계약의 범위

- 기존 V0~V1 릴리스는 해시로 검증한 정적 UI·manifest·SQL fixture다. 추가한 앱 CLI는 같은 로컬 Docker Engine의 불변 이미지 ID로 실제 앱을 배포한다. 원격 이미지 전송은 아직 지원하지 않는다.
- Docker 앱의 backend API는 권한·CSRF·릴리스 검사와 서명된 사용자 정보를 통해 프록시한다. 기존 미배포 fixture에서 SDK의 `api` 호출은 `app_runtime_not_deployed`로 거부한다.
- migration은 트랜잭션 가능한 SQL만 지원한다. 적용한 파일의 변경·알 수 없는 이력·비트랜잭션 표시는 거부한다.
- 통합 UI는 신뢰된 코드다. 브라우저 보안 격리를 제공하는 것으로 해석하지 않는다.
- Keycloak의 `resource-indicators`는 실험 기능이다. 이번 호환성 결과를 기업 SSO·다른 클라이언트 전체의 지원으로 확대하지 않는다.
- 기동·검사 스크립트의 Python과 브라우저 검증 도구는 개발 보조 도구다. 플랫폼 코어와 앱 백엔드의 실행 프로필은 Rust다.
