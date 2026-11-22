# 범용 기능 2차 리뷰 — 2026-11-22

검토 기준은 `164ed9d`, 변경 비교 기준은 `c842615`다. 863개 변경 파일 중 생성된 안내·SDK 사본은 계약 및 보호 파일 일치 여부로 확인했다. 플랫폼·앱 구현은 아래 경로와 사용자 흐름을 검토했다. 앞선 [격리 검증 결과](../releases/common-features-review-followup-2026-11-22-validation.json)의 PASS는 해당 검사에 그대로 적용한다.

## 검토 단계와 방법

| 단계 | 적용한 검증 방법 | 이번에 확인한 범위 |
| --- | --- | --- |
| 1. 제품 계약 대조 | V 모델의 요구–인수 추적 | M5 초안 보호와 세션 한도, M6 발급·회수, M12 제공 앱 권한, 업무 앱의 공식 설치, 반영 도구의 schema와 CLI 정합성 |
| 2. 분야별 독립 리뷰 | 권한 결정표, 상태 전이, 데이터 불변조건 | 게시판의 공개·비밀·익명·관리자 역할, 메신저의 가입·퇴장·읽음·신고, PMS의 편집권·업무 트리·상태, Core의 세션·프레임·갱신 |
| 3. 실행 검증 | 경계값·동등 분할, 경합 순서 제어, 실패 주입 | 브라우저 3종에서 너비·최대 고정 수와 탭 상태, 일회용 PostgreSQL에서 일반 사용자의 PMS 쓰기, 응답 유실 모의 |
| 4. 교차 검증 | UI → API → DB와 문서·배포물 비교 | 모의 환경 관찰의 적용 범위, 응답 전에 이미 커밋한 변경, 생성 API의 실제 경로, schema 단계와 같은 버전의 서로 다른 도구 |

**재현**은 테스트에서 관찰한 동작이다. **소스**는 실행 경로와 저장 조건을 확인한 것이다. 권한 경계와 강제 경합은 해당 행에 실행 여부를 표시한다.

## P1 · 권한 침해 또는 데이터 유실

| 항목 | 발견과 보완 조건 | 근거 |
| --- | --- | --- |
| PMS 워크플로 모드 | 이미 사용자 정의인 리스트에 같은 모드 요청을 다시 보내면 상태 정의 전체를 기본값으로 교체한다. 기존 업무는 사라진 상태 키를 계속 참조한다. 현재 모드와 같은 요청은 정의와 참조를 보존해야 한다. | **PG 재현.** 상태를 복사할 때 삭제하는 [statuses.rs 130, 240행](../../apps/pms/src/statuses.rs) |
| PMS 업무 계층 | 하위 업무를 다른 상위 업무로 이동하는 요청은 200으로 끝난다. 이전 상위 업무의 동시 삭제가 먼저 읽은 하위 목록을 사용해 204 후 이동한 업무까지 지운다. 이동과 삭제는 같은 계층 잠금 아래 최신 행을 확인해야 한다. | **PG 재현.** 이동·삭제의 [tasks.rs 1367, 2145행](../../apps/pms/src/tasks.rs) |
| PMS 첨부 응답 유실 | 파일이 이미 저장된 뒤 업로드 응답을 잃으면 UI가 취소 요청으로 해당 파일을 삭제한다. 요청 ID와 상태 조회로 확정 여부를 복구하고, 삭제는 사용자가 명시적으로 요청할 때만 진행해야 한다. | **UI 모의 장애 재현.** [pms-api.ts 1195행](../../apps/pms/web/src/app-modules/pms/api/pms-api.ts) · [파일 계약](../../apps/pms/docs/CONTRACT.md) |
| PMS 쓰기 권한 | 업무 생성·재정렬·일괄 변경은 역할을 읽은 뒤 업무나 리스트 잠금을 기다린다. 잠금 뒤 현재 역할을 확인하는 앞선 보완이 이 진입점들에 빠져 있다. | **소스.** [tasks.rs 1029, 1654, 1872행](../../apps/pms/src/tasks.rs) |
| 게시판 검색 권한 | 잠긴 비밀글의 실제 제목이 검색 포함 여부와 총건수에 반영된다. 일부 비공개 채널에서는 출력에서 가린 작성자도 검색 판정에 남는다. 필드 조회 권한을 결과와 건수 양쪽의 조건에 적용해야 한다. | **소스.** 출력 마스킹과 검색의 [community.rs 512, 1025, 1059, 1078행](../../apps/board/src/community.rs) · [기존 테스트 기대값](../../apps/board/src/api_tests.rs) |
| 메신저 참여자 회수 | 메시지 전송은 참여자 권한을 읽고 대화 잠금을 얻은 뒤 현재 멤버십을 확인하지 않는다. 요청은 쓰기를 커밋한 뒤 현재 참여자 판정으로 404가 될 수 있다. 메시지 수정·삭제에도 멤버십 변경과 공통 잠금이 필요하다. | **소스.** [messages.rs 161, 184, 226, 252행](../../apps/messenger/src/messages.rs) · [참여자 변경](../../apps/messenger/src/conversations.rs) |

## 플랫폼과 브라우저

| 우선순위 | 발견 | 근거 |
| --- | --- | --- |
| P2 | 사용자 전환 뒤 기존 포털 탭이 이전 사용자 이름과 CSRF를 표시한다. 새 탭은 전환한 사용자로 바뀌지만, 기존 탭의 폴링에는 새 세션의 응답이 섞인다. 계정 변경 시 같은 출처 탭의 데이터와 세션을 함께 갱신해야 한다. | **격리 Core·PostgreSQL과 Chromium에서 이름 불일치 재현.** 브라우저 3종의 모의 검사는 폴링 결과 혼합도 관찰했다. [SwitchUser.tsx 25행](../../web/platform/src/pages/SwitchUser.tsx) · [session.tsx 140행](../../web/platform/src/data/session.tsx) |
| P2 | 허용된 패널 고정 16개에서 1024~1920px 화면까지 패널 메뉴와 로그아웃 버튼이 화면 밖으로 밀린다. 1023px에서는 바로가기 숨김이 적용돼 다시 들어온다. 가로 예산을 정해 메뉴와 계정 조작 영역을 우선 보존해야 한다. | **브라우저 3종.** [PanelMenu.tsx 52행](../../web/platform/src/components/panels/PanelMenu.tsx) · [panels.css 84, 113행](../../web/platform/src/styles/panels.css) |
| P2 | 문서 새로고침에서 프레임을 새로 열면서 기존 슬롯을 반환하지 않는다. SPA의 내부 이동에서는 반환을 확인했다. 반복 새로고침으로 남은 슬롯이 쌓이면 아직 열린 다른 탭에서 가장 오래된 프레임이 먼저 회수된다. | **브라우저 3종에서 반환 요청 부재 관찰.** 쿼터 영향은 [app_frames.rs 1397행](../../crates/miy/src/app_frames.rs) · [AppFrame.tsx 89행](../../web/platform/src/app/AppFrame.tsx)에서 확인 |
| P2 | 비밀번호 재설정 전에 발급된 사용자 전환 코드가 재설정 완료 후에도 남을 수 있다. 일반 로그인은 시작 시각 하한을 따르고, 전환은 재설정 진행 상태만 확인한다. 이미 발급된 앱 후속 호출 증표와 열린 포털 알림 채널에도 재설정 진행 조건이 빠져 있다. | **소스.** [accounts.rs 602행](../../crates/miy/src/accounts.rs) · [user_switch.rs 200행](../../crates/miy/src/user_switch.rs) · [app_calls.rs 183행](../../crates/miy/src/app_calls.rs) · [notifications.rs 729행](../../crates/miy/src/notifications.rs) |
| P2 | 프레임 토큰 갱신 교환과 계정 중지·재설정이 반대 순서로 사용자와 프레임 행을 잠근다. 드물게 계정 중지 또는 회수가 교착으로 롤백될 수 있다. | **소스.** [app_frames.rs 1561행](../../crates/miy/src/app_frames.rs) · [http.rs 879행](../../crates/miy/src/http.rs) |
| P2 | M4의 프레임 WebSocket 중계는 전송이 막혀 있으면 권한 재확인과 유휴 종료 분기를 진행하지 못한다. M6 전환 세션 회수에도 영향을 줄 수 있다. | **소스.** [live.rs 110, 145행](../../crates/miy/src/live.rs) · [시간 계약](../APP-BOUNDARIES.md) |
| P3 | 앱이 정지되어 iframe이 생성되지 않은 화면에서도 홈 이동 시 미저장 상태를 확인한다. | **브라우저 3종.** [AppView.tsx 41, 138행](../../web/platform/src/app/AppView.tsx) |
| P3 | 중복 로그아웃이나 회수 경합에서 전환 세션 행이 먼저 없어지면 일반 IdP 로그아웃으로 갈 수 있다. | **소스.** [auth.rs 578행](../../crates/miy/src/auth.rs) |

## 앱 데이터와 회복

| 우선순위 | 발견 | 근거 |
| --- | --- | --- |
| P2 | PMS 리스트 이름과 보관 여부를 동시에 변경하면 두 응답이 성공해도 나중 쓰기가 이전 이름을 되살린다. 일부 정상 등록된 상태는 정의에서 slug 40자를 허용하지만 업무 열은 24자라 적용 시 500이 난다. | **PG 재현 2건.** [lists.rs 429, 467행](../../apps/pms/src/lists.rs) · [workflow.rs 117행](../../apps/pms/src/workflow.rs) · [업무 스키마 161행](../../apps/pms/migrations/0002_pms.sql) |
| P2 | 게시판 공지의 수정은 이전 행 전체를 바탕으로 갱신해 동시 부분 변경을 덮을 수 있다. 공간 공지 쓰기는 ACL을 별도 조회로 먼저 확인한다. 최근 공지와 알림 재확인에도 이전 공간 권한을 읽은 뒤 데이터를 읽는 경합이 남았다. | **소스.** [announcements.rs 134, 440, 607행](../../apps/board/src/announcements.rs) · [notify.rs 273행](../../apps/board/src/notify.rs) |
| P2 | 게시판 첨부 연결 확정이 실패하면 같은 글을 저장하거나 열어도 남은 파일을 다시 확정하지 않는다. 늦은 성공 응답이 동시에 지우는 중인 파일 상태를 덮을 수 있다. | **소스.** [media.rs 109, 139행](../../apps/board/src/media.rs) |
| P2 | 메신저 재가입 후 방장이 된 사람이 다시 나가면 과거 참여 역할을 읽어 소유자 승계가 빠질 수 있다. | **소스.** [conversations.rs 460행](../../apps/messenger/src/conversations.rs) · [store.rs 180행](../../apps/messenger/src/store.rs) |
| P2 | 메신저 업로드 응답 유실 후 재시도는 Core의 `Ready`를 읽고도 앱의 `reserved`를 바꾸지 않는다. 첨부 확정 응답을 잃고 메시지를 지우는 경우에는 이미 연결된 파일에 취소만 반복할 수 있다. | **소스.** [attachments.rs 298, 445행](../../apps/messenger/src/attachments.rs) · [messages.rs 128행](../../apps/messenger/src/messages.rs) |
| P2 | 메신저 동시 읽음 요청은 커서를 뒤로 돌릴 수 있다. 이전 페이지는 순번으로 정렬하면서 시각만으로 자르므로 동일 시각의 메시지가 누락될 수 있다. | **소스.** [conversations.rs 564행](../../apps/messenger/src/conversations.rs) · [messages.rs 47행](../../apps/messenger/src/messages.rs) |
| P2 | 메시지 신고와 삭제가 겹칠 때 두 경로가 하나의 잠금 아래 상태를 확인하지 않아 신고의 증거 사본을 잃을 수 있다. | **소스.** [safety.rs 457행](../../apps/messenger/src/safety.rs) · [messages.rs 384행](../../apps/messenger/src/messages.rs) |

## API 계약과 인수 범위

- **P2 · CLI 버전과 실제 계약의 단계별 불일치.** schema 28–32의 소스는 같은 CLI 0.17.0과 번들 0.12.0을 표시하며 manifest와 보호 파일은 달라진다. 반영 도구는 한 단계의 고정 migration만 허용하며 같은 버전이면 기존 게시 CLI를 유지할 수 있다. 중간 후보를 게시했을 때 이후 manifest를 동일 버전 CLI가 거부하는지 확인해야 한다. 이전 로컬 0.17.0의 manifest 거부는 앞선 PMS 검증에 기록돼 있다. 네 앱의 현재 보호 파일과 고정 migration 27–32의 해시는 일치했다. [반영 도구](../../scripts/core-rollout.py) · [번들 검사](../../crates/miy/src/bundle.rs) · [업그레이드 분기](../../crates/miy/src/upgrade.rs)
- **P3 · 생성 API 기술서의 오기.** 격리 Core의 실제 OpenAPI에 검색 주소 전체가 경로 키로 들어 있었고 정규 경로와 필수 검색 매개변수가 없었다. 사용자 전환의 콘솔 세션 전용 제약과 프레임 종료 경로도 카탈로그·기술서에 충분히 표현되지 않는다. [domain.rs 530, 547행](../../crates/miy/src/domain.rs) · [http.rs 1447행](../../crates/miy/src/http.rs)
- **P3 · 실행 문서의 기준 차이.** 설치·반영 문서의 schema 표기와 회귀 문서의 149건 기대값이 각각 후보 schema 32와 러너의 158건과 다르다. 운영 반영은 이 리뷰의 범위 밖에 있다. [설치 안내](../SETUP.md) · [반영 안내](../SERVER-OPERATIONS.md) · [회귀 러너](../../scripts/runtime-regression/run.py)

| 기능 | 지난 검증 및 이번 검증 | 남은 인수 |
| --- | --- | --- |
| M5 | 모의 앱 브라우저와 실제 Core HTTP. 이번에 브라우저 3종에서 한도·새로고침·정지 화면을 검사했다. | 설치된 두 업무 앱 프레임을 동시에 사용하고 메모리를 재측정한다. |
| M6 | 실제 Core 발급·회수 API와 새로 추가한 격리 Core 2탭 시나리오에서 기존 탭 표시 불일치를 확인했다. | 권한 회수와 열린 프레임 갱신·비밀번호 재설정·느린 채널의 교차 경합 인수. |
| M12 | 모의 브라우저와 실제 Core의 합성 제공자 검사. 현재 플래너·PMS에 제공자 소스가 있다. | 두 앱을 실제로 설치한 통합 검색과 권한 회수 인수. |
| 앱 출시와 M10 | 네 앱의 공식 카탈로그에 출시 항목이 비어 있다. 문서·화이트보드 작업본의 메인 라우트와 UI는 초기 연결 상태다. M10은 설계다. | 실제 앱 설치·프레임 인수, M9/M11 포팅, M10 분할 복구와 전송 권한 구현. |

## 검증 기록

- **결함 관찰:** 변경 소스로 격리 Vite 빌드를 만든 뒤 `pnpm exec playwright test -c <검토>/playwright.config.ts` 및 `cross-browser.config.ts`를 실행했다. Chromium·Firefox·WebKit 각각 네 시나리오에서 같은 결함을 관찰했다. 테스트는 현재 동작을 확인하는 진단이므로 성공 종료는 기능 인수를 뜻하지 않는다.
- **격리 Core:** `cargo build --locked --offline -p miy`와 `pnpm exec playwright test -c <검토>/real-core.config.ts`를 실행했다. 기존 서버 UI 8건은 PASS였다. 추가 2탭 검사 첫 실행은 사용자 선택자 시간 초과로 FAIL이었다. 정확한 사용자 이름으로 고쳐 `--grep 'real Core user switch'`를 재실행했고, 기존 탭 이름과 실제 서버 세션이 다름을 관찰했다. 기술서 진단은 첫 포털 요청에서 403으로 실패했다. 콘솔 출처로 수정한 최종 실행에서 기존 8건과 두 진단이 끝났으며, 기술서의 경로·쿼리 문제도 관찰했다.
- **FAIL, 불변조건 재현:** `cargo test --offline --locked review_workflow_ -- --ignored --nocapture --test-threads=1`, 같은 옵션의 `review_concurrency_`와 `review_hierarchy_`를 실행했다. 일회용 Unix 소켓 PostgreSQL 18에서 정상 사용자의 HTTP 시나리오 4건이 불변조건을 위반했다. PMS Vitest의 합성 응답 유실 1건도 데이터 보존 조건을 위반했다. DB는 종료했다.
- **PASS:** 메신저 `cargo test --locked --offline`의 DB 미사용 30건, `node scripts/api-types.mjs --check`, `python3 scripts/check-doc-links.py`, 추적 소스의 `git diff --check`. 메신저 DB 필요 11건은 이번 실행에서 `NOT_RUN`이다.
- **NOT_RUN:** 실제 설치된 플래너·PMS 검색, 공식 앱 프레임·메모리 재측정, 권한 경합·네트워크 단절의 실제 재현, 서버 반영. 이전 검증 범위를 확대하지 않았다.
- 진단 테스트와 로그는 `.local/evidence/common-features-re-review.ejl2hPm8/`에 있다. PMS 테스트는 `pms-diagnostic-tests.patch`로 보존했다. 사례에 사용한 데이터는 격리 환경의 합성 데이터다.
