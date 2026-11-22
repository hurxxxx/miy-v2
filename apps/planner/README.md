# 플래너 (`planner`)

개인 일정(캘린더)과 개인 할 일·메모를 다루는 miy 공식 앱이다. AI-DO `0002af062`의 `planner`·`personal_widgets` 도메인과 화면을 옮겼다. 원본과의 차이와 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 개인 일정 작성·수정·삭제, 종일 일정, 시작·종료 시각을 선택적으로 두는 일정, 월·주·일·아젠다 보기, 드래그로 일정 이동·길이 변경, 검색, 미니 달력, 이번 주 요약·바쁜 날·다가오는 일정, 공휴일 표시(데이터가 없는 연도는 "휴일 정보 없음"), 개인 할 일·메모(메모 자동 저장).
- 모든 데이터는 본인 것만 보인다. 소유자는 게이트웨이가 서명한 호출자(`Identity::user_id`)이며 요청 본문의 사용자 값은 받지 않는다.
- 권한은 `database`만 요청한다.

## 화면 경로

앱 기준 경로(`runtime.location.pathname`)다.

| 경로 | 화면 |
| --- | --- |
| `/` (그 밖의 경로 포함) | 플래너. 오른쪽 끝에 할 일·메모 도크가 있다. `?event=<id>`로 일정 편집 창을 바로 연다. |
| `/widgets/todos` | 할 일 패널만(좁은 폭). manifest `panels`의 `todos`로 포털 보조 패널에 뜬다. |
| `/widgets/memos` | 메모 패널만(좁은 폭). manifest `panels`의 `memos`. |

## API

UI는 `runtime.api('/v1/...')`로 부르고, 게이트웨이가 `/api/v1/...`로 Rust 서버에 전달한다(AI-DO와 같은 경로). 날짜·시각은 AI-DO와 같이 naive UTC로 저장한다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `planner_event_not_found`).

전체 요청·응답 계약은 Rust 라우터에서 생성한 [OpenAPI 명세](docs/openapi.json)에 있다. 생성과 검사 절차는 [API 가이드](docs/API.md)를 따른다.

캘린더 조회 범위와 일정 창의 시각은 포털 시간대를 따른다. 수정 창은 변경한 필드만 보내고, 일정 시각을 바꿀 때는 시작·종료를 함께 보낸다. 할 일 추가·수정 중 시작한 이전 조회는 저장한 항목이나 새 초안을 덮어쓰지 않는다.

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/planner/events?from&to` | 본인 일정. `from`·`to`는 함께 주거나 둘 다 생략. 범위 최대 366일 |
| `POST /v1/planner/events?timeZone=IANA` | 일정 생성(201). 본문 `{title, description?, location?, allDay?, start, end}`. `timeZone`은 일정에 저장되는 시간대(포털 표시 설정, 없거나 모르는 값이면 `Asia/Seoul`) |
| `GET /v1/planner/events/{id}` | 일정 하나. 남의 일정은 404 |
| `PATCH /v1/planner/events/{id}` | 일부 수정. `start`나 `end` 중 하나만 보내면 나머지는 기존 값을 사용한다. 종일 여부도 바꾸는 경우 두 경계 값을 함께 보내거나 둘 다 생략한다. |
| `DELETE /v1/planner/events/{id}` | 삭제(204) |
| `GET /v1/calendar/events?from&to&sources` | 캘린더 표시용 투영. 소스는 `planner_event`만 있다 |
| `GET /v1/personal-widgets/todos?include_completed` | 할 일 목록(미완료 → 정렬 순서 → 최근 생성 순) |
| `POST /v1/personal-widgets/todos` | 할 일 추가(201). 제목 1~240자 |
| `PATCH /v1/personal-widgets/todos/{id}` | 제목·완료 변경 |
| `DELETE /v1/personal-widgets/todos/{id}` | 삭제(204) |
| `GET /v1/personal-widgets/memo` | 본인 메모(없으면 빈 메모) |
| `PUT /v1/personal-widgets/memo` | 메모 저장. `{body, expectedUpdatedAt}`에서 `body`는 최대 20,000자, `expectedUpdatedAt`은 마지막 조회에서 받은 `updatedAt`(첫 저장은 `null`). 이후 다른 화면에서 변경한 경우 409 `personal_widgets_memo_conflict` |
| `POST /v1/home-card` | 포털 홈 요약 카드(manifest `platformEndpoints.homeCard`). 플랫폼 호출(`Identity.context.platform`)만 받고 그 밖은 403 `planner_platform_only`. 본문 `{timeZone, locale}`의 시간대로 오늘 일정을 고른다. 응답 `{title:"오늘 일정", count, items[≤5]: {title, subtitle(장소·종일), at(시각 일정의 시작), path:"/?event=<id>"}}` |
| `POST /v1/search` | 포털 통합 검색(manifest `platformEndpoints.search`). 플랫폼 호출만 받는다. 본문 `{query, limit, locale}`. 제목·장소·설명이 맞는 본인 일정(최근 시작 순)과 제목이 맞는 본인 할 일을 합쳐 최대 `limit`개(최대 10). 항목 `{title, snippet, at, path, kind}`(일정은 `/?event=<id>`, 할 일은 `/widgets/todos`) |

## 데이터

`migrations/0002_planner.sql`: `planner_events`, `personal_todo_items`, `personal_memos`. `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

## 검사

```sh
cargo test                                   # DB 없는 단위 테스트
PLANNER_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored            # 일회용 PostgreSQL로 HTTP·DB 테스트까지
pnpm install && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
```

`PLANNER_TEST_DATABASE_URL`에는 버려도 되는 PostgreSQL만 준다. 테스트가 그 서버에 새 데이터베이스를 만들고 migration을 적용한다.
