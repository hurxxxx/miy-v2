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
| `/widgets/todos` | 할 일 패널만(좁은 폭). 보조 프레임(M5)에서 쓸 화면이다. |
| `/widgets/memos` | 메모 패널만(좁은 폭). |

## API

UI는 `runtime.api('/v1/...')`로 부르고, 게이트웨이가 `/api/v1/...`로 Rust 서버에 전달한다(AI-DO와 같은 경로). 날짜·시각은 AI-DO와 같이 naive UTC로 저장한다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `planner_event_not_found`).

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/planner/events?from&to` | 본인 일정. `from`·`to`는 함께 주거나 둘 다 생략. 범위 최대 366일 |
| `POST /v1/planner/events?timeZone=IANA` | 일정 생성(201). 본문 `{title, description?, location?, allDay?, start, end}`. `timeZone`은 일정에 저장되는 시간대(포털 표시 설정, 없거나 모르는 값이면 `Asia/Seoul`) |
| `GET /v1/planner/events/{id}` | 일정 하나. 남의 일정은 404 |
| `PATCH /v1/planner/events/{id}` | 일부 수정. `start`·`end`는 함께 |
| `DELETE /v1/planner/events/{id}` | 삭제(204) |
| `GET /v1/calendar/events?from&to&sources` | 캘린더 표시용 투영. 소스는 `planner_event`만 있다 |
| `GET /v1/personal-widgets/todos?include_completed` | 할 일 목록(미완료 → 정렬 순서 → 최근 생성 순) |
| `POST /v1/personal-widgets/todos` | 할 일 추가(201). 제목 1~240자 |
| `PATCH /v1/personal-widgets/todos/{id}` | 제목·완료 변경 |
| `DELETE /v1/personal-widgets/todos/{id}` | 삭제(204) |
| `GET /v1/personal-widgets/memo` | 본인 메모(없으면 빈 메모) |
| `PUT /v1/personal-widgets/memo` | 메모 저장. 본문 최대 20,000자 |

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
