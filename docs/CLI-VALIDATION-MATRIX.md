# CLI 0.5 상세 검증 기록

- 갱신: 2026-10-27
- 코드 버전: CLI 0.5.0 / 번들 0.6.2
- 범위: [구현 계획](CLI-IMPLEMENTATION-PLAN.md). [전체 V0~V7 인수](VALIDATION.md)는 별도다.

실제 실행 결과와 남은 환경 검사를 구분한다. 아래 Linux 검사는 `fb42b38` 이후 구현 작업 트리에서 실행했다. 커밋된 코드의 OS별 재검사는 CI 링크로 추가한다. `PASS`는 명시한 fixture·환경·assertion 범위에만 적용하고, 실제 장비/IdP/원격 서버/AI 세션을 대신하지 않는다.

## 로컬 결과

| 요구 | 실제 검사와 확인한 조건 | 결과 |
| --- | --- | --- |
| U1 서버 경계 | Rust 단위 101개, ignored 외부 서비스 검사 3개 별도; Clippy warnings 금지 | PASS |
| U2 클라이언트 경계 | `--no-default-features` 단위 89개와 Clippy; 서버/worker 명령 제외 | PASS |
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

**최종 통합 PASS: 52개 (runtime 33 + operations 6 + CLI 6 + auth 7), 140.96초.** 증거는 `/tmp/miy-runtime-regression-ojcff20t/evidence/`의 `results.jsonl`, `integration.log`, `cli-identity.json`, `cleanup.log`다. 기존 컨테이너 보존 검사와 cleanup errors 0을 확인했다. 검사한 CLI SHA-256은 `39f3fbed21bc1acaaa576956a55cf2ced9a48f3ed0edcbe4de56307f707aad5e`다. Runner는 각 묶음의 개수를 따로 확인하여 새 suite가 빠진 결과를 통과로 인정하지 않는다. `cli-identity.json`에 source commit·dirty 여부·검사한 실행 파일 SHA-256을 남긴다. 검사용 Python 앱 이미지는 실제 사용자 Rust/React 앱과 구분한다.

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
python3 scripts/check-cli-recovery.py --cli target/debug/miy
python3 scripts/check-bundle-portability.py check --cli target/debug/miy --output .local/ci/bundle.json
MIY_PREVIOUS_BUNDLE_CLI=/absolute/path/to/previous/miy node scripts/check-cli-guidance.mjs
python3 scripts/runtime-regression/test_cleanup.py
python3 scripts/runtime-regression/run.py
pnpm typecheck
pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
node --test scripts/lib/cli-result.test.mjs
python3 scripts/test-cli-release.py
```

`check-cli-recovery.py`의 Docker export는 합성 파일이다. 실제 서버의 업로드/worker 검사는 통합 suite와 별도 원격 인수로 확인한다. Windows 출력 fixture는 콘솔 Ctrl+C를 건너뛰므로 수동 검사 결과를 따로 기록한다. 기존 전체 서비스/AI 스크립트는 문법과 공통 helper를 이전했으나 이 작업에서 일반 플랫폼을 대상으로 실행하지 않았다.

PowerShell 개발 검사 증거: `.local/evidence/cli-installer/summary.json`. 실제 파일 복구 fixture에서 PowerShell의 null 인수 전달을 수정해 File.Replace가 올바른 null backup 경로를 받게 했다. Windows PowerShell 5.1과 네이티브 파일 잠금/ACL 검사는 별도 CI·사용자 결과로 기록한다.

원격 문서 commit `6e2e107`의 실제 Windows 기록도 통합했다. CLI 0.4.2 (`fb42b38`)는 Windows 11 Pro build 26200, PS7.6.5/5.1.26100.9711에서 각 단위37개·출력9개·release/번들/MCP 오프라인 검사를 통과했다. 상세 환경·hash·미실행 범위는 Windows 문서의 이력을 그대로 보존하며 새 0.5 인수로 이월하지 않는다.
