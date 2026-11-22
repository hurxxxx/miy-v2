# AI-DO PMS 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음). 서버 `apps/api/src/ai_do_api/domains/pms/**`(+ PMS가 쓰던 `auth/app_spaces_service.py`·`app_access.py`의 앱 스페이스 규칙), 화면 `apps/web/src/app-modules/pms/**`와 그 모듈이 가져오는 공용 화면 코드, 문구 `apps/web/src/platform/i18n/resources.ts`·`apps/api/src/ai_do_api/core/i18n_catalog.py`, 테스트 `apps/api/tests/test_pms_*.py`·모듈 `*.spec.ts(x)`
- 대상: miy 공식 업무 앱 `pms`(이름 `PMS`), 번들 0.12.0, Rust SDK 0.4.0, 공용 UI 2.0.0(오프라인으로 `registry/components/ui/2.0.0`을 `component add`와 같은 형태로 복사, lock은 플래너와 같다)
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M2 플랫폼–앱 연결과 알림", "M5 보조 프레임과 홈·개인 위젯", "M7~M11 공식 업무 앱", §2.2 PMS 행. 구조·관례는 [플래너 포팅 기록](../../planner/docs/PORTING.md)을 따른다.
- 화면 기반(플래너에서 그대로 가져옴): `platform/runtime/{runtime-context,navigation-state}`, `platform/api/{client,api-error}`, `platform/i18n/{i18n,index,locales}`, `platform/time/*`, `platform/theme/*`, `platform/calendar/calendar-types`, `components/date/*`, `components/calendar/*`, `lib/{utils,korean-holidays}`, `styles/fullcalendar-theme.css`, `test-setup.ts`, `vitest.config.ts`. `planner-frame.css`는 `pms-frame.css`로 이름을 바꾸고 범위를 넓혔다(§5.9).
- 원칙: AI-DO 구현을 그대로 옮기고 miy의 경계(인증·저장·앱 프레임·앱별 DB·플랫폼 디렉터리·파일·알림) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다.

## 1. 옮긴 범위와 뺀 범위

**서버에서 옮긴 것:** 스페이스(생성·수정·보관)·멤버(추가·일괄 추가·역할·제외·나가기, owner/admin 규칙)·사이드바 고정·숨김·보기 선호, 스페이스·리스트 워크플로 상태(상속/자체, 종료 상태 하나 규칙, 사용 중 삭제 거부), 폴더(기본 리스트 생성), 리스트(목록·생성·수정·보관·삭제·재정렬), 마일스톤, 라벨, 태스크(목록 필터·정렬·검색·페이지, 생성, 수정의 필드별 이력, 하위 태스크와 순환 검사, 순서·상위 변경, 이름순 저장, 일괄 변경·삭제, 삭제), 내게 배정됨·오늘 및 지연·개인 위젯 배정 목록, 상세, 댓글·멘션, 변경 이력, 체크리스트, 담당자·팔로워, 템플릿, 사용자 정의 필드와 값, 복사, 엑셀 내보내기, 대시보드 요약, 첨부.

**miy에서 더한 것:** 플랫폼 알림 발행·회수·읽음(§3.7), `notificationCheck`(§3.7), 보조 패널 `my-tasks`(§5.2), 홈 요약 카드 "내 업무"(§3.8), 통합 검색(§3.8), 현재 사용자 `GET /v1/pms/me`(§3.3).

**뺀 것:**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| AI 도구·승인 미리보기 | `tools.py`, `approval_preview.py`, 서비스의 `approved_call_id` 재실행 분기, `_bind_workspace_context`·`_require_user_write_principal`(principal 검사) | AI 기능(M13) 범위. miy 요청은 항상 서명된 사용자 한 명이다 |
| RAG 동기화 | `rag_sync.py`, `enqueue_*_rag_sync`·`*_recompute`, `pms_tasks.retrieval_partition_id`, `can_read_task_for_rag` | M13 |
| 검색 훅·투영 | `search_hooks.py`, `search_projection.py`, `source_access.py`, `projections.py`의 검색 쪽 | M12 통합 검색 때. 지금은 단순 제목 검색 엔드포인트만 둔다(§3.8) |
| Docs·Whiteboard·Recording·Meeting 연결 | `task_doc_links.py`, `links.py`의 문서 경로, `pms_task_doc_links`, `/tasks/{id}/docs*` 3경로, 상세의 `linked_docs`(빈 배열로 응답), `access_grants.py`(회의 참석자 부여·회수·만료 연장) | 그 앱들이 아직 없다. 문서 연결 표는 `docs_native_docs` FK라 사소하지 않아 두지 않았다. 단일 태스크 부여 표(`pms_task_user_access`)와 읽기 규칙은 남겼다(§3.4) |
| 앱 안 알림함 | `Notification`(`pms_notifications`), `/notifications`·`/unread-count`·`/{id}/read`·`/read-all`, `_create_notification`, `NotificationSnapshot`, `_TaskNotificationSnapshots` | 플랫폼 알림함이 대신한다(§3.7) |
| 근태 앱 중계·모바일 푸시 | `notifications/mobile_push_*`, 근태 중계 | 계획 §2.2에서 제외 |
| 반복 태스크 자동 생성·마감 알림 | AI-DO 작업자의 반복·마감(`pms_overdue`) 처리 | 서비스 주체가 생긴 뒤. `recurrence_rule`은 AI-DO처럼 값만 저장한다 |
| 설명 이미지(미디어) | `sync_embedded_media`, `cleanup_media_for_resource`, `clone_resource_media`·`copy_cloned_media_objects`, `media.storage_upload_failed` | AI-DO 미디어 서비스가 없다. 편집기도 이미지 업로드를 뺐다(§1 "화면에서 뺀 것") |
| 공용 파일 처리 레인 | `run_file_processing`(일괄 변경·복사·이름순 저장·내보내기), 503 `Retry-After` | 요청 안에서 바로 실행한다. 결과·검증 규칙은 같다 |
| 레거시 경로·워크스페이스 연결 | `legacy_routes.py`, `app_catalog.py`, `links.py`의 `/w/<slug>/pms` 경로, 리소스 경로 정책(`PMS_RESOURCE_ROUTE_POLICIES`), 옛 팀 데이터 migration | miy는 manifest·포털이 담당하고 옮길 옛 데이터가 없다 |
| 의존관계·자동화·시간 기록·위키 | (`wiki_pms` 도메인 등) | 계획 §2.2에서 뒤로 미룸 |

**화면에서 옮긴 것:** 스페이스 트리(스페이스·폴더·리스트 CRUD, 드래그 정렬, 고정·숨김, 순서 편집기), 스페이스 개요, 스페이스 멤버 창(역할, 개별·부서 초대), PMS 전체 개요, 리스트 보기(리스트·보드·캘린더·간트·테이블), 스페이스 통합 태스크 보기, 필터 막대(저장 필터 포함), 일괄 작업 막대(상태·우선순위·담당자·라벨·날짜·보관·삭제·복사), 태스크 복사, 엑셀 내보내기, 보기 선호(그룹 기준), 새 태스크 창(템플릿, 상위 태스크 선택), 태스크 상세(제목·설명(BlockNote)·상태·우선순위·담당자·팔로워·날짜·마일스톤·반복 규칙·라벨·체크리스트·하위 태스크·첨부·댓글·멘션·활동 기록), 리스트 설정(워크플로/상태, 라벨, 멤버), 나에게 배정됨, 오늘 및 지연, 떠 있는 PMS 위젯(보조 패널로), 단일 태스크 공유 보기.

**화면에서 뺀 것(화면·API 호출 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| Docs 연결·스페이스 문서 | `views/SpaceDocsView.tsx`, `views/TaskDocPickerModal.tsx`(+spec), `views/useTaskDetailLinkedDocs.ts`(+spec), `sidebar/SortableDocLink.tsx`(+spec), `PmsSpaceToolTabs`의 문서 탭, `TaskDetail`의 "연결된 문서" 영역·"설명을 문서로" 버튼, `PmsSidebarSpaces`·`SpaceTree`·`SpaceOrderEditorModal`·`SpaceAddPopover`·`FolderAddPopover`의 문서 항목, `space-tree-model`·`space-order-editor-model`·`space-order-persistence`·`space-overview-model`의 문서 절반, `pms-view-route`의 `spaceDocs` 경로·`buildPmsSpaceDocsTool*`, `pms-sidebar-scope`의 `pmsSidebarDocsPath`, `pms-events`의 `docChanges`, `pms-api`의 `listTaskDocs`·`attachTaskDoc`·`detachTaskDoc`·`PmsTaskDocLink` | Docs 앱이 miy에 아직 없다(`@/src/app-modules/docs/public-api`) |
| 화이트보드 | `views/SpaceWhiteboardsView.tsx`, `PMSView`의 Whiteboard 탭·`WhiteboardContextSlotPanel`, `pms-view-route`의 `spaceWhiteboards` 경로·`buildPmsSpaceWhiteboardsTool*` | 화이트보드 앱 없음(`whiteboard/public-api`) |
| 녹음 연결 | `TaskDetail`의 `LinkedRecordingsForTarget` | 녹음 앱 없음(`recording/public-api`) |
| 편집기 이미지 업로드 | `platform/media/use-media-upload`·`media-api`(`uploadFile`·`resolveFileUrl`·`linkMedia`·`extractMediaIds`), `TaskDetail`·`NewTaskModal`·`TaskDetailActivityPanel`의 해당 속성 | AI-DO 미디어 서비스(`media:{id}`)가 miy에 없다. 편집기는 업로드 처리기 없이 동작한다(이미지 업로드 메뉴 없음). 첨부는 §5.6 |
| PMS 앱 안 알림 | `pms-api`의 `listNotifications`·`getUnreadNotificationCount`·`markNotificationRead`·`markAllNotificationsRead`·`PmsNotification*`, 셸 내비 `pms-inbox` | miy 플랫폼 알림 패널이 대신한다 |
| 포털 전역 위젯 이벤트 | `platform/personal-widgets/floating-panel-events`(`dispatchFloatingPmsOpen`, `FLOATING_PMS_OPEN_EVENT`, `PERSONAL_TODO_PMS_TASK_CREATED_EVENT`), `FloatingPmsWidget`의 `openRequest`·`onCreateTaskOpenRequestHandled`·개인 할 일 전환 처리, `PMSView`의 할 일 전환 이벤트 발송, `useFloatingPmsAssignedSummary`·`useFloatingPmsAssignedCount`(포털 도크 배지) | 플랫폼은 프레임 사이에 아무것도 전달하지 않는다(CONTRACT "Side panels") |
| 위젯 바닥 "PMS" 버튼 | `FloatingPmsWidget` 바닥 버튼(`navigate(buildAppRoutePath({routeBase:'/pms',...}))`) | 패널 프레임은 자기 위치만 바꿀 수 있어 주 프레임을 열 수 없다. PMS는 포털 런처로 연다 |
| AI-DO 셸 연동 | `index.ts`, `manifest.ts`, `routes.ts`, `shell-nav.ts`, `public-api.ts`, `help-guide.ts`, `sidebar/config.tsx`(+spec), `sidebar/pms-sidebar-config-model.ts`(+spec), `platform/app-runtime/ResourceEntry`, 워크스페이스 카탈로그 갱신(`CreateSpaceModal`의 `reloadGlobalApps`) | miy는 manifest·포털이 담당. 제목·활성 표시는 앱 왼쪽 열이 같은 규칙으로 보인다(§5.2) |
| 쓰이지 않게 된 공용 코드 | `platform/i18n/resource-location-label`(SpaceDocsView 전용), 편집기 `compactSchema`·`BlockEditorMini`·협업 편집기·markdown | 위 제외로 참조가 없어짐 |

AI 도구·승인 미리보기, 검색 훅, 통근 연계, 의존 관계, 자동화, 시간 기록, 위키는 PMS 화면 모듈에 코드가 없어 뺄 것이 없었다. 반복 규칙(`recurrence_rule`) 선택은 AI-DO에서도 값 저장만 하고 자동 생성이 없어서 그대로 두었다.

## 2. 서버 파일 대응

FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust). 화면은 §4.

| AI-DO `apps/api/src/ai_do_api/domains/…` | miy `src/…` |
| --- | --- |
| `pms/models.py`, `auth/models.py`의 `Workspace`·`WorkspaceUserBinding`(PMS가 쓰던 열) | `migrations/0002_pms.sql`, `models.rs` |
| `pms/workflow.py`, `status_lifecycle.py`의 기본 상태, `task_update_plan.py` | `workflow.rs` |
| `pms/status_lifecycle.py`, `status_router.py` | `statuses.rs` |
| `pms/access.py`, `auth/app_access.py`(`resolve_space_access`) | `access.rs` |
| `pms/service.py`(스페이스·멤버·선호), `view_preferences.py`, `auth/app_spaces_service.py`(생성·수정·보관·멤버 규칙) | `spaces.rs` |
| `pms/service.py`(`list_task_lists`, `create_task_list`, 키 생성, 기본 라벨), `board_configuration.py`, `router.py`(마일스톤·라벨·폴더·대시보드), `dashboard_summary.py` | `lists.rs` |
| `pms/projections.py`(`serialize_task_summary`), `service.py`(`_serialize_task_list`) | `summary.rs` |
| `pms/service.py`(태스크 목록·생성·수정·삭제·재정렬·일괄·댓글·상세·배정·오늘/지연·개인 위젯), `router.py`(일괄 변경) | `tasks.rs` |
| `pms/task_work_items.py`, `task_list_customization.py`, `router.py`(활동 이력·담당자·팔로워) | `task_items.rs` |
| `pms/attachments.py` | `attachments.rs`(miy 파일 API로 재구성) |
| `pms/task_copy.py` | `copy.rs` |
| `pms/task_export.py`(openpyxl) | `export.rs`(rust_xlsxwriter) |
| `core/i18n_catalog.py`의 PMS 코드, `core/i18n.localized_http_exception`, 요청 검증 오류 | `error.rs`, `error_catalog.rs`([scripts/gen-error-catalog.py](../scripts/gen-error-catalog.py)로 생성), `input.rs` |
| `notifications/mobile_push_display.py`(한글 제목표), `notifications/display_snapshot.py`(`task_status_label_ko`) | `notify.rs` |
| `auth/users`(이름·주 조직) | `users.rs`(플랫폼 디렉터리) |
| (없음) | `home_card.rs`, `search.rs`, `notify.rs`의 `notification_check`, `api_tests.rs` |

## 3. 서버: 원본과 달라진 점과 이유

### 3.1 데이터 모델

표·열 이름, 타입, 제약은 AI-DO와 같다. 다른 곳은 다음과 같다(`migrations/0002_pms.sql` 머리말에도 적었다).

- 사용자 참조(`reporter_id`, `author_id`, `user_id`, `created_by_id` …)는 miy 사용자 ID(`text`)이며 FK가 없다. 사용자 표는 코어 DB에 있다.
- 스페이스: AI-DO PMS 스페이스는 공용 `workspaces` 표의 `kind='app_space', owner_app_id='pms'` 행이었고 멤버는 `workspace_user_bindings`(역할 viewer·member·admin·owner)였다. miy에서는 앱 DB의 `pms_spaces`(`id, key, name, description, active, created_at, updated_at`)와 `pms_space_members`(`id, space_id, user_id, role, created_at`, `(space_id, user_id)` 유일)로 옮겼다. `team_id`는 AI-DO 이름을 유지하고 `pms_spaces`를 가리킨다. 키는 AI-DO와 같은 `s-<id>`. `trashed_at`·`org_unit_id`·`migrated_from_workspace_id`·`kind`·`owner_app_id`는 PMS가 쓰지 않아 두지 않았다. 스페이스 삭제는 AI-DO `archive_app_space`처럼 `active=false`.
- `pms_view_preferences`는 사용자당 한 행(기본 키 `user_id`). AI-DO는 PMS 워크스페이스와 사용자로 키를 잡았는데 miy는 환경마다 설치 하나라 워크스페이스가 없다.
- `pms_tasks`에 `retrieval_partition_id` 없음(RAG 제외).
- `pms_task_assignees`·`pms_task_followers`·`pms_task_labels`에 `position`을 더했다. AI-DO는 ORM 컬렉션 순서로 담당자 순서(첫 담당자가 주 담당자 `assignee_id`)와 라벨 순서를 지켰다. SQL에서는 순서를 저장해야 같은 결과가 나온다.
- `pms_attachments`: 객체 저장소 키(`storage_key`) 대신 플랫폼 파일 ID(`file_id`)와 예약·연결 요청 ID(`request_id`, `attach_request_id`), 상태(`pending`·`attached`)를 둔다(§3.6). `pms_attachment_releases`(miy 추가)는 앱 참조를 지운 뒤 코어에서 연결 해제를 아직 확인하지 못한 파일이다.
- `pms_task_user_access.granted_by_meeting_id`는 FK 없음(회의 앱 없음).
- `pms_notifications`(앱 안 알림함)는 없고 `pms_notification_deliveries`(miy 추가)가 이 앱이 플랫폼에 보낸 알림(받는 사람·태스크·묶음 키·읽음)을 기억한다(§3.7).
- `pms_task_doc_links`는 없다(§1).
- FK 삭제 규칙: AI-DO ORM의 `cascade="all, delete-orphan"`을 `ON DELETE CASCADE`로 옮겼다(태스크의 댓글·이력·라벨·첨부·체크리스트·담당자·팔로워·값·부여, 리스트의 상태·마일스톤·라벨·태스크·템플릿·필드, 하위 태스크). `milestone_id`는 `SET NULL`.
- ID는 AI-DO와 같은 UUID v4 문자열, 시각은 naive UTC(`timestamp`). `0001_initial.sql`은 번들 보호 파일이라 그대로 두고 0002를 더했다(플래너와 같다).

### 3.2 권한

- AI-DO `access.py`의 규칙을 그대로 옮겼다: 활성 스페이스의 직접 멤버만 스페이스·리스트·태스크를 읽는다. viewer는 읽기만, member 이상은 태스크 쓰기, owner·admin은 리스트·스페이스 관리, 보관된 리스트는 (보관 해제 외) 읽기 전용, 단일 태스크 부여는 그 태스크 읽기만.
- AI-DO는 `resolve_space_access`에서 앱 관리자가 남의 스페이스 데이터를 보지 않게 했다. miy에서도 `Identity::admin`을 읽지 않는다. 플랫폼 관리자 표시는 아무것도 우회하지 않는다(테스트 `any_user_creates_a_space_and_owns_it_others_need_membership`).
- AI-DO는 멤버로 추가할 사람이 PMS 앱 접근 권한을 가졌는지(`platform.app_access_required`)도 봤다. 앱은 다른 사람의 앱 사용 권한을 조회할 수 없다. 디렉터리에 있는 활성 사용자면 추가하고, 그 사람이 앱을 열 수 있는지는 플랫폼이 앱을 열 때 판단한다. 알림도 플랫폼이 사용 권한 없는 사람을 뺀다. (플랫폼 변경 후보, §9)
- 스페이스 생성은 AI-DO PMS 카탈로그의 `space_creation_role="user"` 그대로 누구나 할 수 있다. AI-DO의 앱 관리자 전용 키 지정·다중 스페이스 설정 검사는 PMS에 해당 없음.
- 담당자·팔로워 검증은 AI-DO처럼 스페이스 멤버인지로 한다. AI-DO는 사용자 행이 있는지도 봤는데, 멤버는 추가할 때 디렉터리로 확인했으므로 쓰기 때마다 디렉터리를 부르지 않는다.
- 플랫폼 호출(`identity.context.platform`)만 받는 엔드포인트는 그 밖의 호출에 403 `pms_platform_only`.

### 3.3 사람과 이름

AI-DO는 응답마다 `users` 표의 `full_name`·주 조직을 조인했다. miy에서 사람 정보는 코어에 있으므로 플랫폼 디렉터리(`platform:directory:read`)로 읽는다(`users.rs`).

- 요청의 신원으로 ID 묶음(최대 100개씩)을 조회하고, 결과를 설치·환경·사용자 ID별로 5분간 메모리에 둔다. 목록 응답 한 번에 디렉터리 호출은 보통 한 번 이하다.
- 디렉터리가 답하지 못하거나 사용자가 없으면(비활성·삭제) 이름 자리에 사용자 ID를 넣는다. AI-DO 자신의 대체값(`assignee_names`의 `or task.assignee_id`)과 같다. 읽기 요청은 실패시키지 않는다. 멤버 추가처럼 사람이 있어야 하는 쓰기만 503 `pms_directory_unavailable` 또는 404 `auth_user_not_found`로 답한다.
- `email`은 디렉터리에 없어 빈 문자열이다. `SpaceMemberItem.is_admin`은 항상 `false`(앱은 다른 사람의 관리자 표시를 받지 않고, 아무 권한도 주지 않는 표시였다).
- 현재 사용자: AI-DO 화면은 인증 문맥(`useAuth`)에서 사용자를 읽었다. miy 프레임에는 사용자가 주어지지 않으므로 `GET /v1/pms/me`(`{id, name, primary_org_unit_id, primary_org_unit_name}`)를 더했다.
- `GET /v1/pms/users`: AI-DO는 PMS 접근 권한이 있는 활성 사용자 전부를 줬다. miy는 디렉터리 활성 사용자를 커서로 끝까지(최대 5,000명) 읽어 같은 모양으로 준다. 부서 초대는 화면이 이 목록을 주 조직으로 거른다(디렉터리에 그룹 구성원 조회가 없다).
- 활동 이력 문장의 사람 이름(`"{name} created KEY-1."`)은 쓰는 순간 디렉터리 이름이다(AI-DO도 그때의 `full_name`).

### 3.4 단일 태스크 공유

AI-DO의 `pms_task_user_access`는 회의 도메인이 참석자에게 부여했다(PMS REST로 만드는 경로는 없었다). 표와 읽기 규칙(취소되지 않고 만료 전인 부여 → 그 태스크와 활동 이력·사용자 정의 필드 값·첨부 내려받기 읽기, 리스트·쓰기 불가)을 그대로 두었다. 회의 앱이 없어 지금은 부여를 만드는 곳이 없다(§8).

### 3.5 오류 응답과 입력 검증

- 플래너와 같다: `{"error":{"code":"pms_task_not_found","message":"Task not found."}}`. 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것, `message`는 AI-DO 서버 카탈로그의 영어 문장(매개변수 `{count}`·`{statuses}`·`{limit_mb}`는 채움). 화면은 코드를 AI-DO 카탈로그의 한국어·영어 문장으로 번역한다.
- 요청 검증 실패는 FastAPI처럼 422 `validation_request_invalid`. Pydantic 규칙(길이는 문자 수, `extra="forbid"`, `Literal`, `ge`/`le`, 목록 길이, `str_strip_whitespace`)을 `input.rs`로 재현했다. 보낸 필드와 `null`로 보낸 필드, 보내지 않은 필드를 구분한다(`model_fields_set`). 질의의 반복 키(`status=a&status=b`)는 목록이다.
- miy에서 생긴 코드: `pms_platform_only`, `pms_directory_unavailable`, `pms_attachment_upload_failed`, `pms_attachment_not_pending`, `pms_attachment_expired`, `pms_attachment_request_conflict`, `pms_task_list_key_exists`(AI-DO는 리스트 키 유일 위반이 500으로 끝났다), `file_upload_in_progress`.

### 3.6 첨부

AI-DO는 multipart 한 번으로 객체 저장소(최대 50 MiB)에 넣고 서명 URL을 줬다. miy 작은 첨부(`platform:files:*`)는 바이트가 코어에 있고 앱 서버만 다룰 수 있으며 파일당 10 MiB다. CONTRACT "Small attachments"의 순서를 따른다.

1. `POST /v1/pms/tasks/{id}/attachments` `{request_id, filename, content_type, size_bytes}`: 쓰기 권한 확인 → 대기 기록(`pending`, 요청 ID 포함)을 먼저 커밋 → 같은 요청 ID로 `files_create`(응답을 잃으면 `files_by_request`). 같은 요청 ID를 다시 보내면 같은 첨부를 준다.
2. `PUT /files/pms/attachments/{id}/content`(프레임의 `runtime.fileApi`): `files_upload` → 첨부를 `attached`로 바꾸고 활동 이력을 남긴 뒤 커밋 → `files_attach(file, "pms_attachment:<id>", attach_request_id)`. 연결 요청 ID가 고정이라 다시 불러도 같은 결과다.
3. `GET /files/pms/attachments/{id}/content`: 매번 태스크 읽기 권한을 확인하고 `files_download`. 응답 `AttachmentItem.download_url`은 이 앱 기준 경로다.
4. `DELETE /v1/pms/attachments/{id}`, 태스크·리스트 삭제: 앱 참조(행)를 지우고 같은 트랜잭션에서 `pms_attachment_releases`에 넣은 뒤, 커밋 후 연결 해제(`files_detach`, 아직 연결 전이면 `files_cancel`). 실패한 것은 남겨 다음 삭제 요청이 다시 시도한다(요청마다 최대 20개). 백그라운드 작업자가 없어서다.

크기 초과는 413 `pms_file_size_limit_exceeded`(`limit_mb` 10). 상세의 `attachments`에는 `attached`만 보인다.

### 3.7 알림

AI-DO는 앱 안 알림 행(영어 제목 + 근태 앱용 한글 표시 값)을 만들었다. miy는 같은 계기에 플랫폼 알림(`Identity::notify`)을 보낸다.

| 계기(AI-DO) | AI-DO 종류 | miy `kind` | 받는 사람 |
| --- | --- | --- | --- |
| 태스크 수정에서 담당자 필드를 보냄 | `assigned` | `assigned` | 현재 담당자 전원(행위자 제외) |
| 상태가 바뀜 | `status_changed` | `status` | 담당자·팔로워·보고자(행위자 제외) |
| 댓글 | `commented` | `comment` | 담당자·팔로워·보고자(행위자 제외) |
| 댓글 멘션(`@<uuid>` 본문, 블록의 mention) | `mentioned` | `mention` | 위에서 알림받지 않은 멘션 대상(행위자 제외) |

- 묶음 키는 태스크마다 `task:<id>`라 한 사람에게 태스크 하나당 한 줄이 남고 새 알림이 덮어쓴다. 목적지는 `/tasks/<id>`, `resourceId`는 태스크 ID, 요청 ID는 알림마다 새 UUID.
- 제목은 AI-DO 근태 표시표(`mobile_push_display.NOTIFICATION_TITLES`, `task_status_label_ko`)를 따른 한국어 `"<행위자> · 업무 배정 · <태스크>"`, 상태 변경은 끝에 `· <새 상태>`. 댓글 본문은 넣지 않는다(계약). 알림은 받는 사람별 언어가 없어 포털 기본 언어인 한국어로만 만든다.
- AI-DO는 태스크를 읽을 수 없는 사람(목록 밖에서 멘션된 사람 등)에게도 제목만 있는 알림을 남겼다. miy 플랫폼은 패널을 열 때 `notificationCheck`로 다시 확인해 그런 줄을 바로 숨기므로, 지금 그 태스크를 읽을 수 있는 사람에게만 보낸다(`access::readers_among`).
- 발행은 업무 커밋 뒤에 하고 실패해도 업무를 되돌리지 않는다. 실패는 로그만 남기고 다시 보내지 않는다(서비스 주체 없음, 계약 2단계).
- 회수: 태스크(하위 포함)·리스트·스페이스 삭제 때 그 태스크의 묶음 키를 전원에게서, 멤버 제외·나가기 때 그 스페이스 태스크의 묶음 키를 그 사람에게서 회수한다. `pms_notification_deliveries`로 실제 보낸 키만 회수한다(요청당 최대 200개, 나머지는 재확인이 숨긴다).
- 읽음: 태스크 상세를 열면(`GET /v1/pms/tasks/{id}`) 그 사람의 안 읽은 전달 기록이 있을 때만 `mark_notifications_read("task:<id>")`.
- `notificationCheck`(`POST /platform/notification-check`): 묶음 키 중 지금 읽을 수 있는 태스크(활성 스페이스의 멤버이거나 유효한 단일 부여)만 `allowed`로 답한다. 읽기 전용, 한 번에 최대 1,000개.
- 일괄 변경의 담당자 변경은 AI-DO처럼 알림을 만들지 않는다. AI-DO처럼 태스크 생성 때 담당자에게도 알림이 가지 않는다(AI-DO는 수정에서만 `assigned`). 담당자 필드를 보내면 바뀌지 않은 담당자에게도 다시 보낸다(AI-DO 동작, 같은 묶음 키라 줄 하나가 갱신된다).

### 3.8 홈 카드·통합 검색(플랫폼 호출)

- 홈 카드 "내 업무"(`POST /v1/home-card`): 본문 `timeZone`(없거나 모르면 `Asia/Seoul`)의 오늘을 기준으로 AI-DO 오늘 및 지연과 같은 집합(내게 배정, 보관 아님, 활성 리스트, 완료·종료 상태 제외, 마감 ≤ 오늘)을 마감일·최근 수정 순으로 고른다. `title`은 `"내 업무 · 오늘 N · 지연 M"`(영어 `My tasks · today N · overdue M`), `count`는 합계, 항목은 최대 5개(`subtitle` `"KEY-12 · 오늘 마감"`/`"N일 지연"`, `path` `/tasks/<id>`). AI-DO 위젯의 오늘 일정이 아니라 계획 M5의 "내 업무" 요약이다.
- 통합 검색(`POST /v1/search`, 새 0.12.0 CONTRACT의 `platformEndpoints.search`): 읽을 수 있는 태스크 중 제목이나 번호(`KEY-12`)가 맞는 것을 최근 수정순으로 최대 `limit`개(1~10). `LIKE` 패턴의 `% _ \`는 이스케이프한다. 항목 `{title, snippet:"KEY-12 · 리스트", path, at, kind:"업무"}`. AI-DO 검색 투영(M12)은 옮기지 않았다.

### 3.9 그 밖의 서버 동작

- `updated_at`은 SQLAlchemy `onupdate`처럼 태스크 행 열이 실제로 바뀐 수정에서만 갱신한다(라벨·팔로워 연결만 바뀌면 그대로). 이름순 저장은 AI-DO처럼 `updated_at`을 유지한다.
- 수정 이력의 `from_value`/`to_value`는 Python `str()`과 같은 모양(`True`/`False`, `2026-11-22`)이다.
- 이름순 정렬 키(`_task_title_order_key`: 소문자, 숫자 묶음을 "자릿수 3자리 + 앞 0을 뗀 숫자"로, `COLLATE "C"`)를 Rust로 같은 규칙으로 계산한다. 검색어가 있는 목록의 정렬은 AI-DO처럼 읽은 뒤 Python 정렬 규칙(안정 정렬, 날짜 없는 행은 뒤)으로 한다.
- 진행률은 Python `round(x, 2)`와 같게 반올림한다.
- 리스트 키 자동 생성(`_auto_key_from_name`)은 AI-DO 정규식의 실제 동작(원시 문자열이라 공백도 지워진다)까지 재현했다. 한글 이름은 `LS`, `LS1`, …
- 리스트·폴더를 요청의 워크스페이스로 대신 만드는 AI-DO 분기(`team_id` 생략 시 현재 app_space)는 miy에 워크스페이스 문맥이 없어 409 `pms_task_list_space_missing`이다. 화면은 항상 `team_id`를 보낸다.
- 엑셀은 AI-DO와 같은 시트·열·한글 값·트리 순서·들여쓰기·날짜 형식·열 너비 규칙·고정 행·필터이며 텍스트 셀은 수식이 되지 않게 문자열로 쓴다. 동아시아 너비는 `unicode-width`로 계산한다(Python `east_asian_width` F/W와 같은 2칸). 파일 이름은 같은 규칙의 `Content-Disposition`으로도 주지만 `runtime.apiBlob`은 헤더를 주지 않아 화면이 같은 규칙으로 만든다(§5.7).
- 개인 위젯 배정 목록 `GET /v1/personal-widgets/pms/tasks/assigned`는 AI-DO 경로·모양대로 두었다. 화면은 이 경로 대신 스페이스·리스트·배정 목록을 조합한다(§5.1).

## 4. 화면 파일 대응

화면은 AI-DO `apps/web/src/<경로>` → `web/src/<경로>`로 같은 상대 경로에 두었다. "그대로"는 import 경로 외 변경이 없다는 뜻이다(§5.10). 상태는 스크립트로 원본과 비교해 분류했다.

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/pms/api/pms-filters.ts`, `pms-permissions.ts`, `pms-sidebar-reorder.ts`, `test-fixtures.ts` | 같은 경로 | 그대로 |
| `app-modules/pms/api/pms-api.ts` | 같은 경로 | 수정: `runtime.api`, 앱 기준 경로·ID 인코딩, `token`→`api`, 첨부(파일 API), 엑셀(`apiBlob`·파일 이름 계산), 개인 위젯 배정 목록 조합, 문서·알림 함수 제거 |
| `@ai-do/contracts` `openapi.generated.d.ts`의 PMS 스키마 | `app-modules/pms/api/pms-api-types.ts` | 생성: pms-api.ts가 이름 붙인 48개 스키마 원문 |
| `sidebar/FolderContextMenu`, `ListContextMenu`, `SortableListLink`, `SpaceContextMenu`, `space-tree-drag`, `space-tree-floating` | 같은 경로 | 그대로 |
| `sidebar/PmsSidebarSpaces`, `SpaceTree`, `SpaceOrderEditorModal`, `SpaceAddPopover`, `FolderAddPopover`, `pms-sidebar-scope`, `space-tree-model`, `space-order-editor-model`, `space-order-persistence` | 같은 경로 | 수정: 문서 제거(순서 편집기 본문은 리스트 한 열), `useAuth`→현재 사용자·`api` |
| `views/BoardView`, `BulkDatesForm`, `FilterBar`, `GanttView`, `InlineSaveError`, `ListAssigneeMenu`, `ListView`, `MemberSuggestionDropdown`, `PmsCenteredStateBlock`, `SpaceDepartmentInvite`, `StatusIcon`, `TableView`, `TaskAssigneeStack`, `TaskCard`, `TaskDetailFields`, `TaskDetailModal`, `TaskSelectAllCheckbox`, `UserRolePicker`, `usePmsTaskListChangeSubscription` | 같은 경로 | 그대로 |
| `views/*-model.ts` 중 `bulk-action-bar`, `calendar-view`, `filter-bar`, `gantt-timeline`, `gantt-view`, `list-view-editing`, `list-view-grouping`, `pms-color-palettes`, `pms-constants`, `pms-create-modal`, `pms-task-context`, `pms-task-hierarchy`, `pms-task-list-catalog`, `pms-task-selection-workflow`, `pms-view-scoped-state`, `space-members-modal`, `space-tasks-layout`, `task-copy`, `task-detail-editing`, `task-detail-mutation`, `task-list-members`, `task-list-presentation`, `task-list-workflow-settings`, `task-picker`, `today-overdue` | 같은 경로 | 그대로 |
| `views/CalendarView` | 같은 경로 | 수정: FullCalendar 로캘이 포털 언어를 따름(`locale={i18n.language}`, 플래너와 같은 결정. AI-DO는 항상 한국어) |
| `views/PMSView` | 같은 경로 | 수정: `/`는 항상 전체 개요, Whiteboard 탭·문서/화이트보드 경로 제거, 할 일 전환 이벤트 제거, 엑셀에 `apiBlob` |
| `views/TaskDetail`, `TaskDetailActivityPanel`, `useTaskDetailIssueEditing`, `useTaskDetailAttachments`, `useTaskDetailResources`, `task-detail-resources-model` | 같은 경로 | 수정: 문서·녹음·미디어 제거, 첨부 미리보기·내려받기는 파일 API, 미저장 보고 |
| `views/NewTaskModal`, `new-task-modal-model` | 같은 경로 | 수정: 미디어 제거, 미저장 보고(`isNewTaskDraftDirty` 추가) |
| `views/FloatingPmsWidget` | 같은 경로 | 수정: 포털 이벤트·바닥 버튼·도크 배지 훅 제거, 스페이스별 리스트는 `team_id` |
| `views/PmsGlobalOverview` | 같은 경로 | 수정: 앱 우선 런타임 대신 id 경로, 스페이스 생성은 항상 제안(서버가 판단) |
| `views/PmsResourceTaskView` | 같은 경로 | 재구성: `/tasks/:taskId` 진입점(§5.2) |
| `views/PmsSpaceToolTabs`, `pms-view-route`, `pms-events`, `SpaceOverviewView`, `space-overview-model` | 같은 경로 | 수정: 문서·화이트보드 제거, 경로 빌더는 앱 기준 경로 |
| `views/CreateSpaceModal`, `CreateFolderModal`, `CreateTaskListModal`, `TaskListSettingsPanel` | 같은 경로 | 수정: 미저장 보고, 카탈로그 갱신 제거(스페이스), 시간대는 표시 설정 |
| `views/AssignedToMeView`, `BulkActionBar`, `SpaceMembersModal`, `SpaceTasksView`, `TaskCopyDialog`, `TaskPickerModal`, `TodayOverdueView`, `usePmsSpacePreferences`, `usePmsTaskListGroupPreference`, `useTaskDetailChecklist`, `useTaskDetailComments`, `useTaskDetailSubtasks`, `useTaskList{Labels,Members,WorkflowStatuses}Controller` | 같은 경로 | 수정: `useAuth`→`useAppApi`/`usePmsCurrentUser`, `token`→`api`만(TaskPickerModal은 고정 리스트 공간 검사, TodayOverdueView는 시간대, TaskCopyDialog는 접근 조건) |
| `components/common/NoAccessNotice`, `components/form/FormDialog`, `components/picker/ResourcePickerDialog`, `platform/users/UserSearchMultiSelect`, `user-option-picker-model`, `platform/network/request-concurrency`, `platform/ordering/ordered-reorder`, `platform/pickers/picker-model`, `platform/format/byte-size`, `platform/app-runtime/AppRuntimeNotice` | 같은 경로 | 그대로 |
| `packages/ui/src/lib/editor/{block-editor,block-viewer,blocknote-interaction-policy,types,use-theme}`, `custom-blocks/*`, `custom-inline/*` | `components/editor/` | 그대로(공용 UI 2.0.0에 편집기가 빠져 앱이 가진다) |
| `packages/ui/src/lib/editor/{index,schema}.ts` | `components/editor/` | 축소: PMS가 쓰는 편집기·보기·`fullSchema`만 |
| `platform/app-runtime/index.ts`, `use-app-target-spaces.ts` | 같은 경로 | 재구성: §5.3 |
| `platform/auth/auth-api.ts` | 같은 경로 | 축소: `teamRoleAllows`(원문), `hasWorkspaceMembership`(PMS 사용자면 참) |
| `platform/workspaces/workspace-utils.ts` | 같은 경로 | 재구성: 앱 기준 경로 반환 |
| `packages/core-web` `buildAppRoutePath` | `platform/core-web/index.ts` | 재구성: 앱 우선 분기용 자리(닿지 않음), 앱 기준 경로 반환 |
| `react-router-dom` | `platform/router/index.tsx` | 신규: `useNavigate`·`useLocation`·`useSearchParams`·`useParams`·`Link`·`Navigate`를 런타임 위에 |
| `platform/auth/auth-provider`의 `useAuth` | `platform/auth/current-user.tsx` | 신규: `GET /v1/pms/me`, `usePmsCurrentUser` |
| AI-DO 셸 SubSidebar + `sidebar/config.tsx` + `shell-nav.ts` | `app-modules/pms/shell/PmsAppSidebar.tsx` | 신규: 앱 왼쪽 열(같은 클래스) |
| `TaskDetail`의 `<img src={download_url}>` | `views/PmsAttachmentThumbnail.tsx` | 신규: 파일 API blob → object URL |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: [scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py) |
| (없음) | `AppRoot.tsx`, `platform/runtime/app-location.ts`, `styles/pms-frame.css`, `test-utils/runtime.tsx` | miy 연결 |

## 5. 화면: 원본과 달라진 점과 이유

### 5.1 API 전송

- AI-DO는 `fetch` + bearer 토큰으로 `/api/v1/pms/...`(앱 우선 런타임은 `/api/v1/workspaces/<slug>/pms/...`로 바꿔 씀)를 불렀다. miy는 `runtime.api`에 앱 기준 경로 `/v1/pms/...`를 넘기고 게이트웨이가 Rust `/api/v1/pms/...`로 보낸다. 경로 안의 ID는 `encodeURIComponent`로 감쌌다.
- `pms-api.ts`의 함수 이름·인자 순서는 유지했다. 첫 인자 `token: string`은 `api: AppApi`(runtime.api)가 되었고, 화면 코드의 `token` 변수도 모두 `api`로 바꿨다. `workspaceSlug` 인자는 남겨 두고 무시한다(호출부를 바꾸지 않기 위해). 요청 본문·응답은 AI-DO JSON 그대로다(`pms-api-types.ts`).
- 오류: `runtime.api`는 오류 본문 중 코드만 남긴다. 서버는 AI-DO 코드의 `.`을 `_`로 바꾼 코드(`pms_task_not_found`)로 답하고, 화면은 AI-DO **서버** 카탈로그(`core/i18n_catalog.py`)의 한국어·영어 문장을 `errors` 네임스페이스로 보인다(`pms.*` 59개 + `auth.user_not_found`, `platform.app_access_required`, `validation.request_invalid`). 매개변수 문장: `pms.file_size_limit_exceeded`는 상수 10을 넣고, `pms.status_in_use`(`{count}`)·`pms.status_mode_incompatible`(`{statuses}`)는 값 부분을 뺀 문장으로 보인다(플래너와 같은 처리). 모르는 코드·네트워크 오류는 플래너와 같이 `common:feedback.requestFailed`/`requestFailedRetry`.
- 개인 위젯 배정 목록(`listPersonalPmsAssignedTasks`): AI-DO는 `/api/v1/personal-widgets/pms/tasks/assigned`(모든 PMS 스페이스의 진행 중 배정 태스크 + 스페이스)를 썼다. miy는 같은 응답을 `/v1/pms/spaces` + `/v1/pms/lists`(전체 쪽) + `/v1/pms/tasks/assigned`(전체 쪽)로 화면에서 조합한다. 스페이스 참조의 `slug` 자리에는 스페이스 id를 넣어 위젯이 그 스페이스 리스트를 `team_id`로 읽게 했다. 새 서버 경로를 만들지 않기 위해서다.

### 5.2 화면 경로와 이동

- react-router 대신 `platform/router`(같은 이름의 훅·컴포넌트)를 두어 화면 코드의 import만 바꿨다. 위치는 `runtime.location`, 이동은 `runtime.navigate`이며 history API를 쓰지 않는다. `navigate(-1)`은 무시한다(기록은 포털 것). `Link`는 `href`에 앱 기준 경로를 두고 클릭을 가로챈다. 가운데 클릭은 프레임 출처에 그 경로의 화면이 없어 막았다.
- 경로: AI-DO의 `/w/:workspaceSlug/pms` 아래 경로에서 앞부분을 뗐다. `/`(전체 개요), `/assigned`, `/today`, `/spaces/:spaceId`, `/lists/:taskListId`(`?task=`, `?tab=`, `?settings=1`, `?create=1` 등 AI-DO 질의 그대로), `/tasks/:taskId`, `/widgets/my-tasks`. 알 수 없는 경로는 개요다.
- `/`: AI-DO 워크스페이스 경로는 첫 스페이스로 보냈고, 앱 우선 런타임의 `/pms`(`entryBehavior: 'overview'`)는 `PmsGlobalOverview`를 보였다. miy는 설치마다 PMS 하나라 항상 전체 개요를 보인다.
- `/tasks/:taskId`(알림·홈 카드 목적지, miy 추가): 태스크를 읽고 그 리스트(`GET /v1/pms/lists/{id}`)를 열 수 있으면 `/lists/<listId>?task=<taskId>`로 바꿔 이동(replace)하고, 아니면 AI-DO 단일 태스크 공유 보기(`PmsResourceTaskView`, 읽기 전용 상세)를 보인다.
- `/widgets/my-tasks`(보조 패널, 약 400px): `FloatingPmsWidget` 내용만 보인다. 태스크를 누르면 위젯이 하던 대로 같은 프레임 안에서 상세 창(`TaskDetailModal`, `document.body` 포털)을 연다. 좁은 프레임에서는 AI-DO의 `lg:` 미만 배치라 화면 전체를 덮는다. 패널을 `/tasks/:id`로 이동시키지 않은 이유: 위젯의 원래 동작이고 패널을 떠나지 않아 목록으로 바로 돌아올 수 있다.
- 앱 왼쪽 열(`PmsAppSidebar`): AI-DO는 포털 셸의 보조 사이드바에 PMS 항목을 그렸다. miy에서는 프레임이 작업 영역 전체라 앱이 같은 클래스(`sidebar-submenu-*`, `bg-app-surface-sidebar`)로 그린다. 순서는 AI-DO와 같이 Spaces(`PmsSidebarSpaces`) 다음 Personal("My Tasks" 묶음: 내게 배정됨·오늘 및 지연). 맨 위에 개요 링크("PMS 전체", AI-DO 스페이스 전환기의 `appRuntime.appOverview` 문구)를 더했다. 활성 표시는 AI-DO `pmsShellNavResolver` 규칙을 앱 기준 경로에 적용한다. "My Tasks"는 AI-DO가 서버 제목을 번역 없이 보이던 그대로 두 언어 모두 영어다. Inbox(`pms-inbox`)는 알림 제외로 뺐다. 너비는 AI-DO 기본(256px)으로 고정하고 크기 조절·고정 버튼·모바일 메뉴는 옮기지 않았다. `md`(768px) 이상에서 보이고 그보다 좁으면 숨긴다(AI-DO는 `lg`였지만 포털 내비가 옆에 있어 프레임 폭이 1024px보다 좁은 경우가 많다).
- 미저장 보고(miy 계약): 태스크 상세(바뀐 제목, 지연 저장 중인 설명, 댓글 초안, 체크리스트 입력·편집, 하위 태스크 입력, 업로드), 새 태스크 창(`isNewTaskDraftDirty`), 스페이스·폴더·리스트 생성 창(입력·생성 중), 리스트 설정(라벨·상태 입력·편집)을 플래너의 `useNavigationDirty`로 합쳐 `runtime.setNavigationState`에 보고한다. 문구는 `apps:pms.unsavedChanges`. AI-DO에는 이동 확인이 없었다.

### 5.3 런타임·스페이스 모델

- AI-DO는 PMS를 앱 우선 런타임(`/pms`, `/pms/s/:spaceKey`, 플랫폼 스페이스마다 런타임)과 워크스페이스 경로(`useOptionalAppRuntime()`이 null) 두 가지로 돌렸다. miy 경로(id로 스페이스·리스트를 가리킴)는 워크스페이스 경로 모양이라 `useOptionalAppRuntime()`은 null을 돌려주고 화면은 런타임 없는 분기를 탄다. 앱 우선 분기 코드는 원본대로 두었다(닿지 않음). 그 분기의 `buildAppRoutePath`는 `platform/core-web`의 자리 함수가 앱 기준 경로를 돌려준다.
- `workspaceSlug`: AI-DO 화면은 워크스페이스 키를 곳곳에서 넘기고 "선택된 워크스페이스가 있는지"를 검사한다. miy는 상수 `PMS_WORKSPACE_KEY`(`'pms'`)를 `useParams()`로 준다. API는 이 값을 무시하고 경로 빌더는 떼어 낸다.
- `useAppTargetSpaces('pms')`(태스크 복사·태스크 선택): AI-DO 플랫폼 앱 스페이스 목록 대신 `GET /v1/pms/spaces`. 첫 선택지는 PMS 전체(`app_global`, 키 `pms`), 다음은 스페이스(`workspace_key`, `current_user_role`). 뷰어는 `includeViewers`일 때만.
- 태스크 선택 창(`TaskPickerModal`)은 AI-DO에서 고정 리스트(상위 태스크 선택)일 때도 그 리스트의 소유 스페이스 키가 선택지에 있어야 했다. miy 화면이 넘기는 키는 PMS 상수라 스페이스 키와 맞지 않으므로, 고정 리스트는 스페이스 검사 없이 리스트 id로 읽는다(서버가 권한 검사).
- 스페이스 생성: AI-DO PMS는 모든 PMS 사용자가 스페이스를 만들 수 있었다(`space_creation_role="user"`). 화면은 항상 생성 버튼을 보이고 서버가 판단한다. 앱 관리자·시스템 역할은 miy 앱에 주어지지 않으며, AI-DO에서도 스페이스 멤버십을 우회하지 않았다.

### 5.4 현재 사용자와 시간대

- AI-DO `useAuth()`는 토큰과 사용자 프로필을 줬다. miy는 토큰이 없고, 화면이 쓰던 필드(`id`, `primary_org_unit_id`, `primary_org_unit_name`)를 앱의 `GET /v1/pms/me`(`{id, name, primary_org_unit_id, primary_org_unit_name}`)에서 한 번 읽는다(`PmsCurrentUserProvider`, 읽는 동안 `appRuntime.loading`, 실패 시 `appRuntime.loadFailed`+다시 시도). `useAuth()` 호출은 `useAppApi()`와 `usePmsCurrentUser()`로 바꿨다.
- `user.time_zone`(오늘 및 지연의 "오늘", 리스트 설정 날짜 표시)은 `runtime.settings.timeZone`. 날짜 형식·시간대 저장소는 플래너와 같이 `AppRoot`가 표시 설정을 넣는다.

### 5.5 편집기(BlockNote)

공용 UI 2.0.0은 AI-DO UI의 편집기를 뺐다. 태스크 설명·새 태스크 설명은 AI-DO처럼 BlockNote라 앱에 `components/editor`로 옮겼다(AI-DO 고정 버전 `@blocknote/core`·`react`·`mantine` 0.52.1, `@mantine/core`·`hooks` 8.3.18). 스키마는 AI-DO `fullSchema`(callout, divider, mention, task ref). 이미지 업로드 처리기(`uploadFile`/`resolveFileUrl`)는 넘기지 않아 편집기에 업로드 메뉴가 없다(§1). BlockNote의 하위 의존성(prosemirror 등)은 이 앱 lockfile에서 새로 풀렸고 AI-DO lockfile과 같은지는 대조하지 않았다.

### 5.6 첨부

- 올리기: AI-DO는 multipart 한 번이었다. miy는 `POST /v1/pms/tasks/{id}/attachments`(JSON `{request_id, filename, content_type, size_bytes}`, 201 `status: "pending"`) → `runtime.fileApi('/pms/attachments/{id}/content', PUT, application/octet-stream)`가 완성된 `AttachmentItem`을 돌려준다. 두 번째 단계가 실패하거나 취소되면 `DELETE /v1/pms/attachments/{id}`로 대기 기록을 지운다. 10 MiB를 넘는 파일은 요청 전에 `errors:pms_file_size_limit_exceeded` 문장으로 거부한다.
- 미리보기·내려받기: AI-DO는 서명 URL(`download_url`)을 `<img src>`·새 탭 링크로 열었다. miy는 파일 API로 blob을 읽어 object URL로 보이고 쓰고 나면 해제한다(`PmsAttachmentThumbnail`, `downloadAttachment`). 내려받기 링크는 같은 클래스의 버튼이 되었다. 실패하면 상세의 저장 오류 자리에 문장을 보인다(`apps:pms.attachmentPreviewFailed` 또는 서버 코드 문장).

### 5.7 엑셀 내보내기

`runtime.apiBlob('/v1/pms/lists/{id}/export?...')`(AI-DO와 같은 질의). blob에는 헤더가 없어 파일 이름을 AI-DO 서버(`task_export.content_disposition`)와 같은 규칙으로 화면에서 만든다: 리스트 이름에서 `\ / : * ? " < > |`를 공백으로 바꾸고 다듬은 값(없으면 리스트 키) + 필터가 있거나 검색어가 공백이 아니면 ` (필터 적용)` + `.xlsx`. 리스트 이름은 내보내기와 함께 `GET /v1/pms/lists/{id}`로 읽는다(AI-DO 서버도 내보낼 때의 이름을 썼다). 내려받기는 object URL.

### 5.8 문구(i18n)

`scripts/gen-i18n-resources.py`가 AI-DO `resources.ts`에서 `common`, `shell.searchSpace`, `shell.appRuntime`, PMS 셸 내비 제목 두 줄(`nav.pms-tasks-assigned`, `nav.pms-tasks-today`), `apps.pms` 전체를 한국어·영어 원문 그대로 옮기고, `errors`는 AI-DO 서버 카탈로그 Python 모듈을 읽어 만든다. miy 추가 키: `apps.pms.unsavedChanges`, `sidebarLabel`(왼쪽 열 이름), `attachmentPreviewFailed`. 블록 시작 줄은 0002af062 기준이며 어긋나면 assert로 멈춘다. 문서·화이트보드 문구 키는 원문 블록에 남아 있지만 쓰지 않는다.

### 5.9 스타일

- 공용 UI 2.0.0 `theme.css`·`app.css`(AI-DO 앱 공통 클래스)를 `app.css`에서 import하고, `fullcalendar-theme.css`(PMS 캘린더 보기)와 `pms-frame.css`를 더했다.
- `pms-frame.css`: 프레임 기본 스타일의 `a { color: brand }`는 Tailwind 계층 밖이라 preflight의 `color: inherit`뿐 아니라 링크에 붙은 색 유틸리티까지 이긴다. PMS는 사이드바·개요 카드·태스크 행을 링크로 그리므로, 계층 밖 규칙 `:root a { color: revert-layer; text-decoration: revert-layer }`로 계층 안(preflight·유틸리티) 값으로 되돌린다. FullCalendar 링크 규칙은 플래너 것을 함께 둔다.
- AI-DO 셸이 앱 화면을 `LazyMotion features={domAnimation}`으로 감쌌으므로 `AppRoot`도 같게 감쌌다(PMS의 `m.*` 애니메이션). 알림 토스트는 AI-DO 앱 루트와 같은 `FeedbackProvider` 문구로 둔다.

### 5.10 그 밖의 변경

- **import 경로:** `@/src/…` 별칭을 상대 경로로 바꿨다(보호 파일인 미리보기 빌드 설정이 `@miy/*`만 안다). `@ai-do/ui`(하위 경로 포함)는 `@miy/ui`, `BlockEditor`·`BlockViewer`·`BlockContent`만 `components/editor`. `react-router-dom`→`platform/router`, `@ai-do/core-web`→`platform/core-web`. 스크립트로 일괄 변환했다.
- **위젯 리스트 읽기:** AI-DO 위젯은 스페이스 워크스페이스 경로로 그 스페이스 리스트만 읽었다. miy는 `listAllPmsTaskLists(api, <스페이스 id>)`로 `team_id` 필터를 건다.
- **캘린더 로캘:** PMS 캘린더 보기의 FullCalendar도 플래너처럼 포털 언어를 따른다. 공휴일 이름은 데이터가 한국어라 영어 화면에서도 한국어다.
- **스페이스 생성 뒤:** AI-DO는 포털 앱·스페이스 카탈로그를 다시 읽었다(`reloadGlobalApps`, 실패 시 경고 토스트). miy에는 그 카탈로그가 없어 뺐다.

## 6. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 스페이스 멤버십은 SDK ACL(`sdk/rust/src/acl.rs`)이 아니라 AI-DO 모델(`pms_space_members`)로 옮김 | SDK ACL은 만든 사람을 항상 관리자로 고정하고(`acl_owner_protected`) 권한 목록을 통째로 바꾸는(`replace_in`) 방식이며 자원당 100개까지다. AI-DO PMS는 owner를 넘길 수 있고 마지막 owner만 지키며, owner만 admin·owner를 다루고, 멤버를 한 명씩·부서 단위(100명 넘게)로 추가·제외하는 REST였다. 그룹 권한도 없었다(부서 초대는 사람으로 펼침). 규칙을 그대로 지키려면 AI-DO 모델이 맞다. ACL 검사마다 디렉터리 호출이 드는 점도 목록 화면에 부담이었다 |
| 단일 태스크 공유는 AI-DO `pms_task_user_access` 표와 읽기 규칙 | 위와 같은 이유로 SDK ACL을 쓰지 않았다. 그 태스크 읽기만 준다 |
| 이름은 디렉터리에서 요청마다(5분 메모리 보관) | 사람 정보는 코어에 있고 앱 DB에 사본을 두지 않는다(계획 §1). 실패해도 읽기는 ID로 계속 |
| 알림은 지금 읽을 수 있는 사람에게만, 한국어 제목 | 플랫폼 재확인이 읽을 수 없는 줄을 숨기고, 알림에 받는 사람별 언어가 없다 |
| 보낸 알림을 `pms_notification_deliveries`에 기록 | 멤버 제외·태스크 삭제 때 실제 보낸 묶음 키만 회수하려고 |
| 첨부 해제 대기열(`pms_attachment_releases`) | 계약상 참조 삭제를 먼저 커밋하고 연결 해제는 확인될 때까지 다시 해야 하는데 백그라운드 작업자가 없다 |
| 일괄 변경·복사·이름순 저장·내보내기는 요청 안에서 실행 | AI-DO의 파일 처리 레인은 공용 스레드 보호용이었다. Axum은 비동기라 같은 문제가 없다 |
| 엑셀은 rust_xlsxwriter 0.103(기본 기능 끔, chrono만) | openpyxl과 같은 서식(굵은 머리, 들여쓰기, 줄바꿈, 숫자 형식, 열 너비, 고정, 자동 필터)을 낼 수 있는 순수 Rust 구현 |
| DB 테스트는 `#[ignore]` + `PMS_TEST_DATABASE_URL`, 가짜 코어 | 서버 빌드의 `cargo test`에는 DB가 없다. 디렉터리·알림·파일은 SDK가 실제로 부르는 경로를 가짜 코어가 받아 SDK 호출까지 확인한다 |
| `token` 인자를 `api`(runtime.api)로 바꾸고 `workspaceSlug` 인자는 남겨 무시 | 전송 수단이 바뀌는 자리는 바꿔야 하고, 워크스페이스 인자는 수백 곳 호출부를 바꾸지 않고 둘 수 있다. 원본과 비교하기 쉽다 |
| react-router를 더하지 않고 같은 이름의 얇은 대체 모듈 | 브리프와 플래너 결정(라우터 라이브러리 없음). 화면 코드는 import만 바뀐다 |
| 런타임 없는(워크스페이스 경로) 분기 사용, 앱 우선 분기는 원본대로 둠 | miy 경로가 id 기반이라 그 분기와 맞는다. 원본 코드를 덜 건드린다 |
| 위젯 배정 목록은 화면 조합 | 브리프 계약 밖 서버 경로를 늘리지 않는다 |
| 편집기는 AI-DO 원본 파일을 앱에 복사 | 공용 UI에서 빠졌고 PMS 설명·멘션 데이터(`description_blocks`)가 BlockNote 형식이다(리드 지시) |
| 의존성은 플래너 고정 버전 + AI-DO의 `@dnd-kit/core` 6.3.1·`sortable` 10.0.0·`utilities` 3.2.2, BlockNote 0.52.1·Mantine 8.3.18 | AI-DO lockfile 해석 버전 |
| 테스트용 `test-utils/runtime.tsx` | AI-DO 스펙의 `MemoryRouter`·`useAuth` 모의를 런타임·현재 사용자 문맥으로 대체 |
| `platformEndpoints.search`도 구현 | 새 0.12.0 CONTRACT가 정한 계약이고 PMS에서는 읽을 수 있는 태스크의 제목 검색으로 간단하다(리드 요청) |

## 7. 테스트 대응

**서버** (`cargo test`: 단위 27개, `-- --include-ignored`로 DB·HTTP 22개 더, 2026-11-22 일회용 PostgreSQL 18.3 컨테이너에서 49개 통과)

| AI-DO | miy | 비고 |
| --- | --- | --- |
| `test_pms_workflow.py`, `test_pms_status_lifecycle.py` | `workflow.rs` 단위 9개 | 상태 분류·이름·진행률·종료 상태 하나·상속 호환·슬러그·키 생성·반올림 |
| `test_pms_space_membership.py`, `test_pms_app_workspaces.py`, `test_pms_issues.py`(`test_workspace_admin_needs_direct_pms_space_membership`) | `any_user_creates_a_space_and_owns_it_others_need_membership`, `member_roles_follow_the_app_space_rules` | 앱 접근 권한 검사 사례는 해당 없음(§3.2) |
| `test_pms_space_preferences.py`, `test_pms_view_preferences.py` | `space_preferences_pin_and_hide_exclude_each_other`, `view_preferences_default_to_status_grouping` | 워크스페이스 멤버십 사례는 해당 없음 |
| `test_pms_task_list_customization.py`, `test_pms_task_list_serialization.py`, 상태 라우터 사례 | `task_lists_inherit_the_space_workflow_and_get_default_labels`, `checklist_comments_templates_and_custom_fields` | |
| `test_pms_list_archive.py`, `test_pms_issues.py`(리스트 삭제·이름 변경·멤버는 삭제 불가) | `archived_lists_are_read_only_and_must_be_archived_before_delete`, `folders_create_a_default_list_and_keep_lists_on_delete`, `deleting_a_space_hides_its_data` | |
| `test_pms_issues.py`(생성·수정·명시 null·날짜 범위·완료일·뷰어 쓰기 금지·담당자/팔로워 멤버 검사·정렬·이름순·100개 넘는 페이지·재정렬·순환·중복·하위 태스크 분리 위치·이름순 저장·배정/오늘·지연·개인 위젯) | `task_crud_status_flow_and_activity`, `viewers_read_but_cannot_modify_and_assignees_must_be_members`, `task_list_sorts_by_dates_titles_and_paginates`, `reorder_moves_tasks_and_parents_without_cycles`, `assigned_and_today_overdue_show_only_the_callers_open_tasks` | 동시성 사례(`*_concurrency.py`, 교착 검사)는 옮기지 않았다(행 잠금 순서는 원본과 같게 두었다). 미디어·문서 사례는 기능과 함께 제외 |
| `test_pms_issues.py`(알림 사례 6개) | `notifications_reach_readers_and_are_retracted_and_read` | 알림함 행 대신 가짜 코어가 받은 발행·회수·읽음을 확인. "읽을 수 없는 사람에게 제목만" 사례는 miy에서 보내지 않음으로 바뀜(§3.7) |
| `test_pms_bulk_task_dates.py`, `test_pms_bulk_update_limit.py`, `test_pms_issues.py`(일괄 사례) | `bulk_changes_status_labels_dates_and_deletes` | |
| `test_pms_task_work_items.py` | `checklist_comments_templates_and_custom_fields` | |
| `test_pms_task_copy.py` | `copy.rs` 단위 2개, `copy_keeps_matching_values_and_counts_the_rest` | 미디어 복제 사례 제외 |
| `test_pms_task_export.py` | `export.rs` 단위 3개, `export_returns_a_workbook_named_after_the_list` | 셀 값 하나하나의 비교는 하지 않았다(§8) |
| `test_pms_attachments.py` | `attachments_upload_download_and_release_through_the_platform` | 서명 URL 사례 대신 파일 API 세 단계와 해제 |
| `test_pms_dashboard_summary.py` | `checklist_comments_templates_and_custom_fields`의 요약 호출 | 숫자 전부의 비교는 하지 않았다 |
| (miy 추가) | `a_single_task_grant_reads_that_task_only`, `platform_endpoints_answer_platform_calls_only`, `the_frame_learns_the_current_user_and_the_directory`, `notify.rs`·`home_card.rs`·`search.rs`·`error.rs`·`tasks.rs`·`lists.rs` 단위 | 단일 공유·`notificationCheck`·홈 카드·검색·`/me`·신원 없음 401 |

옮기지 않은 서버 테스트: `test_pms_approval_preview.py`(AI), `test_pms_doc_link_locators.py`(Docs), `test_pms_links.py`(AI-DO 경로), `test_pms_space_promotion_migration.py`·`test_pms_view_preferences_label_migration.py`(옛 데이터 migration), `*_concurrency.py` 4개(§8).

**화면**은 §4~5의 표와 같은 기준이다. 아래는 화면 포팅의 테스트 대응이다.

결과: `pnpm exec vitest run` 90개 파일 523개 통과(2026-11-22). 화면 테스트 설정은 플래너와 같다(jsdom, globals, `TZ=Asia/Seoul`, i18n 초기화). 컴포넌트 스펙은 AI-DO의 `MemoryRouter`·`useAuth` 모의 대신 `test-utils/runtime.tsx`(가짜 런타임 + 현재 사용자 문맥)로 그리고, 이동은 `runtime.navigate(<앱 기준 경로>, { replace })`로 확인한다. API 함수 첫 인자 `'token'` 단언은 `expect.any(Function)`/런타임 `api`로 바꿨다.

| AI-DO | miy | 결과 |
| --- | --- | --- |
| 모델·훅 스펙 30개(`bulk-action-bar`·`calendar-view`·`filter-bar`·`gantt-timeline`·`gantt-view`·`list-view-editing`·`list-view-grouping`·`pms-create-modal`·`pms-task-list-catalog`·`pms-view-scoped-state`·`space-members-modal`·`space-tasks-layout`·`task-copy`·`task-list-members`·`task-list-presentation`·`task-list-workflow-settings`·`task-picker`·`today-overdue` 모델, `pms-constants`, `pms-task-context`, `pms-task-hierarchy`, `pms-task-selection-workflow`, `task-detail-editing-model`, `space-tree-model`, `pms-sidebar-reorder`, `useTaskDetail{Attachments,Checklist,Comments,IssueEditing,Subtasks}`)과 컴포넌트 스펙 8개(`CalendarView`·`FilterBar`·`GanttView`·`ListAssigneeMenu`·`ListView`·`TableView`·`TaskDetailActivityPanel`·`SpaceContextMenu`) | 같은 경로 | 내용 그대로, 통과. `new-task-modal-model.spec`은 편집기 import 경로만 바뀜 |
| 공용 스펙(`DateInput`, `date-input-model`, `unified-calendar-adapter`, `korean-holidays`, 시간 5개, `ResourcePickerDialog`, `byte-size`, `request-concurrency`, `ordered-reorder`, `picker-model`, `user-option-picker-model`) | 같은 경로 | 그대로, 통과 |
| `api/pms-filters`, `pms-space-members`, `pms-task-lists`, `pms-task-reorder`, `pms-task-export` | 같은 경로 | 수정: 전역 `fetch` 모의 → `runtime.api`/`apiBlob` 가짜, 경로 `/v1/pms/...`. `pms-task-reorder`는 워크스페이스 경로 정책 설정을 빼고 "한 번의 일괄 요청" 확인만 |
| `api/pms-runtime-scope.spec.ts` | 삭제 → `api/pms-api.spec.ts`(miy 8개) | 워크스페이스 API 경로 재작성과 app-first 런타임 명시 스페이스 강제는 miy에 없음. 새 스펙: 앱 기준·인코딩 경로, 첨부 두 단계·실패 시 삭제·10 MiB 거부, 엑셀 파일 이름·필터 판정·object URL, 위젯 배정 목록 조합 |
| `views/pms-view-route.spec.ts`(18) | 같은 경로(14) | 수정: 앱 기준 경로. 제외: canonical `/pms/s/<key>` 해석·"인가된 스페이스 안 canonical 링크"(app-first 전용), 스페이스 문서·화이트보드 경로(기능 제외), 빌더 케이스 안 docs/whiteboards 단언 |
| `views/space-scoped-redirects.spec.ts` | 삭제 | SpaceDocsView·SpaceWhiteboardsView 리디렉션과 app-first 화이트보드 전용 |
| `views/PMSView.scope.spec.tsx` | 같은 경로(2) | 수정: `/lists/list-one`·`/`에서 모든 요청이 `/v1/pms/`로 시작, 스페이스 생성 뒤 `/spaces/<id>`로 이동, 개요의 새 스페이스 링크 |
| `views/TodayOverdueView.spec.ts`, `useTaskList{Labels,Members,WorkflowStatuses}Controller`, `usePmsSpacePreferences`, `usePmsTaskListGroupPreference`, `useTaskDetailResources` | 같은 경로 | 수정: `token`→`api`, `useAuth` 대신 `useAppApi` 모의, 상세 응답 fixture에서 `linked_docs` 제거 |
| `sidebar/PmsSidebarSpaces.spec.tsx` | 같은 경로(5) | 수정: 런타임 없이 `PMS_WORKSPACE_KEY`. 제외: "생성 요청 완료 후 리디렉션 안 함"(Docs 생성·app_space 전환 전용), "서버가 허용할 때만 생성 제안"(`can_create_space`, app-first 없음 → "모든 PMS 사용자에게 생성 제안"으로 대체), "확인되지 않은 역할로 생성 추론 안 함"(miy 앱에 관리자·시스템 역할 없음), "capability 없이 Docs 요청 안 함"(Docs 제외), "스페이스 전환 후 이전 응답·다른 스페이스 이벤트 무시"(app_space 런타임 전환 전용) |
| `sidebar/SortableListLink`, `pms-sidebar-scope`, `space-order-editor-model`, `space-order-persistence` | 같은 경로 | 수정: `renderWithRuntime`, 앱 기준 경로, 문서 단언·"문서 정렬 순서 이동" 케이스 제외, `pms-sidebar-scope`의 canonical 케이스 제외(app-first 전용) |
| `views/SpaceOverviewView.navigation.spec.tsx`, `space-overview-model.spec.ts`, `pms-events.spec.ts` | 같은 경로 | 수정: 문서 단언 제외. "canonical 런타임 밖 legacy 루트 유지"는 miy에서 첫 케이스와 같아져 제외 |
| `views/PmsGlobalOverview.spec.tsx` | 같은 경로(6) | 수정: id 링크(`/spaces/..`, `/lists/..`)와 클릭 이동 확인 추가. 제외: "앱 역할만으로는 생성 제안 안 함", "생성 권한 없을 때 초대 요청 안내"(miy에는 앱 역할이 없고 모든 PMS 사용자가 생성 가능) |
| `views/TodayOverdueView.navigation.spec.tsx` | 같은 경로(2) | 수정: `timeZone: 'UTC'` 런타임, `/lists/<id>?task=<id>`. "목록·소유 스페이스가 없으면 링크 안 만듦" 2개는 app-first 소유 스페이스 추정 방지용이라 "카탈로그에 목록이 없어도 목록 id로 연결"로 바꿈, "legacy 워크스페이스 링크 유지"는 `/w/<slug>` 경로가 없어 제외 |
| `views/CreateSpaceModal.spec.tsx` | 같은 경로(2) | 수정: `reloadGlobalApps` 단언과 카탈로그 새로고침 실패·재시도 제외(카탈로그 갱신을 옮기지 않음), 케이스 본문 유지 |
| `views/TaskPickerModal.spec.tsx` | 같은 경로(8) | 수정: "없는 고정 목록은 불러오지 않음" → "스페이스 선택 없이 고정 목록을 id로 불러옴"(§5.3) |
| `views/FloatingPmsWidget.spec.tsx` | 같은 경로(3) | 수정: 스페이스 리스트를 `(api, slug, slug)`로 읽음. 제외: createTask 열기 요청(포털 이벤트), 셸 배정 건수(도크 배지 훅), canonical PMS 개요 열기·인증 전 바로가기 비활성(바닥 버튼 제거, 토큰 없음) |
| `views/NewTaskModal.spec.tsx` | 같은 경로(6) | 수정: `components/editor`의 `BlockEditor` 모의. 추가: 제목 입력 시 `dirty: true` 보고 |
| `views/SpaceTasksView.spec.tsx`(14), `BulkActionBar`(9), `TaskCopyDialog`(6), `SpaceMembersModal`(4) | 같은 경로 | 수정: 런타임 문맥(SpaceTasksView는 `?task=` 왕복을 위해 위치를 상태로 가진 래퍼), `'token'` → `expect.any(Function)` |
| `sidebar/SortableDocLink`, `views/TaskDocPickerModal`, `views/useTaskDetailLinkedDocs` 스펙 | 옮기지 않음 | Docs 제외 |
| `sidebar/config.spec.tsx`, `sidebar/pms-sidebar-config-model.spec.ts` | 옮기지 않음 | AI-DO 셸 사이드바 설정(`PmsAppSidebar`가 대신) |
| `platform/i18n/resource-location-label.spec.ts` | 옮기지 않음 | 쓰는 화면(SpaceDocsView)이 없음 |
| (miy 추가) | `AppRoot.spec.tsx`(개요+왼쪽 열 한국어, 패널 경로 영어·왼쪽 열 없음, `/tasks/:id` → 리스트 이동, 읽기 전용 단일 태스크), `platform/runtime/app-location.spec.ts`, `platform/api/api-error.spec.ts`(PMS 코드·매개변수·언어 전환) | 통과 |

## 8. 남은 일

- **실제 프레임·서버 확인(계획 절차 9):** 서버는 일회용 PostgreSQL과 가짜 코어로, 화면은 `tsc`·jsdom 테스트·프로덕션 빌드로만 확인했다. 실제 프레임 CSP에서 BlockNote·Mantine(전역 CSS 충돌 포함), @dnd-kit 드래그, FullCalendar, 간트, `revert-layer` 링크 색 복원을 확인해야 한다. 미리보기(`miy app dev`)와 실제 코어의 디렉터리·알림·파일·패널·홈 카드·검색 호출도 확인하지 않았다.
- AI-DO 기준 화면(가상 계정·가상 데이터)과의 나란한 비교, 카탈로그 공식 릴리스(`miy app source-pin` → `releases`)·릴리스 권한 승인(요청 권한 5개)·dev 설치·인수·prod 승격.
- 단일 태스크 공유를 만드는 곳이 없다(AI-DO는 회의 앱이 만들었다, §3.4). 회의 앱이나 공유 화면이 생기면 연결한다.
- 동시성 테스트(`test_pms_*_concurrency.py`: 일괄 변경·복사·이름순 저장·내보내기·하위 태스크 분리 교착)는 옮기지 않았다. 잠금 순서(리스트 행 → 태스크 행 id 순)는 원본과 같게 두었다.
- 엑셀·대시보드 요약은 형식과 일부 값만 시험했다. AI-DO 테스트의 셀 단위 비교는 옮기지 않았다.
- 알림 발행이 실패하면 다시 보내지 않는다(서비스 주체 뒤). 첨부 해제 대기열은 다음 삭제 요청에서만 다시 시도한다.
- 보강(계획 §2.2): 템플릿·사용자 정의 필드·마일스톤을 만드는 화면, 저장 필터의 서버 보관은 포팅 인수 뒤에 한다(AI-DO의 저장 필터는 브라우저 보관 그대로).
- 큰 화면 묶음(`TaskDetail` 940 KB, `PMSView` 520 KB, 압축 전)은 BlockNote·Mantine 때문이다. 나누는 것은 실제 프레임 확인 뒤에 정한다.
- 좁은 주 프레임(768px 미만)에서 왼쪽 열 대신 쓰던 AI-DO 모바일 메뉴는 옮기지 않았다(개요 화면의 링크로는 이동 가능).
- AI-DO의 앱 우선(app-first) 분기 코드(`runtime ? buildAppRoutePath(...) : ...`, `pms-view-route`의 canonical `/pms/s/` 해석)는 원본대로 남아 있고 miy에서는 닿지 않는다. 지울지는 후속 정리에서 정한다.
- jsdom 실행 로그에 `Not implemented: navigation`(내려받기 앵커 클릭) 경고 2건이 남는다. 테스트 결과에는 영향이 없다.

## 9. 플랫폼에 필요한 변경(이 앱 밖, 하지 않음)

- **필수 아님:** 지금 코어·CLI·번들 0.12.0으로 앱 검사가 통과한다. 출시에는 카탈로그 `pms` 항목에 릴리스를 기록하고(`registry/apps/catalog.json`, 지금 `releases: []`) 요청 권한 5개를 승인해야 한다.
- **런타임에 현재 사용자:** 프레임은 사용자 ID·이름을 받지 못해 앱마다 `/me` 같은 경로를 만든다. `AppRuntime`에 `user: {id, name}`(표시용, 권한 판단 아님)을 주면 이 경로를 없앨 수 있다.
- **앱 사용 권한 조회:** AI-DO는 스페이스 멤버로 추가할 사람이 PMS를 쓸 수 있는지 검사했다(`platform.app_access_required`). 앱이 "이 사람이 이 설치를 쓸 수 있는가"를 디렉터리 질의로 거를 수 있으면 같은 검사를 옮길 수 있다.
- **디렉터리:** 그룹 구성원 조회(부서 초대가 지금은 전체 사용자 목록을 주 조직으로 거른다)와 사용자 이메일(AI-DO 멤버·선택기 표시)이 없다.
- **알림의 받는 사람별 언어:** 제목을 한국어로만 만든다. 알림에 언어별 제목(예: `titles: {ko-KR, en-US}`)을 둘 수 있으면 영어 사용자도 영어로 받는다.
- **서비스 주체:** 반복 태스크 자동 생성, 마감 알림, 실패한 알림 재전송, 첨부 해제 재시도에 필요하다(계획 §2.2·M2).
