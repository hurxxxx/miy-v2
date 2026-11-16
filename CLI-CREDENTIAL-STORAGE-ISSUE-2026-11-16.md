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
