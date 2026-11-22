# PMS (`pms`)

프로젝트·업무 관리 miy 공식 업무 앱이다. AI-DO `0002af062`의 `pms` 도메인(서버)과 `app-modules/pms`(화면)를 옮겼다. 원본과의 차이, 뺀 기능과 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 스페이스 → 폴더 → 리스트 → 태스크 → 하위 태스크. 상태 흐름(스페이스 상속·리스트 자체 워크플로), 우선순위, 여러 담당자, 팔로워, 시작·마감·완료일, 라벨, 체크리스트, 첨부(파일당 10 MiB), 마일스톤·템플릿·사용자 정의 필드(AI-DO에 있던 만큼), 리스트·보드·캘린더·간트·표 보기, 내게 배정됨, 오늘 및 지연, 댓글·멘션, 변경 이력, 일괄 변경, 복사, 엑셀 내보내기, 보기 선호, 스페이스 고정·숨김.
- 권한: AI-DO와 같다. 누구나 스페이스를 만들고 그 소유자가 된다. 다른 스페이스는 직접 멤버(viewer·member·admin·owner)여야 읽는다. 앱 관리자나 플랫폼 관리자 표시는 멤버십을 대신하지 않는다(`Identity::admin`을 읽지 않는다). 단일 태스크 명시 공유(`pms_task_user_access`)는 그 태스크 읽기만 준다. 모든 판정은 서버가 요청마다 다시 한다.
- 요청 권한: `database`, `platform:directory:read`(이름·사용자 선택), `platform:files:read`·`platform:files:write`(첨부), `platform:notifications:write`(알림).
- 알림: 배정(`assigned`), 상태 변경(`status`), 댓글(`comment`), 댓글 멘션(`mention`)을 플랫폼 알림함으로 보낸다. 묶음 키는 태스크마다 `task:<id>`, 목적지는 `/tasks/<id>`. 태스크 삭제·멤버 제외·스페이스 삭제 때 회수하고, 태스크를 열면 읽음 처리한다. 지금 그 태스크를 읽을 수 있는 사람에게만 보낸다.

## 화면 경로

앱 기준 경로(`runtime.location.pathname`)다. history API는 쓰지 않는다.

| 경로 | 화면 |
| --- | --- |
| `/` (모르는 경로 포함) | PMS 전체 개요. 왼쪽 열에 스페이스 트리와 내 업무 묶음 |
| `/assigned` | 내게 배정됨 |
| `/today` | 오늘 및 지연(포털 시간대 기준) |
| `/spaces/:spaceId` | 스페이스 개요·통합 태스크 |
| `/lists/:taskListId` | 리스트(리스트·보드·캘린더·간트·표). `?task=<id>`로 상세를 연다. AI-DO 질의(`tab`, `settings`, `create` 등) 그대로 |
| `/tasks/:taskId` | 알림·홈 카드·검색 목적지. 리스트를 열 수 있으면 `/lists/<listId>?task=<id>`로 바꾸고, 아니면 단일 태스크 읽기 화면 |
| `/widgets/my-tasks` | 보조 패널 "내 업무"(약 400px, AI-DO 떠 있는 PMS 위젯). manifest `panels`의 `my-tasks` |

## API

UI는 `runtime.api('/v1/...')`로 부르고 게이트웨이가 `/api/v1/...`로 Rust 서버에 전달한다(AI-DO와 같은 경로). 요청·응답은 AI-DO JSON(snake_case) 그대로다. 날짜는 `YYYY-MM-DD`, 시각은 AI-DO처럼 naive UTC(`2026-11-22T09:00:00.123456`). 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것(`pms_task_not_found`), 요청 검증 실패는 422 `validation_request_invalid`다.

**사람**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/pms/me` | 현재 사용자 `{id, name, primary_org_unit_id, primary_org_unit_name}`(miy 추가) |
| `GET /v1/pms/users` | 사용자 선택용 디렉터리 사용자(`SpaceUserItem`, `email`은 빈 값) |

**스페이스·멤버·선호**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/pms/spaces` | 내가 멤버인 활성 스페이스(이름순) |
| `POST /v1/pms/spaces` | 생성(201). 만든 사람이 owner, 기본 상태 5개 생성 |
| `PATCH /v1/pms/spaces/{id}` | 이름·설명(owner·admin) |
| `DELETE /v1/pms/spaces/{id}` | 보관(비활성, 데이터 유지. owner·admin) |
| `GET /v1/pms/space-preferences` | 내 고정·숨김 |
| `PATCH /v1/pms/spaces/{id}/preference` | 고정·숨김(서로 배타) |
| `GET`·`PATCH /v1/pms/view-preferences` | 리스트 그룹 기준(`none`·`status`·`assignee`·`label`) |
| `GET /v1/pms/spaces/{id}/members` | 멤버(owner·admin 먼저, 이름순, 페이지) |
| `POST /v1/pms/spaces/{id}/members` | 추가(201) |
| `POST /v1/pms/spaces/{id}/members/bulk` | 여러 명 추가(이미 멤버는 건너뜀) |
| `PATCH`·`DELETE /v1/pms/spaces/{id}/members/{user_id}` | 역할 변경·제외(본인은 나가기). 마지막 owner는 남아야 한다 |

**상태(워크플로)**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET`·`POST /v1/pms/spaces/{id}/statuses` | 스페이스 상태 |
| `PATCH`·`DELETE /v1/pms/space-statuses/{id}` | 스페이스 상태 수정·삭제(사용 중·종료 상태는 거부) |
| `GET`·`POST /v1/pms/lists/{id}/statuses` | 리스트 상태(추가하면 리스트가 자체 워크플로로 바뀜) |
| `PATCH /v1/pms/lists/{id}/status-mode` | `inherit`·`custom` 전환 |
| `PATCH`·`DELETE /v1/pms/task-list-statuses/{id}` | 리스트 상태 수정·삭제 |

**폴더·리스트·마일스톤·라벨·템플릿·사용자 정의 필드**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET`·`POST /v1/pms/folders` | 폴더(만들면 기본 리스트 "List"도 생김) |
| `PATCH`·`DELETE /v1/pms/folders/{id}` | 폴더 수정·삭제(리스트는 폴더 밖으로) |
| `GET /v1/pms/lists?team_id` · `GET /v1/pms/spaces/{id}/lists` | 리스트(`page`, `page_size`, `sort_by`, `sort_dir`, `q`, `archived`) |
| `POST /v1/pms/lists` | 리스트 생성(스페이스 상태 상속, 기본 라벨 3개) |
| `GET`·`PATCH`·`DELETE /v1/pms/lists/{id}` | 조회·수정(보관 포함)·삭제(보관된 리스트만) |
| `PATCH /v1/pms/spaces/{id}/lists/reorder` | 폴더·순서 일괄 변경 |
| `GET`·`POST /v1/pms/lists/{id}/milestones`, `PATCH /v1/pms/milestones/{id}` | 마일스톤 |
| `GET`·`POST /v1/pms/lists/{id}/labels`, `PATCH`·`DELETE /v1/pms/labels/{id}` | 라벨 |
| `GET`·`POST /v1/pms/lists/{id}/templates`, `PATCH`·`DELETE /v1/pms/templates/{id}` | 태스크 템플릿 |
| `GET`·`POST /v1/pms/lists/{id}/custom-fields`, `DELETE /v1/pms/custom-fields/{id}` | 사용자 정의 필드 |
| `GET /v1/pms/lists/{id}/export` | 엑셀(.xlsx). 리스트 태스크 질의와 같은 필터 |
| `GET /v1/pms/dashboard/summary?list_id` | 요약 |

**태스크**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/pms/lists/{id}/tasks` | 목록(`page`, `page_size`, `sort_by`, `sort_dir`, `q`, `status`(여러 번), `assignee_id`, `priority`, `label_id`, `milestone_id`, `archived`, `due_date_from/to`, `start_date_from/to`) |
| `POST /v1/pms/lists/{id}/tasks` | 생성(201) |
| `PATCH /v1/pms/lists/{id}/tasks/reorder` · `…/reorder-by-title` | 순서·상위 변경, 이름순을 사용자 순서로 저장 |
| `PATCH /v1/pms/lists/{id}/tasks/bulk` | 일괄 변경·삭제(최대 200개) |
| `POST /v1/pms/lists/{id}/tasks/copy` | 복사(최대 500개, 하위 포함) |
| `GET /v1/pms/tasks/assigned` · `GET /v1/pms/tasks/today-overdue?today=` | 내 업무 |
| `GET /v1/personal-widgets/pms/tasks/assigned` | AI-DO 위젯 경로(화면은 위 경로들을 조합해 쓴다) |
| `GET`·`PATCH`·`DELETE /v1/pms/tasks/{id}` | 상세(댓글·하위·첨부·체크리스트; 내 알림 읽음 처리)·수정·삭제(하위 포함) |
| `POST /v1/pms/tasks/{id}/comments` | 댓글(멘션 알림) |
| `GET /v1/pms/tasks/{id}/activity-logs` | 변경 이력 |
| `POST /v1/pms/tasks/{id}/checklist`, `PATCH /v1/pms/tasks/{id}/checklist/reorder`, `PATCH`·`DELETE /v1/pms/checklist/{id}` | 체크리스트 |
| `PUT /v1/pms/tasks/{id}/assignees` · `…/followers` | 담당자·팔로워(스페이스 멤버만) |
| `GET`·`PUT /v1/pms/tasks/{id}/custom-field-values` | 사용자 정의 필드 값 |

**첨부(miy 파일 API)**

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /v1/pms/tasks/{id}/attachments` | 1단계: `{request_id, filename, content_type, size_bytes}`로 대기 기록과 플랫폼 파일 예약(201 `status: "pending"`) |
| `PUT /files/pms/attachments/{id}/content` | 2단계(`runtime.fileApi`): 바이트 업로드 → 첨부 확정 → 플랫폼 파일 연결. `AttachmentItem` 반환 |
| `GET /files/pms/attachments/{id}/content` | 내려받기(`runtime.fileApi`, 매번 읽기 권한 확인) |
| `DELETE /v1/pms/attachments/{id}` | 삭제(대기 중이면 예약 취소). 플랫폼 파일은 연결 해제 |

**플랫폼 호출(manifest `platformEndpoints`, 읽기 전용, 플랫폼 호출이 아니면 403 `pms_platform_only`)**

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /platform/notification-check` | `notificationCheck`: `{items:[{groupKey}]}` 중 지금 읽을 수 있는 태스크의 키만 `{allowed}` |
| `POST /v1/home-card` | `homeCard` "내 업무": 본문 `timeZone` 기준 오늘 마감·지연 건수(`title`)와 최대 5개 항목(`/tasks/<id>`) |
| `POST /v1/search` | `search`: 읽을 수 있는 태스크 중 제목·번호가 맞는 것(최근 수정순, 최대 `limit`), `kind` "업무" |

## 데이터

`migrations/0002_pms.sql`: AI-DO PMS 표(`pms_*`)와 스페이스·멤버(`pms_spaces`, `pms_space_members`), 알림 전달 기록(`pms_notification_deliveries`), 첨부 해제 대기열(`pms_attachment_releases`). `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

## 검사

```sh
cargo test                                   # DB 없는 단위 테스트
PMS_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored            # 일회용 PostgreSQL + 가짜 코어로 HTTP·DB 테스트까지
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
```

`PMS_TEST_DATABASE_URL`에는 버려도 되는 PostgreSQL만 준다. 테스트가 그 서버에 새 데이터베이스를 만들고 migration을 적용한다. 디렉터리·알림·파일은 테스트가 띄우는 가짜 코어가 받는다.

문구 원문 생성: `python3 scripts/gen-i18n-resources.py …`(화면), `python3 -I scripts/gen-error-catalog.py …`(서버 오류 문장). 두 스크립트의 머리말에 정확한 인자가 있다.
