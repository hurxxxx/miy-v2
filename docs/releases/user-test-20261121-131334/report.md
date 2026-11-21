# 신규 사용자 검증 — 2026-11-21

대상은 `https://portal.miy.works` (`185.223.207.238`)이다. 플랫폼 저장소 밖의 `~/miy-user-test/20261121-131334`에 Codex와 Claude Code의 작업 폴더와 miy 설정을 분리했다. 포털과 포털이 배포한 CLI·프로젝트 안내를 사용했다.

| 검사 | Codex 노트 | Claude Code 파일 |
|---|---|---|
| 포털 로그인 | PASS — Safari 개인정보 보호 창 | PASS — 공통 진입점 확인 |
| macOS arm64 설치와 새 셸 | PASS — `miy 0.14.0`, 활성 서버 일치 | PASS — `miy 0.14.0`, 활성 서버 일치 |
| macOS CLI 인증 | FAIL — macOS 키체인 쓰기 오류 | FAIL — 같은 키체인 쓰기 오류 |
| Debian 13 arm64 CLI 인증·카탈로그·등록 승인 | PASS | PASS |
| CLI 프로젝트 생성 | PASS — `user-test-codex-20261121` | PASS — `user-test-claude-20261121` |
| 에이전트의 실제 miy MCP 호출 | PASS — `get_cli_help`, 인증된 `get_contract` | PASS — `get_cli_help`, 인증된 `get_contract` |
| 개발 배포 | PASS — 0.1.0 | PASS — 최종 0.2.1 |
| 운영 배포 | PASS — 0.1.0 | PASS — 0.2.1 |
| 브라우저 기능·권한 검사 | PASS — 아래 확인 범위 | PASS — 0.2.1에서 재검증, 아래 확인 범위 |

## 실행과 재현

- `curl -fsSL 'https://portal.miy.works/api/v1/cli/install.sh' | bash`를 에이전트별 HOME에서 실행했다. 새 `zsh -ic`에서 `command -v miy`와 `miy --version`이 모두 성공했다.
- `miy auth login`은 브라우저를 열기 전에 종료 코드 7과 `credential_storage_unavailable`을 반환했다. `miy auth login --no-open --no-wait --output json`으로 두 환경에서 재현했다. 안전 진단 값은 `backend=macos_keychain`, `operation=write`, `osStatus=-60008`, `reason=credential_keychain_write_failed`였다. `miy doctor --for operate --output json`은 서버 연결과 CLI 호환성을 통과했고 세션이 없어 인증 검사를 건너뛰었다.
- Linux 대체 경로에서 Debian 12 arm64 설치는 `GLIBC_2.39` 요구 조건 때문에 두 번 실패했다. Debian 13 arm64 재시도에서는 양쪽 모두 설치에 성공했고 `miy auth login --no-open --no-wait`가 기기 승인 요청을 생성했다. 공개 설치 화면에서 확인한 Linux 안내에는 최소 glibc 버전이 표시되지 않았다.
- `miy app init ./workspace --id <앱 ID> --name <앱 이름>`은 두 프로젝트에서 성공했다. 생성한 프로젝트에는 SDK 번들 `0.11.0`과 Codex·Claude용 MCP 설정이 포함됐다. `miy component list --output json`은 두 곳에서 `login_required`로 종료했다.
- 새 Codex 설정으로 실행하면 OpenAI 요청이 `401 Unauthorized`로 끝났다. 재시도에서는 호스트에 이미 로그인된 Codex 세션을 사용하고 miy HOME과 앱 작업 폴더는 계속 격리했다. `codex exec --trust`에서 생성된 프로젝트의 `miy_app.get_cli_help` 호출 완료를 확인했다. 이 호출은 사용자용 CLI 안내를 반환한다.
- 생성된 프로젝트의 MCP 설정을 사용한 `claude -p`는 `Not logged in · Please run /login`을 반환했다. Anthropic의 일반 브라우저 로그인으로 복구한 뒤 `claude auth status`에서 로그인 상태를 확인했다.

- Debian 13에서 두 CLI의 기기 승인을 콘솔에서 완료했다. `miy auth wait`, `miy auth status`, `miy component list`, `miy component sync`, `miy app register`, `miy registration approve`가 각각 성공했다.
- 관리 콘솔에서 인사 조직 하나와 일반 사용자 세 명을 만들었다. 소유자와 조직원 두 명의 주 소속을 지정하고 외부 사용자에게는 소속을 지정하지 않았다. 세 사용자의 두 테스트 앱 `use` 권한을 dev·prod에 부여해, 외부 사용자도 앱 내부의 자료 권한을 확인할 수 있게 했다.
- 각 에이전트에 계획의 한 줄 요청을 입력했다. 에이전트가 생성한 앱 소스를 인증된 Linux 작업본에 복사하고 작업본에서만 커밋한 뒤 `miy app build --no-wait`를 제출했다.

## 개발·운영 결과

| 앱 | 개발 포털 | 운영 포털 |
|---|---|---|
| 모아 노트 | [개발 노트](https://dev.miy.works/environments/dev/apps/user-test-codex-20261121) | [운영 노트](https://portal.miy.works/environments/prod/apps/user-test-codex-20261121) |
| 파일 보관함 | [개발 파일](https://dev.miy.works/environments/dev/apps/user-test-claude-20261121) | [운영 파일](https://portal.miy.works/environments/prod/apps/user-test-claude-20261121) |

- macOS 프로젝트의 MCP에서 라이브 계약·API 탐색은 인증이 없어 실패했다. 재검증에서는 각 에이전트의 stdio 설정에 별도 실행 래퍼를 사용해 Debian 13 프로젝트의 인증된 `.miy-tools/miy mcp`를 실행했다. 두 실제 에이전트에서 `get_contract` 응답을 확인했다. 일반 macOS 인증 경로의 성공으로 집계하지 않았다.
- 노트의 비동기 `app build --no-wait` 완료를 `job get`으로 확인한 직후 배포 dry-run은 `build_required`였다. `job resume <빌드 ID> --environment dev` 이후 dry-run에 성공했다. `--if-current`에 dry-run JSON 전체 또는 `expectedState`만 전달하면 `invalid_input_schema`로 거부됐다. 별도 조건 인자 없이 가이드의 `app deploy --environment dev --no-wait`를 실행해 배포했다.
- 파일의 최초 서버 빌드는 Rust 포맷 검사에서 `build_compilation_failed`로 종료됐다. 생성된 앱에서 `cargo fmt --all`과 `cargo fmt --all --check`를 실행하고 커밋한 뒤 다시 빌드했다. 0.2.0 빌드와 개발 배포가 성공했다.
- 두 개발 설치 모두 `app status` 및 실제 브라우저 진입을 확인했다. 일반 사용자의 노트 목록·저장 요청은 처음에 일반 권한 오류 안내를 표시했고, 파일 업로드는 `delegation_unavailable`을 표시했다. `permissions get`에서 승인 배열이 비어 있는 것을 확인하고 `permissions set`으로 릴리스별 요청 권한만 승인했다. 노트에는 디렉터리 읽기, 파일에는 디렉터리 읽기와 파일 읽기·쓰기를 승인했다.
- 개발 노트에서 소유자가 개인 문서를 작성·수정·새로고침·휴지통 이동했다. 조직원은 공용 문서 본문을 읽을 수 있었고 편집·휴지통 버튼이 없었다. 조직원의 개인 링크와 외부 사용자의 공용 목록·직접 링크는 차단됐다. 조직원의 주 소속을 해제한 뒤 기존 세션 새로고침과 재로그인에서 공용 링크를 다시 열어 차단을 확인했다. 다음 검증에 쓰기 위해 해당 조직원의 주 소속을 복구했다.
- 개발 노트의 정확한 릴리스를 `app accept --input -`로 인수하고, `app promote --environment prod --release <릴리스 ID>`의 dry-run과 제출을 실행했다. 운영 작업은 성공했고 `app status`에서 정상 실행을 확인했다. 운영 설치의 릴리스 권한도 확인했다. 일반 소유자로 운영에서 개인·공용 문서를 작성하고 개인 문서 수정 결과가 새로고침 후 유지되는 것을 확인했다. 운영의 첫 목록에는 개발 문서가 없었다. 조직원에게는 공용 본문만 열렸고, 개인 링크는 차단됐다. 외부 계정의 운영 공용 링크도 차단됐다. 운영에서 소유자가 공용 문서를 수정하고 휴지통 이동·복원까지 확인했다. 운영 노트의 권한 회수 재검사는 `NOT_RUN`이다.
- 파일 0.2.0은 권한 승인 이후 선택한 합성 파일의 업로드에서 `http_404`를 재현했다. 생성된 SDK 안내에서 `fileApi`가 이미 `/api/files`를 붙이는 것을 확인했다. Claude Code가 업로드·다운로드 두 호출의 중복 `/files`를 제거하고 버전을 0.2.1로 올렸다.

- 파일 0.2.1을 서버에서 새로 빌드·개발 배포한 뒤 해당 릴리스의 권한을 다시 승인했다. 소유자의 개인·공용 업로드, 새로고침 후 개인 목록 유지, 소유자의 개인 다운로드, 조직원의 공용 다운로드가 성공했다. 두 다운로드의 바이트를 합성 원본과 비교했다. 조직원에게 소유자 개인 파일이 보이지 않았고 공용 파일에는 관리 동작이 표시되지 않았다. 외부 사용자는 두 목록에서 검증용 파일을 볼 수 없었다.
- 파일 0.2.1을 `app accept`와 `app promote`로 운영에 승격했다. 운영 설치 상태는 정상이며 같은 릴리스를 사용했다. 운영 권한 승인 후 첫 개인 목록에 개발 자료가 없었다. 운영용 개인·공용 합성 파일을 올리고, 소유자의 개인 다운로드와 조직원의 공용 다운로드가 각각 원본과 일치함을 확인했다. 조직원과 외부 사용자의 목록·동작 범위를 개발에서와 같이 확인했다.
- 운영 파일 앱에서 조직원의 주 소속을 해제하고 기존 브라우저 세션 새로고침과 재로그인 후 공용 파일이 목록에서 사라짐을 확인했다. 외부 계정에도 확인 후 해당 조직원의 주 소속을 다시 지정했다. 앱은 다운로드 버튼에서 세션 내 Blob을 생성하며 자료를 다시 열 수 있는 고정 링크를 제공하지 않았다. 파일 직접 링크 검사는 `NOT_RUN`이다.

## 미실행 및 최종 상태

- 파일 삭제는 사용자 지시에 따라 개발·운영 모두 `NOT_RUN`으로 남겼다. 합성 파일은 보존했다.
- 앱 화면에서 다운로드용 고정 링크가 제공되지 않아 파일 직접 링크 접근 검사는 `NOT_RUN`이다. 노트 직접 링크는 개발·운영에서 검증했다.
- 권한 회수는 개발 노트와 운영 파일에서 기존 세션과 새 세션으로 확인했다. 개발 파일과 운영 노트의 추가 회수 재검사는 Safari 개인정보 보호 창 잠금으로 `NOT_RUN`이다.
- 마지막 `miy app status --output json`에서 두 앱의 개발·운영 설치 네 곳이 모두 `running`, `healthy`였다. 노트는 0.1.0, 파일은 0.2.1이다.
- `git diff --check`와 변경한 Markdown의 상대 링크 및 끝 공백 확인이 통과했다. 이 저장소의 빌드와 테스트는 소스 변경 없이 문서만 추가해 `NOT_RUN`이다. 두 테스트 앱의 서버 빌드와 브라우저 결과는 위에 기록했다.

## 개선 후보

1. Linux 설치 화면과 사전 검사에서 최소 glibc 버전을 알려준다. 현재 사용자는 다운로드한 바이너리의 로더 오류로 호환 조건을 처음 확인한다.
2. macOS 키체인 쓰기 실패 메시지에서 현재 실행 환경으로 해결할 수 있는지 알려준다. 이 실행에서 웹 포털 로그인과 CLI 설치는 별도로 성공했다.
3. 배포 성공 직후 화면에서 릴리스 권한 승인 필요 여부와 승인 경로를 표시한다. 파일 API 가이드에 전체 라우트와 상대 경로를 짝지은 업로드·다운로드 예제를 추가한다.
4. 공용 파일 공유 대상은 이름으로 검색·선택할 수 있게 한다. 파일 앱 초판은 원시 조직 UUID가 필요해 관리 콘솔 감사 기록에서 식별자를 확인한 뒤에야 업로드할 수 있었다. 노트 앱에서는 같은 조직을 표시 이름으로 선택할 수 있었다.
5. [공통 컴포넌트 후보](../../PLATFORM-COMPONENT-CANDIDATES.md)에 파일 전송 UI와 저장소의 후속 검증 범위를 기록했다.

## 2차 검증 — CLI·HTTPS·파일 체크섬

기존 배포와 합성 자료를 사용했다. Chrome 시크릿 창에서 운영 포털과 로그인 화면까지 확인했다. 입력 중 Chrome이 다른 창으로 전환되어 일반 사용자 로그인 이후는 `NOT_RUN`으로 기록했다. 사용자가 브라우저 작업을 이어받은 뒤 재검증은 다음 방식으로 진행했다.

| 검사 | 결과 | 관찰 |
|---|---|---|
| `miy app source-pin` ↔ `miy app status` | PASS | 두 앱 모두 로컬 소스 커밋이 개발·운영 배포에 기록된 커밋과 일치했다. 앱별로 개발·운영은 같은 이미지와 릴리스를 사용했고 설치 ID는 달랐다. 네 설치가 `running` / `healthy` / `ready`였다. |
| `miy permissions get`, `miy grant list --app` | PASS | 네 설치에서 승인 권한과 릴리스 요청 권한이 일치했다. 테스트 일반 사용자 세 명의 두 앱 개발·운영 권한은 모두 `use`였다. |
| `curl` HTTPS 응답 | PASS | 개발·운영의 앱 진입 URL 4개가 지정 IP에서 HTTP 200을 반환했고 TLS 인증서 검증이 통과했다. 두 포털의 `/auth/login`은 HTTP 303으로 miy 콘솔의 OpenID Connect 인증 경로로 이동했다. |
| 다운로드 파일 SHA-256 | PASS | 개발·운영 각 환경에서 앞서 내려받은 개인·공용 합성 파일 4개가 모두 해당 원본과 일치했다. |

HTTP 200은 포털 진입 응답을 확인한 결과다. 이번 라운드의 앱 내부 일반 사용자 접근, 새 파일 전송, 파일 삭제는 `NOT_RUN`이다. 1차 브라우저 결과와 2차 CLI·HTTPS 결과를 별도로 기록한다.
