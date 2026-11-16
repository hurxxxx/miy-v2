# macOS CLI 인증 저장소 오류 관찰 — 2026-11-16

## 현재 상태와 영향

**미해결.** 설치된 CLI 0.11.0에서 `credential_storage_unavailable` 오류가 발생했다. 사용자가 브라우저에서 로그인 승인을 완료했다고 확인한 뒤에도 `auth wait`와 `auth status`가 같은 오류로 종료됐다. 실제 실패한 내부 단계와 근본 원인은 아직 확인하지 않았다.

사용자는 새 Todo 앱 구현과 개발 환경 배포를 요청했다. 로컬 기본 템플릿의 포맷·검사는 통과했지만 인증이 필요한 MCP 계약 조회와 컴포넌트 동기화가 실패했다. Todo 기능 구현, 앱 등록, 로컬 preview, 서버 빌드와 개발 배포는 수행하지 않았다. 이 기록은 해당 오류를 전달하기 위한 문서이며 수정·복구 완료 보고가 아니다.

## 관찰 환경

| 항목 | 확인 내용 |
| --- | --- |
| 실행 환경 | macOS, Codex 도구의 zsh 명령 실행 |
| 앱 프로젝트 | `/Users/edward/projects/miy-apps/my-app` |
| 실행 파일 | 프로젝트의 `.miy-tools/miy` |
| CLI / SDK 번들 | `0.11.0` / `0.11.0` |
| 선택 context | `default` |
| 서버 origin | `https://portal.miy.works` |
| manifest | 앱 ID `my-app`, 버전 `0.1.0` |
| 관찰 범위 | 위 프로젝트의 CLI와 프로젝트 stdio MCP. 일반 사용자 터미널에서의 재현 여부는 미확인 |

## 재현 과정과 실제 결과

다음 CLI 명령은 앱 프로젝트 디렉터리에서 실행했다. `<LOGIN_ID>`는 `auth login`이 반환한 공개 로그인 요청 ID를 뜻한다. 만료된 로그인 코드와 승인 링크는 재사용하지 않는다.

| 순서 | 실행한 명령 또는 도구 | 결과 |
| --- | --- | --- |
| 1 | `.miy-tools/miy describe --output json` | PASS, CLI `0.11.0`, 선택 context와 origin 확인 |
| 2 | `.miy-tools/miy context list --output json` | PASS, 활성 context `default`와 서버 연결 설정 확인 |
| 3 | MCP `get_contract({})`, `search_api({query:"identity"})` | FAIL, `credential_storage_unavailable` |
| 4 | MCP `get_cli_help({})` | PASS, 명령 목록 조회 가능 |
| 5 | `.miy-tools/miy --context default auth login --no-open --output json` | PASS, 종료 코드 0, `status: pending`, 승인 URL·사용자 코드·로그인 요청 ID 반환 |
| 6 | 사용자 브라우저 승인 | 사용자가 “로그인 승인 완료”라고 확인. 자동 브라우저 도구로 승인 화면을 검증하지는 못함 |
| 7 | `.miy-tools/miy --context default auth wait <LOGIN_ID> --output json` | FAIL, 종료 코드 7, `credential_storage_unavailable` |
| 8 | `.miy-tools/miy --context default auth status --output json` | FAIL, 종료 코드 7, 같은 오류. 일반 실행과 PTY 실행 모두 동일 |
| 9 | MCP `get_contract({})` 재조회 | FAIL, 같은 오류 |
| 10 | MCP `list_components({query:"ui"})`, `get_component({name:"ui"})` | FAIL, 같은 오류 |
| 11 | `.miy-tools/miy --context default component sync --output json` | FAIL, 종료 코드 7, 같은 오류 |

`auth wait`의 실제 응답에서 확인한 오류 부분은 다음과 같다. `auth status`와 `component sync`도 같은 오류 코드와 메시지를 반환했다.

```json
{
  "code": "credential_storage_unavailable",
  "details": {},
  "message": "The credential store is unavailable. Check its OS service, backend and private file permissions.",
  "retryable": false
}
```

응답에는 저장소 backend, OS 오류 코드, 실패한 작업 종류, 구조화된 복구 실행 인자가 없었다. 따라서 이 출력만으로 읽기·쓰기·세션 교환 중 어디에서 실패했는지 또는 OS 서비스·접근 권한·파일 권한 중 무엇이 원인인지 판단할 수 없다.

## 별도로 실행한 로컬 검사

| 명령 | 결과와 한계 |
| --- | --- |
| `.miy-tools/miy --context default doctor --for develop --output json` | FAIL, 종료 코드 7. `savedSession: session_unavailable`, `authentication: skipped`, `rustfmt: tool_unavailable` |
| 같은 doctor 응답의 나머지 항목 | 서버 연결 설정·서버 capability·manifest·번들·CLI 지침·MCP 설정·컴포넌트 무결성·Cargo/Rust·Node/pnpm/Git·Rust linker·MCP stdio 시작 검사는 PASS |
| `.miy-tools/miy app fmt --output json` | PASS, 종료 코드 0 |
| `.miy-tools/miy app check --output json` | PASS, 종료 코드 0. manifest, bundle-integrity, cli-guidance-integrity, component-integrity, migration-contract, rust-format, rust-tests, frozen-dependencies, typescript, ui-build, federation-assets 통과 |

`app check`는 **기존 기본 템플릿**에 대한 검사다. Rust 업무 테스트는 0개였으며 Todo 구현, 실제 DB migration, 브라우저 통합 또는 배포 성공을 증명하지 않는다.

doctor의 `rustfmt` 실패와 이후 `app fmt`·`app check` 성공이 함께 관찰됐다. 이 차이의 원인도 확인하지 않았고 인증 저장소 오류와의 인과관계를 주장하지 않는다. 서버 capability 조회와 로컬 검사의 성공 역시 인증된 서버 접근 성공을 뜻하지 않는다.

## 추가 관찰 — 2026-11-16 22:10~22:11 KST

원인 구분을 돕기 위해 자격증명 저장소를 열지 않고 OS·실행 파일·프로세스 세션의 공개 메타데이터와 오류 발생 시간을 확인했다. **주요 추가 단서는 CLI 실행 세션이 `Background`라는 점이다.** 키체인이 실제 backend인지, 이 세션이 오류 원인인지는 아직 확인하지 않았다.

### OS와 실행 파일

| 명령 / 항목 | 실제 결과 |
| --- | --- |
| `sw_vers` | macOS `26.6.2`, build `25G83` |
| `uname -m` / `file .miy-tools/miy` | `arm64` / `Mach-O 64-bit executable arm64` |
| `.miy-tools/miy --version` | `miy 0.11.0` |
| `shasum -a 256 .miy-tools/miy` | `84c10356c4589cb4284d310c90716bed41413ba6355590bb12fe53001a9af60f` |
| CLI 파일 권한·소유자·크기 | `-rwxr-xr-x`, `edward:staff`, `17434704` bytes |
| `codesign -dv --verbose=2 .miy-tools/miy` | `Signature=adhoc`, `flags=0x20002(adhoc,linker-signed)`, `TeamIdentifier=not set`, identifier `miy-15b00bdffb95902a` |
| `codesign --verify --strict --verbose=2 .miy-tools/miy` | PASS, 종료 코드 0. `valid on disk`, `satisfies its Designated Requirement` |

서명 검사의 PASS는 디스크상의 코드 서명 검증 결과다. 자격증명 저장소 접근 권한이나 해당 서명 방식의 적합성을 검증한 결과가 아니다. 실패 당시 사용한 바이너리를 후보 빌드와 구분할 수 있도록 해시를 기록했다.

### Codex 실행 세션

| 명령 / 항목 | 실제 결과 |
| --- | --- |
| `id -u`, `id -ru` | 둘 다 `501`; root 실행 아님 |
| `launchctl managername` | `Background` |
| `launchctl manageruid`, `launchctl managerpid` | `501`, `1` |
| `stat -f 'console_owner=%Su' /dev/console` | `console_owner=edward` |
| 환경 변수 존재 여부만 확인 | `SSH_CONNECTION`, `SSH_TTY` 존재. 값은 출력하지 않음 |
| context/config 환경 변수 존재 여부 | `MIY_CONTEXT`, `MIY_CONFIG_DIR` 없음 |
| 실행 부모 계보 | Python 진단 프로세스 → Codex 데몬 → Codex 데몬 → `/sbin/launchd` |
| Codex 실행 파일 | `/Users/edward/.codex/packages/app-server-daemon/releases/0.169.0-aarch64-apple-darwin/bin/codex` |
| Codex 부모 프로세스의 TTY | 두 프로세스 모두 `??` |

프로세스 계보는 `ps`의 PID·PPID·UID·TTY·`comm`만 사용했다. 인자 전체와 환경 변수 전체는 출력하지 않았다. SSH 변수의 존재만으로 현재 SSH 접속을 통해 실행된다고 단정하지 않는다. 명확히 관찰한 사실은 launchd manager가 `Background`이며 Codex 데몬 아래에서 실행된다는 점이다.

### 같은 명령의 실행 조건 비교

명령은 모두 `.miy-tools/miy --context default auth status --output json`이다. Python `time.monotonic()`으로 자식 프로세스 실행부터 종료까지 측정했다.

| 시작 시각 (UTC) | 조건 | 소요 시간 | 결과 |
| --- | --- | --- | --- |
| `2026-11-16T13:10:50.083403Z` | 기본 login shell, 비-PTY, 출력 캡처 | `13.22 ms` | FAIL, 종료 코드 7, `credential_storage_unavailable`, `details: {}`, stderr 비어 있음 |
| `2026-11-16T13:11:20.032473Z` | non-login shell, PTY 할당, 자식 출력 캡처 | `12.56 ms` | FAIL, 같은 오류, stderr 비어 있음 |
| `2026-11-16T13:11:48.164095Z` | non-login shell, PTY의 입출력을 CLI에 그대로 상속. `stdin_is_tty=true`, `stdout_is_tty=true` 확인 | `13.10 ms` | FAIL, 같은 오류 |

PTY를 할당하고 실제 CLI 입출력을 터미널에 연결해도 재현됐다. 다만 모두 같은 Codex 실행 환경이므로 GUI 터미널 실행과 동등한 시험은 아니다. 빠른 실패는 관찰값일 뿐, 네트워크 요청 부재나 특정 내부 함수의 실패를 입증하지 않는다.

같은 시점에 MCP `get_contract({})`도 재실행했고 동일한 오류와 `auth login`·`doctor --for develop`를 권하는 안내가 반환됐다. 신규 로그인, 키체인 잠금 해제, 저장소 변경 또는 환경 우회는 하지 않았다.

### 서버 측 대조에 사용할 공개 로그인 요청 정보

- 이전 `auth login`의 공개 `loginId`: `23a3e73c-360a-478f-8f58-55fdbaefcbef`
- 응답의 `expiresAt`: `1794834130` = `2026-11-16T13:02:10Z` = `2026-11-16 22:02:10 KST`
- 응답의 `expiresIn`: `300`초. 이를 역산한 발급 시각은 약 `12:57:10 UTC`이며, 정확한 서버 이벤트 시각은 별도 확인이 필요하다.
- 승인 완료는 사용자 확인이며, 승인·교환·로컬 저장의 각 단계 성공을 서버 기록으로 검증하지는 않았다. 서버 담당자는 위 공개 요청 ID로 해당 단계의 처리 여부를 대조할 수 있다.
- private device code, access/refresh token, credential 파일, 쿠키는 수집하거나 기록하지 않았다.

### 개발자에게 전달할 비교 조건

진단 필드를 추가한 후보로 **같은 UID·같은 바이너리·같은 context의 GUI 터미널 실행과 Codex `Background` 실행**을 비교하면 이번 관찰을 좁힐 수 있다. backend 초기화·자격증명 읽기·세션 갱신·자격증명 쓰기를 구분하고, 해당 OS 오류가 있을 때 안전한 숫자 코드와 오류 도메인을 남기는 것이 유용하다. 현재 정보로 임의의 `OSStatus` 값을 원인으로 지정하거나 저장소 우회를 권하지 않는다.

## 서버 대조·소스 조사 — 2026-11-16

클라이언트 추가 기록 커밋 `61ce286`을 miy.works 작업본에 fast-forward로 받았다. 아래는 서버에서 수행한 읽기 전용 DB 대조이며 자격증명 값·해시·쿠키는 조회하지 않았다.

| 항목 | 확인 결과 |
| --- | --- |
| 해당 공개 loginId의 상태 | `consumed` |
| 서버 생성 시각 | `2026-11-16T12:57:10.197052Z` |
| 최종 poll 시각 | `2026-11-16T12:59:25.490809Z` |
| 같은 client_id의 발급 구간 내 CLI 세션 | `2026-11-16T12:59:25.490050Z` 생성, 조회 시 미회수 |
| refresh 자격증명 | 발급 행 존재, 사용 이력 없음. 값·해시 미조회 |

서버에서 승인·device 교환·세션 발급은 완료됐다. CLI 0.11.0의 `cli_login::wait`는 교환 후 `verify_identity`를 거쳐 **기존 세션의 `vault_get`**을 호출한다. 그 읽기는 `credential_storage_unavailable`을 그대로 반환하지만, 그 뒤 새 자격증명 쓰기 실패는 다른 코드 `login_credential_not_saved`로 바꾼다. 교환 완료와 이번 오류 코드를 함께 보면 기존 세션 자격증명을 읽는 경로가 실패 지점이라는 소스상 추론이 가능하다. 아직 실제 backend·파일 권한·Keychain OSStatus는 확인하지 못했으며 Background 실행 자체를 원인으로 확정하지 않는다.

0.12.0 후보 소스에는 다음을 보완했다. 게시·Mac 실기 완료를 뜻하지 않는다.

- 저장소의 read/write/remove, 허용된 내부 사유, Keychain 숫자 OSStatus 보존. 원문 OS 메시지·비밀값은 출력하지 않음.
- 기존 자격증명을 읽지 못하면 방금 발급된 서버 세션의 회수를 시도하고 `phase=after_device_exchange`, `remoteSessionRevoked`로 결과 표시. 기존 자격증명은 덮어쓰지 않음. 이 경로는 로컬 HTTP fixture의 실제 CLI 실행으로 검증했다.
- MCP 저장소 오류의 반복 로그인 안내 제거. 비로그인 신고·로컬 보관·동일 ID 재전송 구현([신고 계약](docs/SUPPORT-REPORTS.md)).

**남은 확인:** 같은 후보 바이너리·UID·context로 GUI 터미널과 Codex Background 실행의 안전한 오류를 비교하고, 실제 Mac의 저장소 접근을 복구한 뒤 계약 조회·동기화를 다시 확인한다. 이번 서버 조회에서 발견한 기존 세션을 임의 회수하거나 사용자 Mac의 저장소를 변경하지 않았다.

## 조사 시 확인할 사항

아래는 후속 조사 항목이며 아직 수행 결과가 아니다. 전체 작업 순서는 기존 [추진 계획](docs/PLAN.md)을 따른다.

1. 같은 사용자·프로젝트·실행 파일로 일반 터미널과 Codex 실행 환경에서 `auth status`를 비교한다. 현재는 Codex의 비-PTY·PTY 실행 결과만 있다.
2. 자격증명 값이나 파일 내용을 노출하지 않는 진단으로 실제 backend, 실패한 작업 종류와 OS 오류 코드를 확인한다. 현재의 빈 `details`와 일반 메시지만으로는 복구 조치를 정하기 어렵다.
3. 브라우저 승인 후 로컬 저장이 실패하는 경우의 로그인 요청·서버 세션 상태와 안전한 복구 절차를 확인한다. 이번 관찰만으로 세션 교환 성공 여부나 새 로그인 필요 여부를 단정하지 않는다.
4. 원인을 확인한 뒤 `auth status`, MCP `get_contract`·`search_api`, `component sync`를 다시 실행한다. 실패 기록을 지우지 않고 후속 결과를 추가한다.
5. 인증 복구 후 Todo 구현·검사와 등록/승인·preview·서버 빌드·dev 배포를 진행하고 실제 release/image 및 브라우저 결과를 기록한다.

## 수행하지 않은 작업

- **NOT_RUN:** 사용자 일반 터미널의 로그인/상태 확인. 실행을 요청했지만 사용자가 대신 오류 문서 작성과 커밋·푸시를 요청했다.
- **NOT_RUN:** 인증 저장소 backend·권한·OS 서비스의 내부 조사 및 수정. credential 파일·토큰·쿠키·키체인 항목을 읽거나 출력하지 않았다.
- **NOT_RUN:** 플랫폼 소스 수정, CLI 교체·업데이트, 전역 설정 변경, 저장소 우회 또는 다른 인증 backend 강제 사용.
- **NOT_RUN:** Todo 기능 구현, 앱 등록/승인, `app dev`, `app build`, `app deploy`, `app accept`, production 승격.
- **NOT_RUN:** 실제 DB·브라우저 업무 통합 검사. 기본 템플릿 검사 PASS를 해당 범위로 확대하지 않는다.

관련 기준: [플랫폼 개발 규칙](AGENTS.md), [변경별 검사 지도](docs/CHECKS.md). 이 문서는 사용자 요청에 따라 저장소 루트에 단일 오류 보고서로 남긴다.
