# 문서 (`docs`)

블록 편집기로 문서와 페이지 트리를 다루는 miy 공식 업무 앱이다. AI-DO `0002af062`의 `docs` 도메인과 화면을 옮겼다. 실시간 공동편집 대신 **한 번에 한 사람이 편집하고, 저장할 때 다른 사람의 변경과 충돌하면 알린다**(페이지 revision). 원본과의 차이, 제외 범위, 결정, 프레임 CSP 확인 결과는 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 문서 목록(전체·내 문서·공유받은 문서·비공개·최근, 검색·정렬), 문서 만들기(공간 문서/비공개), 페이지 트리(추가·삭제·끌어서 이동), BlockNote 0.52.1 편집기(이미지 포함), 사용자 공유(읽기·편집), 공개 범위(공간 문서 ↔ 비공개), 즐겨찾기·최근 페이지, 페이지 첨부(10MiB), Markdown 가져오기·내보내기, 전체 보기(발표 모드: 레이저·펜), 복제·이름 변경·삭제(휴지통), 인쇄, 링크 복사.
- 보강(miy, AI-DO에 없던 기능): 공간별 **휴지통**(복원·영구 삭제), 페이지 **버전 기록**(미리 보기·새 revision으로 복원), 페이지 **댓글**(답글·해결/다시 열기·`@` 멘션과 알림), 공간 **템플릿**(페이지를 템플릿으로 저장·템플릿으로 새 문서/페이지). 결정은 [PORTING §8](docs/PORTING.md#8-보강-휴지통버전-기록댓글템플릿).
- 제외: 실시간 공동편집·커서, HTML 페이지, 링크 공유(익명·내부 링크), AI 도구·RAG, 회의록 생성, 컬렉션(API 전용이던 기능), PMS 태스크 연결(앱 간 계약 필요), 레거시 경로.
- 권한 판단은 모두 서버가 한다. 호출자는 게이트웨이가 서명한 신원(`Identity::user_id`)이며 요청 본문의 사용자 값과 플랫폼 관리자 표시는 쓰지 않는다.
- 공간: **전체 문서**(`global`, AI-DO의 앱 전역 공간 — 앱을 쓸 수 있는 모든 사람이 멤버)와 문서 관리자가 만드는 공간(SDK ACL, 역할 `viewer`·`member`·`admin`). AI-DO의 앱 관리자 역할은 **문서 관리자**(앱 역할, 게시판의 `board_managers`와 같은 방식)가 맡는다.
- 요청 권한: `database`, `platform:directory:read`, `platform:files:read`, `platform:files:write`, `platform:notifications:write`.
- 플랫폼 엔드포인트: `notificationCheck` → `/platform/notification-check`, `homeCard` → `/v1/home-card`("최근 문서"), `search` → `/v1/search`.

## 화면 경로

앱 기준 경로(`runtime.location.pathname`)다. 공간은 `?space=<id>`(없으면 전체 문서), 선택한 페이지는 `?page=<id>`.

| 경로 | 화면 |
| --- | --- |
| `/?space=&view=` | 공간의 문서 목록. `view`: `all`·`mine`·`shared`·`private`·`recent`(AI-DO의 `archived`도 주소로는 열리지만 왼쪽 열에서는 휴지통이 대신한다). `create=1`이면 새 문서 창을 연다 |
| `/<docId>?space=&page=&comment=` | 문서: 왼쪽 페이지 트리, 오른쪽 페이지 편집기·첨부·댓글. `comment`는 알림이 가리키는 댓글(펼쳐 보이고 강조). 문서 메뉴에 버전 기록·템플릿으로 저장 |
| `/trash?space=` | 공간의 휴지통: 관리할 수 있는 문서와 편집할 수 있는 문서의 페이지, 삭제한 묶음마다 한 줄. 복원·영구 삭제 |
| `/settings` | 문서 설정: 공간 만들기(문서 관리자)·멤버 관리(공간 관리자)·문서 관리자 |

AI-DO 경로 기준(`/docs`, `/docs/<docId>`)도 같은 화면을 연다. 그 밖의 경로는 목록을 보인다. 왼쪽 열에 공간 선택, 새 문서, 라이브러리 항목과 휴지통, 즐겨찾기·최근 페이지(모든 공간), 설정이 있다. 보내지 않은 댓글은 앱을 쓰는 동안 입력란마다 남고, 남아 있으면 포털에 저장하지 않은 작업으로 알린다.

## API

UI는 `runtime.api('/v1/...')`로 부르고 게이트웨이가 `/api/v1/...`로 전달한다(AI-DO와 같은 경로·요청·응답 형태, snake_case). 시각은 naive UTC다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `docs_doc_not_found`).

요청 형식과 상태 코드는 Rust 라우터에서 생성하는 [OpenAPI 명세](docs/openapi.json)에 있다. 생성 및 검사는 [API 가이드](docs/API.md)를 따른다.

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/docs/hub?space&view&q&sort_by&sort_dir&page&page_size` | 공간의 문서 중 볼 수 있는 것(AI-DO `DocsHubResponse`). 공간 역할이 없으면 403 `docs_requests_workspace_context_required` |
| `POST /v1/docs/items?space` | 문서 만들기(201). `primary_target`이 `{app:"docs",type:"workspace_sidebar",id:<space>}`이면 공간 문서, 없으면 비공개. `template_id`이면 첫 페이지 본문을 그 공간의 템플릿에서 가져온다 |
| `GET·PATCH·DELETE /v1/docs/items/{id}` | 문서, 이름·유형 변경(관리 권한), 휴지통으로 이동(204, 파일은 그대로 두고 공유·댓글 알림 회수) |
| `POST /v1/docs/items/{id}/duplicate` | 복제(201, 볼 수 있으면, 복제한 사람이 소유자) |
| `GET·POST /v1/docs/items/{id}/pages` | 페이지 목록·추가(201). `template_id`(본문 `content_blocks`와 함께 쓰면 400) |
| `GET·PATCH·DELETE /v1/docs/pages/{id}` | 페이지, 수정, 하위 포함 휴지통으로 이동(204). 본문 저장은 `{content_blocks, expected_revision}`; 오래된 revision이면 409 `docs_page_revision_conflict`. 제목·부모·순서는 revision을 바꾸지 않는다. 본문을 저장할 때마다 버전이 남는다 |
| `GET /v1/docs/pages/{id}/versions` | 버전 목록(최신 먼저, 본문 없이, 저장한 사람·시각, `current_revision`, `can_restore`) |
| `GET /v1/docs/pages/{id}/versions/{versionId}` | 버전 하나와 본문(읽기 전용 미리 보기) |
| `POST /v1/docs/pages/{id}/versions/{versionId}/restore` | `{expected_revision}` — 그 버전 본문을 새 revision으로 저장(편집 권한). 오래된 revision이면 409 `docs_page_revision_conflict` |
| `GET·POST /v1/docs/pages/{id}/comments` | 댓글 스레드(읽기 권한, 열면 내 댓글 알림 읽음), 댓글 쓰기 `{body, parent_id?, mention_ids, request_id}`(201; 같은 요청 ID·같은 내용은 200으로 그 댓글, 다른 내용은 409 `docs_comment_request_conflict`) |
| `PATCH·DELETE /v1/docs/comments/{id}` | 수정 `{body, mention_ids}`(쓴 사람), 삭제(쓴 사람·문서 관리 권한, 204, 알림 회수) |
| `POST /v1/docs/comments/{id}/resolve`, `…/reopen` | 스레드 해결·다시 열기(첫 댓글만; 편집 권한 또는 스레드를 연 사람) |
| `GET /v1/docs/trash?space` | 휴지통: 관리 권한이 있는 문서, 편집 권한이 있는 살아 있는 문서의 삭제된 페이지(삭제 묶음마다) |
| `POST /v1/docs/trash/items/{id}/restore`, `DELETE /v1/docs/trash/items/{id}` | 문서 복원(함께 삭제된 페이지 포함)·영구 삭제(관리 권한, 커밋 뒤 첨부·이미지 detach) |
| `POST /v1/docs/trash/pages/{id}/restore`, `DELETE /v1/docs/trash/pages/{id}` | 페이지 복원(편집 권한, 같은 때 삭제된 하위 포함; 상위가 아직 휴지통이면 문서 맨 위로 → `moved_to_root`)·영구 삭제(관리 권한) |
| `GET·POST /v1/docs/templates?space`, `GET·DELETE /v1/docs/templates/{id}` | 공간 템플릿 목록(`can_create`), 페이지를 템플릿으로 저장 `{page_id, title?, description?}`(그 공간의 멤버 이상), 템플릿과 본문, 삭제(만든 사람·공간 관리자) |
| `PUT·DELETE /v1/docs/items/{id}/target` | 공개 범위: 공간 문서로/비공개로(편집 권한) |
| `PATCH /v1/docs/items/{id}/favorite` | 즐겨찾기 전환 `{is_favorite}` |
| `POST /v1/docs/items/{id}/view` | 열람 기록 `{page_id}`(최근 페이지, 공유 알림 읽음) |
| `GET /v1/docs/favorites?scope=space|docs_spaces&space` | 즐겨찾기(`docs_spaces`는 모든 공간, 항목마다 `space`) |
| `GET /v1/docs/recent-pages?limit&scope&space` | 최근 페이지(1~20) |
| `GET /v1/docs/shareable-users?q` | 공유할 사람 검색(플랫폼 디렉터리, 30명, 본인 제외) |
| `GET /v1/docs/items/{id}/sharing` | 공유 상태(공유 권한) |
| `PUT·DELETE /v1/docs/items/{id}/sharing/users/{userId}` | 사용자 공유 `{access_level: read|edit}`·해제 |
| `GET·POST /v1/docs/pages/{id}/attachments` | 첨부 목록, 첨부 예약 `{request_id, filename, content_type, size_bytes}`(201, 편집 권한, 10MiB). 같은 요청 ID는 같은 첨부를 돌려준다 |
| `PUT·GET /files/docs/attachments/{id}/content` | `runtime.fileApi`: 바이트 올리기(올린 사람, 편집 권한), 내려받기(읽기 권한, 항상 `application/octet-stream`) |
| `DELETE /v1/docs/attachments/{id}` | 첨부 삭제(204, 편집 권한, 코어 파일 detach) |
| `POST /v1/media/upload` | 본문 이미지 예약 `{requestId, name, mime, size}` → `{id, url:"media:<id>", status}`(PNG·JPEG·GIF·WebP, 10MiB) |
| `POST /v1/media/link` | 내 업로드를 페이지에 연결 `{media_ids, resource_type:"docs_native_page", resource_id}`(편집 권한) |
| `PUT·GET /files/media/{id}/content` | `runtime.fileApi`: 이미지 올리기(올린 사람), 보기(페이지 읽기 권한 또는 연결 전 올린 사람) |
| `GET·POST /v1/spaces`, `PATCH·DELETE /v1/spaces/{id}` | 들어갈 수 있는 공간(역할 포함, 전체 문서 먼저), 공간 만들기(문서 관리자), 수정·삭제(공간 관리자; 살아 있는 문서가 있으면 409 `docs_space_not_empty`, 보관된 문서와 그 파일은 함께 정리) |
| `GET·PUT /v1/spaces/{id}/sharing`, `GET …/sharing/requests/{requestId}` | 공간 멤버(SDK ACL 공유 계약) |
| `GET /v1/docs/me` | `{userId, isManager, canBootstrap}` |
| `POST /v1/docs/managers/bootstrap` | 관리자가 없을 때 첫 관리자 맡기(201, 이미 있으면 409) |
| `GET·POST /v1/docs/managers`, `DELETE …/{userId}` | 문서 관리자 목록·추가·해제(마지막 관리자는 409) |
| `POST /platform/notification-check` | 플랫폼 호출 전용(읽기 전용): 지금 읽을 수 있는 문서의 `doc:<id>`, 지우지 않은 댓글이 살아 있는 페이지에 있는 `comment:<id>`·`mention:<id>`만 허용 |
| `POST /v1/home-card` | 플랫폼 호출 전용(읽기 전용): 최근 문서 최대 5개 |
| `POST /v1/search` | 플랫폼 호출 전용(읽기 전용): 지금 읽을 수 있는 문서 중 제목·페이지 제목·본문이 맞는 것 |

## 알림

| 종류 | 언제 | 받는 사람 | 묶음 키 |
| --- | --- | --- | --- |
| `share` | 문서를 새로 공유함(권한 변경은 다시 알리지 않음) | 공유받은 사람 | `doc:<id>` |
| `comment` | 페이지에 댓글·답글을 남김 | 문서 소유자와 스레드 참여자(쓴 사람·멘션된 사람 제외) | `comment:<id>` |
| `mention` | 댓글에서 사람을 `@` 멘션함 | 멘션된 사람(쓴 사람 제외) | `mention:<id>` |

공유를 해제하면 그 사람에게서, 문서를 휴지통으로 보내면 모두에게서 회수한다. 댓글 알림은 지금 그 페이지를 읽을 수 있는 사람에게만 보내고, 댓글을 지우면 모두에게서, 멘션을 빼면 그 사람에게서, 페이지·문서를 휴지통으로 보내거나 공유·공간 멤버·공개 범위가 바뀌어 읽을 수 없게 된 사람에게서 회수한다. 받은 사람이 문서를 열거나(공유) 페이지 댓글을 열면(댓글·멘션) 읽음 처리한다. 다른 사람의 그룹은 앱이 알 수 없어 그룹으로만 페이지를 읽는 사람은 댓글 알림에서 빠진다.

## 데이터

`migrations/0002_acl.sql`(SDK `acl.sql` 그대로), `migrations/0003_docs.sql`: `docs_spaces`(전체 문서 공간 시드), `docs_managers`, `docs_native_docs`, `docs_doc_targets`, `docs_native_doc_pages`(+`revision`, `search_text`), `docs_page_attachments`, `docs_native_doc_user_shares`, `docs_user_item_prefs`, `media_files`. `migrations/0004_docs_extras.sql`(보강): 문서·페이지 `trashed_by_id`, 첨부 행의 페이지 FK를 `ON DELETE SET NULL`로(영구 삭제 뒤에도 detach 기록이 남도록), `docs_page_versions`(기존 페이지는 현재 본문을 첫 버전으로 채움), `docs_page_comments`, `docs_comment_notifications`, `docs_templates`. `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

휴지통에 있는 동안 첨부와 본문 이미지는 그대로 남고(복원할 수 있도록), 영구 삭제나 공간 삭제 때 앱 커밋 뒤에 코어 파일을 detach한다. 자동으로 비우지 않는다.

## 검사

```sh
cargo test                                   # DB 없는 단위 테스트
DOCS_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored --test-threads=1   # 일회용 PostgreSQL과 가짜 코어로 HTTP·DB 테스트까지
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
python3 scripts/gen-i18n-resources.py ../../../ai-do/prod/apps/web/src/platform/i18n/resources.ts web/src/platform/i18n/resources.ts
```

`DOCS_TEST_DATABASE_URL`에는 버려도 되는 PostgreSQL만 준다. 테스트가 그 서버에 새 데이터베이스와 `app` 스키마를 만들고 migration을 적용한다. 디렉터리·알림·파일 저장소는 테스트 안의 가짜 코어가 받는다.
