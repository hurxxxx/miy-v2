# CLI 상세 검증 기록

- 갱신: 2026-10-29
- 최신 개선 검증: CLI **0.6.3** / 번들 **0.7.1**, 기능 소스 `0db21c3`·fixture 보완 `8d7d042`. 실제 Windows 전체 배포 흐름은 아래 **0.6.2** 결과와 구분한다. 공개 설치 후보 0.6.0을 교체한 기록이 아니다.
- 범위: [구현 계획](CLI-IMPLEMENTATION-PLAN.md). [전체 V0~V7 인수](VALIDATION.md)는 별도다.

실제 실행 결과와 남은 환경 검사를 구분한다. 최신 Windows 개선과 0.6.2 전체 흐름을 별도로 기록하고, 0.6.0·0.5의 검증은 아래 이력에 보존한다. `PASS`는 명시한 소스·바이너리·fixture·환경·assertion 범위에만 적용하고, 실제 장비/IdP/원격 서버/AI 세션을 대신하지 않는다.

## CLI 0.6.3 개선 검증 — 2026-10-29

[상세 결과](WINDOWS-VALIDATION.md)·[결과 JSON](releases/cli-0.6.3-experience-validation.json)·[Linux 후속 작업](CLI-EXPERIENCE-SERVER-HANDOFF.md)을 보존한다. Windows 단위 106개, fmt/Clippy/release, debug/release 출력 13그룹, 원격 빌드 fixture 10그룹, 복구 6그룹 PASS. 로컬 Chrome 화면/API fixture 14개, TypeScript/플랫폼 build PASS. 실제 0.6.3 Windows GUI 설치 파일의 PowerShell 5.1 네이티브 검사 24개와 잘못된 출처·해시 거부 4개 PASS. 패키지 검사 15개 PASS, Linux 게시 4개 SKIP.

`8d7d042`의 [기반 CI 40826944825](https://github.com/hurxxxx/miy-v2/actions/runs/40826944825)는 Linux 서버, Linux/macOS/Windows 클라이언트와 OS 간 번들 비교 전체 PASS다. 5대상 배포 후보 CI와 실제 Linux 인증 DB/IdP 회귀·플랫폼 UI 재배포·다운로드 게시를 별도로 관리한다. 새 화면의 모바일 검사는 모의 API 경계이며 실제 서버의 모바일 Todo 재검증을 대신하지 않는다.

## CLI 0.6.2 Windows 실제 서버 빌드 — 2026-10-29

Windows 11 Pro x64의 비승격 PowerShell 7.6.5에서 Docker Desktop 없이 검증했다. [상세 결과](WINDOWS-VALIDATION.md#2026-10-29-cli-062-windows--서버-빌드--dev-배포-검증), [구조화 증거](releases/cli-0.6.2-windows-server-build-validation.json), [배포 화면](releases/evidence/cli-0.6.2-windows-server-build/deployed-final.jpg)을 함께 보존한다. CLI를 프로젝트에 소스 빌드로 갱신했으며 공개 GUI 설치기는 새로 게시하지 않았다.

| 검증 대상 | 실제 인수 상태 |
| --- | --- |
| Windows 소스·실행 파일 | 단위 **106개**, fmt·strict Clippy·release PASS. 새 closure 두 곳의 Windows 오류 크기 경고를 수정 후 재실행 |
| 자동 계약·복구 | 출력 **11그룹**, runtime 복구 **6그룹**, 원격 빌드 fixture **10그룹**, 지침/MCP/업그레이드 **5그룹** PASS. fixture UTF-8/LF·Windows 도구 trap 보완 |
| 네이티브 개발 | doctor **17항목**, Rust 서버·React 정적 화면·실제 서버 개발 DB CRUD·브라우저 PASS |
| Windows → Linux 빌드 | `app.builds.v1` 실제 서버의 작업 `1ba1607e-9915-4b2b-b946-d1fa8018e961` 성공. 반복 요청/resume 동일 job, 순서 로그 114개, cursor 이후 빈 로그 PASS. 제한 PATH에서 로컬 빌드 도구/SSH/WSL 호출 0회 |
| dev 배포·브라우저 | Todo 0.1.2, release `871549de-041d-4fa7-8c17-b8e7b3c54e47`, 서버 빌드와 동일 이미지·running/healthy. 브라우저 완료 취소/새로고침·API CRUD·release guard PASS |
| DB 보존·종료 | 같은 DB UUID·스키마, 기존/새 레코드 유지. 서버 변경을 로컬 재시작에서도 확인. 임시 데이터만 삭제하고 로컬 프로세스·포트·서버 개발 세션 정리 PASS |
| 남은 범위 | admin 계정으로 검사. 개발자/일반 사용자·PS5.1 새 전체 흐름·실콘솔 Ctrl+C/워커 crash/실제 통신 장애·모바일/이탈 방지·운영 승격은 NOT_RUN |

서버가 Linux에서 수행한 이전 검증은 [별도 결과](releases/cli-0.6.2-server-build-validation.json)에 남긴다. 이번 Windows 결과로 해당 서버의 전체 회귀를 다시 수행했다고 기록하지 않는다. 해당 앱 release에는 실제 범위를 명시한 구조화 인수를 저장했다.

## CLI 0.6.0 검증 이력 — 2026-10-28

아래 상태는 이번 후보에서 실제 수행한 범위다. 0.5의 PASS를 이월하지 않으며, 작업 트리 검사·깨끗한 후보 검사·공개 서버의 전체 흐름을 따로 기록한다.

| 검증 대상 | 완료 판단에 필요한 증거 | 현재 인수 상태 |
| --- | --- | --- |
| 서버 명시 등록 | 설치 명령 주소 전달, 입력 요청·취소, 비대화형 미등록 상태, 설치 후 등록, 기존 연결·다른 요청 주소 표시, 기본 주소 없음 | `b67798b` 네이티브 설치/등록 CI와 Windows PS5.1/7 설치기 PASS. 공개 설치 주소·다운로드 7개 검증 PASS |
| HTTP IP | 인증서 없는 비-loopback HTTP의 OIDC·CLI·MCP·WS DB 터널·배포와 HTTPS 회귀 | 실제 HTTP OIDC/device·인증 API·쿠키/CSRF/Origin 9개 PASS. HTTP MCP·WS DB·전체 배포 흐름은 NOT_RUN |
| 개발 준비 진단 | rustfmt, 실제 Rust 링크, Codex/Claude 설정 파싱, 제한 시간 내 local stdio initialize/tools/list | 단위 검사와 실제 생성 앱의 doctor/check PASS. 생성된 stdio MCP의 실제 계약·API·컴포넌트·CLI 도움말 도구 4종 PASS. 독립 AI와 사용자 Windows 인수는 별도 |
| 서버 개발 DB | 로컬 CRUD 후 개발 배포에서 같은 DB·레코드 유지, 운영 DB 분리 | 깨끗한 `b67798b`의 PostgreSQL·HTTP/WS 16개 PASS. 실제 Rust/React 앱의 로컬 CRUD → 동일 dev DB/레코드 → 별도 prod DB CRUD 및 양방향 격리 PASS |
| 복구·격리 | 동시 세션 충돌, 만료·권한 회수, SQL 변경/부분 실패, 연결 단절, 서버 재시작, 오래된 preflight | 후보 회귀 52개·공유 DB 16개, 이미지 수정 회귀 54개, gateway 재현 3개 PASS. 실패했던 실제 앱도 동일 이미지·릴리스·DB·레코드로 복구 PASS |
| 실제 HTTPS 전체 흐름 | 게시된 CLI로 생성 앱의 로컬 CRUD → dev 배포·같은 데이터 → 인수·prod 별도 DB 확인 | 동일 Linux 호스트의 공개 HTTPS 경로에서 신규 앱 18개 관찰 PASS. 이전 실패 앱의 복구까지 누적 20개 관찰 PASS. 서로 다른 호스트와 Windows 결과로 확대하지 않음 |
| Windows 11 | 일반 사용자 PS5.1/7, Docker Desktop, 홈페이지 설치부터 운영 승격까지 | NOT_RUN |
| 독립 AI 세션 | 새 Codex·Claude 프로젝트에서 생성 지침·MCP·CLI만으로 업무 앱 개발/검증 | NOT_RUN |

로컬 MCP process probe는 실제 AI 세션이나 서버 API 도구 성공을 증명하지 않는다. CI Windows Server 결과와 사용자 Windows 11 결과를 각각 기록한다. 최종 후보의 commit·hash·실제 수행 명령·실패·NOT_RUN을 함께 남기고 UI/UX 개선은 TODO로 관리한다.

### 게시한 CLI 후보와 후속 서버 검증

배포 CLI 소스는 `b67798bfb84f299257ba4326beaf034b125cde7a`다. [기반 CI 40602323702](https://github.com/hurxxxx/miy-v2/actions/runs/40602323702)와 [후보 CI 40602334949](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949)가 모두 통과했다. 후속 서버 수정 `1c6ba50e1431c8bbd1093056f0ae12e381c3cbbd`의 [기반 CI 40604813425](https://github.com/hurxxxx/miy-v2/actions/runs/40604813425)도 Linux 서버/클라이언트·Windows·macOS·번들 비교를 모두 통과했다. 이 서버 수정으로 이미 게시한 CLI 후보의 소스 commit이나 바이너리 hash를 바꾸어 기록하지 않는다.

| 검사 | 실제 결과와 증거 |
| --- | --- |
| 네이티브 후보 5종 | Linux x64/ARM64, macOS x64/ARM64, Windows x64의 fmt/check/Clippy·단위·최적화 빌드·실행 파일/번들 검사 PASS. aggregate는 5개 CLI와 Linux helper 2개의 버전·소스·CPU·아카이브/내부 hash를 검증하고 5개 내장 번들의 일치를 확인 |
| Windows Server 2022 | [Windows release job](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949/job/122533455640): 단위 95개, 출력 계약 11그룹, 서버 등록 9그룹, 내장/Git 파일 31개 PASS. PowerShell 5.1.20348.5756과 7.6.8의 설치/갱신·등록·기존 설정 보존·실패/잠금/복구·제거 검사 각각 9개 PASS. Windows 11/Docker Desktop 인수는 NOT_RUN |
| 서버 게시·실제 다운로드 | [후보 artifact 12779751510](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949/artifacts/12779751510)을 내려받아 재검증하고 원자적으로 게시. `check-cli-downloads.py`가 `https://v2.dwdcc.kr`의 commit 고정 URL 7개, 총 247,218,991바이트의 SHA-256 일치를 확인. [보존한 배포 index](releases/cli-0.6.0-candidate.json)는 게시 index와 동일 |
| 깨끗한 CLI 후보의 격리 회귀 | `b67798b`, `workingTreeDirty=false`. runtime 33 + operations 6 + CLI 6 + auth 7 = **52개 PASS**. `/tmp/miy-runtime-regression-k67e8rtg/evidence/`의 `cli-identity.json`, `results.jsonl`, `cleanup.json`. 기존 실행 컨테이너 22개 보존, 소유 리소스 12개 정리, cleanup errors 0 |
| 깨끗한 CLI 후보의 공유 개발 DB | `b67798b`, `workingTreeDirty=false`. 실제 PostgreSQL·HTTP/WS **16개 PASS**. `.local/development-regression-96d5ba1371/identity.json`, `test.log`; 임시 컨테이너 제거 확인. 첫 세션 DB 생성·같은 DB/레코드의 dev 배포 재사용·prod DB/역할 분리, 임시 CRUD 권한, DDL/다른 DB 차단, 만료·권한/CLI 세션/사용자 철회, SQL 부분 실패·복구·인수 스키마 경계 확인 |
| 서버 이미지 이식성 수정 작업 트리 | 기준 commit `b67798b`, **`workingTreeDirty=true`**. runtime 35 + operations 6 + CLI 6 + auth 7 = **54개 PASS**. `/tmp/miy-runtime-regression-0ay807qu/evidence/`의 `cli-identity.json`, `results.jsonl`, `portable-image-import.json`, `cleanup.json`. 기존 실행 컨테이너 22개 보존, cleanup errors 0. 이 실행은 깨끗한 `1c6ba50` 실행이나 게시한 CLI 실행으로 기록하지 않음 |

위 52개 검사의 격리 CLI SHA-256은 `84f6f3bf88e96cfd9cfe3c86546336fb9e5763bb3f54cbdab95e2b2b2cdbe603`, 공유 DB 16개 검사의 test binary SHA-256은 `48645bb5e927482ba0238bec09826cb47255a2a297db0f16fab5a0894a39b8f2`다. 수정 작업 트리 54개의 격리 CLI SHA-256은 `08a6b8ad2430ca0bd98046d16f0383f9a6c7a04e8b07b6031b6a268378bcf464`이며, 게시한 Linux CLI SHA-256 `134f8f95e7e3c39f0f42139e0c5384171c0a2779f45de677352899ce03ea4924`와 구분한다.

54개 회귀에 추가한 두 관찰은 classic 형식 Docker archive를 containerd의 검증된 불변 이미지로 연결하는 처리와, 원래 영수증 이미지 식별자를 유지한 재시작·조정·health 검사다. config 충돌 거부도 확인했다. 실제 Docker를 사용하지만 격리 suite의 Python 앱 이미지·합성 인증 계정은 실제 사용자 Rust/React 앱·실제 OIDC 전체 인수와 다르다. 위 로컬 증거 디렉터리는 Git으로 전달되지 않는다.

실제 HTTPS 앱의 첫 배포에서는 PostgreSQL에 앱 네트워크를 추가하며 기본 경로가 바뀌어, 열린 코어 DB 연결과 advisory lock 연결이 끊기는 결함을 발견했다. 앱 자체 health는 통과했지만 잠금 해제 단계에서 작업이 실패했다. 새 연결의 gateway 우선순위를 기존 모든 연결보다 낮추는 서버 수정을 적용했고, 실패를 무시하거나 잠금 검사를 제거하지 않았다. 별도 Docker/PostgreSQL에서 기존 방식의 연결 단절, 수정한 실제 함수의 동일 backend PID·임시 데이터·잠금 유지, 반복 연결 시 변화 없음의 **3개 관찰 PASS**를 확인했다. 증거는 `.local/miy-gateway-test-b0ddcc032620/`다. 처음 재현 fixture의 동적 published port가 바뀌어 실패한 기록은 `.local/miy-gateway-test-e2b62ca4b60c/`에 보존하며, 실제 서버와 같은 고정 포트 fixture로 수정했다. 서버 단위 **121개 PASS / 외부 서비스 5개 ignored**, fmt와 strict Clippy도 통과했다. 기존 플랫폼 PostgreSQL·Keycloak을 재시작하지 않았다.

최종 반영한 서버 소스 `5ca4047cdaa77c5ebb970060b91a8073bd6c7ae7`의 [기반 CI 40606859082](https://github.com/hurxxxx/miy-v2/actions/runs/40606859082)도 모두 통과했다. Linux 서버 121개/외부 통합 5개 ignored, Linux·macOS·Windows 클라이언트 각각 99·97·95개, 출력·설치·복구 검사와 3개 운영체제의 내장 번들 비교가 통과했다. 외부 통합 5개를 CI 실행으로 세지 않는다.

### 현재 서버의 실제 Rust/React 전체 흐름

검사 도구는 `scripts/check-server-development-flow.mjs`이며 CLI는 게시된 Linux 네이티브 후보 `b67798b`, 서버는 `5ca4047`이다. 서버 바이너리 SHA-256은 `bc4d772f04477bb459cd0b650bc1679b83c553ed53785eb6b996923e49ad8cbb`다. 클라이언트와 플랫폼은 같은 Linux 호스트에서 실행했고, 공개 도메인의 HTTPS/Nginx·실제 Keycloak·Docker DB 터널을 사용했다. 별도 PC 네트워크나 실제 AI 세션을 검증한 것으로 기록하지 않는다. [커밋한 결과 요약](releases/cli-0.6.0-core-validation.json)에 두 소스·해시·관찰·미실행 범위를 보존한다.

| 실행 | 관찰과 결과 |
| --- | --- |
| 실패 앱의 복구 | `.local/evidence/server-development-flow/20261027231515301-b142d5/`. 원래 실패 보고서를 별도 보존하고 일반 OIDC/device 재로그인, 실패 job 조회, 허용되지 않은 자동 retry 거부를 확인. 새 사전 검사를 거친 배포로 원래 이미지·release·DB·로컬 레코드 유지. 운영 권한 거부/관리자 승인·동일 이미지 승격·별도 prod DB CRUD까지 누적 **20개 관찰 PASS** |
| 신규 앱 전체 흐름 | `.local/evidence/server-development-flow/20261028001234088-a064df/`. 명시적 서버 설치, 분리한 개발자/관리자의 실제 로그인·앱 승인, 생성 지침·doctor/check, 로컬 브라우저/API CRUD, 첫 dev 배포의 동일 DB/레코드, 사용자별 데이터 경계, 인수·운영 권한·동일 이미지 승격, prod CRUD·양방향 DB 격리, 로그/상태의 **18개 관찰 PASS** |
| 실제 프로젝트 MCP | 신규 앱의 생성 `.mcp.json`으로 stdio 프로세스를 실행. `get_contract`의 보호 파일 19개, `search_api`의 API 51개, `list_components`의 컴포넌트 1개, `get_cli_help`의 `app dev` 도움말을 실제 브라우저 API/CLI 결과와 대조하여 PASS. 두 AI 제품의 독립 실행은 NOT_RUN |
| 정리·서비스 보존 | 두 실행의 dev/prod 런타임을 정상 중지하고 테스트 DB는 보존. 각 실행에서 사용한 두 CLI 세션을 철회. 서버 수정 반영 후 PG·Keycloak ID/이미지/시작 시각 보존과 serve/worker의 재시작 횟수 0 확인 |

후보 workflow의 `publish=false`에 따라 정식 GitHub Release/tag는 발행하지 않았다. 실제 Windows 11·PowerShell 5.1/7·Docker Desktop, 독립 Codex/Claude, 서로 다른 호스트의 원격 개발, HTTP IP에서의 MCP/WS DB/전체 배포 흐름은 여전히 NOT_RUN이다. 사용자 테스트는 [핵심 흐름 테스트](CORE-TESTING.md)에서 시작한다.

### 앞선 0.6 구현 작업 트리 검증 이력

2026-10-28 Linux에서 다음 검사를 수행했다. 이 기록은 후보 CI나 사용자 Windows 결과와 합산하지 않는다.

| 검사 | 관찰 결과 |
| --- | --- |
| Rust | 클라이언트 98개, 서버 118개 PASS; 외부 서비스 4개 ignored. 두 빌드 경계 Clippy warnings 금지·fmt PASS |
| CLI 계약 | 출력 12개(이전 runtime capability 거부 포함), 요청 복구 6개, 명시적 서버 설치 17개 PASS |
| 프런트 | TypeScript·production build PASS; 설치 화면 4개 PASS(배포 파일/체크섬 링크 포함) |
| 서버 설정 | HTTP 허용 명시, 기존 비밀 보존, 컨테이너 UID의 초기화 파일 접근, Nginx·실제 systemd 단위 검증, 기존 realm 설정 갱신 PASS |
| 개발 DB | 실제 PostgreSQL·HTTP/WS 16개 PASS: 동일 DB 재사용, 권한/세션 철회, DDL·다른 DB 접근 차단, 부분 SQL 실패·재개·스키마 revision. `.local/development-regression-57c52f5455/test.log` |
| 기존 배포 회귀 | 두 실행 모두 runtime 33 + operations 6 + CLI 6 + auth 7 = 52개 관찰 PASS. 전체 runner는 무관한 컨테이너의 외부 변경 때문에 baseline 보존 검사에서 FAIL. 검증 조건을 완화하지 않았으며 소유 테스트 리소스 12개는 모두 정리됨. `/tmp/miy-runtime-regression-0fxroqjq/evidence/`. 이후 깨끗한 후보의 52개 PASS와 별도 실행으로 보존 |
| 실제 HTTP IP | 별도 임시 서버에서 실제 Keycloak OIDC, 설치 CLI device 승인·인증 API, 쿠키·CSRF·Origin 검증 9개 PASS. 테스트 리소스 정리·기존 v2 PG/Keycloak 보존 확인. `.local/miy-http-db48ead1ea25/summary.json` |
| 실제 HTTPS 로그인 | `v2.dwdcc.kr`에서 분리한 개발자·관리자의 OIDC/device 승인, 앱 등록과 권한 거부/관리자 승인 7개 PASS. 전체 DB·배포 단계는 현재 검증 표의 별도 진행 항목 |
| 기존 앱 갱신 | 보관된 CLI 0.5/번들 0.6.2로 만든 앱을 0.6/0.7로 업그레이드. 업무 Rust/React, 원본 SQL·사용자 CRLF SQL, manifest·의존성·컴포넌트 선언 등 10개 파일 hash 보존 PASS. 보호 SQL 변조는 거부. `.local/evidence/upgrade062-703e5ae10d/result.json` |

DB 연결 helper는 Docker classic/containerd의 서로 다른 로컬 image ID 대신 검증한 config SHA로 동일성을 확인하고, 실행 시에는 검증을 마친 로컬 불변 ID를 사용한다. 실제 containerd export와 두 아카이브 형식·변조 거부 단위 검사를 통과했다. 실제 Windows Docker Desktop 결과는 NOT_RUN이다.

추가 검토에서 발견한 컨테이너 루프백 URL은 서버 DB 모드의 빌드·DB 요청 전에 명확한 오류로 거부하도록 수정했다. 주소 가드 단위 검사에 루프백 IPv4/IPv6·매핑 주소·미지정 주소·localhost 별칭과 정상 사내 IP/도메인을 포함했다. 쿼리 취소·장시간 마이그레이션의 즉시 중단은 지원 범위에서 제외하고 RUNNING에 기록했다.

사용자 테스트의 시작 순서는 [핵심 흐름 테스트](CORE-TESTING.md)를 따른다.

## 이력 — CLI 0.5 로컬 결과와 설치 개선

### 서버 주소를 전달하는 설치 개선 — 2026-10-27 작업 트리

기존 후보 이후 변경으로, 서버 `MIY_ORIGIN` 기반 `/cli/install` 안내와 `--server`/`-Server`, 설정이 없는 경우에만 생성하는 `context init --url`을 추가했다. 아래 결과는 이 변경 작업 트리의 Linux 실행이며 이전 공개 후보나 실제 Windows 통과 기록으로 이월하지 않는다.

| 검사 | 결과 |
| --- | --- |
| 서버·클라이언트 Rust | 서버 단위 106개 통과(외부 서비스 3개 ignored), 클라이언트 91개 통과. 양쪽 Clippy warnings 금지·fmt 통과 |
| 설치·기본 연결 블랙박스 | `check-cli-install.py` 11개 통과. 제공한 URL 정규화, 신규 활성 연결, 기존 파일·사용자·CA·인증정보 보존, 명시적 선택과 프로젝트 binding 독립, 잘못된 입력/설정 거부, 실제 Unix 설치·갱신·부분 실패 처리 |
| 기존 CLI 출력 계약 | `check-cli-output.py` 11개 통과 |
| PowerShell 공통 초기화·복구 | Linux PowerShell 7.6.9 + 실제 Linux CLI로 9개 통과. 새 URL 등록, 기존/빈 설정 보존, 오류 코드 전달, 파일 복구 확인. Windows 전용 설치 실행과 구분 |
| 설치 안내 API | 서버 단위 검사에 포함된 3개 통과. 서로 다른 공개 URL, 셸 인용, 외부 Host 헤더와 무관한 서버 설정 사용 확인 |
| 설치 안내 UI | Playwright 3개 통과. 공개/로그인 후 접근, 서버 제공 URL·정확한 명령 복사, API 실패 표시, 데스크톱·모바일 화면 확인 |
| 기존 UI 회귀·빌드 | runtime-health UI 6개, TypeScript, 플랫폼 production build 통과 |

당시 네이티브 CLI 설치 검사를 foundation/release CI에 추가하고 Windows PowerShell 5.1/7 설치기를 9개 시나리오로 확장했지만 네이티브 Windows/macOS 실행은 아직 미실행이었다. 당시 검사에는 `v2.dwdcc.kr` 반영·실제 로그인·새 설치 패키지가 포함되지 않았다. 이후 0.6 후보에서 완료한 결과는 위 현재 검증 표를 따른다.

### 기존 후보 결과

| 요구 | 실제 검사와 확인한 조건 | 결과 |
| --- | --- | --- |
| U1 서버 경계 | Rust 단위 103개, ignored 외부 서비스 검사 3개 별도; Clippy warnings 금지 | PASS |
| U2 클라이언트 경계 | `--no-default-features` 단위 91개와 Clippy; 서버/worker 명령 제외 | PASS |
| B1 출력 | 실제 CLI Python 블랙박스 11묶음: 자동/text/JSON, argv 오류 비밀 차단, UTF-8, 상태별 종료 코드, terminal9, POSIX Ctrl+C130, refresh 응답 유실 후 재사용 차단 | PASS |
| B2 전송 복구 | 실제 CLI + 합성 archive/loopback fault fixture 6묶음: 완전한 사전 journal, upload/runtime ACK 유실, receipt 대상/해시 거부, resume 대상 거부, timeout 후 재개, 실패 lifecycle 후 새 요청 | PASS |
| G1 지침·갱신 | 실제 도움말/describe/MCP 일치, 사용자 문서 충돌 보존, 누락 복구, 반복 setup, 비공개 MCP 유지 | PASS |
| G2 번들 전환 | 실제 보관 0.6.1 CLI로 생성한 프로젝트 → 0.6.2 upgrade; 업무 Rust/React·CRLF SQL·컴포넌트·manifest·의존성 hash 보존 | PASS |
| G3 포터빌리티 | 내장 파일 31개·보호 해시 검증, autocrlf=true Git commit/clone 후 관리/SQL/컴포넌트 파일 31개 바이트 보존 | PASS |
| W1 화면 | Playwright 6개: health/SDK/미저장 입력·경계 응답, device 계정/서버/code/CSRF, prod 확인 취소·expected-state·409 | PASS |
| S1 스크립트 | TypeScript typecheck, CLI result Node 테스트 4개, 이전한 스크립트 syntax/helper 계약 | PASS |
| C1 정리 | 실제 runner/cleanup 코드의 준비 실패 주입 1개 테스트 안 3개 시나리오, 반복 정리 | PASS |
| P2 PowerShell 도구 | Linux의 portable PowerShell 7.6.9: 스크립트6개 parser, 실제 journal4개 시나리오, 비Windows 설치 거부/디렉터리 미생성 | PASS (Windows 실행 아님) |
| P1 배포 게이트 | 누락/FAIL/다른 commit/중복 AI 세션/권한 우회 기록으로 공개 차단 | PASS |

G1/G2 증거는 `.local/evidence/cli-guidance/mvq9xhg4/result.json`, G3는 `.local/evidence/cli05/bundle-linux.json`, W1은 `.local/evidence/runtime-health-ui/`와 `/tmp/miy-cli05-browser.log`다. B1/B2의 임시 프로젝트는 종료 시 삭제하며 요약 로그와 검사 스크립트를 보존한다. 이 절의 성공은 이전 실패 실행을 성공으로 바꾼 기록이 아니다.

## 이력 — CLI 0.5 HTTP/PostgreSQL/Docker 통합

`scripts/runtime-regression/run.py`는 일회성 PostgreSQL·앱 이미지·network, 실제 router/worker와 CLI subprocess를 함께 사용한다. 일반 플랫폼의 runtime.env·인증·DB는 읽지 않고 중지/재배포하지 않는다. 원래 실행 중인 컨테이너 ID·이미지·시작 시각을 전후 비교한다. 단계가 실패해도 자기 label/소유 기록의 리소스만 정리한다.

| 묶음 | 확인하는 세부 조건 |
| --- | --- |
| Runtime 33개 | 16개 동시 중복 접수 1개 작업, 타입/권한/이미지/버전 차단, 동일 이미지 prod 승격, 독립 DB, migration rollback/기존 데이터, worker crash 복구, 제거 후 DB 보존, 다른 컨테이너 보존, health 독립성·동시성·늦은 응답 |
| Operations 6개 | 권한 적용 뒤 keyset page, 사용자/필터 cursor 경계, 이벤트 rollback/순서/terminal 마지막 페이지, dry-run/승인/상태 충돌의 부작용 0, typed retry/이전 작업 연결/멱등성, 업로드 원본 식별·소유·TTL |
| Logs (Operations 안) | 실제 소유 컨테이너 2MiB 출력→응답 1MiB 상한·truncated, label/immutable ID/실행 세대/권한/TTL 재검사, 2개 진단이 막혀 있어도 prod start 완료 |
| CLI 6개 | 실제 개발 로그인→전역 private vault, 두 프로세스 refresh 1회, flag/env/project/active 우선순위, 프로젝트 밖 운영·typed input, dry-run 및 대상/확인 누락 변경0, 요청 ID 복구·JSONL·logout |
| Auth 7개 | device pending/slowdown/CSRF/Origin/일회사용/만료/거부, 15분/30일, refresh rotation/reuse/동시 교환, 본인 세션 페이지/철회, Core 비활성화·재활성화·기존 세션 차단, rate limit |

**CLI 0.5 당시 최종 통합 PASS: 52개 (runtime 33 + operations 6 + CLI 6 + auth 7), 140.33초.** `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 clean 작업본에서 실행했으며, 증거는 `/tmp/miy-runtime-regression-k3bkplo6/evidence/`의 `results.jsonl`, `integration.log`, `cli-identity.json`, `cleanup.json`, `cleanup.log`다. 기존 실행 컨테이너 20개의 ID·이미지·시작 시각·실행 상태 보존과 cleanup errors 0을 확인했다. 검사한 CLI SHA-256은 `f4301fc8b7c787f5d210d66a7c5a32b6e77ccf364383c63e0ea1533701eb4fac`다. Runner는 빌드한 CLI를 격리 경로에 복사한 뒤 실행하고, 각 묶음의 개수를 따로 확인하여 새 suite가 빠진 결과를 통과로 인정하지 않는다. `cli-identity.json`의 `workingTreeDirty`는 `false`다. 검사용 Python 앱 이미지는 실제 사용자 Rust/React 앱과 구분한다.

앞선 dirty 작업본의 52개 PASS(140.96초)는 `/tmp/miy-runtime-regression-ojcff20t/evidence/`에 보존한다. 최종 결과와 합산하지 않는다. 위 로컬 증거 경로는 Git으로 전달되지 않는다.

## 이력 — CLI 0.5 실패를 통해 보완한 부분

- 재개 시 dev 요청도 명시한 앱·환경과 journal 대상이 다르면 충돌로 거부한다.
- runtime payload/hash/path를 완성하고 저장한 다음 journal index를 공개한다. 불완전 기록 때문에 정상 실행을 복구할 수 없는 경우를 제거했다.
- 실패한 lifecycle 이후 원인을 해결하고 새 명령을 내리면 새 preflight/UUID로 실행한다. `resume`은 원래 요청, `retry`는 타입상 허용된 실패에만 적용한다.
- 실제 로그 fixture의 API 프록시 경로와 prod 제어 관찰 대상을 수정해 유효한 2MiB/독립성 검사를 만들었다.
- 실제 CLI fixture의 허용 개발 계정 이름·명시 앱 대상·충돌 종료 코드(6)를 바로잡았다. 해당 실패 run은 최종 통과 증거에 합치지 않는다.
- 소스 경계의 dead-code/Clippy 지적을 서버 또는 test cfg로 정리했다. 잘못된 검사 기대값 때문에 제품의 실패 정책을 완화하지 않았다.

## 이력 — CLI 0.5에서 남겨둔 외부 인수

다음은 0.5 종료 시점의 상태다. 현재 0.6에서 추가로 수행한 실제 HTTP/OIDC·CI·다운로드 결과와 미완료 항목은 문서 위의 현재 검증 표를 따른다.

| 요구 | 상태와 필요한 증거 |
| --- | --- |
| 이번 0.5 사용자 Windows 11 | NOT_RUN — PowerShell 5.1/7, 일반 사용자, Credential Manager/DPAPI·ACL, 콘솔 Ctrl+C·프로세스 소유·실제 MCP |
| Docker Desktop | NOT_RUN — init/check/dev/dev-stop/dev/build/deploy, Linux 모드, CPU 선택·볼륨/다른 앱 보존 |
| 별도 원격 Linux | NOT_RUN — 실제 네트워크/CA/이미지 전송과 생성 앱 업무 UI/API·데이터; loopback fixture를 원격 실증으로 세지 않음 |
| 실제 IdP | NOT_RUN — 실제 OIDC 승인 + Core 사용자 비활성화 후 발급/갱신/API/worker 차단. Keycloak 비활성화 자동 전달은 구현된 계약이 아님 |
| 독립 AI | NOT_RUN — Codex/Claude 각 Windows/Linux/macOS 새 세션 3회, 동일 후보 commit·업무 인수·호출/문서량/개입/복구 측정 |
| 공개 릴리스 | NOT_RUN — 위 증거를 갖춘 뒤 [candidate 공개 게이트](CLI-RELEASE.md) 통과 필요 |

Windows 사용자는 [Windows 안내](WINDOWS-VALIDATION.md)의 각 명령·종료 코드·환경·hash·증거를 전달한다. 자동 CI의 Windows Server 결과와 실제 Windows 11 결과를 구분한다. fixture 성공을 실제 보안 저장소 서비스나 실제 OIDC 로그인 성공으로 확대하지 않는다.

## 재실행 명령

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
cargo test --workspace --locked --no-default-features
cargo build --locked --no-default-features
python3 scripts/check-cli-output.py --cli target/debug/miy
python3 scripts/check-cli-install.py --cli target/debug/miy
python3 scripts/check-cli-recovery.py --cli target/debug/miy
python3 scripts/check-bundle-portability.py check --cli target/debug/miy --output .local/ci/bundle.json
MIY_PREVIOUS_BUNDLE_CLI=/absolute/path/to/previous/miy node scripts/check-cli-guidance.mjs
python3 scripts/runtime-regression/test_cleanup.py
python3 scripts/runtime-regression/run.py
pnpm typecheck
pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
pnpm exec playwright test --config scripts/runtime-regression/cli-install-ui.config.ts
node --test scripts/lib/cli-result.test.mjs
python3 scripts/test-cli-release.py
```

`check-cli-recovery.py`의 Docker export는 합성 파일이다. 실제 서버의 업로드/worker 검사는 통합 suite와 별도 원격 인수로 확인한다. Windows 출력 fixture는 콘솔 Ctrl+C를 건너뛰므로 수동 검사 결과를 따로 기록한다. 기존 전체 서비스/AI 스크립트는 문법과 공통 helper를 이전했으나 이 작업에서 일반 플랫폼을 대상으로 실행하지 않았다.

PowerShell 개발 검사 증거: `.local/evidence/cli-installer/summary.json`. 실제 파일 복구 fixture에서 PowerShell의 null 인수 전달을 수정해 File.Replace가 올바른 null backup 경로를 받게 했다. Windows PowerShell 5.1과 네이티브 파일 잠금/ACL 검사는 별도 CI·사용자 결과로 기록한다.

원격 문서 commit `6e2e107`의 실제 Windows 기록도 통합했다. CLI 0.4.2 (`fb42b38`)는 Windows 11 Pro build 26200, PS7.6.5/5.1.26100.9711에서 각 단위37개·출력9개·release/번들/MCP 오프라인 검사를 통과했다. 상세 환경·hash·미실행 범위는 Windows 문서의 이력을 그대로 보존하며 새 0.5 인수로 이월하지 않는다.

### CLI 0.5 이력: 첫 네이티브 CI에서 보완한 경계

`34546ae`의 [CI 40458485585](https://github.com/hurxxxx/miy-v2/actions/runs/40458485585)는 Linux 서버/클라이언트를 통과했지만 macOS 임시 경로와 Windows 종료코드 검사에서 실패했다. macOS의 root 소유 `/var → /private/var`, `/tmp → /private/tmp` 시스템 조상 별칭만 허용하고 설정 root·임의 조상·credentials/record 링크 거부를 유지했다. fixture 경로도 정규화했다. Windows Job Object 종료는 특정 종료코드를 보장하지 않으므로, 테스트를 종료 전 생존·소속, 종료 후 제한 시간 내 완료, 별도 Job의 프로세스 생존 확인으로 수정했다. 실패를 성공으로 집계하지 않으며 후보 CI `40458514245`는 해당 수정 후 다시 만들기 위해 취소했다.

보완 후 로컬 서버 단위103개/클라이언트91개, 출력11묶음, 클라이언트 Clippy와 Windows 최소 harness all-targets Clippy를 통과했다. Windows 개발 안내의 `doctor --for develop` 및 선행 `component sync`도 수정했다. 실제 OS 재실행 결과는 다음 CI 기록으로 추가한다.

### CLI 0.5 이력: Windows debug 스택과 설치기 실증

`021c3b`의 [release Windows CI](https://github.com/hurxxxx/miy-v2/actions/runs/40459450347/job/122040382727)는 Windows Server 2022에서 네이티브 단위87개·출력10개·번들31파일과 PowerShell 5.1.20348.5756/7.6.8 설치기 각각6시나리오를 통과했다. 체크섬 거부, 설치/갱신, 파일 잠금, 손상 backup 거부, 중단 복구, 제거 시 보존을 실제 Windows에서 실행했다. Windows 11/Docker Desktop 인수는 계속 별도다.

같은 코드의 [foundation CI](https://github.com/hurxxxx/miy-v2/actions/runs/40459433715)는 debug CLI 진입 시 Windows 1MiB 스택에서 overflow를 발견했다. Linux 자식 프로세스의 RLIMIT_STACK을 1MiB로 제한해 재현하고, async 명령 dispatch를 heap에 분리하고 업로드 hash buffer도 heap으로 옮겼다. Linux 출력 fixture와 CI의 요청 복구 fixture를 같은 한도로 실행하도록 바꿨다. 변경 후 출력11묶음·복구6묶음 및 서버103/클라이언트91 단위 검사를 통과했다.

병렬 단위 검사에서 컴포넌트 잠금 재획득이 한 번 실패한 것도 기록한다. 자식 생성 시 잠금 descriptor가 일시적으로 남을 수 있어 guard 종료에 명시적 unlock을 추가했고, 기존 동시 설치 거부·중단 복구 검사와 전체 단위 검사를 다시 통과했다. 새 수정의 전체 native/candidate 결과는 뒤의 최종 CI 기록을 따른다.

### CLI 0.5 이력: 최종 커밋의 기본 CI

소스 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 [CI 40460546995](https://github.com/hurxxxx/miy-v2/actions/runs/40460546995)는 **전체 5개 job 성공**이다. 이전 실패 run의 결과와 합산하지 않는다.

| 환경 | 실제 통과 범위 |
| --- | --- |
| Linux 서버 | fmt/check/Clippy, 단위103개·외부 서비스 검사3개 ignored |
| Linux 클라이언트 | 단위91개, 출력11묶음, 요청 복구6묶음, 1MiB 스택 제한, 번들/Git31파일 |
| macOS 클라이언트 | 단위89개, 출력11묶음, 시스템 임시 경로 별칭, 번들/Git31파일 |
| Windows MSVC 클라이언트 | 단위87개, debug 실행·출력10묶음, Job Object 소유 종료, 번들/Git31파일 |
| OS 간 비교 | Linux/macOS/Windows 내장 번들 해시 일치 |

Windows 환경은 Windows Server 2022 Datacenter `10.0.20348`, runner image `20261022.344.1`, Rust `1.99.1`, Python `3.12.10` x64다. 출력 fixture의 Windows 콘솔 Ctrl+C는 명시적으로 SKIP이며, 외부 실제 서비스는 사용하지 않았다. Windows 11 사용자 인수와 구분한다.

### CLI 0.5 이력: 최종 5개 배포 후보

같은 소스의 [release candidate CI 40460582868](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868)는 **candidate·5개 native·aggregate job 전부 성공**했고 `publish`는 요청값 `false`에 따라 SKIP했다.

| 대상 | 단위 검사 | 최적화 실행 파일의 검사 |
| --- | --- | --- |
| Linux x64 / ARM64 | 각각91개 | 각각 출력11묶음, 내장31파일·Git 왕복31파일 |
| macOS x64 / ARM64 | 각각89개 | 각각 출력11묶음, 내장31파일·Git 왕복31파일 |
| Windows x64 MSVC | 87개 | 출력10묶음, 내장31파일·Git 왕복31파일, PS5.1/7 설치기 각6시나리오 |

모든 target의 fmt/check/Clippy 경고 금지와 release build가 통과했다. aggregate는 5개 아카이브의 SHA-256, 내부 바이너리 hash·CPU header·버전·동일 소스 commit·내외부 provenance를 검증하고, 다섯 native build의 내장 bundle/scaffold hash 일치를 확인했다. 최종 artifact를 이 Linux 환경에 내려받아 `python3 scripts/package-cli-release.py verify --directory .local/evidence/cli05/release-0fe94a4`로 아카이브 검증을 다시 통과했다. 다른 OS 바이너리를 Linux에서 실행한 결과는 아니다.

- [전체 후보 artifact](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868/artifacts/12715067551)
- [커밋한 배포물별 SHA-256·binary hash·source index](releases/cli-0.5.0-candidate.json)
- Windows Server 설치기 환경과 ZIP hash는 [Windows 안내](WINDOWS-VALIDATION.md)에 별도로 기록했다.

이로써 이전 0.5 후보의 로컬 자동 검사·기본 CI·5개 배포 후보 생성 검사를 완료했다. 당시 외부 인수의 `NOT_RUN`과 정식 릴리스 미발행 상태를 이력으로 보존한다. 결과 문서만 수정한 후속 commit을 검증 대상 소스 commit으로 바꿔 기록하지 않는다.
