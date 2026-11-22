# 화이트보드 (`whiteboard`)

Excalidraw 보드를 만들고 함께 보는 miy 공식 업무 앱이다. AI-DO `0002af062`의 `whiteboard` 도메인과 화면을 옮기고, 보드 이미지를 플랫폼 Storage로 분리했다(보강). 원본과의 차이, 결정과 프레임 CSP 확인 결과는 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 보드 목록(전체·내 보드·최근·즐겨찾기·휴지통, 검색, 정렬, 카드·목록 보기, 비공개/워크스페이스 공개 구역, 미리보기), Excalidraw 편집(라이브러리 5종, 그리드, 선택 요소 삭제), 공개 범위 변경, 사용자 공유(읽기·편집), 휴지통·복원·영구 삭제, PNG·SVG·Excalidraw JSON 내보내기, 공간.
- 공동편집은 없다. 한 사람씩 저장하며, 저장할 때 마지막으로 불러온 리비전을 보낸다. 다른 사람이 먼저 저장했으면 409와 함께 "최신 내용 불러오기 / 내 변경 유지"를 묻는다.
- 권한 판단은 모두 서버가 한다. 호출자는 게이트웨이가 서명한 신원(`Identity::user_id`)이며 플랫폼 관리자 표시는 아무 권한도 주지 않는다. 모든 쓰기는 보드 행을 잠근 뒤 현재 권한을 다시 판정한다.
- 요청 권한: `database`, `platform:directory:read`, `platform:files:read`, `platform:files:write`, `platform:notifications:write`.
- 플랫폼 엔드포인트: `notificationCheck` → `/platform/notification-check`, `search` → `/v1/search`.
- 뺀 것: 실시간 공동편집과 커서, AI 연동, 링크 공유(공유 링크 화면), PMS·회의 연결(대상·컨텍스트 슬롯·선택 창), 웹 임베드.

## 화면 경로

앱 기준 경로(`runtime.location`)다. AI-DO의 `/w/:workspaceSlug/whiteboard[/:whiteboardId]`에서 워크스페이스는 `?space=`(없으면 전사 공간)가 된다.

| 경로 | 화면 |
| --- | --- |
| `/?view=all\|mine\|recent\|favorites\|archived&space=<id>` | 보드 목록. 왼쪽에 공간 선택과 보기 메뉴 |
| `/<보드 ID>?space=<id>` | 보드 편집기 |

## API

앱 디렉터리에서 `pnpm api:generate`를 실행하면 라우터에서 HTTP 명세와 Web 타입을 갱신한다. [OpenAPI 명세](docs/openapi.json) · [Web 타입](web/src/api.generated.ts)

UI는 `runtime.api('/v1/...')`로 부르고 게이트웨이가 `/api/v1/...`로 전달한다. 전사 공간은 `/v1/whiteboard/...`, 앱 공간은 `/v1/workspaces/{space_id}/whiteboard/...`를 사용한다. 항목 경로는 그 공간의 보드에만 답한다. 보드 생성·수정의 JSON 본문 한도는 4MiB다. 보드 요청·응답은 snake_case이고 시각은 naive UTC다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼다. 예를 들어 `whiteboard_not_found`를 반환한다.

| 메서드·경로(`…` = 화이트보드 경로) | 설명 |
| --- | --- |
| `GET …/hub?view&category&q&sort_by&sort_dir&page&page_size&source_app&source_kind&target_app&target_type&target_id` | 볼 수 있는 보드(AI-DO 필터·정렬, `page_size` 최대 200). 앱 공간은 멤버만 |
| `POST …/items` | 보드 만들기(201) `{title, scene?, primary_target?, …}`. `primary_target`은 그 공간의 `whiteboard/workspace_sidebar/<spaceId>`만 받는다 |
| `GET …/items/{id}` | 보드(장면 포함, `revision`) |
| `PATCH …/items/{id}` | 이름(관리 권한)·장면(편집 권한) 저장 `{title?, scene?, base_revision}`. 장면이 바뀌었는데 `base_revision`이 현재보다 오래되면 409 `whiteboard_revision_conflict`. 편집기 상태만 다른 장면은 변화 없음 |
| `DELETE …/items/{id}` | 휴지통으로(204). 공유 알림 회수 |
| `POST …/items/{id}/restore` | 복원 |
| `DELETE …/items/{id}/permanent` | 영구 삭제(휴지통에서만, 아니면 409). 저장된 이미지 해제 |
| `PUT·DELETE …/items/{id}/target` | 워크스페이스 공개로 바꾸기·비공개로 되돌리기 |
| `PATCH …/items/{id}/favorite` | 즐겨찾기 전환 `{is_favorite}` |
| `POST …/items/{id}/view` | 최근 본 시각 기록(204), 그 보드 알림 읽음 |
| `GET …/shareable-users?q` | 공유할 사람(디렉터리, 본인 제외, 앱 공간은 직접 멤버, 최대 30) |
| `GET …/items/{id}/sharing` | 공유 상태 `{whiteboard_id, owner_id, users, link_share: null}` |
| `PUT·DELETE …/items/{id}/sharing/users/{userId}` | 사용자 공유 `{access_level: read\|edit}`·해제. 새로 공유하면 알림, 해제하면 회수 |
| `POST …/items/{id}/files` | 이미지 예약 `{request_id, file_id, name, mime, size}` → `{id, file_id, status}`(PNG·JPEG·GIF·WebP, 10MiB). 같은 요청 ID 재전송은 같은 행 |
| `PUT·GET /files/whiteboard/items/{id}/files/{fileId}` | `runtime.fileApi`: 이미지 올리기(올린 사람, 편집 권한)·내려받기(볼 권한) |
| `GET·POST /v1/spaces`, `PATCH·DELETE /v1/spaces/{id}` | 내 공간 목록, 공간 만들기(앱 사용자 누구나), 수정·삭제(공간 관리자, 보드가 남아 있으면 409) |
| `GET·PUT /v1/spaces/{id}/sharing`, `GET …/sharing/requests/{requestId}` | 공간 멤버(SDK ACL 공유 계약, 역할 `member`·`admin`) |
| `POST /platform/notification-check` | 플랫폼 호출 전용(읽기 전용): 지금 열 수 있는 보드의 묶음만. 공간 멤버는 직접 부여와 플랫폼이 서명한 현재 그룹으로 판정 |
| `POST /v1/search` | 플랫폼 호출 전용(읽기 전용): 지금 열 수 있는 보드(그룹 멤버 포함, 같은 판정) 중 제목이 맞는 것 |

## 알림

| 종류 | 언제 | 받는 사람 | 묶음 키 |
| --- | --- | --- | --- |
| `share` | 보드를 새로 공유함 | 공유받은 사람 | `whiteboard:<id>` |

공유 해제는 그 사람에게서, 휴지통 이동·영구 삭제는 모두에게서 회수한다. 보드를 열면(`…/view`) 읽음 처리한다. 공유 대상은 항상 사용자라 그룹 수신자(`notify_with_groups`)는 쓰지 않는다.

## 데이터

`migrations/0002_acl.sql`(SDK `acl.sql` 그대로), `migrations/0003_whiteboard.sql`: `whiteboard_spaces`, `whiteboards`(`revision` 포함), `whiteboard_targets`, `whiteboard_user_shares`, `whiteboard_user_item_prefs`, `whiteboard_files`. `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

## 화면 빌드와 글꼴

`vite.config.ts`가 Excalidraw의 글꼴(약 13.6MB, 234개)을 `dist/ui/excalidraw-assets/fonts/`에 원래 이름으로 복사하고, `web/src/platform/excalidraw/self-hosted-assets.ts`가 Excalidraw가 그 경로를 쓰게 한다(프레임 CSP `font-src 'self'`). SDK 미리보기 빌드(`pnpm build:preview`)에는 이 복사가 없으므로 미리보기에서 손글씨 글꼴을 보려면 `dist/ui/excalidraw-assets`를 `dist/preview/`에 복사한다.

## 검사

```sh
cargo test                                   # DB 없는 단위 테스트
WHITEBOARD_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored            # 일회용 PostgreSQL과 가짜 코어로 HTTP·DB 테스트까지
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
```

`WHITEBOARD_TEST_DATABASE_URL`에는 버려도 되는 PostgreSQL만 준다. 테스트가 그 서버에 새 데이터베이스와 `app` 스키마를 만들고 migration을 적용한다. 디렉터리·알림·파일 저장소는 테스트 안의 가짜 코어가 받는다. AI-DO의 i18n 문구는 `node scripts/gen-i18n-resources.mjs <AI-DO resources.ts> web/src/platform/i18n/resources.ts`로 다시 만든다.
