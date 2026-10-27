# miy CLI 0.5 구현 계획

- 작성일: 2026-10-27
- 상태: CLI 0.5.0 / 번들 0.6.2 코드 구현·지침 이전 및 로컬 자동 검사·5개 배포 후보 CI 완료 — 실제 Windows 11·독립 실증·정식 릴리스 조건은 미완료
- 기준 코드: `ea61ad5ca83a724833dd3bcdb5ed1de5b61bd9c6` — CLI 0.4.2 / 개발 번들 0.6.1
- 목표: CLI 0.5.0 / 개발 번들 0.6.2

사람이 직접 운영하기 편하고 AI 에이전트가 발견·호출·복구할 수 있는 CLI를 같은 계약으로 제공한다. 여러 서버와 앱의 관리, 브라우저·인증 코드 로그인, 구조화 출력, 작업 이력·로그, Windows 네이티브 실행과 배포를 함께 구현한다. 이 문서의 0.5 명령과 API는 **목표 계약**이며 [개발 인계](CLI-HANDOFF.md)와 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 구현·검사 범위를 기록한다. 코드 구현과 환경별 인수 완료는 구분한다. 다른 환경에서 시작할 때는 [개발 인계](CLI-HANDOFF.md)를 먼저 읽는다.

## 1. 확정 범위와 지원 환경

명령과 출력의 호환성을 유지하는 작은 수정으로 제한하지 않는다. 0.5에서 명령 체계와 출력 계약을 정리하고, 관련 스크립트·템플릿·MCP·스킬·매뉴얼을 한 번에 갱신한다. 앱의 업무 코드·기존 데이터·적용된 SQL 이력은 보존한다.

| 환경 | 지원할 배포물과 사용 범위 |
| --- | --- |
| macOS ARM64 / x64 | 각 CPU용 네이티브 CLI, 원격 운영과 로컬 앱 개발 |
| Linux ARM64 / x64 | 각 CPU용 네이티브 CLI, 원격 운영과 로컬 앱 개발 |
| Windows 11 x64 | `miy.exe`, ZIP, PowerShell 설치·갱신·제거, 원격 운영과 로컬 앱 개발 |
| WSL2 | Linux 배포물 사용. Windows 네이티브 지원과 별도로 취급 |

Windows는 **PowerShell 7과 Windows PowerShell 5.1**을 모두 검증한다. 원격 Linux 서버를 관리할 때 Rust·Node·Docker 설치를 요구하지 않는다. 로컬 앱 개발에는 Git, Rust MSVC 도구 체인과 해당 빌드 도구, Node/pnpm, Docker Desktop의 Linux 컨테이너 환경이 필요하다. 서버·worker와 앱 배포 이미지는 Linux를 유지한다.

인증은 로그인한 사용자 세션을 저장·갱신·철회하는 방식이다. AI도 같은 사용자 권한으로 동작한다. CI 전용 서비스 계정은 후속 범위다. prod 변경과 모든 환경의 `app remove`는 앱·환경의 대상과 영향을 표시하고 확인하며, 자동화에서는 명시적인 대상과 `--yes`를 요구한다. `context remove`는 명시한 로컬 context와 설정·자격증명 연결에 미치는 영향을 별도로 확인한다.

이번 범위에서 제외하는 기능은 Windows ARM64·Windows 10·Windows Server 호스팅, 서버 백업·복원, 일반적인 배포 롤백, 실행 중 migration 취소, 로그의 무손실 연속 구독이다. 이들은 기존 기능의 데이터 보존과 실패 진단을 생략할 이유가 되지 않는다.

## 2. 현재 구현과 개선 지점

| 영역 | 0.4.2에서 확인한 상태 | 0.5에서 바꿀 점 |
| --- | --- | --- |
| 명령 | `app` 아래 개발·인증·사용자·컴포넌트·운영 명령 혼재 | 역할별 루트 그룹, 프로젝트 밖에서도 원격 운영 |
| 출력 | 다수 성공 결과는 JSON. 실행 오류는 `error.message`, Clap 인수 오류는 일반 텍스트 | 성공·실패 envelope, 오류 코드·종료 코드·복구 정보 통일 |
| 발견 | `app describe`는 전체 help 문자열을 반환 | 명령별 타입·입력·출력·권한·부작용 스키마와 선택 조회 |
| 인증 | 프로젝트 `.miy/session.json`, 브라우저 승인, 8시간 토큰 | 전역 context별 사용자 세션, device code, 안전한 저장, 자동 갱신·철회 |
| 작업 | 요청 전 멱등성 키 저장, ID 조회, 마지막 작업 재개 | 여러 작업의 목록·이벤트·명시적 재개·제한된 재시도 |
| 배포 | dev 배포와 검증한 릴리스의 prod 승격 | 대상 확인, dry-run, 현재 상태 재검사. `deploy --environment prod`를 도움말에서도 제거 |
| 진단 | Docker 실행 파일 존재와 버전 검사 | daemon 연결·Linux 엔진 모드·서버·인증·개발 도구를 구분 |
| Windows | Unix 파일 권한·실행 파일 경로·브라우저 실행 가정이 남음 | OS 추상화와 실제 PowerShell 개발·운영 검증 |
| 배포물 | Bash 소스 빌드/기존 바이너리 설치 스크립트 | 다섯 OS/CPU 배포물, 체크섬, Windows 설치·갱신·제거, 릴리스 CI |

프로젝트 MCP는 계약·API·컴포넌트를 조회하는 읽기 중심 어댑터다. 0.5에서는 기본 명령 목록을 짧게 유지하고 command/search/all로 필요한 명령·스키마를 선택한다. 실제 코드 위치와 기존 검증 결과는 [개발 인계](CLI-HANDOFF.md)에 정리한다.

## 3. 명령과 기계 처리 계약

### 명령 체계

| 그룹 | 구현할 명령과 역할 |
| --- | --- |
| `context` | `add`, `list`, `show`, `use`, `remove`: 서버 주소·CA·사용자 세션 연결 관리 |
| `auth` | `login`, `wait`, `status`, `logout`, `sessions list`, `sessions revoke`: 브라우저/코드 로그인과 사용자 세션 관리 |
| `app` | `init`, `setup`, `upgrade`, `check`, `fmt`, `dev`, `dev-stop`, `build`, `register`, `rename`, `accept`, `deploy`, `promote`, `list`, `show`, `status`, `start`, `stop`, `restart`, `remove`, `logs` |
| `job` | `list`, `get`, `watch`, `resume`, `retry`: 여러 서버 작업의 상태·이력·복구 |
| `component` | 기존 `list`, `show`, `add`, `update`, `sync`, `check`, `publish` 이전 |
| `registration` | `list`, `approve`, `reject` |
| `user` / `grant` | `user list`, `grant list`, `grant set`, `grant revoke` |
| `describe` / `doctor` | 필요한 명령 계약 조회, 개발/운영 조건 진단 |
| `completion` / `mcp` | 셸 완성 생성, 프로젝트 stdio MCP |

이전 이름과 충돌하지 않는 구 명령은 조용히 다른 작업을 실행하지 않고 대체 명령을 안내하는 오류를 반환한다. 제거할 `app resume`은 `job resume`, `app control`은 명시적인 수명주기 명령으로 연결한다. `app setup-codex`와 `app codex`도 일반 Codex·Claude 실행 및 `app setup` 안내로 대체한다. 기존 서버용 `serve`, `worker`, `migrate`, `seed`는 서버 빌드에 남기고 Windows 클라이언트 배포물에서는 제외한다.

### 출력과 입력

- 공통 `--output auto|text|json|jsonl`을 제공한다. `auto`는 stdout TTY에서 text, 비TTY에서 JSON이다. JSONL은 작업 관찰 등 명시적으로 지원하는 명령에만 허용한다.
- JSON 단일 응답은 `schemaVersion`, `command`, `ok`, `data`, `error`, `meta`를 가진다. 성공 시 `error: null`, 실패 시 `data: null`을 기본으로 한다. `meta`에는 요청 ID·대상 context·페이지 cursor 등 필요한 비밀이 아닌 정보를 넣는다.
- JSON 모드의 인수 오류도 같은 실패 envelope로 stdout에 한 번 출력한다. stderr는 진단·진행 메시지 전용이다. 하위 빌드 도구의 stdout이 JSON에 섞이지 않게 전달 경로를 분리한다.
- 오류는 안정적인 `code`, 사람이 읽는 `message`, `retryable`, 구조화한 `details`, 선택적인 `recovery`를 갖는다. 복구 명령은 셸 문자열 대신 실행 파일과 인수 배열로 표현하며 자동 실행하지 않는다.
- 일반 `--help`/`--version`, 셸 completion 스크립트, MCP JSON-RPC 스트림은 고유 출력 형식을 유지한다. 인수 파싱 실패에서도 출력 모드·TTY를 판별할 수 있게 진입점을 정리한다.
- 구조화 조회에는 `--fields`, 목록에는 `--limit`, `--cursor`를 제공한다. 서버에서 권한과 필터·페이지 경계를 적용한다. 필드 선택으로 권한을 우회하지 않는다.
- 복합 입력이 필요한 `app accept`와 `grant set` 등은 `--input @file` 또는 `--input -`를 지원한다. 스키마 검증, 알 수 없는 필드 거부, 같은 필드의 플래그·JSON 중복 지정 거부를 공통화한다.
- `--no-input`과 비TTY에서는 자동 질문·브라우저 실행을 하지 않는다. 입력이나 승인이 필요하면 구체적인 오류와 다음 명령을 반환한다. `--yes`는 실행 확인만 생략하고 인증·권한·필수 인수를 대체하지 않는다.
- JSON/JSONL의 바이트 인코딩은 UTF-8로 고정한다. PowerShell 5.1의 네이티브 파이프·리디렉션 차이를 실제 검증하고 파일 입력 및 프로세스/세션 범위 인코딩 예제를 제공한다. 시스템 전역 인코딩을 변경하지 않는다.

종료 코드는 다음 계약으로 고정하고 명령별 스키마와 테스트에 반영한다.

| 코드 | 의미 |
| --- | --- |
| 0 | 완료 또는 `--no-wait` 요청 접수 성공 |
| 1 | 분류하지 못한 내부 오류 |
| 2 | 명령·인수·입력 스키마 오류, 제거된 명령 |
| 3 | 로그인 필요·세션 만료·철회 |
| 4 | 권한 부족 |
| 5 | 대상 없음 |
| 6 | 현재 상태·버전·멱등성 충돌, 사용 중인 실행 파일 |
| 7 | 환경·도구·서버 기능·검증 조건 미충족 |
| 8 | 네트워크·일시적 서비스 장애·관찰 시간 초과 |
| 9 | 접수된 원격 작업의 최종 실패 |
| 130 | 사용자가 로컬 실행/관찰을 중단 |

### 발견과 에이전트 지침

`miy describe`의 기본값은 짧은 명령 목록이다. `--command "app deploy"`는 인수 타입·enum·필수값·기본값·충돌·입력/출력 스키마·권한·부작용·오류·예제를 반환한다. `--search`로 후보를 찾고 `--all`은 명시적인 전체 조회로 남긴다.

Clap 명령 정의와 타입이 있는 공통 메타데이터에서 help, describe, JSON Schema, Bash/Zsh/Fish/PowerShell completion, 매뉴얼과 MCP 설명을 생성한다. 런타임 입력 검증도 같은 타입을 사용한다. 스킬은 발견→필요한 계약 조회→실행→오류 복구의 짧은 안내를 제공하며 전체 매뉴얼을 매 요청에 삽입하지 않는다.

## 4. 구현 순서와 완료 조건

### 단계 0. 기준선 재검증과 별도 Linux 서버 실증

현재 checkout·도구체인·OS/CPU와 서버에 실제 배포한 코드·migration 버전을 먼저 기록한다. 현재 코드로 정적 검사, 단위 검사, 격리 Docker/PostgreSQL 런타임 회귀와 브라우저 회귀를 재현하고 과거 환경의 결과와 구분한다. 최신 런타임 변경과 migration 0008이 실행 서버에 적용됐다고 가정하지 않는다.

별도 Linux 검증 서버에서 기존 CLI의 로그인·이미지 전송·dev 배포·동일 릴리스 prod 승격·실제 UI/API·데이터 보존을 초기에 확인한다. 서버 CPU와 이미지 platform도 기록한다. 이 실증을 Windows 설치기 완성 뒤로 미루지 않는다. 장비나 서비스가 없는 항목은 미실행 사유를 남기고 독립적인 기반 구현은 진행할 수 있지만, 해당 환경의 지원 완료로 표시하지 않는다.

완료 조건은 기준 코드의 재현 결과와 별도 Linux 서버 실증 증거다. 실패·미실행 항목은 다음 단계의 변경으로 가리지 않고 별도 추적한다.

### 단계 1. OS 경계와 출력 계약

#### 1A. 빌드·파일·프로세스 기반

클라이언트가 서버 전용 모듈과 Unix API에 의존하지 않도록 빌드 경계를 분리한다. 공유 계약·이미지 메타데이터·로컬 Docker 호출은 클라이언트에서도 사용할 수 있게 분리하고, 코어 서버의 Docker 접근 권한은 늘리지 않는다.

`x86_64-pc-windows-msvc` 네이티브 클라이언트 빌드와 계약 검사를 CI에 이 단계부터 추가한다. Linux 서버 빌드도 유지한다. CI 설정 추가·로컬 교차 검사·실제 Windows runner 통과를 구분하여 기록한다.

파일 권한/비밀 저장, 실행 파일 이름·탐색, 브라우저 실행, 프로세스 제어, 경로·파일 교체를 OS 추상화로 모은다. Windows의 `.exe`, `PATHEXT`, `pnpm.cmd` 같은 런처를 처리한다. 명령 문자열 연결을 피하고 인수 배열을 사용하되 `.cmd`/`.bat`의 특별한 해석 규칙도 별도로 검증한다. 지원하지 않는 위험한 인수 조합은 거부한다.

템플릿 키와 잠금·보호 파일 목록은 `/`를 쓰는 논리 경로로 통일한다. `build.rs`의 OS별 경로 표현과 Git CRLF 변환으로 번들 해시가 달라지지 않도록 `.gitattributes`와 관리 파일의 LF 정책을 추가한다. 플랫폼 저장소뿐 아니라 새로 생성하거나 명시적으로 upgrade한 앱의 관리 파일에도 정책을 전달하고 OS 간 checkout·검사로 확인한다. 기존 사용자 `.gitattributes`는 충돌을 검사하며 임의로 덮어쓰지 않는다. 이미 앱 DB에 적용한 SQL 파일의 바이트를 일괄 변환하지 않는다. 한글·공백 경로, 다른 드라이브, junction/reparse point를 통한 관리 범위 이탈을 검사한다.

실행 파일·지침·매뉴얼·잠금 파일을 교체하는 공통 절차는 이 단계에서 설계한다. 쓰기 전 잠금·충돌 검사, staging·검증·교체 순서, 중단 후 복구에 필요한 기록과 이전 버전 보존을 정의하고 실패 주입으로 확인한다. 여러 파일의 교체를 단일 파일 rename만으로 원자적이라고 간주하지 않는다. 설치기·`app setup/upgrade`는 단계 5에서 이 절차를 사용한다.

#### 1B. 명령·출력·운영 요청 계약

공통 오류·출력을 먼저 만들고 기존 명령을 이전한다. `app deploy`는 dev만 허용하고 prod는 `accept` 후 `promote`를 사용한다. `doctor --for develop|operate`는 도구 존재, Docker daemon 연결/모드, 서버 도달성, 세션, 프로젝트 계약을 별도 결과로 출력한다.

운영 명령을 확장하기 전에 공유 preflight 입력·결과와 expected-state 계약을 정의한다. 대상 앱·환경·설치, 릴리스·실행 세대·목표/실제 상태 중 비교할 값과 충돌 오류를 공통 타입으로 정하고, CLI 확인·서버 접수·worker 실행이 같은 계약을 사용한다. 실행 코드 연결은 단계 4에서 검증하되 단계 3의 retry API가 확인 절차를 우회하는 별도 경로가 되지 않게 한다.

작업 실패는 안정적인 코드와 타입이 있는 재시도 분류를 저장한다. 임의 오류 문자열 검색으로 재시도 가능성을 판단하지 않는다. 원본 작업·새 요청 ID·재검증 결과의 관계와 재시도 불가 사유를 먼저 고정한다. prod 변경 또는 `app remove`의 재시도에도 최초 실행과 같은 명시적 대상·영향 확인·expected-state 검사를 요구한다.

완료 조건은 Windows 네이티브 클라이언트 빌드, 세 OS의 같은 출력 스키마·종료 코드, 인수 오류 JSON 처리, 프로젝트 경로·번들 해시의 OS 간 일치와 preflight·expected-state·재시도 타입의 공통 계약 검사다.

### 단계 2. Context와 사용자 로그인

대상 선택 순서는 `--context` → `MIY_CONTEXT` → 프로젝트 바인딩 → 활성 context다. 프로젝트에는 서버/context 연결 정보만 두고 토큰을 두지 않는다. 로컬 작업은 `--project`, 프로젝트 밖 원격 작업은 `--app`을 지원한다. prod 변경과 모든 환경의 `app remove`는 실행 시 `--context`, `--app`, `--environment`를 명시해야 하며 기본값에 의존하지 않는다. `context remove`는 context 이름을 명시하고 로컬 설정·자격증명 연결에 미치는 영향을 확인한다. 앱·환경 플래그를 요구하지 않으며 원격 세션 철회는 `auth logout` 또는 `auth sessions revoke`로 별도 수행한다. 비TTY에서 context 제거도 `--yes`가 필요하다.

설정 루트는 `MIY_CONFIG_DIR`를 우선하고, Unix는 `$XDG_CONFIG_HOME/miy` 또는 `~/.config/miy`, Windows는 `%APPDATA%\miy`를 사용한다. Windows와 WSL은 자격증명을 자동 공유하지 않는다. 서버 origin과 사용자 식별자를 자격증명 키에 포함하고 context 변경으로 다른 서버에 토큰이 전송되지 않게 한다. 현재의 HTTPS 기본값, loopback HTTP 예외, 명시적 CA와 리디렉션 제한을 유지한다.

`auth login`은 브라우저 승인을, `auth login --device-code`는 인증 URL과 짧은 사용자 코드를 제공한다. 승인 유효기간은 5분, 기본 polling 간격은 5초이며 서버의 interval/slow-down·만료·거부를 따른다. `--no-wait`는 공개 login ID·URL·사용자 코드만 반환하고 `auth wait <id>`로 이어간다. 토큰 교환용 비밀 device code/verifier는 로컬 보안 저장소에 둔다. 승인 페이지는 대상 서버와 승인 계정을 명확히 표시하고, 코드 추측 제한·일회 사용·CSRF 검사를 구현한다.

Access token은 15분, refresh session은 최초 발급부터 최대 30일로 제한한다. 갱신 시 refresh token을 회전시키고 context별 잠금으로 동시 CLI/MCP 갱신을 직렬화한다. 서버에는 토큰 원문 대신 해시와 세션 관계를 저장하며, 회전 토큰 재사용·세션 철회·사용자 비활성화를 검증한다. 갱신 응답 유실은 임의 재로그인이나 중복 갱신으로 숨기지 않고 다시 인증할 수 있는 명확한 결과로 처리한다.

인증 구현 전에 사용자 비활성화의 기준 원천(Keycloak 또는 코어의 명시적 상태), 변경 전달 방식, 최대 반영 지연, 원천 조회 실패 시 정책을 확정한다. 로컬 CLI 토큰은 기존 OIDC 토큰 introspection과 다른 경로이므로, 사용자 비활성화가 자동으로 전달된다고 가정하지 않는다. 발급·갱신·일반 API 요청·대기 작업 실행의 각 차단 시점과 이미 발급한 access token 처리 방식을 계약·검사로 고정한다. 원천에서 사용자를 비활성화한 실제 전파 증거 없이 즉시 차단 지원을 완료로 표시하지 않는다.

Windows는 Credential Manager를 우선하고 DPAPI CurrentUser 암호화+사용자 한정 ACL 파일을 대체 경로로 제공한다. Unix는 OS 보안 저장소를 우선하고 불가능할 때 0700 디렉터리·0600 파일을 사용한다. Docker에 넘기는 임시 비밀 파일도 Windows에서는 내용을 쓰기 전에 ACL을 제한하고 종료 시 정리한다. 토큰·verifier·비밀 파일 내용은 출력·진단·로그에서 제외한다.

`auth logout`은 서버 철회 성공과 로컬 삭제 결과를 구분한다. `--local-only`는 원격 세션이 남을 수 있음을 결과에 표시한다. `auth sessions list/revoke`는 본인 세션을 관리한다. AI가 로그인한 사용자를 자동 교체하거나 권한을 확장하지 않는다. 기존 프로젝트 토큰은 전역 파일로 무조건 복사하지 않고 서버/context를 확인한 뒤 새 로그인으로 전환하며, 이전 파일 정리 여부를 명시한다.

완료 조건은 브라우저·코드 로그인, 만료·거부·자동 갱신·동시 갱신·철회, 합의한 지연 안의 사용자 비활성화 전파, 다중 context 격리, OS별 저장 보호와 비밀 출력 방지다.

### 단계 3. 앱 목록과 작업 관찰 및 로그

앱·설치·사용자·권한·등록·작업 목록은 서버에서 권한을 적용한 뒤 필터링·페이지 처리한다. 안정적인 정렬과 opaque cursor를 사용한다. `job get/watch/resume`은 명시적인 ID 또는 저장한 요청 ID로 동작하고 `.miy/last-job.json` 하나에 의존하지 않는다.

기존의 runtime 요청 전 멱등성 정보 저장을 유지한다. 이미지 업로드 접수부터 요청 ID·대상 context/app·archive 해시를 로컬 journal에 먼저 기록하고, 받은 upload ID·상태를 연결한다. 접수 응답이나 업로드 완료 응답이 유실되어도 같은 요청 ID로 서버 업로드를 재조회하며 중복 접수·import·배포를 만들지 않는다. 권한·입력 해시·만료 상태를 확인하고 이후 runtime 작업 ID까지 이어서 복구한다. 이는 업로드 접수·상태 복구이며 대용량 전송의 byte-range 이어받기/재전송 재개를 포함하지 않는다.

`job retry`는 단계 1B의 타입 분류상 재시도 가능한 최종 실패에 한해 원본과 연결된 새 작업을 만들고 새 요청 ID를 먼저 저장한다. 인증·권한·계약·migration 실패는 자동 재시도하지 않는다. 현재 상태와 권한을 다시 검사하며 재시도 자체에도 멱등성을 적용한다. prod 변경 또는 `app remove` 작업은 명시적인 context/app/environment·영향 확인·비TTY `--yes`를 다시 요구한다. 원본 작업의 승인을 재사용하지 않는다.

작업 상태 변경과 이벤트를 같은 DB 트랜잭션에 저장한다. `job watch --output jsonl --after <cursor>`는 순서가 있는 이벤트를 재조회할 수 있게 한다. 시간 초과·Ctrl+C는 로컬 관찰만 종료하며 이미 접수된 원격 작업을 취소하지 않는다. 마지막 작업 ID와 cursor를 결과에 남긴다. terminal 실패는 종료 코드 9로 구분한다.

`app logs`는 현재 컨테이너의 유한한 스냅샷부터 제공한다. 기본 100줄, 요청당 최대 1,000줄/1MiB, `--tail`·`--since`, 잘림 여부·대상 컨테이너·실행 세대·회전으로 인한 누락 가능성을 응답에 포함한다. 앱/환경의 관리 권한으로 제한하고 작업 소유·컨테이너 소유·현재 실행 세대를 재검사한다.

코어에 Docker 접근을 추가하지 않는다. worker의 제한된 별도 진단 처리 경로로 로그를 수집하여 배포 제어와 health 관찰을 막지 않게 한다. 짧은 보존기간의 로그 결과는 TTL로 정리하고 조회 시에도 권한을 검사한다. 연속 `--follow`와 완전한 과거 로그 보관은 제공된 것으로 표시하지 않는다.

완료 조건은 프로젝트 밖 운영, 다중 작업 목록, 업로드 접수부터 runtime까지 응답 유실 복구, 재접속 이벤트 순서, 중복 업로드·작업 방지, 권한 회수 후 조회 차단, retry의 동일한 확인 정책, 로그 수집 중 다른 앱 제어의 독립성이다.

### 단계 4. 실행 전 검증과 확인

`app deploy`, `app promote`, `app start`, `app stop`, `app restart`, `app remove`에 `--dry-run`을 제공한다. 단계 1B의 공유 preflight·expected-state 타입을 사용하여 대상·권한·현재 상태·릴리스·이미지·예상 변경·DB 보존 여부를 검사하고 결과를 반환한다. 이미지 업로드, 작업 등록, migration, 컨테이너 변경은 하지 않는다. 실행 시점에만 검사 가능한 항목은 `not_checked`와 이유를 반환한다.

모든 prod 변경과 환경을 불문한 `app remove`는 명시적인 context/app/environment와 영향 요약을 요구한다. TTY에서는 확인하고, 비TTY에서는 `--yes`가 없으면 실패한다. dry-run 자체에는 변경 승인을 요구하지 않는다. 화면에서 확인한 상태와 실제 접수 시점의 상태가 다르면 새 확인을 요구하는 충돌로 처리한다. 서버 접수와 worker 실행 때 권한·기대 상태를 다시 확인한다. `job retry`도 원본 작업의 종류·대상에 따라 같은 경로를 사용한다. 로컬 `context remove`의 확인 범위는 단계 2를 따른다.

완료 조건은 dry-run의 원격 부작용 0건, 대상 누락·승인 누락 시 변경 0건, 확인 후 상태 변화/권한 회수 차단, 삭제 후 기존 DB·업무 데이터 보존이다.

### 단계 5. Windows 설치와 로컬 개발 및 배포물

Windows 빌드 대상은 `x86_64-pc-windows-msvc`다. 배포 CLI의 설치·원격 운영에는 Rust나 Visual Studio Build Tools를 요구하지 않는다. ZIP 수동 설치와 PowerShell 설치·갱신·제거를 제공한다. 기본 실행 파일 위치는 `%LOCALAPPDATA%\Programs\miy\bin`이고 `-AddToPath`를 지정했을 때 사용자 PATH만 바꾼다. 시스템 PATH·전역 실행 정책을 수정하지 않는다.

설치·갱신은 다운로드→체크섬 확인→임시 경로 검증→교체 순서다. 실행 중인 `miy.exe`나 프로젝트 MCP가 파일을 잠갔으면 기존 파일을 유지하고 `cli_in_use`와 재시도 방법을 반환한다. 자기 자신을 실행 중인 채 덮어쓰는 경로를 만들지 않는다. 설치 스크립트가 외부 프로세스로 교체를 수행하도록 한다. 제거 시 프로젝트·앱 데이터·설정·인증은 기본 보존한다.

`app setup/upgrade`도 파일 잠금과 사용자 문서 충돌을 쓰기 전에 검사한다. 중간 실패 때 실행 파일·스킬·매뉴얼·잠금 파일이 다른 버전으로 남지 않도록 staging과 복구 절차를 구현한다. 실행 중인 프로젝트 MCP는 자동 강제 종료하지 않는다.

PowerShell에서 `init → check → dev → dev-stop → dev → build → deploy`를 끝까지 수행한다. `.miy-tools/miy.exe`와 Codex/Claude MCP의 실행 경로·cwd를 올바르게 생성하고, JSON/TOML은 직렬화기로 쓴다. Windows child process는 Job Object 등 소유권이 있는 종료 수단을 쓰되 기존 run ID·프로젝트 잠금·협력 종료 계약을 유지한다. Ctrl+C·초기화 중단·재시작 후 정리에서 다른 앱과 기존 볼륨을 건드리지 않는다.

Docker Desktop이 Windows 컨테이너 모드라면 `doctor`가 필요한 Linux 모드로 안내한다. 로컬 CPU와 배포 서버 CPU를 구분하여 서버 capability와 이미지 platform을 대조하고, 필요한 경우 명시적인 빌드 platform을 선택하게 한다. 이미지 형식과 서버 실행 방식은 Linux OCI를 유지한다.

완료 조건은 실제 Windows 11에서 두 PowerShell 버전의 설치·운영·갱신·제거, 로컬 Docker 개발 흐름, Codex/Claude 프로젝트 MCP, 실행 파일 잠금 실패 후 기존 버전 보존이다.

## 5. 서버와 저장 계약 변경

기존 인증·권한·runtime 도메인을 재사용하고 API·CLI·MCP의 권한 판단을 분리하지 않는다. 다음은 추가할 인터페이스다. 상세 request/response 타입과 오류는 공통 계약에서 정의하고 서버·CLI 회귀 검사로 고정한다.

| 인터페이스 | 목적과 필수 조건 |
| --- | --- |
| `GET /api/v1/capabilities` | 서버/CLI 계약 버전, 지원 기능, 배포 CPU/platform, 미지원 기능의 명시적 판별 |
| `POST /auth/cli/device`, `/auth/cli/token`, `/auth/cli/revoke` | 코드 승인·토큰 교환·갱신·철회. 브라우저 승인도 같은 사용자 세션으로 연결 |
| `/api/v1/cli/sessions` | 본인 세션 목록과 ID별 철회 |
| 앱·권한·등록·작업 목록 API | 서버 측 권한 필터와 pagination. 기존 API가 있으면 계약 확장 |
| `GET /api/v1/runtime/jobs/{id}/events` | cursor 이후 상태 이벤트와 최종 상태 |
| 이미지 업로드 접수·상태 API 확장 | 요청 ID·archive 해시·소유자에 묶인 멱등 접수와 응답 유실 후 재조회. byte-range 전송 재개와 구분 |
| `POST /api/v1/runtime/jobs/{id}/retry` | 타입에 따른 재시도 가능성·권한·expected-state 검사, 원본 연결과 멱등성, prod/앱 제거 확인 |
| `POST /api/v1/apps/{app}/runtime/preflight` | 공유 검증과 expected-state 결과를 반환하는 dry-run. 작업 등록 없음 |
| 설치별 로그 요청·결과 API | worker 진단 경로, 관리 권한, 소유·실행 세대 검사, 제한·TTL |

Core DB에는 사용자 CLI 세션/refresh 관계, device 승인, 이미지 업로드 멱등 접수·재조회 정보, 작업 이벤트·타입별 실패·재시도 연결, 임시 진단 요청·결과를 추가한다. 사용자 비활성화 상태의 저장 여부와 전파 방식은 단계 2의 선행 계약에서 확정한다. `0008_runtime_health.sql` 뒤에 새로운 migration을 추가하며 기존 migration을 수정하지 않는다. 앱 DB 스키마 변경은 이 CLI 개선의 범위가 아니다.

서버를 먼저 배포하고 capability로 새 CLI의 필수 기능을 확인한다. 구 서버에는 `unsupported_capability`를 반환하며 위험한 우회 실행을 하지 않는다. 서버 갱신 중 기존 0.4.2 CLI가 사용하는 인증·작업 조회 경로를 유지하고, 새 CLI 전환과 구 세션 만료 후 제거 시점을 별도로 관리한다.

## 6. 검증과 릴리스 기준

### 공통 검증

- 명령 파싱·describe·help·입출력 스키마·매뉴얼·MCP의 필수값/enum/오류가 일치한다. JSON 성공·실패는 stdout 한 응답이며 stderr가 섞이지 않는다.
- 인증 승인 대기·거부·만료·갱신 경쟁·회전 재사용·철회·사용자 비활성화 전파·서버/context 혼동을 검사한다. 정상·오류·진단 출력에 비밀이 나오지 않는다.
- 이미지 업로드 접수·완료 및 runtime 접수 후의 응답 유실, 중복 접수·worker 중단·이벤트 재접속·재시도에서 업로드·작업·업무 변경이 중복되지 않는다.
- dry-run·권한 거부·승인 누락에서 원격 변경이 없고, 확인 후 상태/권한 변경을 다시 검사한다. prod 변경·`app remove`의 retry도 같은 확인과 expected-state 검사를 통과해야 한다.
- 로그 요청이 제어·health 관찰을 막지 않으며 다른 앱/환경 로그를 노출하지 않는다.
- 기존 runtime·DB·UI·컴포넌트·프로젝트 upgrade 회귀를 유지한다. 외부 서비스가 필요한 검사는 독립 환경에서 수행하고 증거 위치를 기록한다.

### Windows 필수 릴리스 검사

| 시나리오 | 통과 조건 |
| --- | --- |
| 설치·업데이트·제거 | 실제 Windows 11 x64, PowerShell 5.1/7, 일반 사용자 계정에서 성공 |
| 원격 운영 | Rust/Node/Docker가 없는 PC에서 로그인·앱 조회·작업 관찰·Linux 서버 관리 |
| 입출력 | 한글/공백 경로, 다른 드라이브, 파일/파이프 JSON UTF-8, `$LASTEXITCODE` 일치 |
| 자격증명 | Credential Manager 및 DPAPI 대체 경로, ACL, 갱신 경쟁, 출력 비밀 없음 |
| 파일 잠금 | 실행 중인 CLI/MCP·중간 실패에서 기존 바이너리/지침/잠금 일관성 보존 |
| 로컬 개발 | Docker Desktop Linux 모드에서 생성·검사·dev·정지·재실행·빌드·dev 배포 |
| 프로세스 정리 | Ctrl+C/준비 실패/중복 dev에서 자기 프로세스·컨테이너만 정리, 볼륨 유지 |
| 번들·MCP | OS 간 같은 번들 해시, Codex/Claude 프로젝트 MCP 실행과 계약 조회 |

일반 Windows CI에서 네이티브 빌드와 계약 검사를 수행하고, **별도 실제 Windows 11 + Docker Desktop 환경**에서 개발·배포 E2E를 수행한다. Windows 호스티드 runner에서 Docker Desktop이 작동한다고 가정하지 않는다. 실행 증거를 릴리스에 연결하며 컴파일 성공만으로 Windows 지원 완료를 선언하지 않는다.

### AI 사용 검증

Codex와 Claude Code 각각 독립적인 새 세션 3회에서 평가한다. 각 에이전트의 세 세션에 Windows PowerShell 7·Linux·macOS를 포함한다. 필요한 명령 찾기, 로그인 필요 상태 안내, 앱 개발/dev 배포, 잘못된 입력 복구, 관찰 중단 후 재개, 권한 거부, 명시적으로 승인한 prod 승격을 수행한다. 업무 결과는 별도의 인수 검사로 확인한다.

통과 조건은 필수 시나리오 성공, 의도하지 않은 변경·권한 우회·중복 작업 0건이다. 호출 수, 읽은 문서/스키마 크기, 수동 개입 수, 오류 복구 시간도 기록한다. 사람에게 보이는 친절한 안내와 AI의 구조화된 계약을 동일한 실행 경로에서 검증한다.

이 평가는 CLI 0.5의 사용성·운영 계약 검사다. [전체 검증 계획](VALIDATION.md)의 V7에 필요한 독립 작업공간·설치의 v1→v2 변경, 데이터 보존, 코어·runner·호스트 재시작과 전체 인수 3회를 대신하지 않는다. CLI 0.5 완료와 정식 V2~V7 완료는 별도로 기록한다.

### 배포 순서

1. 위 단계별 구현과 검사 결과를 [개발 인계](CLI-HANDOFF.md)의 체크리스트에 갱신한다. 커밋·환경·명령·결과·증거를 남긴다.
2. 서버 migration과 API/capability를 먼저 적용하고 기존 CLI 회귀를 확인한다.
3. CLI 0.5.0과 개발 번들 0.6.2를 준비한다. 기존 0.6.1 번들 스냅샷·보호 해시를 보존하고 명시적인 upgrade 경로를 추가한다. UI/Rust SDK 런타임 계약과 공용 컴포넌트 호환성을 불필요하게 변경하지 않는다.
4. 스크립트·생성 지침·프로젝트 CLI·MCP·매뉴얼을 새 명령으로 함께 이전한다. 기존 앱 업무 코드·SQL·컴포넌트·DB 보존을 검사한다.
5. OS/CPU 검사와 실제 Windows·AI 평가를 통과한 뒤 네 가지 Unix 바이너리와 Windows x64 ZIP을 GitHub Releases에 공개한다. 각 파일의 SHA-256, 빌드 출처/commit, 설치·갱신·제거 안내를 함께 제공한다.

계획을 저장한 `9757cc8`은 문서만 변경한 커밋이다. 후속 구현으로 CLI 0.5.0/번들 0.6.2를 준비했으며 서버·DB 적용 및 검사 결과는 실제 수행한 범위만 별도로 기록한다. **Windows에서 실제 CLI 실행·관리·로컬 개발까지 통과해야 이 계획을 완료로 처리한다.**

## 7. 설계 참고 자료

다음 자료는 앞선 CLI 조사에서 참고한 공식 문서다. miy에 적용할 계약은 위 구현 계획을 기준으로 하며 외부 도구의 기능 전체를 복제하지 않는다.

- [GitHub CLI formatting](https://cli.github.com/manual/gh_help_formatting): 구조화 필드 선택과 자동화용 출력.
- [Vercel CLI for agents](https://vercel.com/changelog/vercel-cli-for-marketplace-integrations-optimized-for-agents): 명령 발견·안내와 에이전트 사용 평가.
- [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html): 사용자 코드, device authorization, polling·만료·거부 의미.
- [Rust std::process::Command](https://doc.rust-lang.org/std/process/struct.Command.html): Windows 실행 파일 탐색과 배치 런처 인수 처리.
- [Docker Desktop on Windows](https://docs.docker.com/desktop/setup/install/windows-install/): Windows 설치와 Linux 컨테이너 실행 환경. 실제 검증 시 지원 조건을 다시 확인한다.
