# Windows 네이티브 빌드·운영 검증 안내

## Linux 후속 완료 — 새 CI 설치기 0.6.3 게시

아래 Windows 로컬 기록 이후 [Linux 검증과 서버 반영](CLI-0.6.3-LINUX-VALIDATION.md)을 수행했다. [설치 안내](https://v2.dwdcc.kr/cli/install)는 이제 `8d7d042`의 검증된 CI CLI와 설치기를 제공한다. 기존 Windows 로컬 `0db21c3` 파일과 해시는 다르다. 서버·UI·index 0.6.3 반영, 실제 390px Todo와 기존 DB/레코드 보존을 확인했다. 이번 CI 설치기를 사용자 Windows 11에서 실행한 결과와 실제 IdP 정상 로그인은 아직 **NOT_RUN**이다.

## 2026-10-29 CLI 0.6.3 설치·로그인·모바일 개선 검증

0.6.2 실제 전체 흐름에서 남긴 개선을 `codex/cli-experience-improvements`에 구현했다. 기능 소스는 `0db21c3`, Windows CI 도구 검색 격리를 보완한 소스는 `8d7d042`다. CLI **0.6.3** / 번들 **0.7.1**이며 [구조화 결과](releases/cli-0.6.3-experience-validation.json)와 [서버 측 후속 프롬프트](CLI-EXPERIENCE-SERVER-HANDOFF.md)를 보존한다.

| 개선·검사 | 실제 결과 |
| --- | --- |
| 설치기 후보 선택 | 0.6.0 고정 버전·해시를 제거하고 검증한 CLI 후보의 출처/해시를 명시적으로 받음. 로컬 Windows 11 / **Windows PowerShell 5.1.26100.9711에서 실제 설치·업데이트·설정/인증 보존·제거 24개 PASS** |
| 설치기 입력 검증 | ZIP 해시·버전·CLI commit·실행 파일 해시 불일치 **4개 거부 PASS**. 패키지 무결성 검사 **15개 PASS / Linux 게시 검사 4개 SKIP** |
| Docker 없는 빌드 안내 | Docker가 없거나 Linux 엔진이 아니면 `miy app build --remote`를 메시지와 recovery argv에 제공. 자동 소스 업로드나 Docker 설치 전환 없음 |
| 로그인 복구 | CLI 만료 시각·남은 시간·원래 context의 재로그인 명령 제공. 서버 상태 기반 승인 화면, 만료/처리 완료·잘못된 코드·통신 장애 복구 구현. OAuth 실패 쿠키 정리·고정 복구 경로 구현 |
| 모바일 | 390px에서 메뉴 기본 접기·펼치기·이동 후 닫기·전체 workspace 폭 확인. 승인 입력과 요청 ID 줄바꿈, 가로 넘침 없음 확인 |
| Windows CLI | 단위 **106개**, fmt·strict Clippy·release PASS. debug/release 출력 **13그룹**, 원격 빌드 fixture **10그룹**, 복구 **6그룹** PASS |
| 화면·배포물 | 실제 Chrome + 로컬 플랫폼/모의 API 경계의 브라우저 **14개 PASS / 실패·skip·flaky 0**, TypeScript·플랫폼 Vite build PASS |
| 교차 OS 기반 CI | `8d7d042`의 [CI 40826944825](https://github.com/hurxxxx/miy-v2/actions/runs/40826944825): Linux 서버·Linux/macOS/Windows 클라이언트·내장 번들 비교 전체 PASS |

로컬 후보 `MiySetup-0.6.3-windows-x64.exe`는 **6,105,600바이트**, SHA-256 `f201775aa7e9dd7b326e48ae4ea9864e30aa30204acb335557d3eaefbeb97de7`이다. 내장 CLI와 설치기 소스는 `0db21c38d89061c4fb6d774585ffba46ee10407c`, CLI SHA-256은 `2d1b0994b069b6315f816df6d934afba0fe82eb993f408345d01fa187a4cbe61`, ZIP SHA-256은 `401cf840ff2c38dc846f711af8494fb0d4c4e70d202036d3973c2c1f2a765fd9`다. 별도 깨끗한 소스 checkout에서 패키징했고 파일은 `.local/releases/cli-0.6.3-local-0db21c3/`에 있다. 이 후보를 소스가 다른 CI artifact로 표시하지 않는다.

최초 단위 실행의 사용자 context 유입 실패는 격리한 `MIY_CONFIG_DIR`로 재실행했다. 처음 브라우저 설정에 포함했던 실제 서버용 3개 검사는 localhost:18000 미구동으로 실패했으며 모의 API suite와 실제 서버 suite를 명시적으로 분리했다. 첫 CI에서는 빈 PATH만으로 Windows의 시스템 Docker 검색을 막지 못했다. 복사한 CLI 옆에 신뢰한 실패용 실행 파일을 배치해 격리했고 동일한 strict assertion으로 새 CI를 통과했다. 검사 조건을 완화하지 않았다. C# 빌드의 기존 `Capture` 멤버 CS0108 경고는 비차단 이력으로 남긴다.

[모바일 승인 화면](releases/evidence/cli-0.6.3-experience/mobile-approval.png), [만료 복구 화면](releases/evidence/cli-0.6.3-experience/expired-request.png), [설치 완료 화면](releases/evidence/cli-0.6.3-experience/installer-completed-form.png)은 로컬 fixture이며 화면의 계정·코드·loopback 서버는 합성 값이다. 실제 IdP/DB를 사용하는 새 인증 API의 Linux 통합 회귀와 플랫폼 재배포, 실제 서버의 모바일 Todo 확인, 공개 설치 파일 게시, 전역 CLI 교체는 **NOT_RUN**이다. 기존 실제 Todo 앱과 DB는 이 실행에서 변경하지 않았다. `publish=false`인 [5개 OS/CPU 후보 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40826960538)는 결과 JSON의 시점별 상태를 따르며, 미완료 job은 PASS로 세지 않는다.

## 2026-10-29 CLI 0.6.2 Windows → 서버 빌드 → dev 배포 검증

서버 빌드 구현 `1775eac`를 동기화하고 Windows 수정 `50edd05`와 fixture 수정 `2fabe68`을 적용한 CLI **0.6.2** / 번들 **0.7.1**로 실제 전체 흐름을 통과했다. [기계 판독 결과](releases/cli-0.6.2-windows-server-build-validation.json)와 [최종 브라우저 화면](releases/evidence/cli-0.6.2-windows-server-build/deployed-final.jpg)을 보존한다. 환경은 Windows 11 Pro x64 build 26200, 비승격 PowerShell 7.6.5, Rust 1.99.1 MSVC, Node 24.19.0, pnpm 10.34.5다. 프로젝트 CLI를 소스에서 갱신했으며 공개 GUI 설치기·전역 CLI를 새로 게시한 결과는 아니다.

Docker Desktop을 설치하지 않고 기본 `app dev`의 Rust 서버·React 정적 화면·서버 개발 DB 연결을 확인했다. 이후 로컬 세션을 정상 종료하고 `app build --remote` → 작업 조회·로그·resume → dev 배포를 실행했다. 원격 명령 동안 Docker·Rust·Node·pnpm·SSH·WSL을 실패 trap으로 막았고 호출은 **0회**였다. Linux 워커에서 앱 검사와 이미지 생성이 수행됐으며 이미지를 Windows로 내려받거나 다시 업로드하지 않았다.

| 검사 | 실제 결과 |
| --- | --- |
| Windows 클라이언트 | 단위 **106개 PASS / 실패·ignored 0**, fmt, all-targets Clippy `-D warnings`, release 빌드 PASS |
| CLI 계약·복구 | 출력 **11그룹**, runtime 복구 **6그룹**, 서버 빌드 오류·재개 fixture **10그룹**, 지침·MCP·보존 업그레이드 **5그룹** PASS |
| 실제 프로젝트 | 네이티브 doctor **17항목** PASS. 서버 워커의 앱 Rust 검사 **6개**, frozen/offline 의존성·TypeScript·Vite·배포물 검증 PASS |
| 실제 원격 빌드 | `1ba1607e-9915-4b2b-b946-d1fa8018e961` 성공. 반복 요청·resume은 동일 작업, 순서 로그 **114개**, 마지막 cursor 이후 빈 로그 확인 |
| 실제 dev 배포 | `c45dc134-e0ee-45ce-8cc2-658a15b4a6fc` 성공. Todo **0.1.2**, release `871549de-041d-4fa7-8c17-b8e7b3c54e47`, runtime running/healthy |
| 브라우저·API | 로컬 추가·완료·새로고침, 배포 후 기존 데이터 표시·완료 취소·새로고침, 배포 API CRUD PASS. release 헤더 누락은 예상한 409로 거부 |
| 데이터 보존·정리 | 같은 DB UUID·스키마·기존 Todo와 테스트 Todo의 ID·데이터 유지. 서버 브라우저의 변경을 재시작한 로컬에서도 확인. 테스트용 레코드만 삭제, 원래 Todo 보존, 백엔드·포트·개발 세션 정리 PASS |

배포 이미지 `sha256:386e71c86012e6a55c579fc2482bf255fb3cbab8f83f99c0d3bb50799164a75f`는 서버 빌드 영수증과 일치한다. 앱 소스는 `f7e898161688f114edbc81941cb7592cda358b1e`, Windows CLI SHA-256은 `98fa982280fb041b1136a11bdf499979e9ce6133536ae51bd8b05d54888d9b13`이다. [실제 개발 앱](https://v2.dwdcc.kr/environments/dev/apps/todo-win-20261029)은 정상 실행 상태로 남겼고 해당 release에 범위를 명시한 구조화 인수 기록을 저장했다.

최초 strict Clippy는 Windows에서 새 closure 두 곳의 `result_large_err`로 실패했다. 오류 원인을 보존하는 `anyhow::Result`로 수정하고 재검사했다. 원격 빌드 fixture는 Python의 Windows CRLF와 기본 인코딩 때문에 소스 해시가 달라 실패했으므로 UTF-8/LF를 명시하고 Windows CMD trap을 추가했다. 잘못된 로그 cursor와 손상된 영수증의 오류 계약도 추가 검증했다. 검사 완화나 lint suppression은 사용하지 않았다.

이번 계정은 사용자가 선택한 admin이다. 개발자·일반 사용자 최소 권한, PowerShell 5.1의 새 전체 흐름, 실콘솔 Ctrl+C·워커 강제 종료·실제 통신 장애, 모바일·이탈 방지, 운영 승격은 **NOT_RUN**이다. 실패 fixture와 실제 서버 성공을 구분하며 아래 구버전 결과를 이번 소스의 통과로 이월하지 않는다. 원본 로그는 `.local/evidence/windows-server-build/20261029-131310/`에 보존한다. 토큰·쿠키·DB 접속 문자열은 커밋하지 않는다.

## 2026-10-29 Docker 없는 네이티브 개발 검증

사용자 요청으로 Docker Desktop 설치를 중단하고 `b873a81`의 CLI **0.6.1** / 번들 **0.7.1**에서 기본 로컬 개발을 네이티브 Rust·React 정적 미리보기·프로세스 내부 서버 DB 터널로 변경했다. [사용법](NATIVE-DEVELOPMENT.md)과 [기계 판독 결과](releases/cli-0.6.1-native-windows-validation.json)를 함께 보존한다. 기존 홈페이지의 0.6.0 후보와 구분하며 새 GUI 설치기를 게시한 결과가 아니다.

마지막 동기화에서 추가된 main `564df40`의 공용 개발 계정 로그인 변경을 `60a9952`에 병합했고, fmt·Clippy·release build와 단위 **98개**를 다시 통과했다. 네이티브 미리보기 구현은 동일하며, 아래 실제 브라우저 증거의 바이너리는 최초 `b873a81`로 구분한다. [Linux 후속 Codex 프롬프트](LINUX-TODO-HANDOFF.md)는 병합한 브랜치에서 배포를 검증하도록 안내한다.

실제 Windows 11 Pro x64 build 26200 일반 사용자 / PS7.6.5 / Rust 1.99.1 MSVC / Node 24.19.0 / pnpm 10.34.5에서 CLI 단위 **97개**, fmt·Clippy 경고 금지·release build, 출력 계약 **11그룹**, 지침·MCP·0.7.0→0.7.1 보존 업그레이드 **5그룹**을 통과했다. 사용자 설정을 사용하는 첫 단위 실행의 metadata 기대값 실패는 격리한 `MIY_CONFIG_DIR`로 다시 수행해 통과했으며 assertion을 완화하지 않았다.

별도 Codex CLI 새 세션이 프로젝트 지침과 실제 MCP 5종을 사용해 DB Todo 앱을 구현했다. 앱 Rust 검사 **6개**, `app check` **11항목**이 통과했다. 네이티브 업그레이드 뒤 `doctor --for develop`은 Docker 없이 **17항목 전체 통과**했다. 브라우저에서 추가·완료·완료 취소·새로고침, 실제 서버 DB API CRUD, 빈 제목·소유자 위조·CSRF 누락·서명 없는 직접 백엔드 접근 거부를 확인했다. pnpm EPERM 일회 실패는 변경 없는 재실행으로 통과했으며 Federation 분석 시간 경고를 기록한다.

`app dev-stop` 뒤 백엔드 프로세스·로컬 포트·터널·서버 개발 세션의 정리와 exit 0을 확인했고, 재시작 후 DB `129f8abd-62c2-40d1-9175-d73d85196940`와 Todo `8dcfffe2-3854-4ce5-b878-2da9f915c151`의 ID·제목·완료 상태를 그대로 유지했다. 재시작한 브라우저에서도 같은 항목을 확인했다. Windows 세션은 Linux 배포 인계를 위해 정상 종료했다. 업무 소스는 [Todo 예제](../examples/native-todo/README.md)에 있다.

사용자가 admin 계정을 선택했으므로 개발자 최소 권한 인수로 확대하지 않는다. Linux 이미지 빌드·dev 배포·배포 후 동일 DB/레코드 확인, 실제 콘솔 Ctrl+C·강제 종료·다른 OS, 운영 승격은 **NOT_RUN**이다. Linux 후속 Codex에 해당 브랜치와 예제를 전달한다. 원본 증거와 화면은 이 PC의 `.local/evidence/windows-todo/20261029/`에만 보존하며 비밀 정보는 결과 문서에 포함하지 않는다.

## 2026-10-28 CLI 0.6.0 후보 검증 안내 이력

- 당시 갱신일: 2026-10-28
- 당시 구현 대상: CLI **0.6.0** / 개발 번들 **0.7.0**. 아래 0.5 후보·0.4.2 실행 기록은 해당 버전의 이력이다.
- 대상: Windows 11 x64, Windows PowerShell 5.1과 PowerShell 7
- 당시 상태: Windows에서 사용자가 직접 빌드·검증하고 결과를 추가할 안내. **이 0.6.0 후보의 실제 Windows 11·PowerShell 5.1/7·원격 DB·독립 AI 세션 인수는 NOT_RUN이었다.** 최신 0.6.2 결과는 위 절에서 별도로 확인한다. 과거 호스티드 Windows MSVC CI 결과는 이 후보의 인수 결과로 이월하지 않는다.

## CLI 0.6 인수 준비 — 2026-10-28

현재 CLI 후보는 `b67798b`, GUI 설치기 후보는 `94a02c7`이다. 설치기 자체의 Windows Server 네이티브 검사 24개와 게시 증거는 [GUI 설치 안내](releases/windows-installer-0.6.0-candidate.json)에 기록한다. [설치 화면](https://v2.dwdcc.kr/cli/install)에서 Windows GUI 설치 파일을 받는다. [설치 안내](WINDOWS-INSTALLER.md)처럼 빈 입력란에 회사 서버 주소를 입력한다. ZIP과 PowerShell 명령도 보조 경로로 유지한다. [후보 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949)의 Windows Server 2022에서 단위 95개, 출력 11그룹, 서버 등록 9그룹, PowerShell 5.1/7 설치 각각 9개가 통과했다. 사용자의 실제 Windows 11 인수는 아직 NOT_RUN이며 [핵심 흐름 테스트](CORE-TESTING.md)부터 진행한다.

서버 URL은 사용자가 명시하거나 홈페이지 설치 명령의 `-Server URL`로 전달한다. 기본 IP·도메인은 없다. 주소를 생략한 대화형 설치는 URL을 질문하며, 비대화형 설치는 실행 파일만 설치하고 `contextInitialization.status=required`와 `miy context init`을 안내한다. 이후 `miy context init` 또는 최초 대화형 로그인에서 등록한다. `-NonInteractive`/`--no-input`에서는 입력을 기다리지 않는다. 기존 설정은 보존하고 다른 요청 주소는 `activeOrigin`, `requestedOrigin`, `serverMismatch`로 구분한다.

이번 검증은 개발 도구가 준비된 Windows 11 일반 사용자 PC에서 시작한다. 홈페이지의 검증된 후보 설치 → 명시적 서버 등록 → 개발자 로그인 → 앱 생성 → 앱 등록 요청/별도 관리자 승인 → 컴포넌트 동기화 → doctor/check → 로컬 개발 → dev-stop → build/deploy → 개발 배포 확인/인수 → 별도 운영 권한으로 promote 순서다. Codex·Claude는 각각 새 프로젝트 세션에서 업무 요구만 전달하고, 생성된 지침·CLI·MCP가 사용되는지 기록한다.

- `doctor --for develop`은 버전 검사 외에 rustfmt, 임시 Rust 실행 파일 링크(MSVC/Windows SDK), 실제 Codex·Claude MCP 설정과 제한 시간 내 로컬 stdio initialize/tools/list를 검사한다. 이 로컬 probe는 실제 AI 세션과 API 도구 성공을 대신하지 않는다.
- 기본 `app dev`는 서버 개발 DB를 사용한다. 로컬에서 만든 레코드가 개발 배포 후 그대로 남는지 DB 식별자와 함께 확인한다. 별도 로컬 DB는 `--database local`로만 선택한다.
- 앱당 개발 세션 하나이며 배포 전에 `app dev-stop`으로 종료한다. 스키마 변경은 배포된 개발 앱을 중지할 수 있다. 운영 DB는 별도이고 개발 데이터는 복사하지 않는다.
- HTTPS/WSS와 인증서 없는 사내 HTTP IP/WS 구성, 주소 누락·입력 취소·기존 연결 보존, 권한 회수·세션 만료·연결 단절·마이그레이션 실패를 각각 기록한다. 실패 뒤 수행하지 못한 항목은 NOT_RUN으로 남긴다.
- 현재 후보의 commit·배포물 해시·CI와 Linux 서버의 실제 전체 흐름 결과는 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 기록했다. 사용자 Windows 결과는 실제 실행 후 추가한다. 아래 과거 후보 파일을 이번 기능의 설치 파일로 사용하지 않는다. UI/UX 개선은 TODO로 기록한다.

이 문서는 현재 구현된 클라이언트 빌드 경계, OS 유틸리티, 앱 생성·설정과 번들 바이트 보존을 확인한다. [CLI 0.5 구현 계획](CLI-IMPLEMENTATION-PLAN.md)의 전체 기능이나 Windows 정식 지원 완료를 선언하지 않는다. 두 PowerShell 버전에서 아래 절차를 각각 실행하고, 같은 commit이라도 실행 기록은 별도로 남긴다. WSL 결과는 Windows 네이티브 결과로 기록하지 않는다.

## 1. 현재 증거와 이번 검증 범위

[기반 구현 결과](IMPLEMENTATION-RESULTS.md#cli-05-기반-구현-착수-2026-10-27)에는 Linux 서버/클라이언트 Rust 검사, Docker/PostgreSQL 격리 회귀, 브라우저 회귀, CLI 지침·번들 검사가 기록되어 있다. Linux의 별도 최소 crate에서 Windows OS 모듈을 `x86_64-pc-windows-msvc --all-targets`로 **타입검사**했지만, 전체 Windows CLI 링크·실행 결과는 아니다. Linux의 전체 Windows 교차 검사는 MSVC `lib.exe`가 없어 중단됐다.

[CLI 기반 CI](../.github/workflows/cli-foundation.yml)는 Windows MSVC 빌드·단위 검사와 OS 간 번들 비교를 정의한다. 워크플로 파일이 있다는 사실과 실제 실행 성공을 구분한다. 호스티드 Windows Server runner 결과만으로 Windows 11/PowerShell/Docker Desktop 검증을 대신하지 않는다.

`fbbd0870f45ec976d150988cc568c0765c9453f1`의 [CI 실행 40446385352](https://github.com/hurxxxx/miy-v2/actions/runs/40446385352)는 2026-10-27에 Linux 서버/클라이언트, macOS 클라이언트, Windows MSVC 클라이언트와 OS 간 번들 해시 비교를 모두 통과했다. Windows job은 `windows-2022`의 Bash 셸에서 debug 빌드·단위 검사·오프라인 앱 생성·Git 왕복을 수행했다. 아래 사용자 PowerShell release 빌드·ZIP·MCP 검증과 구분한다.

공통 출력 구현 commit `67d7e429d99df10895b409a267c07849431b6160`의 [CI 실행 40447432789](https://github.com/hurxxxx/miy-v2/actions/runs/40447432789)도 같은 날 전체 job을 통과했다. Windows 네이티브 CLI의 출력 envelope·종료 코드·loopback HTTP 오류 검사가 추가로 통과했다. Windows 콘솔 Ctrl+C는 이 검사에서 제외하며 사용자 PowerShell 검증 완료를 뜻하지 않는다.

최종 CLI 0.5 소스 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 [기본 CI 40460546995](https://github.com/hurxxxx/miy-v2/actions/runs/40460546995)는 전체 job을 통과했다. Windows job은 Windows Server 2022 Datacenter `10.0.20348`에서 Rust 1.99.1/Python 3.12.10으로 fmt/check/Clippy/debug build, 단위87개, 출력10묶음, 번들/Git31파일을 통과했다. 기존 Windows debug 스택 초과는 수정 후 재검사에 성공했다. Job Object 검사는 소유 프로세스 종료와 다른 Job의 프로세스 보존을 확인한다. 콘솔 Ctrl+C는 출력 fixture에서 SKIP이며 아래 사용자 검사 대상이다.

같은 소스의 [Windows release job](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868/job/122044280821)도 성공했다. 단위87개, 최적화 실행 파일의 출력10묶음·번들/Git31파일과 **PowerShell 5.1.20348.5756 및 7.6.8 각각 설치기6시나리오**를 통과했다. 설치/갱신, 잘못된 체크섬, 사용 중 파일 잠금, 변조 backup, 중단 복구, 제거 시 사용자 파일 보존을 확인했다. 원격 다운로드와 PATH opt-in 변경은 이 fixture의 통과 범위에 포함하지 않는다.

- [Windows 후보 artifact](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868/artifacts/12714358424): Actions artifact의 바깥 ZIP을 먼저 풀면 아래 배포 ZIP과 `.sha256`, provenance를 얻는다. 공개 GitHub Release는 아직 발행하지 않았다.
- 배포 ZIP: `miy-0.5.0-x86_64-pc-windows-msvc.zip`
- 배포 ZIP SHA-256: `2d6846d1977b748b79054cc053cafa357340b5864cd14c15b6c32d50acb55247`
- 내부 `miy.exe` SHA-256: `a6111f4d4a58b9abd679aaef57659bc324e30e3d9a247273c77e30e668d8f3a9`

CLI 0.5에는 루트 `context/auth/job/describe/doctor/component/mcp`와 앱 lifecycle 명령이 구현되어 있다. JSON 성공은 `data`, 실패는 `error`에 있고 `$LASTEXITCODE`로 종료 코드를 확인한다. `job watch --output jsonl`은 이벤트를 한 줄씩 출력한다. `app login/describe/doctor/mcp/resume/control` 같은 이전 이름은 대체 명령을 안내하고 실행을 거부한다.

아래 빌드·오프라인 검사 뒤 10절의 설치/인증/원격/로컬 개발 검사를 수행한다. **검사 절차가 존재하는 것과 사용자 Windows 11 통과는 구분한다.**

## 2. Windows 준비

### CLI 0.6의 설치 연결 옵션

현재 `b67798b` 후보의 설치기·CLI는 `install-cli.ps1`/`update-cli.ps1`의 `-Server URL`로 명시한 서버를 등록한다. 플랫폼의 `/cli/install` 화면이 서버 `MIY_ORIGIN`에서 주소를 채운 명령을 제공한다. 기존 `config.json`이 있으면 빈 context 목록까지 그대로 보존하며, URL을 새로 전달해도 기존 활성 서버·사용자·인증서 설정을 바꾸지 않는다. 다른 주소는 명시적으로 표시하고 context 추가/선택을 안내한다. 옵션 생략 시 대화형 입력 또는 설치 후 등록이 필요하며 어떤 주소도 대신 적용하지 않는다.

이 기능은 이전 `0fe94a4` 후보에는 없고 현재 `b67798b` 후보에 포함되어 있다. [Windows 후보 job](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949/job/122533455640)에서 `check-cli-install.py` 9그룹과 PowerShell 5.1/7의 설치기 9개 시나리오를 각각 통과했다. Windows 11 PC에서는 검증된 현재 ZIP을 사용하고 실제 설치·Docker Desktop 개발 결과를 별도로 기록한다.

### 필요한 도구

일반 사용자 계정의 Windows 11 x64에서 PowerShell 5.1과 PowerShell 7을 준비한다. 시작 메뉴에서 Windows PowerShell과 PowerShell 7을 각각 열거나, 별도 터미널에서 `powershell.exe -NoProfile`, `pwsh.exe -NoProfile`로 시작한다. 관리자 권한 터미널을 기본 검증 환경으로 사용하지 않는다.

필요한 도구는 다음과 같다.

- Git for Windows와 Python 3.12: `git`, `python` 명령을 실행할 수 있어야 한다. Python이 `py -3.12`로만 제공되면 아래의 `python` 호출을 해당 명령으로 일관되게 바꾸고 실제 버전을 기록한다.
- Windows용 rustup과 Rust **1.99.1 MSVC** 도구 체인.
- Visual Studio Installer에서 **Desktop development with C++** 작업 또는 동등한 MSVC x64/x86 C++ Build Tools 및 Windows SDK. `ring` 등 C 코드를 포함하는 의존성을 위해 C/C++ 컴파일러와 `link.exe`, `lib.exe`도 필요하다. 설치 구성은 [Rust의 MSVC 준비 안내](https://rust-lang.github.io/rustup/installation/windows-msvc.html)와 [Microsoft의 C++ 설치 안내](https://learn.microsoft.com/en-us/cpp/build/vscpp-step-0-installation?view=msvc-170)를 따른다.

이미 필요한 구성요소가 있으면 재설치할 필요가 없다. `link.exe`/`lib.exe` 탐색에 실패하면 Visual Studio Installer의 C++/SDK 설치 상태를 확인한다. 필요할 때 설치된 Visual Studio의 Developer PowerShell에서 x64 도구 환경을 활성화하고, 실제 사용한 셸과 도구 버전을 기록한다. 시스템 PATH·Git 전역 설정·시스템 인코딩·전역 PowerShell 실행 정책을 이 검사 때문에 강제로 바꾸지 않는다.

이 안내의 Rust 빌드는 처음에 패키지 다운로드가 필요할 수 있다. 이후 앱 생성·설정·아래 MCP 도움말 검사는 서버 접속 없이 실행되며 Node/pnpm·Docker·플랫폼 로그인은 필요하지 않다. Docker Desktop을 포함하는 앱 개발 E2E는 10절에서 별도로 실행한다. 배포된 실행 파일의 원격 운영에는 Rust·MSVC·Node·Docker가 필요하지 않다. 이 절의 도구는 소스 빌드용이다.

## 3. Checkout과 실행 기록 준비

기존 수정이 있는 작업본을 강제로 reset하지 않는다. 새 검증용 checkout을 만들거나 원하는 commit을 기존의 깨끗한 작업본에서 선택한다. 다음은 사용자 홈 아래 새 디렉터리의 예다. 이미 디렉터리가 있으면 다른 이름을 선택한다.

```powershell
$ErrorActionPreference = 'Stop'
$MiyCheckout = Join-Path $env:USERPROFILE 'miy-windows-validation'
git clone https://github.com/hurxxxx/miy-v2.git $MiyCheckout
if ($LASTEXITCODE -ne 0) { throw "git clone failed: $LASTEXITCODE" }
Set-Location -LiteralPath $MiyCheckout
$MiyCandidateCommit = 'b67798bfb84f299257ba4326beaf034b125cde7a'
git switch --detach $MiyCandidateCommit
if ($LASTEXITCODE -ne 0) { throw "git switch failed: $LASTEXITCODE" }
git status --short --branch
if ($LASTEXITCODE -ne 0) { throw "git status failed: $LASTEXITCODE" }
git log -5 --oneline
if ($LASTEXITCODE -ne 0) { throw "git log failed: $LASTEXITCODE" }
```

배포 후보와 같은 코드를 검증하도록 위 예제는 CLI 0.6 소스 commit `b67798b`를 고정한다. 이 안내와 실행 결과는 이후 문서 commit에서 갱신되므로, 문서의 최신 commit과 검증 대상 소스 commit을 혼동하지 않는다. 다른 코드를 검사한다면 그 commit을 기록하고 기존 후보의 통과 증거로 합치지 않는다.

Windows PowerShell 5.1에서는 네이티브 실행 파일의 비정상 종료가 `$ErrorActionPreference = 'Stop'`만으로 자동 예외가 되지 않는다. 아래에서는 각 네이티브 명령 직후 `$LASTEXITCODE`를 확인하고 실패하면 중단한다. PowerShell cmdlet은 `$ErrorActionPreference`를 따른다. 종료 코드의 의미는 [Microsoft의 자동 변수 문서](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-5.1)를 참고한다.

아래 준비 블록은 **각 PowerShell 세션에서** 실행한다. `$MiyEvidence`는 Git에서 제외된 실행별 디렉터리다. 다른 환경의 `.local` 증거가 이 checkout에 있다고 가정하지 않는다.

```powershell
$MiyRepo = (Get-Location).Path
$MiyCommit = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "git rev-parse failed: $LASTEXITCODE" }
$MiyDirty = @(git status --porcelain)
if ($LASTEXITCODE -ne 0) { throw "git status failed: $LASTEXITCODE" }
if ($MiyDirty.Count -ne 0) { throw 'Use a clean checkout or record the exact local patch separately.' }
$MiyRun = '{0}-ps{1}-{2}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $PSVersionTable.PSVersion.Major, ([guid]::NewGuid().ToString('N').Substring(0,8))
$MiyEvidence = Join-Path $MiyRepo ('.local\evidence\windows-validation\' + $MiyRun)
New-Item -ItemType Directory -Path $MiyEvidence | Out-Null
$MiyUtf8 = New-Object System.Text.UTF8Encoding($false)
$MiyResults = Join-Path $MiyEvidence 'commands.jsonl'

function Assert-MiyExit([string]$Step) {
    $MiyExit = $LASTEXITCODE
    $MiyRow = [ordered]@{
        time = (Get-Date).ToString('o')
        command = $Step
        exitCode = $MiyExit
        status = $(if ($MiyExit -eq 0) { 'success' } else { 'fail' })
    } | ConvertTo-Json -Compress
    [IO.File]::AppendAllText($MiyResults, $MiyRow + "`n", $MiyUtf8)
    if ($MiyExit -ne 0) { throw "$Step failed: exit $MiyExit; evidence: $MiyEvidence" }
}

$MiyOs = Get-CimInstance Win32_OperatingSystem
$MiyCpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$MiyEnvironment = [ordered]@{
    commit = $MiyCommit
    dirty = $false
    windows = $MiyOs.Caption
    osVersion = $MiyOs.Version
    osBuild = $MiyOs.BuildNumber
    osArchitecture = $MiyOs.OSArchitecture
    cpu = $MiyCpu.Name
    process64Bit = [Environment]::Is64BitProcess
    powershellVersion = $PSVersionTable.PSVersion.ToString()
    powershellEdition = $PSVersionTable.PSEdition
    repository = $MiyRepo
}
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'environment.json'), ($MiyEnvironment | ConvertTo-Json), $MiyUtf8)
Start-Transcript -Path (Join-Path $MiyEvidence 'console.txt')
git --version
Assert-MiyExit 'git --version'
python --version
Assert-MiyExit 'python --version'
rustup --version
Assert-MiyExit 'rustup --version'
```

이 transcript는 아래의 로그인 없는 검사에 사용한다. 토큰·쿠키·verifier·`.miy/session.json` 내용은 기록하지 않는다. 나중에 실제 로그인을 검사한다면 비밀 파일 내용을 출력하지 말고 인증 검사의 성공/실패와 필요한 오류 코드만 별도로 남긴다.

PowerShell 5.1의 `>`/`Out-File` 기본값은 UTF-16LE이므로 CLI JSON을 그대로 UTF-8 파일로 보존한다고 가정하면 안 된다. 위 예제는 기록 파일에 UTF-8을 명시한다. 아래 Python 검사는 UTF-8 JSON을 직접 쓰고 MCP 바이트를 직접 처리하므로 PowerShell의 텍스트 파이프를 거치지 않는다. 한글이 포함된 예제를 `.ps1`로 저장해 5.1에서 실행한다면 그 **로컬 스크립트**는 UTF-8 BOM으로 저장한다. 저장소 SDK·SQL 파일의 인코딩은 바꾸지 않는다. [PowerShell 인코딩 문서](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_character_encoding?view=powershell-5.1).

## 4. MSVC 클라이언트 검사와 release 빌드

명시적인 MSVC 도구 체인과 target을 사용한다. `--no-default-features`는 서버 모듈과 `serve/worker/migrate/seed` 명령을 제외하는 현재 클라이언트 빌드 계약이다. `cargo fmt`는 소스 형식 검사이므로 target/feature 플래그를 받지 않는다.

```powershell
rustup toolchain install 1.99.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy
Assert-MiyExit 'rustup toolchain install 1.99.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy'
rustc +1.99.1-x86_64-pc-windows-msvc -vV
Assert-MiyExit 'rustc +1.99.1-x86_64-pc-windows-msvc -vV'
cargo +1.99.1-x86_64-pc-windows-msvc --version
Assert-MiyExit 'cargo +1.99.1-x86_64-pc-windows-msvc --version'

$MiyTarget = 'x86_64-pc-windows-msvc'
$MiyTargetDir = Join-Path $MiyRepo 'target'
python scripts/check-bundle-portability.py source
Assert-MiyExit 'python scripts/check-bundle-portability.py source'
cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check
Assert-MiyExit 'cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check'
cargo +1.99.1-x86_64-pc-windows-msvc check --workspace --all-targets --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo check --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'
cargo +1.99.1-x86_64-pc-windows-msvc clippy --workspace --all-targets --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir -- -D warnings
Assert-MiyExit 'cargo clippy --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target -- -D warnings'
cargo +1.99.1-x86_64-pc-windows-msvc test --workspace --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo test --workspace --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'
cargo +1.99.1-x86_64-pc-windows-msvc build --release --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo build --release --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'

$MiyExe = Join-Path $MiyTargetDir 'x86_64-pc-windows-msvc\release\miy.exe'
& $MiyExe --version
Assert-MiyExit 'miy.exe --version'
& $MiyExe --help
Assert-MiyExit 'miy.exe --help'
& $MiyExe app --help
Assert-MiyExit 'miy.exe app --help'
```

기대 결과는 `miy 0.6.0`이며 루트 도움말에 서버용 네 명령이 없어야 한다. 테스트 수·ignored 수와 이름은 실제 출력대로 기록한다. 빌드 성공 후의 실행 파일은 `target\x86_64-pc-windows-msvc\release\miy.exe`다. 실패한 검사는 로그와 종료 코드를 남기고 이후 의존하는 항목을 `not_run`으로 기록한다.

## 5. 한글·공백 경로, 번들·Git checkout 검사

```powershell
python scripts/check-bundle-portability.py check --cli $MiyExe --output (Join-Path $MiyEvidence 'bundle-windows.json')
Assert-MiyExit 'python scripts/check-bundle-portability.py check --cli <release miy.exe> --output <evidence>/bundle-windows.json'
```

공통 출력 계약이 포함된 후속 commit에서는 다음도 실행한다. 이 검사는 외부 플랫폼에 접속하지 않고 임시 loopback HTTP 서버와 fixture 자격증명으로 성공/실패 envelope, HTTP 분류, 오류의 비밀 비노출, 작업 실패를 확인한다. 실제 Windows 콘솔 Ctrl+C는 별도 검증 항목이며 Python의 POSIX signal 검사는 Windows에서 명시적으로 건너뛴다.

```powershell
python scripts/check-cli-output.py --cli $MiyExe
Assert-MiyExit 'python scripts/check-cli-output.py --cli <release miy.exe>'
```

이 스크립트는 임시 디렉터리의 `한글 경로/app with spaces`에 실제 CLI로 앱을 생성한다. 내장 템플릿·보호 파일·CLI 지침 해시를 확인하고, 생성 앱을 `core.autocrlf=true`로 commit/clone하여 관리 파일·SQL·CRLF 컴포넌트 fixture의 바이트 보존을 확인한다. fixture SQL은 실행하지 않는다. Git 설정은 명령/검증 checkout 범위이며 전역 `core.autocrlf`를 바꾸지 않는다. 임시 앱은 스크립트 종료 시 정리되고 JSON 해시 manifest가 증거 디렉터리에 남는다.

Windows manifest 하나의 성공은 OS 간 동일성 검사의 성공이 아니다. **같은 commit**에서 얻은 Linux/macOS manifest가 있으면 아래처럼 실제 파일 경로를 지정한다. 없으면 비교 항목은 `not_run`으로 기록한다.

```powershell
# 실제로 가져온 같은 commit의 두 manifest가 있을 때만 실행한다.
python scripts/check-bundle-portability.py compare (Join-Path $MiyEvidence 'bundle-windows.json') 'C:\evidence\same-commit\bundle-linux.json' 'C:\evidence\same-commit\bundle-macos.json'
Assert-MiyExit 'python scripts/check-bundle-portability.py compare <Windows> <Linux> <macOS> (same commit)'
```

## 6. 오프라인 앱 생성·setup·stdio MCP

아래는 공통 출력 계약 구현 이후 checkout 기준이다. 기반 commit `fbbd087`을 검사한다면 describe의 `--output json`을 생략한다. 다음 앱은 실행별 증거 폴더에 보존한다. `--url`은 설정만 저장하며 이 절차에서 해당 서버에 접속하지 않는다. 생성 시 플랫폼 등록·컴포넌트 다운로드·로그인은 수행하지 않는다.

```powershell
$MiyApp = Join-Path $MiyEvidence '한글 경로\app with spaces'
& $MiyExe app init $MiyApp --id windows-foundation-check --name 'Windows 한글 검사' --url http://localhost:18000
Assert-MiyExit 'miy.exe app init <Korean-and-space-path> --id windows-foundation-check --name <Korean-name> --url http://localhost:18000'
Push-Location -LiteralPath $MiyApp
try {
    & $MiyExe app setup
    Assert-MiyExit 'miy.exe app setup (offline)'
    & $MiyExe app setup
    Assert-MiyExit 'miy.exe app setup (repeat)'
    & '.\.miy-tools\miy.exe' --version
    Assert-MiyExit '.miy-tools/miy.exe --version'
    & '.\.miy-tools\miy.exe' describe --all --output json
    Assert-MiyExit '.miy-tools/miy.exe describe --all --output json'
} finally {
    Pop-Location
}
```

`app setup`은 전역 Codex/Claude 설정을 바꾸지 않고 프로젝트 MCP 설정·동봉 CLI·관리 지침을 맞춘다. 기존 사용자 `.gitattributes`와 충돌하면 덮어쓰기 대신 오류를 내는 것이 현재 계약이다. 위의 새 fixture에서는 반복 setup이 성공해야 한다. `app check`는 컴포넌트·개발 도구를 요구하는 전체 앱 검사이므로 여기의 오프라인 setup 성공과 구분한다.

MCP는 `miy.exe mcp`를 단독 실행하면 stdin에서 요청을 기다린다. 다음 로컬 Python probe는 초기화, 도구 목록, 로그인 없는 `get_cli_help`를 실제 stdio로 호출하고 자식 프로세스를 종료한다. 플랫폼이 필요한 `get_contract`나 `search_api`는 호출하지 않는다.

```powershell
$MiyMcpProbe = Join-Path $MiyEvidence 'mcp-offline.py'
$MiyMcpSource = @'
import json
from pathlib import Path
import subprocess
import sys
import threading

project, evidence = map(Path, sys.argv[1:3])
with (evidence / "mcp-stderr.txt").open("wb") as errors:
    process = subprocess.Popen(
        [str(project / ".miy-tools" / "miy.exe"), "mcp"],
        cwd=project, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=errors, text=True, encoding="utf-8",
    )
    timeout = threading.Timer(30, process.kill)
    timeout.start()
    responses = []
    def send(value):
        process.stdin.write(json.dumps(value, ensure_ascii=True) + "\n")
        process.stdin.flush()
    def request(identifier, method, params):
        send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
        while True:
            line = process.stdout.readline()
            if not line:
                raise RuntimeError("MCP exited or timed out before response")
            response = json.loads(line)
            if response.get("id") == identifier:
                if "error" in response:
                    raise RuntimeError(response["error"])
                responses.append(response)
                return response["result"]
    try:
        request(1, "initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "windows-foundation-check", "version": "1"},
        })
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listing = request(2, "tools/list", {})
        assert any(tool["name"] == "get_cli_help" for tool in listing["tools"])
        reply = request(3, "tools/call", {"name": "get_cli_help", "arguments": {}})
        assert not reply.get("isError", False), reply
        help_data = json.loads(next(block["text"] for block in reply["content"] if block["type"] == "text"))
        assert help_data["cliVersion"] == "0.6.0", help_data["cliVersion"]
        assert help_data["bundleVersion"] == "0.7.0", help_data["bundleVersion"]
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        (evidence / "mcp-offline.json").write_text(
            json.dumps({"status": "success", "responses": responses}, ensure_ascii=True, indent=2),
            encoding="utf-8",
        )
        print("PASS: stdio initialize, tools/list, get_cli_help; no platform login")
    finally:
        timeout.cancel()
        if process.poll() is None:
            process.kill()
            process.wait()
'@
[IO.File]::WriteAllText($MiyMcpProbe, $MiyMcpSource, $MiyUtf8)
python $MiyMcpProbe $MiyApp $MiyEvidence
Assert-MiyExit 'python <evidence>/mcp-offline.py <fixture-app> <evidence>'
```

이 성공은 실제 Codex/Claude 앱에서의 MCP 연결 승인·세션 유지 검증을 포함하지 않는다. 실제 클라이언트 검사는 추후 각 클라이언트 버전과 함께 별도로 기록한다.

## 7. 로컬 ZIP과 SHA-256 생성

아래 파일은 검증한 바이너리를 옮기기 위한 **로컬 검증 산출물**이다. 설치기나 공식 릴리스가 아니며 PATH 등록·자동 갱신·제거 기능을 제공하지 않는다.

```powershell
$MiyPackageName = 'miy-0.6.0-windows-x64-' + $MiyCommit.Substring(0,12)
$MiyPackageDir = Join-Path $MiyEvidence $MiyPackageName
New-Item -ItemType Directory -Path $MiyPackageDir | Out-Null
$MiyPackagedExe = Join-Path $MiyPackageDir 'miy.exe'
Copy-Item -LiteralPath $MiyExe -Destination $MiyPackagedExe
$MiyZip = Join-Path $MiyEvidence ($MiyPackageName + '.zip')
Compress-Archive -LiteralPath $MiyPackagedExe -DestinationPath $MiyZip
$MiyExeHash = (Get-FileHash -LiteralPath $MiyPackagedExe -Algorithm SHA256).Hash.ToLowerInvariant()
$MiyZipHash = (Get-FileHash -LiteralPath $MiyZip -Algorithm SHA256).Hash.ToLowerInvariant()
$MiyChecksums = "$MiyExeHash  $MiyPackageName/miy.exe`n$MiyZipHash  $MiyPackageName.zip`n"
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'SHA256SUMS'), $MiyChecksums, $MiyUtf8)
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'source-commit.txt'), $MiyCommit + "`n", $MiyUtf8)
Get-Content -LiteralPath (Join-Path $MiyEvidence 'SHA256SUMS') -Encoding UTF8
Stop-Transcript
```

다른 PC에서 ZIP을 사용할 때에는 받은 ZIP의 `Get-FileHash -Algorithm SHA256` 결과와 기록값을 비교하고, 새 디렉터리에 풀어 `miy.exe --version`의 종료 코드도 확인한다. SDK/Node/Docker 없는 실제 PC에서의 원격 관리 검증은 별도 결과가 필요하다.

## 8. 결과 반영 규칙

CLI 0.5의 새 명령·인증·Job Object·파일 트랜잭션·설치 스크립트는 구현되어 있다. Windows 네이티브 CI는 코드와 자동 fixture를 검사한다. 사용자 Windows 11의 실제 로그인·원격 서버·Docker Desktop·프로젝트 MCP·콘솔 신호는 아래 별도 절차에서 확인해야 한다. 소스 빌드나 호스티드 runner 성공만으로 전체 지원을 완료 처리하지 않는다.

실행 결과는 아래 형식으로 **새 기록을 추가**한다. 실패 뒤 수정하여 재실행했으면 이전 실패 기록을 삭제하거나 성공으로 덮어쓰지 않고 새 run ID·commit으로 추가한다. 검사하지 않은 행은 `not_run`과 이유를 남긴다. 후속 문서 업데이트 시 증거와 실제 실행 범위에 맞춰 [구현 결과](IMPLEMENTATION-RESULTS.md)와 [인계 체크리스트](CLI-HANDOFF.md)를 갱신한다.

```markdown
### Windows 실행 기록 — YYYY-MM-DD / RUN_ID

- 실행자:
- commit (`git rev-parse HEAD`):
- 작업 트리: clean / dirty (dirty면 patch 위치)
- Windows 버전·build / CPU / 64-bit 프로세스 여부:
- PowerShell 전체 버전·Edition:
- Rust / Cargo / MSVC Build Tools / Windows SDK / Git / Python 버전:
- 일반 사용자 / 관리자 여부:
- 실행 명령 원문·종료 코드: <evidence>/commands.jsonl
- 상세 로그: <evidence>/console.txt 및 실패 로그
- 실행 파일·ZIP SHA-256: <evidence>/SHA256SUMS
- 증거 보관 위치 (이 PC 경로와 공유한 위치를 구분):

| 항목 | 명령 또는 시나리오 | 종료 코드 | 결과: success / fail / not_run | 증거·실패 또는 미실행 사유 |
| --- | --- | --- | --- | --- |
| 소스/LF/SQL | check-bundle-portability.py source | | not_run | |
| 형식 | cargo fmt | | not_run | |
| Windows MSVC 타입 검사 | cargo check --no-default-features --target x86_64-pc-windows-msvc | | not_run | |
| Clippy | cargo clippy ... -- -D warnings | | not_run | |
| 단위 검사 | cargo test ...; passed/failed/ignored 수 | | not_run | |
| 네이티브 release 빌드 | cargo build --release ... | | not_run | |
| 실행/명령 경계 | miy.exe --version, --help, app --help | | not_run | |
| 번들·한글/공백·Git 왕복 | check-bundle-portability.py check | | not_run | |
| 공통 출력·오류·종료 코드 | check-cli-output.py --cli <miy.exe> | | not_run | 후속 출력 계약 commit; Windows 콘솔 Ctrl+C 제외 |
| OS 간 해시 비교 | compare; 각 OS의 동일 commit 명시 | | not_run | |
| 앱 생성/반복 setup | app init, app setup 2회 | | not_run | |
| 오프라인 stdio MCP | initialize, tools/list, get_cli_help | | not_run | |
| 로컬 ZIP/체크섬 | Compress-Archive, Get-FileHash (cmdlet: 예외 유무 기록) | 해당 없음 | not_run | |
| 실제 Codex/Claude MCP | 클라이언트·버전·범위 명시 | | not_run | 이번 오프라인 probe와 별도 |
| Docker Desktop 개발·배포 | 수행한 전체 단계 명시 | | not_run | 기반 검사와 별도 |
| 실제 사용자 인증·원격 운영 | 서버 commit·CPU·대상·수행 범위 | | not_run | 기반 검사와 별도 |

- 이번 실행으로 확인한 범위:
- 남은 문제와 다음 작업:
- 이전 실패를 해결한 경우 이전 RUN_ID 및 변경 commit:
```

## Windows 실행 기록

### 공통 실행 환경과 도구 준비 — 2026-10-27

사용자 요청으로 현재 PC에서 Codex가 실행했다. 검증 대상은 `fb42b384081afe8b77eab7b2af419f5c1e79c3ec`이며 두 셸의 검증 시작 시 작업 트리는 clean이었다. 모든 검증을 마친 뒤 결과 문서만 수정했다. WSL이나 호스티드 runner는 사용하지 않았다.

| 항목 | 실제 값 |
| --- | --- |
| Windows / CPU | Windows 11 Pro 10.0.26200, build 26200 / Intel Core i5-14500, x64 |
| 프로세스·권한 | 64-bit, 일반 사용자 (`administrator: false`); 빌드 도구 설치에만 UAC 관리자 승인 사용 |
| Rust / Cargo / rustup | 1.99.1 (`58cf8f9c3`, 2026-10-13) / 1.99.1 (`5f94df478`) / 1.29.2 |
| MSVC / Visual Studio Build Tools | 14.44.35207 / 2022 17.14.43 (`17.14.37824.6`) |
| Windows SDK | 10.0.26100.0 |
| Git / Python / Node | Git for Windows 2.56.0.windows.4 / Codex 동봉 Python 3.12.14 / Node 24.19.0 |
| 저장소 / target | `C:\projects\miy-v2` / `target\x86_64-pc-windows-msvc` |

처음에는 Rust·MSVC·SDK가 없었다. 사용자 설치 승인 후 공식 rustup으로 Rust를 `.local\tools\cargo`, `.local\tools\rustup`에 설치했고 시스템 PATH는 변경하지 않았다. Microsoft 서명이 유효한 공식 Build Tools 설치기로 `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`와 `Microsoft.VisualStudio.Component.Windows11SDK.26100`을 설치했다. 설치기는 **3010 (재부팅 필요)**을 반환했으며 자동 재부팅 없이 아래 검증을 마쳤다. 재부팅 후 검증은 이번 기록에 포함하지 않는다.

각 검증 프로세스에서만 Git/Cargo PATH와 `CARGO_HOME`·`RUSTUP_HOME`을 설정했다. `Microsoft.VisualStudio.DevShell.dll`의 `Enter-VsDevShell -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64'`로 MSVC 환경을 활성화했다. 시스템 인코딩·전역 Git 설정·전역 PowerShell 실행 정책은 변경하지 않았다. PowerShell 5.1 실행의 `-ExecutionPolicy Bypass`는 해당 프로세스에만 적용했다.

두 셸은 같은 `target`을 순차 사용했다. PowerShell 7에서 최초 컴파일하고 PowerShell 5.1에서는 동일한 Cargo 검사·test·release build 명령을 다시 호출하여 최신 빌드 산출물을 재사용했다. 두 번의 별도 clean build 결과로 해석하지 않는다. 실제 실행기와 준비 로그는 이 PC의 `.local\native-validation.ps1` 및 `.local\evidence\windows-validation\20261027-native\`에 있다.

### Windows 실행 기록 — 2026-10-27 / 20261027-native-ps7

- 실행자: 사용자 요청을 수행한 Codex, 일반 사용자 프로세스.
- commit: `fb42b384081afe8b77eab7b2af419f5c1e79c3ec`; 시작 시 작업 트리 clean.
- Windows·CPU·도구 버전: 위 공통 실행 환경과 동일; PowerShell **7.6.5 Core**, 64-bit.
- 완료 시각: **2026-10-27 14:06:43 +09:00**.
- 증거 보관 위치: `C:\projects\miy-v2\.local\evidence\windows-validation\20261027-native-ps7\` (이 PC 로컬 전용; 원본 로그·바이너리·ZIP은 Git 미포함, 외부 공유 없음).
- 명령·종료 코드: `commands.jsonl`; 환경: `environment-source.json`, `environment-full.json`, `msvc-environment.json`; 상세 로그: `console-source.txt`, `console-full.txt`; 완료·해시: `completed.json`, `SHA256SUMS`.

| 항목 | 명령 또는 시나리오 | 종료 코드 | 결과 | 증거·범위 |
| --- | --- | --- | --- | --- |
| 소스/LF/SQL | `check-bundle-portability.py source` | 0 | success | 템플릿 31개 LF 및 추적 SQL의 index 원본 바이트 보존 |
| 형식 | `cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check` | 0 | success | `console-full.txt` |
| Windows MSVC 타입 검사 | `cargo check --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target` | 0 | success | 명시적 1.99.1 MSVC 도구 체인 |
| Clippy | `cargo clippy` 위 target/feature/lock 조건 및 `--all-targets -- -D warnings` | 0 | success | 경고 금지 |
| 단위 검사 | `cargo test --workspace` 위 target/feature/lock 조건 | 0 | success | **37 passed / 0 failed / 0 ignored** |
| 네이티브 release 빌드 | `cargo build --release` 위 target/feature/lock 조건 | 0 | success | `target\x86_64-pc-windows-msvc\release\miy.exe` |
| 실행/명령 경계 | `miy.exe --version`, `--help`, `app --help` | 모두 0 | success | `miy 0.4.2`; 서버 명령 4개 없음 |
| 번들·한글/공백·Git 왕복 | `check-bundle-portability.py check --cli <release miy.exe> --output <evidence>/bundle-windows.json` | 0 | success | 내장 31개 파일, `autocrlf=true` 왕복 관리/SQL/컴포넌트 31개 보존 |
| 공통 출력·오류·종료 코드 | `check-cli-output.py --cli <release miy.exe>` | 0 | success | **9 passed**; 실제 서비스 없는 loopback HTTP fixture, 비밀 비노출·읽기 전용 요청 확인 |
| OS 간 해시 비교 | 같은 commit의 Linux/macOS manifest와 `compare` | 해당 없음 | not_run | 이 PC에 같은 commit의 다른 OS manifest가 없음 |
| 앱 생성/반복 setup | 한글·공백 경로에서 `app init`, `app setup` 2회, 동봉 CLI version·JSON describe | 모두 0 | success | `한글 경로\app with spaces`; CLI 0.4.2 / bundle 0.6.1, 전역 설정 변경 없음 |
| 오프라인 stdio MCP | 문서의 Python probe: initialize, tools/list, get_cli_help, EOF 정상 종료 | 0 | success | `mcp-offline.json`, `mcp-stderr.txt`; 플랫폼 로그인 없음 |
| 로컬 ZIP/체크섬 | `Compress-Archive`, `Get-FileHash`, `Expand-Archive`, 압축 해제한 exe version | cmdlet 예외 없음; exe 0 | success | 압축 전후 exe SHA-256 일치; `SHA256SUMS` |
| Windows 콘솔 Ctrl+C | 실제 Windows 콘솔 신호 전달 | 해당 없음 | not_run | Python 출력 검사에서 POSIX 신호 항목을 명시적으로 skip |
| 실제 Codex/Claude MCP | 실제 에이전트 클라이언트의 승인·세션 유지 | 해당 없음 | not_run | 이번 검사는 Python stdio probe 범위 |
| Docker Desktop 개발·배포 | 전체 `app check/dev/build/deploy` | 해당 없음 | not_run | 이번 오프라인 기반 검사 범위 밖 |
| 실제 사용자 인증·원격 운영 | 실제 로그인·원격 서버·UI/API·데이터 보존 | 해당 없음 | not_run | 실제 플랫폼·인증 세션을 사용하지 않음 |

산출물 이름은 `miy-0.4.2-windows-x64-fb42b384081a`이며 SHA-256은 다음과 같다.

```text
c5510f56a15674d93b6641c3e73ecb04a54c96359aa67e1d98110b2e5b2262ec  miy-0.4.2-windows-x64-fb42b384081a/miy.exe
d6eaf01714d683031454aa03756ef2a0ef7060f02cd87e6e07e3ebc9fbe091e8  miy-0.4.2-windows-x64-fb42b384081a.zip
```

### Windows 실행 기록 — 2026-10-27 / 20261027-native-ps5

- 실행자: 사용자 요청을 수행한 Codex, 일반 사용자 프로세스.
- commit: `fb42b384081afe8b77eab7b2af419f5c1e79c3ec`; 시작 시 작업 트리 clean.
- Windows·CPU·도구 버전: 위 공통 실행 환경과 동일; Windows PowerShell **5.1.26100.9711 Desktop**, 64-bit, `-NoProfile -ExecutionPolicy Bypass`로 실행.
- 완료 시각: **2026-10-27 14:07:28 +09:00**.
- 증거 보관 위치: `C:\projects\miy-v2\.local\evidence\windows-validation\20261027-native-ps5\` (이 PC 로컬 전용; 원본 로그·바이너리·ZIP은 Git 미포함, 외부 공유 없음).
- 명령·종료 코드: `commands.jsonl`; 환경: `environment-source.json`, `environment-full.json`, `msvc-environment.json`; 상세 로그: `console-source.txt`, `console-full.txt`; 완료·해시: `completed.json`, `SHA256SUMS`.

| 항목 | 명령 또는 시나리오 | 종료 코드 | 결과 | 증거·범위 |
| --- | --- | --- | --- | --- |
| 소스/LF/SQL | `check-bundle-portability.py source` | 0 | success | 템플릿 31개 LF 및 추적 SQL의 index 원본 바이트 보존 |
| 형식 | `cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check` | 0 | success | `console-full.txt` |
| Windows MSVC 타입 검사 | `cargo check --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target` | 0 | success | 명시적 1.99.1 MSVC 도구 체인 |
| Clippy | `cargo clippy` 위 target/feature/lock 조건 및 `--all-targets -- -D warnings` | 0 | success | 경고 금지 |
| 단위 검사 | `cargo test --workspace` 위 target/feature/lock 조건 | 0 | success | **37 passed / 0 failed / 0 ignored** |
| 네이티브 release 빌드 | `cargo build --release` 위 target/feature/lock 조건 | 0 | success | 공유 target의 최신 release 산출물 사용 |
| 실행/명령 경계 | `miy.exe --version`, `--help`, `app --help` | 모두 0 | success | `miy 0.4.2`; 서버 명령 4개 없음 |
| 번들·한글/공백·Git 왕복 | `check-bundle-portability.py check --cli <release miy.exe> --output <evidence>/bundle-windows.json` | 0 | success | 내장 31개 파일, `autocrlf=true` 왕복 관리/SQL/컴포넌트 31개 보존 |
| 공통 출력·오류·종료 코드 | `check-cli-output.py --cli <release miy.exe>` | 0 | success | **9 passed**; 실제 서비스 없는 loopback HTTP fixture |
| OS 간 해시 비교 | 같은 commit의 Linux/macOS manifest와 `compare` | 해당 없음 | not_run | 이 PC에 같은 commit의 다른 OS manifest가 없음 |
| 앱 생성/반복 setup | 한글·공백 경로에서 `app init`, `app setup` 2회, 동봉 CLI version·JSON describe | 모두 0 | success | `한글 경로\app with spaces`; CLI 0.4.2 / bundle 0.6.1, 전역 설정 변경 없음 |
| 오프라인 stdio MCP | 문서의 Python probe: initialize, tools/list, get_cli_help, EOF 정상 종료 | 0 | success | `mcp-offline.json`, `mcp-stderr.txt`; 플랫폼 로그인 없음 |
| 로컬 ZIP/체크섬 | `Compress-Archive`, `Get-FileHash`, `Expand-Archive`, 압축 해제한 exe version | cmdlet 예외 없음; exe 0 | success | 압축 전후 exe SHA-256 일치; `SHA256SUMS` |
| Windows 콘솔 Ctrl+C | 실제 Windows 콘솔 신호 전달 | 해당 없음 | not_run | Python 출력 검사에서 POSIX 신호 항목을 명시적으로 skip |
| 실제 Codex/Claude MCP | 실제 에이전트 클라이언트의 승인·세션 유지 | 해당 없음 | not_run | 이번 검사는 Python stdio probe 범위 |
| Docker Desktop 개발·배포 | 전체 `app check/dev/build/deploy` | 해당 없음 | not_run | 이번 오프라인 기반 검사 범위 밖 |
| 실제 사용자 인증·원격 운영 | 실제 로그인·원격 서버·UI/API·데이터 보존 | 해당 없음 | not_run | 실제 플랫폼·인증 세션을 사용하지 않음 |

SHA-256:

```text
c5510f56a15674d93b6641c3e73ecb04a54c96359aa67e1d98110b2e5b2262ec  miy-0.4.2-windows-x64-fb42b384081a/miy.exe
9917751a3a4e749c50a5c13145069997f793e4b45c80b830ee03c66df0c4932a  miy-0.4.2-windows-x64-fb42b384081a.zip
```

추가로 Windows PowerShell 7에서 `node --test scripts/lib/cli-result.test.mjs` **4개 통과**, 종료 코드 0을 확인했다. 로그는 공통 준비 증거 폴더의 `node-cli-result-tests.txt`에 있다. 두 셸의 성공 기록은 Windows 기반 클라이언트 빌드·오프라인 검사 범위이며, 8절의 미완료 구현이나 CLI 0.5 전체 릴리스를 완료 처리하지 않는다. 실행한 검사에서 코드 수정이 필요한 실패는 없었다.

## 10. CLI 0.6 설치·인증·개발 인수

각 PowerShell 버전에서 위 기록 준비를 실행하고 동일한 commit의 바이너리를 사용한다. 실제 서버는 CLI 0.5의 migration/API/capability가 먼저 적용된 **검증 전용 서버**를 선택한다. 아래 새 CLI 0.5 장비 결과는 아직 `NOT_RUN`이다. 앞 절의 0.4.2 기반 검사 이력은 보존한다.

### 정식 구조의 ZIP과 외부 설치 도구

7절의 단일 실행 파일 ZIP은 수동 복사용이다. 자동 설치 도구는 provenance와 스크립트가 포함된 아래 ZIP을 사용한다. `package`는 깨끗한 checkout만 허용한다.

```powershell
python scripts/package-cli-release.py package --binary $MiyExe --target x86_64-pc-windows-msvc --output (Join-Path $MiyEvidence 'candidate')
Assert-MiyExit 'package Windows release candidate'
$MiyCandidate = Join-Path $MiyEvidence 'candidate\miy-0.5.0-x86_64-pc-windows-msvc.zip'
& .\scripts\check-windows-installer.ps1 -ArchivePath $MiyCandidate
Assert-MiyExit 'Windows installer fixture'
$MiyDigest = (Get-FileHash -LiteralPath $MiyCandidate -Algorithm SHA256).Hash
$MiyInstalled = Join-Path $MiyEvidence 'installed CLI'
& .\scripts\install-cli.ps1 -ArchivePath $MiyCandidate -Sha256 $MiyDigest -InstallDir $MiyInstalled
Assert-MiyExit 'install verified candidate'
& (Join-Path $MiyInstalled 'miy.exe') --version
Assert-MiyExit 'installed version'
& .\scripts\update-cli.ps1 -ArchivePath $MiyCandidate -Sha256 $MiyDigest -InstallDir $MiyInstalled
Assert-MiyExit 'external update'
```

fixture는 체크섬 거부, 파일 잠금 시 기존 실행 파일 보존, 중간 중단 복구, 손상된 backup 거부, 제거 시 사용자 파일·설정·PATH 보존을 확인한다. 실제 프로젝트 MCP가 실행 중인 경우도 별도로 `app setup/upgrade`를 실행해 `managed_files_in_use`와 기존 hash 보존을 확인한 뒤 MCP를 정상 종료하고 다시 실행한다. 강제로 다른 프로세스를 종료하지 않는다.

기본 설치 위치는 `%LOCALAPPDATA%\Programs\miy\bin`이다. PATH 변경은 `install-cli.ps1 -AddToPath`로 요청했을 때 사용자 PATH만 적용한다. `uninstall-cli.ps1 -RemoveFromPath`도 명시적으로 요청한다. 설정·인증·프로젝트·앱 데이터는 제거하지 않는다. 업데이트는 `miy.exe` 자기 덮어쓰기가 아니라 PowerShell 외부 프로세스로 수행한다. 전역 실행 정책은 변경하지 않는다.

### 로그인과 프로젝트 밖 운영

아래의 URL·앱 ID·사용자 권한은 실제 검증 대상 값으로 바꾼다. `--account`는 loopback 개발 전용이므로 원격 인수에 사용하지 않는다.

```powershell
$MiyCli = Join-Path $MiyInstalled 'miy.exe'
$env:MIY_CONFIG_DIR = Join-Path $MiyEvidence 'private-config'
$MiyServer = 'https://YOUR-VALIDATION-SERVER'
$MiyAppId = 'YOUR-VALIDATION-APP'
& $MiyCli context add validation --url $MiyServer
Assert-MiyExit 'context add'
& $MiyCli --context validation auth login --device-code
Assert-MiyExit 'device login'
& $MiyCli --context validation auth status --output json
Assert-MiyExit 'auth status'
& $MiyCli --context validation doctor --for operate --output json
Assert-MiyExit 'doctor operate'
& $MiyCli --context validation app list --limit 1 --fields apps.id,nextCursor --output json
Assert-MiyExit 'project-free paginated list'
& $MiyCli --context validation --app $MiyAppId app show --output json
Assert-MiyExit 'project-free app show'
& $MiyCli --context validation --app $MiyAppId app stop --environment dev --dry-run --output json
Assert-MiyExit 'dry-run'
& $MiyCli --context validation auth sessions list --output json
Assert-MiyExit 'auth sessions'
```

브라우저 승인 페이지의 서버·계정·사용자 코드를 확인한다. `--no-wait` 결과의 공개 ID는 `auth wait <id>`로 이어간다. 15분 후 일반 조회 시 갱신되는지와 같은 config를 쓰는 두 터미널의 갱신 경쟁을 확인한다. Credential Manager 사용과 테스트용 `MIY_CREDENTIAL_BACKEND=file`의 DPAPI CurrentUser+ACL 경로를 별도 config로 구분한다. 토큰/DPAPI blob/쿠키/개인 인증 정보는 로그·커밋·공유 증거에 넣지 않는다. 사용자 SID가 다른 계정에서 읽히지 않아야 한다.

실제 작업의 `jobId`로 `job watch <id> --output jsonl`을 실행하고 Ctrl+C 시 `$LASTEXITCODE -eq 130`, 시간 초과 시 8, terminal 실패 시 9를 확인한다. 중단은 관찰만 끝내며 `job resume <request-id>`/`job watch <job-id>`에서 같은 원격 작업을 이어간다. `app logs --tail 100 --environment dev`는 유한 스냅샷과 대상·잘림 정보를 반환한다.

prod 변경과 모든 `app remove`는 `--context`, `--app`, `--environment`를 명시한다. 비TTY에서는 `--yes`가 없으면 종료 코드 2이며 업로드/작업이 생성되지 않아야 한다. 승인 이후 상태가 달라지면 충돌로 거부되어야 한다. `--yes`는 서버 권한을 추가하지 않는다. prod 승격은 실제 인수한 release ID에 대해서만 별도 승인 후 진행한다.

### Docker Desktop 로컬 개발과 프로젝트 MCP

Docker Desktop을 Linux 컨테이너 모드로 준비한다. 한글·공백 경로와 다른 드라이브 경로에서 각각 수행하고 Git·Rust·Node/pnpm·Docker 버전을 기록한다.

```powershell
& $MiyCli --context validation app init .\windows-demo --id windows-demo --name 'Windows 검증 앱'
Assert-MiyExit 'app init'
Set-Location -LiteralPath .\windows-demo
& .\.miy-tools\miy.exe app register
Assert-MiyExit 'app register'
# 별도 관리자 계정이 registration list/approve로 승인한 뒤 계속한다.
& .\.miy-tools\miy.exe component sync
Assert-MiyExit 'component sync'
& .\.miy-tools\miy.exe doctor --for develop --output json
Assert-MiyExit 'doctor develop'
& .\.miy-tools\miy.exe app check
Assert-MiyExit 'app check'
& .\.miy-tools\miy.exe app dev
# 별도 터미널에서 같은 폴더의 app dev-stop을 실행한다.
& .\.miy-tools\miy.exe app dev-stop
Assert-MiyExit 'app dev-stop'
& .\.miy-tools\miy.exe app dev
# 준비 중 Ctrl+C 및 dev-stop 후 재시작도 별도로 기록한다.
```

등록·관리자 승인은 위 로컬 개발 전에 완료한다. 로컬 테스트 후 `app dev-stop`으로 세션을 종료하고 Git commit 후 `app build --platform linux/amd64`와 `app deploy`를 실행한다. 로컬에서 작성한 레코드가 서버 개발 배포에도 남아야 하며, 운영 승격은 별도 DB와 데이터를 사용한다. 서버가 ARM64라면 capability에 맞춰 `linux/arm64`와 Docker emulation을 명시한다. 등록/승인과 실제 인수는 [실행 안내](RUNNING.md)의 순서를 따른다. dev-stop/중단 후 이 프로젝트의 자식·컨테이너만 정리되고 DB 볼륨·다른 프로젝트가 유지되는지 비교한다.

일반 Codex와 Claude Code 각각에서 프로젝트 신뢰·MCP 승인을 정상 진행하고 root `mcp` 서버의 `get_cli_help`, 계약/API/컴포넌트 조회와 실제 앱 작업을 확인한다. 독립 새 세션 평가는 [구현 계획](CLI-IMPLEMENTATION-PLAN.md)의 OS별 3회 규칙으로 별도 기록한다.

마지막으로 `auth logout`의 서버 철회/로컬 삭제 결과를 확인하고, `uninstall-cli.ps1 -InstallDir $MiyInstalled` 후 검증 config·프로젝트 파일이 남는지 확인한다. 결과와 실패 로그를 전달하면 실제 수행 범위만 PASS/FAIL로 갱신한다.
