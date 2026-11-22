# 범용 기능 전체 변경 검토 — 2026-11-22

범용 기능 계획을 추가하기 직전 `a00f6cc`부터 리뷰 시작 시점 `f43a232`까지 Git 경로 1,307개를 목록화했다. 앞선 리뷰의 비교 구간 `c842615..f43a232` 864개에 초기 계획·D0·P1·M1·M2·M4에서만 변경된 443개를 더해 중복을 제거했다. 같은 작업본의 문서·화이트보드 미추적 경로 237개도 포함했다. 앞선 [2차 리뷰](COMMON-FEATURES-SECOND-REVIEW-2026-11-22.md)의 P1 6건과 P2·P3는 보완 대기 상태다. 이 기록은 목록에서 빠졌던 기능과 파일군까지 범위를 넓혀 찾은 항목을 담는다.

## 범위와 적용한 검토

| 범위 | 경로 수 | 검토 방법과 실제 범위 |
| --- | ---: | --- |
| M5 이후 플랫폼 Core·웹 및 계약 | 71 | Rust 19, 플랫폼 웹 21, 계약·API 타입·SDK·템플릿 4, migration 2, 스크립트 4, 브라우저 테스트 8, 문서·검증 기록 13. M5 프레임·패널·홈 카드, M6 전환·로그아웃, M12 검색, 영향 받는 호출·알림·설정과 배포 스크립트까지 변경 대조. 재시도·응답 순서·브라우저 상태 전이를 별도 확인. |
| M5 이후 플래너 추가 변경 | 15 | 두 패널 경로와 자동 저장·편집 충돌, 일정 시간대·종일 범위, 카드·검색 쿼리, 앱 설정·라우터·문서와 기존 회귀 대조. |
| 메신저 | 209 | 독립 리뷰. 대화 생성·멤버십·메시지·읽음·알림·안전 관리·첨부·디렉터리의 Rust/DB와 화면 흐름, 라우팅·초안·입력 경합, 언어·키보드, manifest·빌드·문서와 테스트. |
| 게시판 | 181 | 독립 리뷰. 공간·권한·공지·커뮤니티·댓글·첨부·포털 카드의 Rust/DB와 화면 흐름, 이동 뒤 응답 도착 순서, manifest·빌드·문서와 테스트. |
| PMS | 388 | 독립 리뷰. 공간·공유·리스트·업무 계층·상태·댓글·복사·첨부·검색·요약·알림, 프레임 전송과 목록·보드·간트·캘린더·표·트리의 대표 실행 경로. 키보드·오류·페이지 처리·초안·언어/시간대, manifest·빌드·문서와 테스트. |
| 문서 앱 작업본 | 123 | 작성 중인 Rust 모듈, SQL·파일 회수·매니저 권한과 현재 라우터·웹 진입점 확인. 현재 실행 가능한 범위는 템플릿 화면과 `/ping`이다. |
| 화이트보드 앱 작업본 | 114 | 현재 라우터·웹 진입점, 스키마 초기 상태와 앱 설정 확인. 현재 실행 가능한 범위는 템플릿 화면과 `/ping`이다. |

앞의 표는 기존 비교 구간과 미추적 작업본이다. 초기 계획 커밋의 22개 경로는 요구사항·정본·카탈로그 예약 상태와 대조했다. D0 중간 구간 185개와 P1·M1·M2·M4 중간 구간 344개는 그때의 변경 경계를 함께 추적했다. 구간별 경로 수에는 이후 다시 바뀐 경로가 포함된다.

| 앞선 구현 | 검토 방법과 실제 범위 |
| --- | --- |
| D0 디자인 | 독립 리뷰. UI·토큰·폰트·SDK 생성물·템플릿, 프레임 내 설정 전달과 미리보기 스타일, 의존성 선언과 공용 UI 레지스트리를 비교했다. |
| P1·M2·M4 | 독립 리뷰. 개인 설정·세션과 콘솔·CSV·권한 승인, 런처·키보드 팔레트, 알림 페이지·권한·재확인과 실시간 연결의 수명 주기, 스키마·검사·운영 도구를 대조했다. |
| M1 플래너 앱 최초 포팅 | 독립 리뷰. 전체 플래너 변경 196개에 대해 일정·할 일·메모의 단위와 소유권, 달력·시간대·편집·패널 전이, 라우터·DB·manifest·빌드를 추적했다. |

앱별 검토는 서로 다른 에이전트가 맡았다. 파일 목록에서 작성 소스, 생성 사본, 빌드·설치, 문서·테스트의 위치를 추적했다. 작성 소스는 기능별 진입점과 대표 상태 전이를 읽고, 생성 사본은 원본 일치 여부를 전수 비교했다. 여섯 앱 각각에서 SDK 보호 파일 30개와 CLI 안내 12개의 해시, 공용 UI 레지스트리 56개 파일의 바이트가 일치했다. 경로별 목록과 비교 결과는 검증 폴더 `.local/evidence/common-features-full-review.vxVjQASS/`의 `whole-scope.tsv`, `scope.tsv`, `generated-byte-check.json`에 있다.

실행 검증은 앱 단위·격리 HTTP/DB·플랫폼 브라우저로 진행했다. 기능 리뷰에서 발견한 문제는 지연 응답·입력 전이·뒤로 가기 등 필요한 상황을 별도로 모의했다. 아래에서 **재현**은 실행 중 관찰한 현상, **소스**는 코드를 통해 확인했으며 해당 상황의 실행은 `NOT_RUN`임을 뜻한다.

## 추가 발견 · P2

| 영역 | 발견과 확인할 동작 | 근거 |
| --- | --- | --- |
| D0 공용 프롬프트 | 프롬프트의 Enter는 한글 입력 조합 중에도 곧바로 제출한다. PMS 이름 변경처럼 이 공용 입력에 연결된 화면에 영향을 준다. | **Chromium 재현.** [prompt-dialog-content.tsx 64행](../../packages/ui/src/lib/feedback/prompt-dialog-content.tsx) |
| 런처 관리자 동시 저장 | 두 관리자가 같은 구성을 연 뒤 한 명이 분류를 추가하고 다른 관리자가 기존 제목·순서만 고쳐 저장하면 새 분류가 지워질 수 있다. 서버가 버전 없이 목록 전체 교체를 수락한다. | **소스.** [AdminLauncher.tsx 312행](../../web/platform/src/pages/AdminLauncher.tsx) · [launcher.rs 284행](../../crates/miy/src/launcher.rs) |
| M2 알림 교체 | 재확인 중 같은 그룹의 새 알림이 도착하면 이전 알림에 대한 거절로 새 알림도 지울 수 있다. 이전 화면의 읽음 표시도 새 알림에 적용될 수 있다. | **소스.** [notifications.rs 223, 521, 573행](../../crates/miy/src/notifications.rs) |
| M2 알림 목록과 접근 확인 | 고정된 첫 페이지 50개만 표시하며 후속 커서는 사용하지 않는다. 열린 패널은 새 알림을 받으면 목록을 갱신하지 않고, 항목 클릭은 캐시한 경로를 사용한다. 서버의 조회 재확인도 설치 8개까지만 묻고 나머지 항목은 해당 검사 없이 반환한다. | **모의 Chromium 재현**에서 실제 모의 미확인 56건에 배지가 49건을 표시하고 실시간 갱신 이후 패널에는 옛 목록이 남았다. 접근 재확인 범위는 **소스**. [NotificationPanel.tsx 47, 70행](../../web/platform/src/components/notifications/NotificationPanel.tsx) · [NotificationBell.tsx 21행](../../web/platform/src/components/notifications/NotificationBell.tsx) · [notifications.rs 449, 518행](../../crates/miy/src/notifications.rs) · [코어 권한](../CORE-PERMISSIONS.md) |
| M4 재연결 | 실시간 채널 연결 직전의 토큰 갱신이 일시 실패하면 만료된 토큰으로 연결을 시도한다. 이때 코어의 4401을 프레임 세션 종료로 처리하여 예정된 갱신까지 멈춘다. 프레임은 일시 장애 뒤 회복할 기회를 잃는다. | **소스.** [프레임 런타임 main.tsx 174행](../../web/frame-runtime/src/main.tsx) · [app_frames.rs 706, 898행](../../crates/miy/src/app_frames.rs) |
| M12 검색 회복 | 앱 일부만 답하지 않은 검색에서 현재 검색어를 다시 제출해도 재조회가 일어나지 않는다. 전체 HTTP 오류에는 재시도가 있다. 앱이 복구되면 같은 검색어로 결과를 다시 가져올 수 있어야 한다. | **모의 Chromium 재현.** 같은 검색어의 두 번째 제출에도 백엔드 요청은 한 번이었다. [Search.tsx 36, 53, 87, 106행](../../web/platform/src/pages/Search.tsx) |
| 플래너 일정 시간대 | 달력은 포털 시간대를 사용하지만 일정 작성·편집 창은 브라우저 OS 로컬 시각으로 입력을 채우고 저장한다. 예를 들어 브라우저 UTC, 포털 서울에서 서울 09시 칸을 열면 00시가 들어가고 그 칸에 09시를 다시 입력하면 서울 18시로 저장된다. | **실제 날짜 변환 함수로 재현.** 전체 달력·프레임 인수는 `NOT_RUN`. [PlannerView.tsx 271, 1018행](../../apps/planner/web/src/app-modules/planner/views/PlannerView.tsx) · [planner-event-modal-model.ts 72, 127, 316행](../../apps/planner/web/src/app-modules/planner/views/planner-event-modal-model.ts) · [native-date-input.ts 21, 62행](../../apps/planner/web/src/platform/time/native-date-input.ts) |
| 플래너 일간 조회 | 화면이 날짜만 넘기면 서버는 조회 시작을 UTC 자정으로 읽는다. 서울 11월 22일 08:00–08:30 일정의 종료는 UTC 전날 23:30이므로 서울의 22일 일간 보기에서 누락될 수 있다. | **소스와 시각 경계 계산.** 해당 시간대의 화면·HTTP는 `NOT_RUN`. [planner-calendar-session.ts 230행](../../apps/planner/web/src/app-modules/planner/views/planner-calendar-session.ts) · [calendar.rs 84행](../../apps/planner/src/calendar.rs) · [planner.rs 270행](../../apps/planner/src/planner.rs) |
| 플래너 일정 동시 편집 | 두 탭에서 하나는 제목, 다른 하나는 장소만 수정해도 두 편집 창 모두 읽었던 내용을 포함하는 전체 값을 보낸다. 둘 다 저장에 성공하면서 마지막 요청이 이전 제목을 덮어쓸 수 있다. 제목만 바꿔도 시각을 분 단위로 다시 보낸다. | **소스.** [planner-event-modal-model.ts 72, 316행](../../apps/planner/web/src/app-modules/planner/views/planner-event-modal-model.ts) · [planner.rs 370행](../../apps/planner/src/planner.rs) |
| 플래너 일정·할 일 응답 경합 | 일정 저장 중 입력을 계속할 수 있고 완료 응답이 창을 닫는다. 할 일 생성과 다른 항목의 토글·삭제가 하나의 처리 상태를 덮어쓰며 먼저 보낸 생성이 다음 제목 입력을 지울 수 있다. 오래된 재조회 응답도 더 최신 목록을 덮을 수 있다. | **소스.** [PlannerEventModal.tsx 145, 252행](../../apps/planner/web/src/app-modules/planner/views/PlannerEventModal.tsx) · [PersonalWidgetHost.tsx 169, 224, 321행](../../apps/planner/web/src/platform/personal-widgets/PersonalWidgetHost.tsx) |
| 메신저 전송 중 입력 | 먼저 낸 메시지가 늦게 실패하면 그사이 쓴 다음 초안이 첫 메시지로 덮인다. 한글 조합을 확정하는 Enter도 전송을 호출한다. | **모의 화면 재현 2건.** [DMView.tsx 1900, 3188행](../../apps/messenger/web/src/app-modules/dm/views/DMView.tsx) |
| 메신저 편집·대화 이동 | 메시지 편집을 시작하면 일반 작성 초안이 편집 본문으로 바뀐다. 대화를 떠났다 돌아오면 편집 본문이 새 메시지 초안에 뜬다. 편집 저장 응답이 늦게 오면 다른 대화의 초안도 덮을 수 있다. 이동을 거절해도 대화 뒤로 가기 이력이 먼저 소진된다. | **모의 화면 재현 3건.** [DMView.tsx 722, 863, 1089, 1686, 1733행](../../apps/messenger/web/src/app-modules/dm/views/DMView.tsx) |
| 메신저 이전 메시지 | 더 불러온 과거 메시지는 이후의 제한된 최신 조회에서 빠진다. 다른 사용자가 해당 메시지를 수정·삭제하거나 관리자가 숨겨도 화면에 예전 본문이 남을 수 있다. | **숨김 후 최신 조회에 대한 모의 화면 재현.** [DMView.tsx 974행](../../apps/messenger/web/src/app-modules/dm/views/DMView.tsx) · [dm-conversation-runtime-model.ts 179행](../../apps/messenger/web/src/app-modules/dm/views/dm-conversation-runtime-model.ts) |
| 메신저 안전 관리 | 첫 안전 관리자 지정 버튼은 직접 들어간 화면에는 있지만 신규 설치의 일반 탐색에 진입점이 없다. 규칙 저장 응답은 대기 중 추가로 작성한 문구를 덮는다. 신고 사유와 안전 관리 작성 중 입력을 호스트의 이동 확인에 포함하지 않는다. | **첫 관리자·규칙 저장·신고 초안 모의 화면 재현.** [DmSafetyPage.tsx 108, 242행](../../apps/messenger/web/src/platform/admin/DmSafetyPage.tsx) · [DMView.tsx 2072행](../../apps/messenger/web/src/app-modules/dm/views/DMView.tsx) |
| 메신저 알림 원본 | 알림 재확인은 대화 참여만 확인하고 전달된 원본 ID를 사용하지 않는다. 최신 미확인 메시지를 고를 때 삭제는 걸러도 관리자 숨김은 걸러내지 않아 숨긴 메시지가 알림 원본에 남을 수 있다. | **소스.** [notify.rs 114, 209, 295, 330행](../../apps/messenger/src/notify.rs) · [M2 계약](../COMMON-FEATURES-PLAN.md#m2-플랫폼앱-연결과-알림) |
| 게시판 이동 뒤 늦은 응답 | B 게시글로 이동한 다음 먼저 요청한 A의 응답이 도착하면, B의 주소에 A 본문과 A를 대상으로 하는 편집·삭제 버튼이 표시된다. 성공·실패 처리에 현재 요청을 확인하는 절차가 필요하다. | **모의 화면 재현.** [CommunityView.tsx 381, 552, 605, 630행](../../apps/board/web/src/app-modules/community/views/CommunityView.tsx) |
| 게시판 관리자 변경 | 관리자 추가는 요청자의 자격을 디렉터리 호출 전 확인한 뒤 자동 커밋으로 추가한다. 그사이 권한 회수가 끝나도 추가 쓰기를 계속할 수 있다. 관리자 추가·삭제가 같은 잠금 아래 현재 자격을 확인해야 한다. | **소스.** [managers.rs 151, 186행](../../apps/board/src/managers.rs) |
| 게시판 댓글 편집·삭제 | 연결 이미지를 빼는 댓글 편집과 삭제가 댓글·미디어 행을 반대 순서로 잠근다. 편집의 UPDATE는 삭제 상태를 재확인하지 않아 먼저 삭제된 행에 본문을 다시 쓸 수도 있다. | **소스.** [community.rs 1517, 1572행](../../apps/board/src/community.rs) |
| 게시판 첨부 재시도 | 업로드 재시도마다 새 요청 ID를 쓰고, 서버가 제공하는 동일 ID의 예약·PUT 재개를 사용하지 않는다. 응답을 잃었을 때 확정 여부를 복구하거나 남은 파일을 정리하는 경로가 필요하다. | **소스.** [use-media-upload.ts 26행](../../apps/board/web/src/platform/media/use-media-upload.ts) · [media.rs 323, 398행](../../apps/board/src/media.rs) |
| PMS 이동·상세 닫기 | 목록의 인라인 업무·하위 업무 입력이 호스트에 미저장 상태를 알리지 않는다. 보조 패널의 업무 상세에서 X나 배경을 누르면 로컬 상태가 바로 해제되어 작성 중 댓글·체크리스트·하위 업무가 사라질 수 있다. | **소스.** [ListView.tsx 815행](../../apps/pms/web/src/app-modules/pms/views/ListView.tsx) · [FloatingPmsWidget.tsx 351행](../../apps/pms/web/src/app-modules/pms/views/FloatingPmsWidget.tsx) · [TaskDetail.tsx 249행](../../apps/pms/web/src/app-modules/pms/views/TaskDetail.tsx) |
| PMS 댓글 제출 | 느린 응답 중 다음 댓글을 입력하면 먼저 보낸 댓글의 성공 처리에서 새 초안과 멘션을 지운다. 응답 전 반복 제출도 매번 요청을 보낸다. | **소스.** [useTaskDetailComments.ts 172행](../../apps/pms/web/src/app-modules/pms/views/useTaskDetailComments.ts) · [TaskDetailActivityPanel.tsx 277행](../../apps/pms/web/src/app-modules/pms/views/TaskDetailActivityPanel.tsx) |
| PMS 조회 실패 | 오늘·지연 및 공간 개요에서 조회 오류가 빈 결과나 이전 데이터와 구별되지 않는다. 상세에서는 변경 이력 요청 하나가 실패해도 다른 정상 상세 데이터를 표시하지 못한다. | **소스.** [TodayOverdueView.tsx 75행](../../apps/pms/web/src/app-modules/pms/views/TodayOverdueView.tsx) · [SpaceOverviewView.tsx 206행](../../apps/pms/web/src/app-modules/pms/views/SpaceOverviewView.tsx) · [useTaskDetailResources.ts 40행](../../apps/pms/web/src/app-modules/pms/views/useTaskDetailResources.ts) |
| PMS 목록 페이지 | 화면이 마일스톤 20개·라벨 100개·활동 이력 50개에서 멈춘다. 서버는 후속 페이지를 지원한다. 정렬상 101번째 라벨은 정상 생성한 뒤 다시 열면 설정이나 선택기에서 찾을 수 없다. | **소스.** [pms-api.ts 685, 1049, 1126행](../../apps/pms/web/src/app-modules/pms/api/pms-api.ts) · [lists.rs 844행](../../apps/pms/src/lists.rs) |

## 추가 발견 · P3

| 영역 | 발견 | 근거 |
| --- | --- | --- |
| D0 미리보기 스타일 차이 | 미리보기의 기본 링크색과 폼 글꼴 선언이 Tailwind 유틸리티를 덮는다. 설치 프레임에서 같은 기본값은 base 레이어에 있어 유틸리티가 우선한다. | **Chromium·Firefox·WebKit의 CSS 예제 재현.** [preview.css 26행](../../templates/rust-react/sdk/preview/preview.css) · [frame.css 15, 35행](../../web/frame-runtime/src/frame.css) |
| 공용 대화상자 접근성 | 닫기 레이블을 넘기지 않으면 공용 대화상자의 X 버튼에 접근 가능한 이름이 없다. DetailDrawer도 같은 표식을 쓴다. | **대화상자는 Chromium 접근성 트리에서 재현.** [dialog.tsx 135행](../../packages/ui/src/lib/primitives/dialog.tsx) · [detail-drawer.tsx](../../packages/ui/src/lib/layout/detail-drawer.tsx) |
| 공용 확인 응답 | 확인창을 연 뷰가 해제되면 대기 중인 확인 Promise가 끝나지 않는다. 프롬프트에도 뷰 해제 시 완료 처리가 빠졌다. | **확인창은 Chromium 재현.** 프롬프트는 **소스**. [confirm-dialog.tsx 106행](../../packages/ui/src/lib/feedback/confirm-dialog.tsx) · [prompt-dialog.tsx](../../packages/ui/src/lib/feedback/prompt-dialog.tsx) |
| M6 새 탭 안내 | 첫 빈 탭을 열지 못하고 API 완료 뒤 다시 여는 분기가 `window.open(..., "noopener")`의 반환값만으로 차단을 판정한다. 실제로 탭이 열려도 차단됐다고 보고할 수 있다. | **Chromium·Firefox·WebKit에서 API 동작 검증.** 새 탭이 열리면서 반환값은 세 브라우저 모두 `null`이었다. 전환 UI의 해당 분기 인수는 `NOT_RUN`. [AdminUsers.tsx 318행](../../web/platform/src/pages/AdminUsers.tsx) |
| 패널 고정과 사용자 설정 | 패널 고정과 설정·런처 저장의 응답이 뒤바뀌면 세션의 전체 설정을 오래된 응답으로 덮어쓸 수 있다. 저장은 반영되어도 화면에서 고정이나 다른 설정이 재조회할 때까지 돌아간 것처럼 보인다. | **모의 Chromium 재현.** 서버 이름은 새 값을 유지했고 헤더만 이전 이름으로 돌아갔다. [Shell.tsx 292행](../../web/platform/src/layout/Shell.tsx) · [session.tsx 180행](../../web/platform/src/data/session.tsx) |
| 메신저 미구현 반응 | 포팅에서 미룬 반응 기능이 초점을 받을 수 있는 버튼으로 노출되며 눌러도 실행하지 않는다. | **소스.** [DMView.tsx 2703행](../../apps/messenger/web/src/app-modules/dm/views/DMView.tsx) |
| 플래너 날짜 표시 | 공휴일 레이블과 주간 일정·혼잡한 날의 날짜 계산에 브라우저 시간대가 남아 있어 포털 시간대와 다른 날을 가리킬 수 있다. | **소스.** [unified-calendar-adapter.ts 146행](../../apps/planner/web/src/components/calendar/unified-calendar-adapter.ts) · [planner-month-insights-model.ts 38, 121, 160행](../../apps/planner/web/src/app-modules/planner/views/planner-month-insights-model.ts) |
| 게시판 공지 | 공지 8개 이후로 이동할 목록 UI가 없다. 공지 요청 실패가 빈 목록으로, 직접 링크의 상세 요청 실패가 무반응으로 보인다. | **소스.** [AnnouncementsBoard.tsx 358, 385, 415행](../../apps/board/web/src/app-modules/announcements/views/AnnouncementsBoard.tsx) |
| 게시판 채널 목록 | 채널을 추가·이름 변경·삭제해도 같은 앱 프레임의 사이드바는 이미 읽어 둔 항목을 계속 보여 준다. 변경 이벤트는 캐시를 비우지만 사이드바의 조회를 다시 실행하지 않는다. | **소스.** [sidebar.tsx 36행](../../apps/board/web/src/app-modules/community/sidebar.tsx) |
| PMS 시간대 | 보조 패널의 마감 색은 UTC 날짜를 쓴다. 포털 설정이 서울이면 자정 전후에 오늘·지연 화면과 색상의 판정 날짜가 달라질 수 있다. | **소스.** [FloatingPmsWidget.tsx 59행](../../apps/pms/web/src/app-modules/pms/views/FloatingPmsWidget.tsx) · [TodayOverdueView.tsx 61행](../../apps/pms/web/src/app-modules/pms/views/TodayOverdueView.tsx) |
| PMS 조작·접근성 | 보드 열 추가·더보기와 상세 공유·더보기 표시가 동작하지 않는다. 댓글 전송의 접근 가능한 이름과 상세 오버레이의 대화상자 역할·초점 이동·Escape 닫기가 빠져 있다. 간트의 막대 조작은 마우스 중심이며 날짜는 업무 상세에서 직접 수정할 수 있다. | **소스.** [BoardView.tsx 84행](../../apps/pms/web/src/app-modules/pms/views/BoardView.tsx) · [TaskDetail.tsx 365행](../../apps/pms/web/src/app-modules/pms/views/TaskDetail.tsx) · [TaskDetailActivityPanel.tsx 328행](../../apps/pms/web/src/app-modules/pms/views/TaskDetailActivityPanel.tsx) · [TaskDetailModal.tsx 20행](../../apps/pms/web/src/app-modules/pms/views/TaskDetailModal.tsx) · [GanttView.tsx 1402행](../../apps/pms/web/src/app-modules/pms/views/GanttView.tsx) |

## 포팅·인수 진행 상태

- **문서 M9 작업본.** 삭제할 공간의 페이지 첨부를 먼저 `detaching`으로 기록하지만 공간과 페이지 삭제가 FK를 따라 그 행을 함께 지운다. 커밋 뒤 첫 Storage 회수가 실패하면 재시도에서 볼 행이 없다. 라우터를 연결할 때 회수 기록을 공간 삭제와 독립적으로 보존해야 한다. 매니저 추가 권한도 디렉터리 호출 뒤 다시 확인하는 흐름이 필요하다. 미추적 작업본의 `apps/docs/src/spaces.rs:501`, `src/files.rs:77,150`, `migrations/0003_docs.sql:99`, `src/managers.rs:152`.
- **화이트보드 M11 작업본.** 현재 Rust 라우터와 웹 루트에서 편집·공유·휴지통·내보내기·PMS 연결을 인수할 수 없다. 실제 프레임의 Excalidraw 동작도 `NOT_RUN`이다. 미추적 작업본의 `apps/whiteboard/src/main.rs`와 `web/src/AppRoot.tsx`. [M7~M11 계획](../COMMON-FEATURES-PLAN.md#m7m11-공식-업무-앱)
- **M4 메신저 연결.** 즉시 갱신과 입력 중 표시는 M4 계획에 있고 앱은 여전히 주기 조회를 한다. 메신저 포팅 문서의 “M6이 없다”는 설명은 현재 전환 권한 계약과 대조해 갱신해야 한다. [M4 계획](../COMMON-FEATURES-PLAN.md#m4-업무-실시간-전송) · [메신저 포팅](../../apps/messenger/docs/PORTING.md) · [권한 계약](../CORE-PERMISSIONS.md)
- **M2와 앱 공급.** 런처의 앱별 알림 배지와 브라우저 OS 알림 진입점이 남아 있다. 번들 카탈로그의 플래너·메신저·게시판·PMS 예약 ID에는 공식 릴리스가 없다. [M2 계획](../COMMON-FEATURES-PLAN.md#m2-플랫폼앱-연결과-알림) · [공식 앱 카탈로그](../../registry/apps/catalog.json)
- **M10·M13.** 이번 변경의 M10은 계획·설계 범위다. M13과 함께 소스 구현 및 실제 기능 인수는 `NOT_RUN`이다. [범용 기능 계획](../COMMON-FEATURES-PLAN.md#3-단계별-구현-계획)

## 실행 기록

| 명령과 실행 범위 | 결과 |
| --- | --- |
| `bash scripts/check-static.sh rust`, 빈 CLI 설정 디렉터리에서 `bash scripts/check-static.sh scripts` | `PASS`. 정적·Rust·CLI 단위 검사, 문서·계약 생성, 격리 프록시. |
| `bash scripts/check-static.sh web` | `PASS`. 공용 UI 78건, 모의 포털 110건, CLI 웹 15건, 모의 출처·세션 경계 18건, HTTP IPv4 1건과 격리 빌드. |
| `pnpm exec playwright test -c scripts/runtime-regression/ui.config.ts`, `pnpm exec playwright test -c tests/surface-boundaries.config.ts` | `PASS`. 모의 API 포털 110건과 출처·세션 경계 18건. |
| 빈 CLI 설정과 이 리뷰 전용 증거 경로에서 `python3 scripts/runtime-regression/run.py` | `PASS`. 격리 Core·Docker 회귀 158건. 검사와 소유 자원 정리의 종료 코드가 모두 0이었다. 실행 ID는 `miy-runtime-regression-_e0btw48`. |
| `node scripts/check-token-copies.mjs`, `node scripts/build-ui-component.mjs --check`, `node scripts/api-types.mjs --check` | `PASS`. 토큰 사본, 공용 UI 레지스트리 56개, 생성 API 타입. |
| `node scripts/check-i18n.mjs`, `node scripts/check-token-contrast.mjs`, `node scripts/check-hardcoded-colors.mjs` | `PASS`. 관리되는 화면의 한·영 구조, 두 모드의 대비 42쌍, 유지관리 소스 86개의 색 값. |
| 플래너에서 `pnpm typecheck && pnpm test`, 일회용 PostgreSQL 18에서 `cargo test --locked --offline -- --include-ignored --test-threads=1` | `PASS`. 웹 132건, Rust/HTTP 48건. |
| 메신저에서 `pnpm typecheck`와 `pnpm test`, 일회용 PostgreSQL 18에서 `cargo test --locked --offline -- --include-ignored --test-threads=1` | `PASS`. 웹 187건, Rust/HTTP 41건. |
| 게시판에서 `pnpm typecheck`, `pnpm test`, `cargo test --locked --offline --quiet`와 일회용 PostgreSQL 18의 `cargo test --locked --offline --quiet -- --ignored --test-threads=1` | `PASS`. 웹 66건, Rust 단위 20건과 HTTP/DB 39건. |
| PMS에서 `pnpm exec tsc --noEmit`와 `pnpm test`, 임시 `--outDir` 출력 Vite 빌드, 현재 저장소 CLI의 `miy app check`, 일회용 PostgreSQL 18에서 `cargo test --locked --offline -- --include-ignored --test-threads=1` | `PASS`. 웹 523건, 앱 검사 11개, Rust/HTTP 52건. jsdom 탐색 경고와 번들 크기 경고가 있었다. 앱 검사에는 첨부 장애 복구 인수가 별도로 남는다. |
| 게시판·메신저·플래너에서 현재 저장소 CLI로 `miy app check`, 게시판·메신저에서 임시 `--outDir`로 `pnpm exec vite build` | `PASS`. 앱마다 11개 검사. 메신저 앱 로컬 `.miy-tools/miy app check`는 `invalid_manifest`로 `FAIL`했다. 저장소 CLI로 재실행해 통과했다. 두 파일은 같은 0.17.0 버전을 표시했다. |
| 작업 복사본에서 메신저 지연 응답·IME·이력·안전 관리, 게시판 지연 응답 Vitest | 메신저 9개 문제 상황과 게시판 1개 문제 상황을 재현했다. 게시판은 사용자 기대를 단언한 검사 1건이 `FAIL`했다. 처음 사용한 게시판 검색 폼은 로딩 중 화면에서 내려가 재현 경로를 게시글 이동으로 바꿨다. 메신저 탐색 검사는 대상 대화 구분을 수정한 뒤 재현했다. |
| 임시 `node --input-type=module`의 Playwright 진단 | Chromium에서 통합 검색 재시도·설정 응답 순서·알림 목록 갱신, 임시 Vite에서 공용 프롬프트·확인·닫기 이름 문제를 관찰했다. 알림 진단은 첫 버튼 선택자가 넓어 시간 초과로 `FAIL`한 뒤 단일 항목으로 좁혀 재현했다. 브라우저 3종에서 미리보기 스타일과 프레임 스타일이 달랐다. 모두 격리 화면 및 모의 HTTP 검사다. |
| `TZ=UTC node --input-type=module`에서 Vite `transformWithOxc`로 플래너 날짜 함수 호출 | 포털·브라우저 시간대가 다른 경우 시각이 변하는 동작을 관찰했다. 조회의 UTC 시작 경계를 별도로 계산했다. 첫 설정의 Node TypeScript 직접 실행과 이전 TypeScript 변환 API 호출은 `FAIL`했고 설치된 Vite 컴파일러로 재실행했다. |
| 임시 Node 스크립트의 Playwright `window.open` 검사 | 첫 실행 `FAIL` (`playwright` 직접 import 경로). 저장소의 `@playwright/test`로 다시 실행해 세 브라우저에서 새 탭 열림과 `null` 반환을 함께 관찰했다. |
| `git diff --check c842615..f43a232`, `git diff --check a00f6cc..f43a232` | 두 번 모두 `FAIL`. 생성 CLI 안내는 첫 구간에서 9개·7,749곳, 전체 구간에서 플래너까지 12개·10,332곳에 정렬용 끝 공백이 있다. 이 생성 경로를 제외한 두 구간의 재검사는 `PASS`이며, 생성 파일은 원본 해시와 일치했다. |

실제 앱을 설치한 프레임의 사용자 시나리오·CSP·Storage, 화면낭독과 실제 모바일 기기, 보조 프레임 메모리 재측정, 작업 중 두 앱의 제품 인수는 `NOT_RUN`이다. 본 검토에서 제품 소스는 수정하지 않았다. 커밋된 세 업무 앱과 진행 중인 두 앱의 범위를 [추진 계획](../PLAN.md)에 기록한다. 서버 반영, 운영 migration, 공식 릴리스 게시와 push는 하지 않았다.
