# CLI 0.6.3 Linux 검증과 서버 반영 — 2026-10-29

`https://v2.dwdcc.kr`의 API·worker·플랫폼 UI와 CLI/Windows 설치기 다운로드를 **0.6.3**으로 반영했다. SDK 번들은 **0.7.1**이다. 자동 검사는 통과했으며, 실제 IdP 정상 로그인과 그 로그인에서 소비한 OAuth 콜백의 재사용은 **NOT_RUN**으로 남긴다. [명령·종료 코드·해시·범위 JSON](releases/cli-0.6.3-linux-validation.json)을 함께 확인한다.

## 소스와 배포물

- 후보: `8d7d04271f9467d8bbdd16308e73715b33e4dc0c`, 브랜치 `codex/cli-experience-improvements`.
- 별도 checkout: `/projects/miy-v2-cli-experience-20261029`, 시작 HEAD `84d58cd417c990e37e29d4b8183b260f0f10d0a5`. 후보 이후에는 인수 문서·Windows 증거와 Linux Chromium 선택 설정만 추가됐다. Rust/UI/SDK/패키지 스크립트의 후보와 동일함을 확인했다.
- 기존 `/projects/miy-v2`의 추적 파일은 그대로 보존했다. 강제 reset, core SQL 변경·migrate·seed, Keycloak 초기화를 수행하지 않았다. 적용 SQL 15개는 이전 서버 `1775eac`과 바이트가 같다.
- [CLI CI 40826960538](https://github.com/hurxxxx/miy-v2/actions/runs/40826960538): 다섯 CLI와 두 Linux helper 성공, publish 단계 skipped. Artifact `12876804133`을 내려받아 aggregate 전체를 다시 검증했다.
- [Windows 설치기 CI 40830813012](https://github.com/hurxxxx/miy-v2/actions/runs/40830813012): workflow ref는 지정 브랜치, `installer_commit`과 `cli_commit`은 모두 `8d7d042…`, `cli_run_id=40826960538`. 정확한 CI Windows 행의 해시를 전달했다. Artifact `12878084842`의 내장 ZIP·PE·provenance를 Linux에서 재검증했다.
- [CLI index](releases/cli-0.6.3-candidate.json)와 [설치기 index](releases/windows-installer-0.6.3-candidate.json)에 전체 해시를 보존한다. Windows 로컬 `0db21c3` 후보의 해시를 사용하지 않았다. 공개 GitHub Release/tag는 생성하지 않았다.

| 파일 | SHA-256 |
| --- | --- |
| 배포한 Linux 서버, default features | `4573260257147c2563ba5ffae177b9ac6c6b0a331d2a0b1fef1cf6e60d10064e` |
| CI Linux x64 CLI | `86d0d6294eb751ab88e7d7ed1d6925c0a477b4e71686e1cb087f17f5026bbf54` |
| CI Windows ZIP | `1403083b604b77036785d07c42f13c6aad754ea7ae6ff7de94bb15e0a1eb72e3` |
| CI Windows miy.exe | `055bc8655c706b586f1d8517cb719ac8cd1335107e2815aef10747fa3b7565e3` |
| MiySetup-0.6.3-windows-x64.exe, 6,105,600 bytes | `662f8a59c098ac4b2cbde4041799a99f0c6e21f193bbe8b31d2098f6095233af` |
| 실제 제공되는 플랫폼 index.html | `80509d76a5c38c23c5ae57de3cb5729c4f1388a9bbb0de84f5ab9b6cb9bab73d` |

## 이번 Linux 실행 결과

검사마다 빈 `MIY_CONFIG_DIR`를 사용했다. Windows의 앞선 검사 수를 이월하지 않았다. 아래 최종 명령은 모두 종료 코드 **0**이며, 정확한 argv와 실패했던 시도는 [Rust 명령](releases/evidence/cli-0.6.3-linux/rust-commands.json), [UI 명령](releases/evidence/cli-0.6.3-linux/ui-summary.json), [서버 반영·실제 브라우저 명령](releases/evidence/cli-0.6.3-linux/root-commands.jsonl)에 있다.

| 검사 | 결과 |
| --- | --- |
| `cargo fmt --all -- --check`, default-feature check/Clippy `-D warnings`/test/release build | 단위 **143 PASS**, opt-in **6 ignored**. 그중 runtime/worker-child와 server-build API는 아래 별도 harness로 실행 |
| `--no-default-features` check/Clippy/test/build | 단위 **110 PASS** |
| CLI output / remote-build / recovery fixture | **14 / 10 / 6 그룹 PASS** |
| `python3 scripts/runtime-regression/run.py` | **55 PASS**: runtime 35, operations 6, CLI 6, auth 8 |
| `python3 scripts/check-server-builds.py` | PostgreSQL·HTTP API **8 그룹 PASS**. 이 suite는 실제 앱 Docker 컴파일을 수행하지 않음 |
| `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm --filter @miy/platform build` | PASS |
| `pnpm exec playwright test --config tests/cli-experience.config.ts` | Linux Chromium 모의 API 화면 **14 PASS**, 실패·skip·flaky 0 |
| 실제 서버 `tests/development-login.spec.ts` | **3 PASS**: 네 계정 로그인/로그아웃, 잘못된 Origin·계정 거부, 정상 device 승인·교환·철회 |
| 실제 HTTPS 브라우저 추가 검사 | **8 그룹 PASS**: 동일 admin, 데이터 보존, 데스크톱, 390px 메뉴, 자연 만료, 상태 API/코드 수정/소비 완료, 실패 콜백 복구, 설치기 다운로드 |
| CI Windows 설치기 / Linux 패키지 검사 | **24 / 19 PASS**. 사용자 Windows 11의 새 CI 파일 실행과 구분 |
| 공개 다운로드 | CLI 5 + helper 2 = **7개**, 249,147,486 bytes 실제 다운로드·SHA-256 PASS. EXE도 별도 실제 다운로드·SHA-256 PASS |

새 `device_status` 검사는 실제 격리 HTTP·PostgreSQL에서 실행됐다. 로그인 없는 요청 401, CLI bearer 403, 승인된 다른 사용자 404, `no-store`, 비밀·인증 코드 없는 정확한 5개 공개 필드를 검사했다. 같은 조건을 배포한 실제 서버에서도 다시 확인했다. 자연스럽게 5분이 지난 요청은 `expired`, `expiresIn=0`으로 관찰했다. [인증 증거](releases/evidence/cli-0.6.3-linux/rust-auth-coverage.json)를 보존한다.

## 서버 반영과 Todo 보존

기존 서버 바이너리·UI·두 index·설정의 사본과 해시를 먼저 보존했다. 두 stage 도구로 소스 commit별 불변 디렉터리를 만들고 반복 실행의 동일성도 검사했다. 기존 systemd의 `miy-v2-serve`, `miy-v2-worker`만 중지한 뒤 CLI/installer index, 서버 바이너리, UI를 반영하고 다시 시작했다. 반영은 **2026-10-29 15:53:02–15:53:04 KST**에 수행했다. 실패 시 이전 파일로 되돌리는 절차를 준비했으며 rollback은 사용하지 않았다.

`miy --version`은 `miy 0.6.3`, `/api/health`는 `{"status":"ok"}`, `/api/v1/cli/install`은 동일한 0.6.3/CLI commit/Windows ZIP·바이너리 해시를 반환한다. Nginx·systemd unit·runtime.env·server.json의 해시는 보존됐다. 기존 앱·PostgreSQL·Keycloak 컨테이너 7개의 ID·이미지·시작 시각도 동일하다. [반영 증거](releases/evidence/cli-0.6.3-linux/deployment.json), [보존 증거](releases/evidence/cli-0.6.3-linux/preservation-after.json).

대상 앱은 [기존 Todo dev URL](https://v2.dwdcc.kr/environments/dev/apps/todo-win-20261029)이다. 이번 작업에서는 앱 빌드·배포·버전 증가를 요청하지 않았다. 새 build/deploy job ID는 없으며 아래 기존 값이 전후 동일하다.

| 대상 | 보존 값 |
| --- | --- |
| 앱 버전 / release | `0.1.2` / `871549de-041d-4fa7-8c17-b8e7b3c54e47` |
| image | `sha256:386e71c86012e6a55c579fc2482bf255fb3cbab8f83f99c0d3bb50799164a75f` |
| 앱 source commit | `f7e898161688f114edbc81941cb7592cda358b1e` |
| DB UUID | `129f8abd-62c2-40d1-9175-d73d85196940` |
| Todo UUID | `8dcfffe2-3854-4ce5-b878-2da9f915c151` |
| 제목 / completed | `네이티브 실행 후에도 남는 할 일 20261029` / `false` |

DB schema generation/fingerprint, installation generation, Todo 생성·수정 시각까지 동일하다. 다른 앱 release나 DB를 변경하지 않았고 운영 승격도 하지 않았다.

실제 화면: [데스크톱](releases/evidence/cli-0.6.3-linux/live-todo-desktop.png), [390px Todo](releases/evidence/cli-0.6.3-linux/live-todo-390px.png), [펼친 메뉴](releases/evidence/cli-0.6.3-linux/live-menu-390px.png), [만료 요청](releases/evidence/cli-0.6.3-linux/live-expired-request-390px.png), [이미 승인된 요청](releases/evidence/cli-0.6.3-linux/live-consumed-request-390px.png), [OAuth 실패 복구](releases/evidence/cli-0.6.3-linux/live-oauth-recovery-390px.png), [설치 안내](releases/evidence/cli-0.6.3-linux/live-install-guide-390px.png). 내장 Browser가 연결되지 않아 Linux Playwright Chromium을 사용했다. 입력한 인증 코드나 cookie를 화면·로그로 보존하지 않았다.

## 실패, 수정, 정리와 남은 범위

첫 Docker 회귀는 PASS 22개 후 외부 SIGTERM으로 종료됐다(실행 세션 143). 원인은 확인되지 않았고 검사 assertion 실패는 없었다. 소유권을 확인해 해당 실행 자원을 정리했다. 두 번째는 55개 모두 PASS였지만 wrapper가 auth 7개를 기대해 종료 코드 1이었다. `scripts/runtime-regression/run.py`의 기대값만 **8**로 고친 세 번째 전체 실행이 종료 코드 0으로 통과했다. Rust/API/SQL 구현은 바꾸지 않았다.

`tests/development-login.spec.ts`는 성공·실패 후 자기 개발 로그인 세션도 공식 logout API로 정리하도록 보완했다. CSRF·Origin 검사와 CLI refresh token 철회를 유지했다. 보완 후 typecheck와 실제 서버 3개 검사가 통과했다.

각 Docker 실행의 소유 자원 13개를 제거했고 기존 전체 컨테이너 23개의 ID·이미지·시작 시각을 보존했다. server-build API 컨테이너도 제거했다. 검사 CLI 설정은 비어 있고, 실제 화면 검사의 CLI 세션은 철회·개발 로그인은 logout했다. 초기 읽기 전용 baseline의 브라우저는 logout 보완 전에 닫혀 **개발 로그인 세션 1개가 정상 8시간 만료를 기다린다**. 그 cookie/token은 보존하지 않았고 다른 사용자 세션을 지우는 SQL 정리는 하지 않았다. 만료된 device 요청은 후속 정상 요청의 서버 정리 대상이다. 되돌림 파일과 합성 fixture 증거는 `/projects/miy-cli-experience-run-20261029/private`에 보존하고 Git에는 넣지 않았다.

**NOT_RUN:** 실제 IdP 정상 로그인, 정상 로그인에서 소비한 실제 OAuth callback 재방문, 사용자 Windows 11에서 새 CI 설치기 실행, 관련 없는 opt-in DB integration/shared-development/network harness 3개. 실패·누락·unknown-state 콜백의 303 고정 redirect/쿠키 삭제 검사는 실제 서버와 격리 suite에서 통과했으나 실제 IdP 콜백 재사용 성공으로 확대하지 않는다. [실제 IdP 로그인 화면](releases/evidence/cli-0.6.3-linux/live-idp-login-awaiting-user.png) 도달까지만 자동 확인했고 사용자에게 [정상 로그인](https://v2.dwdcc.kr/auth/login) 직접 확인을 요청했다. 비밀번호·code·state·콜백 URL을 로그나 답변에 공유하지 않는다.

## Windows에서 재확인

[설치 안내](https://v2.dwdcc.kr/cli/install)에서 새 `MiySetup-0.6.3-windows-x64.exe`를 받아 갱신한 다음 새 PowerShell에서 확인한다. 기존 연결·로그인 저장소·앱 소스는 유지한다.

```powershell
miy --version
miy context list
miy auth status
# 선택된 context가 v2.dwdcc.kr인지 확인하고, 필요할 때만 재로그인한다.
miy auth login
```

현재 프로젝트의 서버 빌드를 요청하는 명령은 `miy app build --remote --no-wait`이며 반환한 ID로 `miy job get <ID>`, `miy job logs <ID>`, `miy job watch <ID>`를 실행한다. Windows의 Docker/WSL/SSH는 요구하지 않는다. 이번 서버·CLI 갱신을 확인하기 위해 기존 Todo를 다시 배포하거나 앱 버전을 올릴 필요는 없다.
