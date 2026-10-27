# CLI 상세 검증 기록

- 갱신: 2026-10-28
- 현재 구현 대상: CLI 0.6.0 / 번들 0.7.0. 기존 증거의 기준은 CLI 0.5.0 / 번들 0.6.2다.
- 범위: [구현 계획](CLI-IMPLEMENTATION-PLAN.md). [전체 V0~V7 인수](VALIDATION.md)는 별도다.

실제 실행 결과와 남은 환경 검사를 구분한다. 초기 Linux 검사는 `fb42b38` 이후 구현 작업 트리에서 실행했고, 최종 통합 검사는 미커밋 변경이 없는 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`에서 다시 실행했다. 같은 커밋의 OS별 검사 결과를 아래 CI 기록으로 연결한다. `PASS`는 명시한 fixture·환경·assertion 범위에만 적용하고, 실제 장비/IdP/원격 서버/AI 세션을 대신하지 않는다.

## CLI 0.6 추가 검증 범위 — 2026-10-28

아래는 새 후보에서 실행·기록할 항목이다. 0.5의 PASS를 새 구현의 결과로 이월하지 않는다. 개별 개발 중 검사와 최종 후보 commit의 통합 검사도 구분한다.

| 검증 대상 | 완료 판단에 필요한 증거 | 현재 인수 상태 |
| --- | --- | --- |
| 서버 명시 등록 | 설치 명령 주소 전달, 입력 요청·취소, 비대화형 미등록 상태, 설치 후 등록, 기존 연결·다른 요청 주소 표시, 기본 주소 없음 | 최종 후보 재검증 필요 |
| HTTP IP | 인증서 없는 비-loopback HTTP의 OIDC·CLI·MCP·WS DB 터널·배포와 HTTPS 회귀 | 최종 후보 재검증 필요 |
| 개발 준비 진단 | rustfmt, 실제 Rust 링크, Codex/Claude 설정 파싱, 제한 시간 내 local stdio initialize/tools/list | 최종 후보 재검증 필요 |
| 서버 개발 DB | 로컬 CRUD 후 개발 배포에서 같은 DB·레코드 유지, 운영 DB 분리 | 최종 후보 재검증 필요 |
| 복구·격리 | 동시 세션 충돌, 만료·권한 회수, SQL 변경/부분 실패, 연결 단절, 서버 재시작, 오래된 preflight | 최종 후보 재검증 필요 |
| Windows 11 | 일반 사용자 PS5.1/7, Docker Desktop, 홈페이지 설치부터 운영 승격까지 | NOT_RUN |
| 독립 AI 세션 | 새 Codex·Claude 프로젝트에서 생성 지침·MCP·CLI만으로 업무 앱 개발/검증 | NOT_RUN |

로컬 MCP process probe는 실제 AI 세션이나 서버 API 도구 성공을 증명하지 않는다. CI Windows Server 결과와 사용자 Windows 11 결과를 각각 기록한다. 최종 후보의 commit·hash·실제 수행 명령·실패·NOT_RUN을 함께 남기고 UI/UX 개선은 TODO로 관리한다.

### 0.6 구현 작업 트리 검증

2026-10-28 Linux에서 다음 검사를 수행했다. 이 기록은 후보 CI나 사용자 Windows 결과와 합산하지 않는다.

| 검사 | 관찰 결과 |
| --- | --- |
| Rust | 클라이언트 98개, 서버 118개 PASS; 외부 서비스 4개 ignored. 두 빌드 경계 Clippy warnings 금지·fmt PASS |
| CLI 계약 | 출력 12개(이전 runtime capability 거부 포함), 요청 복구 6개, 명시적 서버 설치 17개 PASS |
| 프런트 | TypeScript·production build PASS; 설치 화면 4개 PASS(배포 파일/체크섬 링크 포함) |
| 서버 설정 | HTTP 허용 명시, 기존 비밀 보존, 컨테이너 UID의 초기화 파일 접근, Nginx·실제 systemd 단위 검증, 기존 realm 설정 갱신 PASS |
| 개발 DB | 실제 PostgreSQL·HTTP/WS 16개 PASS: 동일 DB 재사용, 권한/세션 철회, DDL·다른 DB 접근 차단, 부분 SQL 실패·재개·스키마 revision. `.local/development-regression-57c52f5455/test.log` |
| 기존 배포 회귀 | 두 실행 모두 runtime 33 + operations 6 + CLI 6 + auth 7 = 52개 관찰 PASS. 전체 runner는 별도 GitLab 컨테이너의 외부 변경 때문에 baseline 보존 검사에서 FAIL. 검증 조건을 완화하지 않았으며 소유 테스트 리소스 12개는 모두 정리됨. `/tmp/miy-runtime-regression-0fxroqjq/evidence/` |
| 실제 HTTP IP | 별도 임시 서버에서 실제 Keycloak OIDC, 설치 CLI device 승인·인증 API, 쿠키·CSRF·Origin 검증 9개 PASS. 테스트 리소스 정리·기존 v2 PG/Keycloak 보존 확인. `.local/miy-http-db48ead1ea25/summary.json` |
| 실제 HTTPS 로그인 | `v2.dwdcc.kr`에서 분리한 개발자·관리자의 OIDC/device 승인, 앱 등록과 권한 거부/관리자 승인 7개 PASS. 전체 DB·배포 단계는 후보 파일 준비 후 별도 실행 |
| 기존 앱 갱신 | 보관된 CLI 0.5/번들 0.6.2로 만든 앱을 0.6/0.7로 업그레이드. 업무 Rust/React, 원본 SQL·사용자 CRLF SQL, manifest·의존성·컴포넌트 선언 등 10개 파일 hash 보존 PASS. 보호 SQL 변조는 거부. `.local/evidence/upgrade062-703e5ae10d/result.json` |

DB 연결 helper는 Docker classic/containerd의 서로 다른 로컬 image ID 대신 검증한 config SHA로 동일성을 확인하고, 실행 시에는 검증을 마친 로컬 불변 ID를 사용한다. 실제 containerd export와 두 아카이브 형식·변조 거부 단위 검사를 통과했다. 실제 Windows Docker Desktop 결과는 NOT_RUN이다.

추가 검토에서 발견한 컨테이너 루프백 URL은 서버 DB 모드의 빌드·DB 요청 전에 명확한 오류로 거부하도록 수정했다. 주소 가드 단위 검사에 루프백 IPv4/IPv6·매핑 주소·미지정 주소·localhost 별칭과 정상 사내 IP/도메인을 포함했다. 쿼리 취소·장시간 마이그레이션의 즉시 중단은 지원 범위에서 제외하고 RUNNING에 기록했다.

사용자 테스트의 시작 순서는 [핵심 흐름 테스트](CORE-TESTING.md)를 따른다.

## 로컬 결과 — 이전 후보와 설치 개선 이력

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

네이티브 CLI 설치 검사는 foundation/release CI에 추가했다. 실제 Windows PowerShell 5.1/7 설치기는 신규 연결·설정 보존·부분 실패 검사까지 9개 시나리오로 확장했지만, 이번 변경의 네이티브 Windows/macOS 실행은 미실행이다. `v2.dwdcc.kr` 운영 반영·실제 로그인·새 설치 패키지 발행도 이 검증에 포함하지 않는다.

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

## 실제 HTTP/PostgreSQL/Docker 통합

`scripts/runtime-regression/run.py`는 일회성 PostgreSQL·앱 이미지·network, 실제 router/worker와 CLI subprocess를 함께 사용한다. 일반 플랫폼의 runtime.env·인증·DB는 읽지 않고 중지/재배포하지 않는다. 원래 실행 중인 컨테이너 ID·이미지·시작 시각을 전후 비교한다. 단계가 실패해도 자기 label/소유 기록의 리소스만 정리한다.

| 묶음 | 확인하는 세부 조건 |
| --- | --- |
| Runtime 33개 | 16개 동시 중복 접수 1개 작업, 타입/권한/이미지/버전 차단, 동일 이미지 prod 승격, 독립 DB, migration rollback/기존 데이터, worker crash 복구, 제거 후 DB 보존, 다른 컨테이너 보존, health 독립성·동시성·늦은 응답 |
| Operations 6개 | 권한 적용 뒤 keyset page, 사용자/필터 cursor 경계, 이벤트 rollback/순서/terminal 마지막 페이지, dry-run/승인/상태 충돌의 부작용 0, typed retry/이전 작업 연결/멱등성, 업로드 원본 식별·소유·TTL |
| Logs (Operations 안) | 실제 소유 컨테이너 2MiB 출력→응답 1MiB 상한·truncated, label/immutable ID/실행 세대/권한/TTL 재검사, 2개 진단이 막혀 있어도 prod start 완료 |
| CLI 6개 | 실제 개발 로그인→전역 private vault, 두 프로세스 refresh 1회, flag/env/project/active 우선순위, 프로젝트 밖 운영·typed input, dry-run 및 대상/확인 누락 변경0, 요청 ID 복구·JSONL·logout |
| Auth 7개 | device pending/slowdown/CSRF/Origin/일회사용/만료/거부, 15분/30일, refresh rotation/reuse/동시 교환, 본인 세션 페이지/철회, Core 비활성화·재활성화·기존 세션 차단, rate limit |

**최종 통합 PASS: 52개 (runtime 33 + operations 6 + CLI 6 + auth 7), 140.33초.** `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 clean 작업본에서 실행했으며, 증거는 `/tmp/miy-runtime-regression-k3bkplo6/evidence/`의 `results.jsonl`, `integration.log`, `cli-identity.json`, `cleanup.json`, `cleanup.log`다. 기존 실행 컨테이너 20개의 ID·이미지·시작 시각·실행 상태 보존과 cleanup errors 0을 확인했다. 검사한 CLI SHA-256은 `f4301fc8b7c787f5d210d66a7c5a32b6e77ccf364383c63e0ea1533701eb4fac`다. Runner는 빌드한 CLI를 격리 경로에 복사한 뒤 실행하고, 각 묶음의 개수를 따로 확인하여 새 suite가 빠진 결과를 통과로 인정하지 않는다. `cli-identity.json`의 `workingTreeDirty`는 `false`다. 검사용 Python 앱 이미지는 실제 사용자 Rust/React 앱과 구분한다.

앞선 dirty 작업본의 52개 PASS(140.96초)는 `/tmp/miy-runtime-regression-ojcff20t/evidence/`에 보존한다. 최종 결과와 합산하지 않는다. 위 로컬 증거 경로는 Git으로 전달되지 않는다.

## 실패를 통해 보완한 부분

- 재개 시 dev 요청도 명시한 앱·환경과 journal 대상이 다르면 충돌로 거부한다.
- runtime payload/hash/path를 완성하고 저장한 다음 journal index를 공개한다. 불완전 기록 때문에 정상 실행을 복구할 수 없는 경우를 제거했다.
- 실패한 lifecycle 이후 원인을 해결하고 새 명령을 내리면 새 preflight/UUID로 실행한다. `resume`은 원래 요청, `retry`는 타입상 허용된 실패에만 적용한다.
- 실제 로그 fixture의 API 프록시 경로와 prod 제어 관찰 대상을 수정해 유효한 2MiB/독립성 검사를 만들었다.
- 실제 CLI fixture의 허용 개발 계정 이름·명시 앱 대상·충돌 종료 코드(6)를 바로잡았다. 해당 실패 run은 최종 통과 증거에 합치지 않는다.
- 소스 경계의 dead-code/Clippy 지적을 서버 또는 test cfg로 정리했다. 잘못된 검사 기대값 때문에 제품의 실패 정책을 완화하지 않았다.

## 남은 외부 인수

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

### 첫 네이티브 CI에서 보완한 경계

`34546ae`의 [CI 40458485585](https://github.com/hurxxxx/miy-v2/actions/runs/40458485585)는 Linux 서버/클라이언트를 통과했지만 macOS 임시 경로와 Windows 종료코드 검사에서 실패했다. macOS의 root 소유 `/var → /private/var`, `/tmp → /private/tmp` 시스템 조상 별칭만 허용하고 설정 root·임의 조상·credentials/record 링크 거부를 유지했다. fixture 경로도 정규화했다. Windows Job Object 종료는 특정 종료코드를 보장하지 않으므로, 테스트를 종료 전 생존·소속, 종료 후 제한 시간 내 완료, 별도 Job의 프로세스 생존 확인으로 수정했다. 실패를 성공으로 집계하지 않으며 후보 CI `40458514245`는 해당 수정 후 다시 만들기 위해 취소했다.

보완 후 로컬 서버 단위103개/클라이언트91개, 출력11묶음, 클라이언트 Clippy와 Windows 최소 harness all-targets Clippy를 통과했다. Windows 개발 안내의 `doctor --for develop` 및 선행 `component sync`도 수정했다. 실제 OS 재실행 결과는 다음 CI 기록으로 추가한다.

### Windows debug 스택과 설치기 실증

`021c3b`의 [release Windows CI](https://github.com/hurxxxx/miy-v2/actions/runs/40459450347/job/122040382727)는 Windows Server 2022에서 네이티브 단위87개·출력10개·번들31파일과 PowerShell 5.1.20348.5756/7.6.8 설치기 각각6시나리오를 통과했다. 체크섬 거부, 설치/갱신, 파일 잠금, 손상 backup 거부, 중단 복구, 제거 시 보존을 실제 Windows에서 실행했다. Windows 11/Docker Desktop 인수는 계속 별도다.

같은 코드의 [foundation CI](https://github.com/hurxxxx/miy-v2/actions/runs/40459433715)는 debug CLI 진입 시 Windows 1MiB 스택에서 overflow를 발견했다. Linux 자식 프로세스의 RLIMIT_STACK을 1MiB로 제한해 재현하고, async 명령 dispatch를 heap에 분리하고 업로드 hash buffer도 heap으로 옮겼다. Linux 출력 fixture와 CI의 요청 복구 fixture를 같은 한도로 실행하도록 바꿨다. 변경 후 출력11묶음·복구6묶음 및 서버103/클라이언트91 단위 검사를 통과했다.

병렬 단위 검사에서 컴포넌트 잠금 재획득이 한 번 실패한 것도 기록한다. 자식 생성 시 잠금 descriptor가 일시적으로 남을 수 있어 guard 종료에 명시적 unlock을 추가했고, 기존 동시 설치 거부·중단 복구 검사와 전체 단위 검사를 다시 통과했다. 새 수정의 전체 native/candidate 결과는 뒤의 최종 CI 기록을 따른다.

### 최종 커밋의 기본 CI

소스 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 [CI 40460546995](https://github.com/hurxxxx/miy-v2/actions/runs/40460546995)는 **전체 5개 job 성공**이다. 이전 실패 run의 결과와 합산하지 않는다.

| 환경 | 실제 통과 범위 |
| --- | --- |
| Linux 서버 | fmt/check/Clippy, 단위103개·외부 서비스 검사3개 ignored |
| Linux 클라이언트 | 단위91개, 출력11묶음, 요청 복구6묶음, 1MiB 스택 제한, 번들/Git31파일 |
| macOS 클라이언트 | 단위89개, 출력11묶음, 시스템 임시 경로 별칭, 번들/Git31파일 |
| Windows MSVC 클라이언트 | 단위87개, debug 실행·출력10묶음, Job Object 소유 종료, 번들/Git31파일 |
| OS 간 비교 | Linux/macOS/Windows 내장 번들 해시 일치 |

Windows 환경은 Windows Server 2022 Datacenter `10.0.20348`, runner image `20261022.344.1`, Rust `1.99.1`, Python `3.12.10` x64다. 출력 fixture의 Windows 콘솔 Ctrl+C는 명시적으로 SKIP이며, 외부 실제 서비스는 사용하지 않았다. Windows 11 사용자 인수와 구분한다.

### 최종 5개 배포 후보

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

이로써 이번 코드의 로컬 자동 검사·기본 CI·5개 배포 후보 생성 검사를 완료했다. 위 외부 인수의 `NOT_RUN` 상태와 공개 릴리스 미발행 상태는 유지한다. 결과 문서만 수정한 후속 commit을 검증 대상 소스 commit으로 바꿔 기록하지 않는다.
