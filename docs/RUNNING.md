# CLI 0.6과 독립 앱 개발·운영 실행 안내

이 문서는 CLI 0.6.0 / 개발 번들 0.7.0 작업 트리의 사용 계약을 설명한다. 설치된 실행 파일의 `miy --version`, `miy describe`와 서버 capability가 실제 사용 가능한 범위를 결정한다. 명령 구현과 해당 환경의 검증 통과를 구분한다. 실행 결과는 [구현 결과](IMPLEMENTATION-RESULTS.md), 미실행·외부 환경 항목은 [검증 행렬](CLI-VALIDATION-MATRIX.md)과 [Windows 안내](WINDOWS-VALIDATION.md)를 따른다. CLI 완료가 V7 전체 인수를 뜻하지 않는다.

## 설치와 발견

소스 빌드에는 Rust가 필요하지만, 배포받은 클라이언트로 원격 운영만 수행할 때는 Rust·Node·Docker가 필요하지 않다. 앱 개발에는 해당 도구가 별도로 필요하다. 사용자 전용 설치 경로를 사용하고 시스템 전역 PATH·인코딩·TLS 신뢰 설정을 임의로 변경하지 않는다.

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

소스 설치는 `bash scripts/install-cli.sh`를 사용한다. Windows 클라이언트의 네이티브 빌드·설치·PowerShell 인코딩과 검증 범위는 Windows 안내를 따른다. 프로젝트 실행 파일은 Unix에서 `.miy-tools/miy`, Windows에서 `.miy-tools/miy.exe`다.

플랫폼의 **CLI 설치** 화면(`/cli/install`)은 서버 `MIY_ORIGIN`에서 읽은 주소를 Unix의 `--server` 또는 Windows의 `-Server`에 넣은 명령을 제공한다. 검증한 배포물을 풀어 그 디렉터리에서 명령을 실행한다. 공통 실행 파일에 특정 운영 도메인을 내장하지 않으므로 같은 CLI를 다른 서버에도 사용할 수 있다. 설치 안내 API는 `GET /api/v1/cli/install`이며 요청의 Host나 브라우저 주소를 서버 설정 대신 사용하지 않는다.

CLI에는 기본 IP·도메인이 없다. 설치 명령의 서버 인자 또는 사용자가 직접 입력한 URL만 저장한다. 주소를 생략한 대화형 설치는 주소 입력을 요청하고, 입력 취소·비대화형 설치는 실행 파일만 설치한 뒤 서버 미등록 상태와 `miy context init`을 안내한다. 설치 후 해당 명령을 실행하거나 서버 미등록 상태에서 대화형 로그인을 시작하면 주소를 입력할 수 있다. URL에는 `http://` 또는 `https://`를 명시한다. 기존 설정과 자격증명은 보존하며, 전달한 주소가 기존 활성 연결과 다르면 다른 서버 추가·선택 절차를 안내한다. context 이름 `default`는 저장 이름이며 미리 정해진 서버 주소를 뜻하지 않는다. 이전 CLI 0.5 후보에는 이 흐름이 포함되어 있지 않다.

`describe` 기본 결과는 짧은 루트 목록이다. `--command`는 선택한 명령의 인수·enum·필수값·기본값·충돌·스키마·권한·부작용·오류·예제를, `--all`은 전체 참조를 반환한다. 도움말과 셸 완성은 같은 Clap 트리에서 생성한다.

```sh
miy completion bash
miy completion zsh
miy completion fish
miy completion powershell
```

## 서버 context와 인증

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

설치기는 `miy --output json context init --url URL`을 사용한다. 이 명령은 네트워크 접속 없이 명시된 URL을 검증하고 사용자 설정이 없는 경우에만 연결을 생성한다. `--url`을 생략하면 대화형 터미널에서 주소를 받으며 비대화형 실행은 등록 필요 오류를 반환한다. `--context`, `MIY_CONTEXT`, 프로젝트 binding은 초기화 대상에 영향을 주지 않는다. 기존 파일이 잘못된 형식이면 덮어쓰지 않고 실패하며, 설치기는 실행 파일 설치와 연결 초기화 실패를 구분해 알린다. 사설 CA가 필요한 수동 초기화에는 `context init --url URL --ca-file PATH`를 사용할 수 있다.

로그인 요청은 5분 동안 유효하다. 공개 출력은 URL·userCode·login ID뿐이며 device secret·access/refresh token은 출력하지 않는다. 비TTY, `--no-input`, `--no-wait` 로그인은 pending을 즉시 반환한다. 대화형 기본은 브라우저를 열고 기다릴 수 있고, `--device-code`·`--no-open`은 자동 브라우저 실행을 생략한다. `auth wait`는 서버의 poll 간격과 slow-down을 따르며 Ctrl-C 시 남은 요청을 보존한다.

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

선택한 context가 있으면 init이 프로젝트에 그 연결 이름을 기록한다. context 없이 로컬 프로젝트를 생성할 수 있지만 원격 작업 전에는 context를 추가·선택하고 로그인해야 한다. 등록 요청만으로 권한이 생기지 않는다. 관리자의 승인은 요청자에게 dev/develop 권한만 부여하며 prod/deploy는 별도다.

`doctor --for develop`는 manifest·번들·CLI 지침·컴포넌트 잠금과 Rust/Node/pnpm/Git/Docker 버전, Docker Linux 모드, 엔진/서버 아키텍처를 검사한다. 필수 검사 실패는 exit 7과 개별 결과로 반환한다. 새 앱은 로그인 후 component sync를 먼저 해야 컴포넌트 검사를 통과할 수 있다.

생성기는 SDK·지침·스킬과 프로젝트 Codex/Claude MCP 설정을 제공한다. 호환 CLI로 `app setup`을 실행하면 프로젝트 실행 파일·설정·CLI 지침을 함께 갱신한다. 일반 `codex` 또는 `claude`로 앱 폴더를 열고 각 클라이언트의 프로젝트 신뢰/MCP 승인을 정상적으로 진행한다. 전역 모델·권한 설정을 우회하지 않는다. 루트 `miy mcp` stdio 서버는 선택한 CLI context/session으로 계약과 허용된 API·컴포넌트 목록을 읽는다. API 사용 지침과 실제 배포 실행을 구분한다.

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

```sh
miy app fmt
miy app check
miy app dev --no-open
# 작업 종료 시 별도 터미널에서:
miy app dev-stop
# 변경된 앱 버전을 manifest에 반영하고 소스를 커밋한 다음:
miy app build
# Docker 엔진 CPU와 서버 CPU가 다르면 동작하는 빌더/에뮬레이션을 준비하고 명시:
miy app build --platform linux/amd64
miy app deploy --environment dev --dry-run
miy app deploy --environment dev --no-wait
miy job get JOB_UUID
miy job watch JOB_UUID
miy app status
```

Check는 프로젝트 Rust 테스트와 프런트 도구를 실행하고 의존성·빌드 산출물을 쓴다. 정적 검사 통과가 실제 브라우저·DB 인수를 대신하지 않는다. Dev는 작업 트리를 빌드하여 로컬 백엔드·게이트웨이를 실행하고 서버의 해당 앱 개발 DB에 연결한다. 같은 플랫폼 주소의 WS/WSS 터널을 사용하며 로컬 SQL에 운영 DB·관리자 권한을 제공하지 않는다. 앱당 한 개발 세션만 허용하며, `app dev-stop`으로 연결을 해제한 후 개발 배포하면 같은 DB와 데이터를 사용한다. 연결 실패 시 로컬 DB로 자동 전환하지 않는다. 별도 로컬 DB가 필요한 경우에만 `app dev --database local`을 명시한다. 종료는 소유 프로세스·컨테이너를 정리하고 DB·기존 로컬 볼륨은 보존한다. 스키마 변경은 서버가 적용하며 변경된 이전 SQL 이력을 거부한다. 준비/종료 대기는 성공 완료와 구분한다.

서버 개발 DB 연결에는 Docker에서 접근 가능한 서버 IP·도메인이 필요하다. `localhost`, 루프백·미지정 IP는 컨테이너 자신의 주소이므로 기본 서버 DB 모드에서 빌드·DB 요청 전에 거부한다. 로컬 플랫폼 fixture의 별도 로컬 DB 검증에는 `--database local`을 명시한다. PostgreSQL 쿼리 취소 패킷은 지원하지 않으며 연결 종료 또는 `app dev-stop`을 사용한다. 이미 진행 중인 장시간 서버 마이그레이션은 중지 요청·CLI 대기 시간과 별개로 완료를 기다릴 수 있다. 임시 접속 권한의 만료와 마이그레이션 작업 취소는 구분한다.

Build는 깨끗이 커밋한 소스로 불변 Linux 이미지를 만든다. 배포는 manifest·커밋·이미지·번들 영수증을 확인하며 이미지 platform이 서버 `runtimePlatform`과 같아야 한다. 다른 CPU의 빌드에는 명시적 `--platform`과 실제로 동작하는 빌더가 필요하다. 플래그만으로 에뮬레이션을 설치하지 않는다.

Dry-run은 예상 상태·계획된 변경·worker에서 남은 검사를 반환한다. 이미지 업로드나 runtime job은 생성하지 않는다. 실제 실행은 context/origin/user/app, 요청 키, payload와 expected-state를 먼저 journal에 기록한다. 이미지 archive 상한은 1 GiB다. 업로드 접수·ID 조회부터 runtime 제출까지 복구 대상이며, 전송 바이트의 일부부터 이어 보내는 기능은 제공하지 않는다.

CLI 0.6의 runtime 명령은 `runtime.preflight.v2`를 요구한다. 이 계약은 실행 앱 상태에 더해 독립적인 DB 식별자와 스키마 변경 이력을 비교한다. 이전 CLI나 서버 조합은 capability 오류로 업그레이드를 안내하며, 알 수 없는 상태를 무시하고 배포하지 않는다.

```sh
miy job list --limit 20
miy job resume REQUEST_OR_JOB_UUID
miy --output jsonl job watch JOB_UUID --after EVENT_CURSOR
miy job get FAILED_JOB_UUID
miy job retry FAILED_JOB_UUID
```

응답 유실은 원래 요청을 먼저 조회한다. `job resume`만 조회 후 미접수인 원본 archive/payload를 같은 키로 명시적으로 다시 보낼 수 있다. 새 deploy를 반복해 불명확한 작업을 만들지 않는다. Watch의 Ctrl-C/timeout은 로컬 관찰만 끝내며 원격 작업을 취소하지 않는다. 터미널 runtime 실패는 타입이 허용한 transient 분류만 fresh preflight 후 `job retry`로 재시도한다. Runtime job이 생기기 전 업로드 자체가 terminal 실패한 경우, 원인을 확인하고 새로운 `app deploy`를 명시적으로 실행한다.

상태의 release/image/version과 실제 배포 URL을 검사한다. 이전 릴리스가 실행 중이거나 job 접수만 성공한 상태를 새 배포 성공으로 보고하지 않는다. UI 변경 반영을 위해 플랫폼 코어를 재시작하거나 플랫폼 UI를 다시 빌드하지 않는다.

## 인수 기록, prod 승격과 수명주기

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

로그는 제한된 스냅샷이다. Truncation과 회전에 따른 누락 가능성을 표시하며 영구 보관을 약속하지 않는다. Timeout/중단 시 알려진 요청 ID로 같은 로그 결과를 다시 관찰한다. Rename은 관리자 표시 이름만 바꾸며 소스 manifest·앱 ID를 바꾸지 않는다.

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

## 서버 구성: HTTPS 도메인과 사내 HTTP IP

서버 대표 주소는 운영자가 `MIY_ORIGIN`으로 명시한다. 서버 구성 스크립트의 `--profile server`는 첫 구성 시 주소가 없으면 실패한다. 기존 구성 재실행은 저장된 주소·포트·자격증명을 유지한다. 이 서버 설정은 홈페이지 설치 명령에 인자로 전달되며 클라이언트 실행 파일의 기본 주소가 되지 않는다.

서버의 Docker Engine과 CLI는 `network connect --gw-priority`를 지원하는 28 이상을 사용한다. 앱별 네트워크를 PostgreSQL에 연결할 때 기존 기본 경로보다 낮은 우선순위를 주어 코어의 열린 DB 연결을 유지한다. 지원하지 않는 엔진에서는 우선순위 없이 연결을 재시도하지 않는다. 이 옵션은 [Docker 28 릴리스](https://docs.docker.com/engine/release-notes/28/)에 추가되었다. 기존 네트워크 연결은 자동으로 끊거나 다시 구성하지 않는다.

```sh
python3 scripts/bootstrap.py --profile server \
  --origin https://v2.dwdcc.kr --bind 127.0.0.1:18180 \
  --identity-port 18181 --postgres-port 15432
docker compose --env-file .local/compose.env up -d postgres keycloak
python3 scripts/configure-identity.py
cargo build --release --locked
pnpm install --frozen-lockfile
pnpm --filter @miy/platform build
MIY_BINARY="$PWD/target/release/miy" bash scripts/run-service.sh migrate
MIY_BINARY="$PWD/target/release/miy" bash scripts/run-service.sh seed
python3 scripts/render-server-config.py --binary "$PWD/target/release/miy" \
  --certificate /etc/lego/production/certificates/dwdcc.kr.crt \
  --key /etc/lego/production/certificates/dwdcc.kr.key
```

마지막 명령은 `.local/nginx-miy.conf`, `.local/miy-v2-serve.service`, `.local/miy-v2-worker.service`를 생성한다. 기존 Nginx 사이트를 수정하거나 서비스를 시작하지 않는다. 생성 파일을 검토하여 전용 사이트·unit으로 설치하고, `nginx -t` 통과 후 Nginx reload 및 두 unit 시작을 수행한다. `/identity`는 loopback Keycloak 포트, API·MCP·WebSocket·다운로드는 core bind 주소, 나머지는 빌드한 플랫폼 UI로 연결된다. 이미지 업로드 한도는 core와 동일한 1 GiB이며 WebSocket 전달과 스트리밍 응답을 지원한다. 서비스 wrapper는 private runtime.env를 읽고 serve에서 DB 관리자 연결을, worker에서 OIDC client secret을 제거한다.

다른 사내 서버를 인증서 없이 구성할 때에는 주소와 HTTP 허용을 명시한다.

```sh
python3 scripts/bootstrap.py --profile server \
  --origin http://192.168.0.10:8080 --allow-http \
  --bind 127.0.0.1:18180 --identity-port 18181 --postgres-port 15432
python3 scripts/render-server-config.py --binary "$PWD/target/release/miy"
```

HTTP는 내부망용이며 통신이 암호화되지 않는다. 이 구성은 `MIY_ALLOW_HTTP=1`을 기록하고 CA·TLS 리다이렉트·HSTS를 요구하지 않는다. 일반 로그인·권한·CSRF 검사는 유지한다. HTTPS 구성은 시스템 신뢰 저장소를 사용하며 사설 CA는 `--ca-file` 또는 `MIY_CA_FILE`로 추가한다. TLS 검증을 끄거나 HTTPS 실패를 HTTP로 대체하지 않는다. Keycloak 대표 주소와 MIY realm SSL 정책도 함께 적용하며 master realm은 변경하지 않는다. 관리 API 접근은 loopback으로 제한한다.

`MIY_STATE_DIR`/`--state-dir`는 private 상태 위치, `MIY_BINARY`/`--binary`는 서버 실행 파일을 지정한다. `MIY_CLI_RELEASE_DIR`는 검증된 CLI 다운로드 묶음을 제공할 위치이며 기본은 상태 디렉터리의 `cli-downloads`다. PostgreSQL과 Keycloak 호스트 포트는 loopback에만 공개한다. `credentials.json`, runtime/compose env는 0600이고 상태 디렉터리는 0700이다. 컨테이너에 개별 읽기 전용 mount하는 init.sql/realm.json은 컨테이너 UID가 읽을 수 있도록 0644이며 호스트에서는 상태 디렉터리의 접근 제한을 받는다.

## 로컬 플랫폼 보조 도구와 검증

저장소의 `bash scripts/up.sh --profile local`는 로컬 PostgreSQL·Keycloak·Caddy와 코어/worker/UI 검증 기반을 준비한다. 이 로컬 검증 프로필만 `https://localhost:18443`을 제공하며 클라이언트의 서버 기본값과 관계없다. HTTP 검증에는 `--origin http://IP:18000 --allow-http --proxy-bind 0.0.0.0`을 명시한다. 외부 Nginx를 구성한 서버에서는 `--profile server --origin URL`을 사용하며 로컬 Caddy·브라우저 컨테이너는 시작하지 않는다. `bash scripts/down.sh`는 저장 데이터를 보존하며 해당 기반을 중지한다. 이 도구의 기존 환경 기록을 모든 OS의 지원 증거로 확대하지 않는다. `.local/credentials.json`과 `.local/runtime.env`는 로컬 비밀 설정이므로 출력·게시하지 않는다.

명시적으로 켠 loopback 개발 프로필은 `http://localhost:18000`의 합성 계정 버튼과 `miy auth login --account developer`를 제공한다. 실제 OAuth 승인/기업 SSO 검증과 구분한다. 사설 CA의 CLI 설정과 브라우저 신뢰는 별개이며 TLS 검증을 끄지 않는다.

전체 검사와 격리 회귀의 준비·정리 경계는 [검증 행렬](CLI-VALIDATION-MATRIX.md) 및 [런타임 회귀 안내](../scripts/runtime-regression/README.md)를 따른다. 과거 [휴가 관리 실증](LEAVE-LEDGER-RESULTS.md)·[구현 결과](IMPLEMENTATION-RESULTS.md)는 해당 시점의 증거다. 이 안내를 갱신한 사실만으로 별도 Linux 서버·실제 Windows 11·독립 AI·실제 IdP 전파 검사를 완료한 것으로 기록하지 않는다.
