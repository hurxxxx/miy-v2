# AI-DO 플래너 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 공식 앱 `planner`(이름 `플래너`), 번들 0.12.0, Rust SDK 0.4.0, 공용 UI 2.0.0
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차"와 "M1 플래너", §2.2 플래너 행
- 원칙: AI-DO 구현을 그대로 옮기고, miy의 경계(인증·저장·앱 프레임·앱별 DB) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 개인 일정 CRUD(종일, 시작·종료 시각을 선택적으로 두는 일정 포함), 월·주·일·아젠다 보기, 드래그 이동·길이 변경, 일정 검색, 미니 달력·날짜 선택, 일정 필터, 이번 주 요약·겹치는 일정 많은 날·다가오는 주요 일정 패널, 공휴일 표시, `?event=` 바로 열기, 개인 할 일(추가·완료·제목 수정·삭제)과 개인 메모(800ms 자동 저장), 오른쪽 도크와 떠 있는 패널(패널·전체 화면).

**뺀 것(첫 출시 제외, 화면·API 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 타임라인 보기 | `PlannerTimelineView.tsx`, `planner-timeline-model.ts`, 헤더의 캘린더/타임라인 전환 | 계획 §2.2에서 제외 |
| 통합 캘린더(회의·PMS 일정) | `domains/calendar`의 Meeting·PMS 소스, `MeetingPreviewModal`, `MeetingCreateModal`, `PlannerEventChoicePopover`(+model), PMS 태스크 열기(`dispatchFloatingPmsOpen`) | 다른 앱 데이터. 앱 간 연결은 후속 |
| Google 캘린더 구독 | `google_calendar*.py`, `PlannerGoogleCalendarSync.tsx`, `/planner/google-calendar*` API, `planner_external_*` 테이블 | 계획 §2.2에서 제외 |
| AI 도구·승인 미리보기·RAG·검색 연동 | `tools.py`, `approval_preview.py`, `rag_sync.py`, `source_access.py`, `*_for_ai` 서비스, `retrieval_partition_id` | AI 기능(M13) 범위 |
| 포털 전역 위젯 | `FloatingTodayPlannerWidget`(+model), 도크의 DM·PMS·Today Planner 패널, 할 일의 "PMS로 전환", `floating-panel-events.ts` | 포털 전역 이벤트는 옮기지 않음. 오늘 일정은 M5 홈 요약 카드(`/v1/home-card`)로 제공 |
| 개인 위젯의 PMS 배정 업무 | `GET /personal-widgets/pms/tasks/assigned` | PMS 앱 데이터 |
| AI-DO 셸 연동 | `planner/index.ts`, `manifest.ts`, `routes.ts`, `sidebar.ts`(+spec), `public-api.ts` | miy는 manifest·포털이 담당 |
| 쓰이지 않던 코드 | `SchedulePopover.tsx`, `schedule-popover-model.ts`(+spec) | AI-DO에서도 어디서도 import하지 않음 |

순수 모델(`planner-calendar-session/controller/view-model/schedule-*`)은 타임라인·회의 분기까지 원본 그대로 두었다. 화면에서는 그 분기에 닿지 않는다. 원본 테스트를 그대로 돌리고, 후속 단계에서 기능을 더할 때 다시 옮기지 않기 위해서다.

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>` → `web/src/<경로>`로 같은 상대 경로에 두었다. "그대로"는 import 경로 외 변경이 없다는 뜻이다(§3.10).

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/planner/views/PlannerView.tsx` | 같은 경로 | 수정: 제외 기능 제거, 런타임·위치·설정, 휴일 정보 없음 |
| `…/views/PlannerEventModal.tsx` | 같은 경로 | 수정: `runtime.api`, 미저장 보고 |
| `…/views/planner-event-modal-model.ts` | 같은 경로 | 원본 + 미저장 비교 함수 추가 |
| `…/views/PlannerInsightsPanel.tsx` | 같은 경로 | 수정: 보이는 소스만 요약 타일 |
| `…/views/PlannerSidebar.tsx`, `PlannerMiniMonth.tsx` | 같은 경로 | 그대로(Google 캘린더 묶음 분기는 쓰이지 않음) |
| `…/views/planner-calendar-{view-model,controller,session,schedule-workflow,schedule-policy}.ts`, `planner-event-default-time.ts`, `planner-month-insights-model.ts` | 같은 경로 | 그대로 |
| `app-modules/planner/api/planner-api.ts` | 같은 경로 | 수정: `runtime.api`, 앱 기준 경로, 생성 시 `timeZone`, Google 함수 제거 |
| `components/calendar/UnifiedCalendar.tsx` | 같은 경로 | 수정: `locale` 속성, 내용 모드 전환 시 재생성 |
| `components/calendar/unified-calendar-adapter.ts` | 같은 경로 | 그대로 |
| `components/date/DateInput.tsx`, `date-input-model.ts` | 같은 경로 | 그대로 |
| `platform/calendar/calendar-api.ts`, `use-calendar-events.ts` | 같은 경로 | 수정: `runtime.api` |
| `platform/calendar/calendar-types.ts`, `calendar-events-session.ts` | 같은 경로 | 그대로 |
| `platform/time/*` | 같은 경로 | 그대로. 단 선호 저장소 2개는 저장 실패 시 메모리 보관 추가 |
| `platform/personal-widgets/PersonalWidgetHost.tsx` | 같은 경로 | 재구성: 상태 훅·본문·도크·패널 경로로 분리(마크업 동일) |
| `platform/personal-widgets/personal-widgets-api.ts` | 같은 경로 | 수정: `runtime.api`, 타입을 직접 선언 |
| `platform/personal-widgets/personal-widget-state.ts` | 같은 경로 | 그대로 |
| `platform/theme/app-color-fallbacks.ts`, `lib/utils.ts`, `lib/korean-holidays.ts` | 같은 경로 | 그대로 |
| `styles/fullcalendar-theme.css` | 같은 경로 | 그대로 + `styles/planner-frame.css` 추가 |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: `common`·`shell.personalWidgets`·`apps.planner` 한/영 원문 + miy 키([scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py)) |
| `platform/i18n/i18n.ts`, `locales.ts` | 같은 경로 | 축소: 저장 언어 대신 `runtime.settings.locale` |
| `platform/api/client.ts` | 같은 경로 | 축소: `apiFetchJson`·`apiFetchJsonWithMappedError`만, `runtime.api` 기반 |
| (없음) | `platform/api/api-error.ts`, `platform/runtime/{runtime-context,navigation-state,app-location}.ts(x)`, `AppRoot.tsx` | miy 연결 |
| `index.css`의 앱 공통 클래스 | 공용 UI 2.0.0 `app.css`(`platform-components/ui/app.css`) | 앱에 복사하지 않고 공용 UI를 import |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/…` | miy `src/…` |
| --- | --- |
| `planner/router.py`(일정 경로), `service.py`, `schemas.py`, `event_application.py`, `event_projection.py` | `planner.rs` |
| `planner/event_time.py` | `event_time.rs` |
| `planner/event_access.py` | 해당 없음: 요청마다 검증된 사용자 한 명이며 AI 주체가 없다 |
| `calendar/router.py`, `query_policy.py`, `source_catalog.PlannerCalendarSource`, `source_projection.project_planner_calendar_event`, `service.list_calendar_events` | `calendar.rs`(플래너 소스만) |
| `personal_widgets/router.py`, `service.py`, `schemas.py` | `personal_widgets.rs` |
| `core/i18n.localized_http_exception`, 요청 검증 오류 | `error.rs` |

## 3. 원본과 달라진 점과 이유

### 3.1 소유자와 데이터 모델

| 테이블 | AI-DO 컬럼 | miy |
| --- | --- | --- |
| `planner_events` | `id, workspace_id, retrieval_partition_id, visibility, owner_id(FK users), title, description, location, time_zone, all_day, start_has_time, end_has_time, start_at, end_at, created_at, updated_at` | `workspace_id`·`visibility`·`retrieval_partition_id` 없음, `owner_id`는 `text`·FK 없음. 나머지 이름·타입·제약 동일 |
| `personal_todo_items` | `id, user_id(FK), title(240), completed, sort_order, completed_at, created_at, updated_at` + 인덱스 3개 | `user_id`는 `text`·FK 없음. 나머지 동일 |
| `personal_memos` | `id, user_id(FK, unique), body, created_at, updated_at` | `user_id`는 `text`·FK 없음. 나머지 동일 |

- 소유자는 miy 사용자 ID(`Identity::user_id`)다. 코어의 `users.id`가 `text`이고 사용자 테이블은 코어 DB에 있어 FK를 둘 수 없다.
- AI-DO는 일정을 워크스페이스에서 개인 범위로 옮긴 뒤 `workspace_id`·`visibility`를 감사·되돌리기용으로만 남겼다(새 행은 항상 개인·비공개). miy 플래너는 처음부터 개인 범위라 두 컬럼을 두지 않았다. `retrieval_partition_id`는 RAG 분할(제외 기능)용이다.
- 시각은 AI-DO처럼 naive UTC(`timestamp`)로 저장한다. ID는 AI-DO와 같은 UUID v4 문자열이다.
- 스키마는 `migrations/0002_planner.sql`에 있다. 지시는 `0001_initial.sql` 교체였지만 이 파일은 번들이 해시로 고정한 보호 파일이라 바꾸면 `app check`와 서버 빌드가 `protected_bundle_modified`로 거부한다. 그래서 0001은 그대로 두고 0002를 더했다.

### 3.2 일정 시간대

AI-DO는 일정을 만들 때 로그인 사용자의 프로필 시간대(`current_user.time_zone`)를 일정에 저장하고, 종일·날짜만 있는 일정의 경계를 그 시간대로 계산했다. miy 서버는 사용자 프로필 시간대를 모른다. 시간대는 포털 표시 설정(`runtime.settings.timeZone`)으로 화면에만 전달된다.

- 화면이 일정 생성 요청에 `?timeZone=<IANA>`를 붙인다. 서버는 이름을 검증하고 모르는 값·빈 값은 AI-DO의 `event_time_zone`과 같이 `Asia/Seoul`로 바꾼다. 수정은 AI-DO처럼 일정에 저장된 시간대를 쓴다.
- 헤더가 아니라 질의 문자열인 이유: 게이트웨이는 `Content-Type` 외의 요청 헤더를 앱에 전달하지 않는다(`crates/miy/src/runtime.rs` `proxy`).
- 시간대는 표시 설정이며 권한 판단에 쓰지 않는다.
- 목록·캘린더 조회의 `from`/`to`는 시간대를 쓰지 않는다. AI-DO도 Python 3.12의 `datetime.fromisoformat`이 날짜만 있는 값을 UTC 자정으로 읽어 시간대 분기에 닿지 않았다. 같은 결과를 내도록 그대로 옮겼다.

### 3.3 `fromisoformat` 재현

`event_time.rs`는 Python 3.12 `datetime.fromisoformat`/`date.fromisoformat`의 동작(확장·기본 날짜, 임의 한 글자 구분자, 소수 초 6자리 절사, `Z`/`±HH[:MM]` 오프셋)을 재현한다. ISO 주 날짜(`2026-W19-1`)는 받지 않는다. Pydantic의 느슨한 형 변환(예: `"true"` 문자열을 bool로)은 재현하지 않는다. 길이 제한은 Pydantic처럼 바이트가 아닌 문자 수로 센다.

### 3.4 오류 응답과 문구

- AI-DO는 `localized_http_exception(code="planner.event_not_found")`로 서버에서 번역한 `detail`을 돌려주고 화면이 그 문장을 보였다.
- miy 브라우저 SDK(`runtime.api`)는 오류 본문 중 `error.code`만 남기며, 코드가 `^[a-z][a-z0-9_]*$`일 때만 받는다. 그래서 서버는 `{"error":{"code":"planner_event_not_found","message":"…"}}`로 답한다. 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이고, `message`는 AI-DO 카탈로그의 영어 문장이다.
- 화면은 코드를 AI-DO 서버 카탈로그의 한국어·영어 문장(`errors` 네임스페이스)으로 번역한다. 매개변수가 있는 문장 중 `calendar.range_too_large`는 상수 366을 넣고, `{error}`·`{source_types}`가 있는 두 문장은 매개변수 없이 보인다. 모르는 코드는 `common:feedback.requestFailed`(상태 코드 포함), 네트워크 오류는 `common:feedback.requestFailedRetry`로 보인다. AI-DO의 영어 고정 문장 "Request failed with N."에 해당한다.
- 요청 검증 실패는 AI-DO와 같이 422이며 코드는 `validation_request_invalid` 하나다(AI-DO는 필드별 `validation` 목록도 줬다).
- 상태 코드(400·404·422·201·204)와 검증 규칙(제목 1~200자, 설명 4000자, 장소 240자, 할 일 제목 1~240자 후 공백 제거, 메모 20,000자, 범위 366일, `start`·`end` 함께)은 AI-DO와 같다.

### 3.5 이름

`PlannerEventOut.ownerName`은 AI-DO에서 소유자 실명이었다. 앱은 디렉터리 권한(`platform:directory:read`)을 요청하지 않으므로 이름을 모른다. AI-DO 자신의 대체값인 소유자 ID를 넣는다. 캘린더 투영의 `metadata.ownerName`은 `null`이다. 화면은 두 값을 표시하지 않는다.

### 3.6 통합 캘린더 경로

`GET /v1/calendar/events`는 경로·질의(`from`, `to`, `sources`)·응답 형태를 유지하되 소스는 `planner_event`만 받는다. AI-DO의 다른 소스 이름(`meeting` 등)은 이 앱에서 "알 수 없는 소스"로 400이다. 화면은 이 경로로 일정을 읽으므로 원본 데이터 흐름(`useCalendarEvents`)을 바꾸지 않았다. 사이드바 필터와 이번 주 요약 타일은 `planner_event`만 보인다(AI-DO는 회의·PMS 타일도 0건으로라도 보였는데, miy에서는 그 데이터를 모으지 않으므로 보이지 않게 했다).

### 3.7 화면 경로와 이동

- AI-DO의 react-router(`useSearchParams`) 대신 `runtime.location`과 `runtime.navigate`를 쓴다(`useAppSearchParams`). 앱은 history API를 쓰지 않는다.
- 경로: `/`(플래너), `/widgets/todos`, `/widgets/memos`(좁은 패널). 알 수 없는 경로는 플래너를 보인다. AI-DO의 `/planner`도 플래너로 열린다.
- `?event=<id>`는 그대로 동작하고 창을 닫으면 지운다. `?view=timeline`은 무시한다.
- 플래너 화면은 AI-DO 라우트처럼 지연 로드한다. 패널 경로는 FullCalendar를 불러오지 않는다.

### 3.8 표시 설정

- 언어는 `runtime.settings.locale`(ko-KR 기본·대체, en-US)을 따른다. FullCalendar 로캘도 언어를 따른다(AI-DO는 항상 한국어 로캘). 공휴일 이름은 데이터가 한국어라 영어 화면에서도 한국어다.
- 날짜 형식·시간대는 AI-DO가 사용자 프로필에서 동기화하던 저장소(`ai-do:date-format`, `ai-do:time-zone`, 프레임 출처의 localStorage)에 `AppRoot`가 렌더링마다 `runtime.settings` 값을 넣는다. 저장이 막힌 프레임에서는 메모리에 보관한다.
- 어두운 테마는 플랫폼이 문서에 `.dark`를 붙이며 AI-DO의 `app-*` 토큰·클래스가 그대로 따른다.

### 3.9 공휴일

AI-DO와 같은 `@hyunbinseo/holidays-kr@5.2027.0`을 앱 번들에 포함한다(2018~2027). 보이는 달력 범위에 데이터가 없는 연도가 있으면 제목 옆에 "휴일 정보 없음"(영어 "No holiday data")을 보이고, 마우스를 올리면 해당 연도를 알려 준다. 데이터가 있는데 휴일이 없는 달에는 아무것도 보이지 않으므로 "휴일 없음"과 구분된다. 6주 격자가 다음 해로 넘어가는 달(예: 2027년 12월)에도 보인다. AI-DO에는 없던 표시이며 계획 M1의 요구다.

### 3.10 그 밖의 화면 변경

- **할 일·메모:** AI-DO는 포털 셸 오른쪽에 전역 도크를 뒀다. miy에서는 플래너 화면 오른쪽 끝에 같은 도크를 둔다. 떠 있는 패널 머리에 "패널 화면으로 열기"(`openPanel`) 버튼을 더해 같은 위젯의 좁은 경로로 이동한다. 상태·동작은 `usePersonalWidgets` 훅으로 묶어 도크와 패널 경로가 함께 쓴다. 마크업·클래스·문구는 원본이다.
- **미저장 보고(miy 계약):** 일정 창(열 때·불러온 뒤와 비교한 편집, 저장·삭제 중), 메모(저장 전·저장 중), 할 일(입력 중인 새 항목, 바뀐 제목 편집, 진행 중 요청)을 `runtime.setNavigationState`로 합쳐 보고한다. AI-DO에는 이동 확인이 없었다. 문구는 `apps:planner.unsavedChanges`.
- **API 호출:** 토큰 대신 `runtime.api`. 경로는 앱 기준(`/v1/…`)이며 게이트웨이를 거쳐 AI-DO와 같은 `/api/v1/…`이 된다. 경로의 ID는 `encodeURIComponent`로 감싼다. 일정 드래그 처리 흐름(`createPlannerCalendarScheduleWorkflow`)은 원본 그대로 두고, 어댑터가 `runtime.api`를 감싼다. 회의·태스크 어댑터는 해당 일정이 없으므로 닿지 않는다.
- **import 경로:** AI-DO의 `@/src/…` 별칭을 상대 경로로 바꿨다. 보호 파일인 미리보기 빌드 설정(`sdk/preview/vite.config.ts`)이 `@miy/*` 외 별칭을 모르기 때문이다(`miy app dev`가 이 설정으로 빌드한다).
- **CSS:** AI-DO `index.css`의 앱 공통 클래스는 공용 UI 2.0.0 `app.css`를 import한다. `fullcalendar-theme.css`는 그대로 가져왔다. `planner-frame.css`는 프레임 기본 스타일의 `a { color: brand }`가 Tailwind 계층 밖에 있어 FullCalendar 날짜 숫자·헤더 링크를 파랗게 만드는 것을 되돌린다.
- **FullCalendar 재생성:** 월 보기의 사용자 칩(`eventContent`)에서 주·일 보기의 기본 내용으로 바뀔 때 월 칩이 남아 기본 내용과 겹쳐 보였다(miy 미리보기, React 19.3에서 확인). `UnifiedCalendar`가 내용 모드마다 FullCalendar를 새로 만든다. 현재 보기·날짜는 호스트의 `initialView`/`initialDate`로 이어진다. AI-DO에서도 같은 현상이 있는지는 확인하지 않았다.
- **localStorage:** 위젯 선호(`ai-do.personalWidget.v1`)·타임라인 범위 키는 AI-DO 이름을 유지하고, 읽기·쓰기 실패를 무시하게 했다.

### 3.11 서버 동작의 세부

- `updated_at`은 SQLAlchemy `onupdate`처럼 실제로 값이 바뀐 수정에서만 갱신한다.
- 할 일 정렬 순서(`max(sort_order) + 1000`)는 한 SQL 문으로 계산·삽입한다. 메모 저장은 사용자당 한 행 제약에 `ON CONFLICT`로 갱신해 동시 첫 저장도 같은 결과를 낸다. 결과는 AI-DO와 같다.
- 수정은 행 잠금(`FOR UPDATE`) 트랜잭션 안에서 읽고 쓴다.

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 라우팅은 경로 해석 함수 + `useAppSearchParams` | 경로가 셋뿐이고 프레임에 history API가 없다. 라우터 라이브러리를 더하지 않았다 |
| 의존성은 AI-DO 고정 버전 | FullCalendar 6.1.20(core·daygrid·timegrid·list·interaction·luxon3·react), luxon 3.7.2, i18next 26.0.8, react-i18next 17.0.6, motion 12.38.0, holidays-kr 5.2027.0, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library(react 16.3.0, dom 10.4.0). React·Vite·Tailwind·Radix·lucide·clsx·tailwind-merge는 템플릿·공용 UI 버전 |
| 프런트 테스트는 AI-DO 설정(jsdom, globals, i18n 초기화)에 `TZ=Asia/Seoul` | AI-DO 테스트가 한국 시간 호스트를 전제로 한다 |
| DB 테스트는 `#[ignore]` + `PLANNER_TEST_DATABASE_URL` | 서버 빌드의 `cargo test`에는 DB가 없다. 무시된 테스트는 일회용 PostgreSQL을 줄 때만 돈다 |
| HTTP 테스트는 SDK `serve()`를 그대로 띄우고 서명한 신원으로 호출 | SDK 신원 검사(401·위조 거부)까지 실제 경로로 확인한다 |
| 공용 UI 설치는 `registry/components/ui/2.0.0`을 `component add`와 같은 형태로 복사하고 lock을 계산 | 오프라인 init 뒤 `component sync`에는 로그인이 필요하다. lock의 `registry`는 `http://localhost:18000`(저장소 픽스처와 같은 자리표시). 실제 서버에서 `miy component sync`로 같은 digest인지 확인해야 한다 |
| i18n 원문은 스크립트로 생성 | AI-DO 블록을 손으로 옮기다 생기는 차이를 막는다 |

## 5. 테스트 대응

| AI-DO | miy | 결과 |
| --- | --- | --- |
| `tests/test_planner_event_time.py`(6) | `event_time.rs` 6개 + `fromisoformat` 형식·시간대·DST 3개 | 같은 입력에 같은 값 |
| `tests/test_planner_event_application.py`(4) | `planner.rs` 3개(주체 검사 1개는 해당 없음) + 종일 전환·요청 형식·직렬화 3개 | |
| `tests/test_planner_events.py` | `api_tests.rs`: CRUD, 소유자만 접근, 캘린더에 본인 일정만, 부분 시각 | 회의 가용 시간 테스트 3개는 회의 앱과 함께 제외 |
| `tests/test_calendar_events.py` | `calendar.rs`·`api_tests.rs`: 인증 필요, 366일 제한, 알 수 없는 소스, 빈 소스, 잘못된 날짜, 역순 범위, 날짜/오프셋 범위 동일, `assignee_id` 무시 | 회의·PMS 항목 테스트는 제외. 날짜/오프셋 비교는 회의 대신 플래너 일정으로 |
| `tests/test_personal_widgets.py`(5) | `api_tests.rs` 5개 | |
| (miy 추가) | 생성 시간대 저장, 검증·업무 오류 코드, 신원 없음·위조 신원 거부, 오류 코드 형식 | |
| 화면 `*.spec.ts` 17개 파일(플래너 모델 7, 달력 어댑터, 날짜 입력 모델, 시간 5, 캘린더 세션, 위젯 상태, 공휴일) + `DateInput.spec.tsx` | 같은 경로, 내용 그대로 | 통과 |
| `PersonalWidgetHost.spec.tsx`(6) | 4개 그대로, 1개는 PMS 단언만 제거, 도크 패널 1개 제거, 패널 경로 2개 추가 | |
| (miy 추가) | `app-location.spec.ts`, `api-error.spec.ts`(코드 번역·언어 전환), `planner-event-modal-dirty.spec.ts`, `AppRoot.spec.tsx`(패널 경로·영어 플래너) | |

옮기지 않은 화면 테스트: `planner-timeline-model`, `floating-today-planner-model`, `planner-event-choice-popover-model`, `schedule-popover-model`, `sidebar`(기능 제외), `UserDateTime`(쓰지 않는 컴포넌트).

## 6. 남은 일

- 실제 플랫폼 프레임에서의 확인(계획 절차 9): 이 포팅은 SDK 미리보기 빌드를 로컬 게이트웨이 대역과 실제 Rust 서버·일회용 DB로 띄워 브라우저(Chromium)에서 확인했다. 실제 프레임 CSP, 프레임 런타임의 CSS 로드(빌드가 동기 CSS 두 개를 manifest에 적는다), 포털 이동 확인은 확인하지 않았다. 미리보기는 언어가 ko-KR로 고정이라 영어 화면은 jsdom 테스트로만 확인했다.
- AI-DO 기준 화면(가상 계정·가상 데이터)과의 나란한 비교.
- 카탈로그 공식 릴리스·dev 설치·인수·prod 승격(`miy app source-pin` 이후 절차).
- 앱 간 연결 이후: 통합 캘린더(회의·PMS), 할 일 → PMS 전환.

**M5에서 더한 것(2026-11-22).** manifest `panels`에 `todos`(`/widgets/todos`)·`memos`(`/widgets/memos`)를 선언해 포털 보조 패널에 뜨게 했다. AI-DO `FloatingTodayPlannerWidget`이 보이던 오늘 일정은 홈 요약 카드(`platformEndpoints.homeCard` → `POST /api/v1/home-card`)로 옮겼다. 사람의 시간대는 코어가 본문 `timeZone`으로 보낸다. 실제 프레임·코어 호출로는 확인하지 않았다(단위 테스트만).
