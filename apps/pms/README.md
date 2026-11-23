# PMS (`pms`)

프로젝트·업무 관리 miy 공식 업무 앱이다. AI-DO `0002af062`의 `pms` 도메인(서버)과 `app-modules/pms`(화면)를 옮겼다. 원본과의 차이, 뺀 기능과 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 스페이스 → 폴더 → 리스트 → 태스크 → 하위 태스크. 상태 흐름(스페이스 상속·리스트 자체 워크플로), 우선순위, 여러 담당자, 팔로워, 시작·마감·완료일, 라벨, 체크리스트, 첨부(파일당 10 MiB), 리스트·보드·캘린더·간트·표 보기, 내게 배정됨, 오늘 및 지연, 댓글·멘션, 변경 이력, 일괄 변경, 복사, 엑셀 내보내기, 보기 선호, 스페이스 고정·숨김.
- 보강(계획 §2.2, [PORTING §10](docs/PORTING.md#10-보강-템플릿사용자-정의-필드마일스톤저장-필터)): 리스트 또는 스페이스 전체 마일스톤(만들기·수정·삭제, 진행률, 캘린더·간트 표시), 리스트별 사용자 정의 필드(텍스트·숫자·날짜·단일 선택·다중 선택·사용자; 순서·선택지 편집, 태스크 상세·표 보기에서 값 편집, 필터·정렬), 스페이스 태스크 템플릿(직접 작성 또는 태스크를 하위 태스크·체크리스트·라벨째 저장, 템플릿으로 새 태스크), 서버에 보관하는 저장 필터(사람별, 스페이스 owner·admin은 스페이스에 공유, 필터·정렬·그룹, 열 때 적용할 기본 필터). 화면은 리스트 설정의 `마일스톤`·`필드`·`템플릿` 탭, 새 태스크 창의 템플릿 메뉴(`템플릿 관리`), 태스크 상세의 `템플릿으로 저장`·사용자 정의 필드, 표 보기의 필드 열, 필터 막대의 `필드`·`저장됨`이다.
- 권한: AI-DO와 같다. 누구나 스페이스를 만들고 그 소유자가 된다. 다른 스페이스는 직접 멤버(viewer·member·admin·owner)여야 읽는다. 앱 관리자나 플랫폼 관리자 표시는 멤버십을 대신하지 않는다(`Identity::admin`을 읽지 않는다). 단일 태스크 명시 공유(`pms_task_user_access`)는 그 태스크 읽기만 준다. 모든 판정은 서버가 요청마다 다시 한다.
- 요청 권한: `database`, `platform:directory:read`(이름·사용자 선택), `platform:files:read`·`platform:files:write`(첨부), `platform:notifications:write`(알림), `app:docs:read`·`app:whiteboard:read`(연결된 자원 조회).
- 문서·보드 연결은 설치된 PMS의 태스크 상세·공간 개요·리스트 머리글에서 관리한다. 세 앱은 같은 환경에 설치하고, PMS에 대상 앱 읽기 권한을, PMS와 대상 앱 양쪽에 `platform:directory:read`를 승인한다. PMS DB에는 연결 ID와 대상 ID를 저장하고, 열 때마다 양쪽 자원의 현재 권한을 확인한다. 대상 자원이 휴지통에 있거나 읽기 권한이 없으면 제목·주소는 숨기며 PMS 편집자는 그 연결 행을 해제할 수 있다.
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
| `GET /v1/pms/lists/{id}/milestones` | 리스트 마일스톤과 그 스페이스 전체 마일스톤(`scope` `list`·`space`, `progress` 0~1, `task_count`, `completed_task_count`) |
| `POST /v1/pms/lists/{id}/milestones` | 리스트 마일스톤(owner·admin) `{title, description?, status?(planned·active·complete), start_date?, due_date?, sort_order?}` |
| `GET`·`POST /v1/pms/spaces/{id}/milestones` | 스페이스의 모든 마일스톤 / 스페이스 전체 마일스톤 만들기(owner·admin) |
| `PATCH`·`DELETE /v1/pms/milestones/{id}` | 수정(`null` 날짜는 지움)·삭제(태스크는 남고 마일스톤만 비움, 활동 기록) |
| `GET`·`POST /v1/pms/lists/{id}/labels`, `PATCH`·`DELETE /v1/pms/labels/{id}` | 라벨 |
| `GET /v1/pms/lists/{id}/templates` · `GET`·`POST /v1/pms/spaces/{id}/templates` | 리스트가 속한 스페이스의 템플릿(최근순) / 스페이스 템플릿 목록·직접 작성(member 이상) |
| `GET`·`PATCH`·`DELETE /v1/pms/templates/{id}` | 템플릿 조회(스페이스 멤버)·수정·삭제(member 이상) |
| `POST /v1/pms/tasks/{id}/template` | `{name}`: 태스크(설명·상태·우선순위·체크리스트·라벨·보관 안 된 직계 하위 태스크)를 그 스페이스 템플릿으로 저장(member 이상) |
| `GET`·`POST /v1/pms/lists/{id}/custom-fields` | 사용자 정의 필드(`field_type` `text`·`number`·`date`·`select`·`multi_select`·`user`, 선택 필드는 `options` 필수, 리스트 안 이름 유일, 최대 50개. owner·admin) |
| `PATCH /v1/pms/lists/{id}/custom-fields/reorder` | `{field_ids}` 순서 저장(owner·admin) |
| `PATCH`·`DELETE /v1/pms/custom-fields/{id}` | `{name?, options?, expected_options?, option_renames?: [{from,to}]}`. `options`를 보낼 때 조회한 기존 선택지 배열을 순서대로 `expected_options`에 보낸다. 빠뜨리면 422, 현재 선택지와 다르면 409로 거절된다. 형식은 유지되며 제거한 선택지나 삭제한 필드의 값은 지운다. |
| `GET /v1/pms/lists/{id}/export` | 엑셀(.xlsx). 리스트 태스크 질의와 같은 필터 |
| `GET /v1/pms/dashboard/summary?list_id` | 요약 |

**태스크**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/pms/lists/{id}/tasks` | 목록(`page`, `page_size`, `sort_by`, `sort_dir`, `q`, `status`(여러 번), `assignee_id`, `priority`, `label_id`, `milestone_id`, `archived`, `due_date_from/to`, `start_date_from/to`, `cf_<필드 id>`: 텍스트는 포함, 다중 선택은 그 선택지 포함, 나머지는 같음). `sort_by=cf_<필드 id>`는 그 값 순(숫자·날짜·선택지 순서·사람 이름, 값 없는 태스크는 뒤). 항목의 `custom_fields`는 `{필드 id: 값}` |
| `POST /v1/pms/lists/{id}/tasks` | 생성(201). `template_id`(리스트가 속한 스페이스의 템플릿)를 주면 그 체크리스트·라벨(같은 이름이 리스트에 있을 때)·하위 태스크를 같은 트랜잭션에서 더한다 |
| `PATCH /v1/pms/lists/{id}/tasks/reorder` · `…/reorder-by-title` | 순서·상위 변경, 이름순을 사용자 순서로 저장 |
| `PATCH /v1/pms/lists/{id}/tasks/bulk` | 일괄 변경·삭제(최대 200개) |
| `POST /v1/pms/lists/{id}/tasks/copy` | 복사(최대 500개, 하위 포함) |
| `GET /v1/pms/tasks/assigned` · `GET /v1/pms/tasks/today-overdue?today=` | 내 업무 |
| `GET /v1/personal-widgets/pms/tasks/assigned` | AI-DO 위젯 경로(화면은 위 경로들을 조합해 쓴다) |
| `GET`·`PATCH`·`DELETE /v1/pms/tasks/{id}` | 상세(댓글·하위·첨부·체크리스트; 내 알림 읽음 처리)·수정·삭제(하위 포함) |
| `GET /v1/pms/resource-links/{scope}/{owner_id}` | 읽을 수 있는 연결의 `items` 목록. `task`는 문서, `space`·`list`는 화이트보드 |
| `PUT /v1/pms/resource-links/{scope}/{owner_id}/{resource_id}` | 연결 추가. 태스크와 공간은 각각 최대 12개, 리스트 연결은 보드 한 개를 선택하거나 교체 |
| `DELETE /v1/pms/resource-links/{scope}/{owner_id}/links/{link_id}` | PMS 연결 해제. 대상 앱의 자원은 유지 |
| `POST /v1/pms/tasks/{id}/comments` | 댓글(멘션 알림) |
| `GET /v1/pms/tasks/{id}/activity-logs` | 변경 이력 |
| `POST /v1/pms/tasks/{id}/checklist`, `PATCH /v1/pms/tasks/{id}/checklist/reorder`, `PATCH`·`DELETE /v1/pms/checklist/{id}` | 체크리스트 |
| `PUT /v1/pms/tasks/{id}/assignees` · `…/followers` | 담당자·팔로워(스페이스 멤버만) |
| `GET`·`PUT /v1/pms/tasks/{id}/custom-field-values` | 사용자 정의 필드 값 `{field_id, value}`. 형식 검사와 한 가지 표기(숫자 정규화, 날짜 `YYYY-MM-DD`, 다중 선택은 선택지 순서의 JSON 목록, 사용자는 지금 스페이스 멤버). 빈 값은 지움. 태스크 잠금 뒤 편집 권한 확인 |

**저장 필터(miy 보강)**

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/pms/saved-filters?list_id=` · `?space_id=` | 그 보기(리스트 또는 스페이스 통합 보기)의 내 필터와 공유 필터 `{items, default_filter_id}`(이름순). 항목은 `params`(검색어 제외 필터 막대), `sort`, `group_by`, `shared`, `owner_name`, `is_mine`, `is_default`, `can_manage` |
| `POST /v1/pms/saved-filters` | `{list_id \| space_id, name, params, sort?, group_by?, shared?}`(201). 공유는 스페이스 owner·admin만. 보기마다 사람당 100개 |
| `PATCH`·`DELETE /v1/pms/saved-filters/{id}` | 이름·내용 바꾸기, 공유·해제(owner·admin), 삭제. 만든 사람, 그리고 공유 필터는 owner·admin이 관리한다. 볼 수 없는 필터는 404 |
| `PUT /v1/pms/saved-filters/default` | `{list_id \| space_id, filter_id \| null}`: 그 보기를 열 때 적용할 내 기본 필터. 볼 수 없게 된 기본 필터는 없는 것으로 본다 |

**첨부(miy 파일 API)**

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /v1/pms/tasks/{id}/attachments` | 1단계: `{request_id, filename, content_type, size_bytes}`로 대기 기록과 플랫폼 파일 예약(201 `status: "pending"`) |
| `PUT /files/pms/attachments/{id}/content` | 2단계(`runtime.fileApi`): 바이트 업로드 → 첨부 확정 → 플랫폼 파일 연결. `AttachmentItem` 반환 |
| `GET /files/pms/attachments/{id}/content` | 내려받기(`runtime.fileApi`, 매번 읽기 권한 확인) |
| `DELETE /v1/pms/attachments/{id}` | 삭제(대기 중이면 예약 취소). 플랫폼 파일은 연결 해제 |

예약이나 업로드 응답을 확인하지 못하면 같은 요청 ID와 첨부 ID로 한 번 복구를 시도한다. 오류가 계속되면 태스크 상세에서 같은 파일과 요청 ID로 다시 시도할 수 있다. 앱 서버는 이미 Storage에 들어간 바이트를 다시 올리는 대신 연결 결과를 확인한다. 첨부 삭제는 사용자가 명시적으로 요청할 때 처리한다.

**플랫폼 호출(manifest `platformEndpoints`, 읽기 전용, 플랫폼 호출이 아니면 403 `pms_platform_only`)**

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /platform/notification-check` | `notificationCheck`: `{items:[{groupKey}]}` 중 지금 읽을 수 있는 태스크의 키만 `{allowed}` |
| `POST /v1/home-card` | `homeCard` "내 업무": 본문 `timeZone` 기준 오늘 마감·지연 건수(`title`)와 최대 5개 항목(`/tasks/<id>`) |
| `POST /v1/search` | `search`: 읽을 수 있는 태스크 중 제목·번호가 맞는 것(최근 수정순, 최대 `limit`), `kind` "업무" |

## 데이터

`migrations/0002_pms.sql`: AI-DO PMS 표(`pms_*`)와 스페이스·멤버(`pms_spaces`, `pms_space_members`), 알림 전달 기록(`pms_notification_deliveries`), 첨부 해제 대기열(`pms_attachment_releases`). `0003_task_status_length.sql`은 업무 상태 열을 워크플로 상태 정의와 같은 40자로 넓힌다. `0004_pms_reinforcement.sql`(보강): 마일스톤에 `team_id`(리스트 마일스톤은 `list_id`도, 스페이스 전체는 `list_id` NULL), 템플릿을 스페이스 소속으로 바꾸고(`team_id`, `list_id` 삭제) `title`·`description_blocks`·`labels`·`subtasks`·`created_by_id`·`updated_at` 추가, 저장 필터(`pms_saved_filters`)와 사람별 기본 필터(`pms_saved_filter_defaults`). `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

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
