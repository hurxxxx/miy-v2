# CLI 0.5와 독립 앱 개발·운영 실행 안내

이 문서는 CLI 0.5.0 / 개발 번들 0.6.2 작업 트리의 사용 계약을 설명한다. 설치된 실행 파일의 `miy --version`, `miy describe`와 서버 capability가 실제 사용 가능한 범위를 결정한다. 명령 구현과 해당 환경의 검증 통과를 구분한다. 실행 결과는 [구현 결과](IMPLEMENTATION-RESULTS.md), 미실행·외부 환경 항목은 [검증 행렬](CLI-VALIDATION-MATRIX.md)과 [Windows 안내](WINDOWS-VALIDATION.md)를 따른다. CLI 완료가 V7 전체 인수를 뜻하지 않는다.

## 설치와 발견

소스 빌드에는 Rust가 필요하지만, 배포받은 클라이언트로 원격 운영만 수행할 때는 Rust·Node·Docker가 필요하지 않다. 앱 개발에는 해당 도구가 별도로 필요하다. 사용자 전용 설치 경로를 사용하고 시스템 전역 PATH·인코딩·TLS 신뢰 설정을 임의로 변경하지 않는다.

```sh
bash scripts/install-cli.sh --from /path/to/miy
# PATH에 없으면 현재 셸에만 추가
export PATH="$HOME/.local/bin:$PATH"
miy --version
miy describe
miy describe --search deploy
miy describe --command "app deploy"
```

소스 설치는 `bash scripts/install-cli.sh`를 사용한다. Windows 클라이언트의 네이티브 빌드·설치·PowerShell 인코딩과 검증 범위는 Windows 안내를 따른다. 프로젝트 실행 파일은 Unix에서 `.miy-tools/miy`, Windows에서 `.miy-tools/miy.exe`다.

`describe` 기본 결과는 짧은 루트 목록이다. `--command`는 선택한 명령의 인수·enum·필수값·기본값·충돌·스키마·권한·부작용·오류·예제를, `--all`은 전체 참조를 반환한다. 도움말과 셸 완성은 같은 Clap 트리에서 생성한다.

```sh
miy completion bash
miy completion zsh
miy completion fish
miy completion powershell
```

## 서버 context와 인증

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

Check는 프로젝트 Rust 테스트와 프런트 도구를 실행하고 의존성·빌드 산출물을 쓴다. 정적 검사 통과가 실제 브라우저·DB 인수를 대신하지 않는다. Dev는 작업 트리를 빌드하여 별도 로컬 DB·백엔드·게이트웨이를 실행한다. 연결된 API에는 실제 사용자의 dev 권한을 적용한다. 로컬 DB와 서버 dev/prod DB는 별개다. 종료는 자신이 소유한 프로세스·컨테이너를 정리하고 DB 볼륨을 남긴다. 준비/종료 대기는 성공 완료와 구분한다.

Build는 깨끗이 커밋한 소스로 불변 Linux 이미지를 만든다. 배포는 manifest·커밋·이미지·번들 영수증을 확인하며 이미지 platform이 서버 `runtimePlatform`과 같아야 한다. 다른 CPU의 빌드에는 명시적 `--platform`과 실제로 동작하는 빌더가 필요하다. 플래그만으로 에뮬레이션을 설치하지 않는다.

Dry-run은 예상 상태·계획된 변경·worker에서 남은 검사를 반환한다. 이미지 업로드나 runtime job은 생성하지 않는다. 실제 실행은 context/origin/user/app, 요청 키, payload와 expected-state를 먼저 journal에 기록한다. 이미지 archive 상한은 1 GiB다. 업로드 접수·ID 조회부터 runtime 제출까지 복구 대상이며, 전송 바이트의 일부부터 이어 보내는 기능은 제공하지 않는다.

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

## 로컬 플랫폼 보조 도구와 검증

저장소의 `bash scripts/up.sh`는 로컬 PostgreSQL·Keycloak·Caddy와 코어/worker/UI 검증 기반을 준비한다. `bash scripts/down.sh`는 저장 데이터를 보존하며 해당 기반을 중지한다. 이 도구의 기존 환경 기록을 모든 OS의 지원 증거로 확대하지 않는다. `.local/credentials.json`과 `.local/runtime.env`는 로컬 비밀 설정이므로 출력·게시하지 않는다.

명시적으로 켠 loopback 개발 프로필은 `http://localhost:18000`의 합성 계정 버튼과 `miy auth login --account developer`를 제공한다. 실제 OAuth 승인/기업 SSO 검증과 구분한다. 사설 CA의 CLI 설정과 브라우저 신뢰는 별개이며 TLS 검증을 끄지 않는다.

전체 검사와 격리 회귀의 준비·정리 경계는 [검증 행렬](CLI-VALIDATION-MATRIX.md) 및 [런타임 회귀 안내](../scripts/runtime-regression/README.md)를 따른다. 과거 [휴가 관리 실증](LEAVE-LEDGER-RESULTS.md)·[구현 결과](IMPLEMENTATION-RESULTS.md)는 해당 시점의 증거다. 이 안내를 갱신한 사실만으로 별도 Linux 서버·실제 Windows 11·독립 AI·실제 IdP 전파 검사를 완료한 것으로 기록하지 않는다.
