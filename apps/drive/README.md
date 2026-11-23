# 드라이브 (`drive`)

업무 파일을 공간별로 올리고 폴더로 정리해 공유하는 miy 공식 업무 앱이다. AI-DO `0002af062`의 `files` 도메인과 파일 화면(파일 관리자·검색·업로드 진행 패널)을 옮기고, 파일 이동·이름 변경, 휴지통, 버전, 대용량 전송(2GiB)과 다운로드 링크를 더했다(보강). 원본과의 차이와 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 공간(AI-DO 워크스페이스), 폴더(개인용·공간 공유, 상위 폴더 권한 상속), 업로드(진행·취소·실패 파일부터 다시 올리기, 끌어 놓기), 이미지 미리보기(PNG·JPEG·GIF·WebP, 10MB까지), 묶음 다운로드(파일마다 다운로드 링크), 이름·메타데이터 검색, 파일·폴더 이동과 이름 변경, 휴지통(복원·영구 삭제·비우기), 새 버전 올리기와 버전별 다운로드, 공간 공유 알림.
- 파일 바이트는 플랫폼 Storage에만 있다. 앱 DB에는 공간·폴더·파일·버전(코어 파일 ID)·휴지통·Storage 정리 작업과 ACL이 있다.
- 권한 판단은 모두 서버가 한다. 호출자는 게이트웨이가 서명한 신원(`Identity::user_id`)이며 요청 본문의 사용자 값과 플랫폼 관리자 표시는 쓰지 않는다.
- 공간 역할(SDK 자원 ACL, 사용자·그룹 부여): `viewer`(공유된 파일 보기), `member`(파일·폴더 올리기), `admin`(모든 파일·멤버 관리, 남의 개인 파일도 봄). 만든 사람이 보호된 소유자(`admin`)다.
- 요청 권한: `database`, `platform:directory:read`, `platform:files:read`, `platform:files:write`, `platform:notifications:write`.
- 플랫폼 엔드포인트: `fileAccess` → `/v1/file-access`, `search` → `/v1/search`, `notificationCheck` → `/platform/notification-check`.

## 화면 경로

앱 기준 경로(`runtime.location.pathname`)다. AI-DO의 `/w/<워크스페이스>/files`가 `/spaces/<공간 id>`가 되었고 쿼리는 같다.

| 경로 | 화면 |
| --- | --- |
| `/` | 마지막으로 연 공간(없으면 첫 공간)으로 이동. 공간이 없으면 안내 |
| `/spaces/<id>`, `?folder=<폴더 id>` | 파일 관리자(전체 파일 또는 폴더). 왼쪽에 공간 선택·전체 파일·파일 검색·휴지통·폴더 트리 |
| `/spaces/<id>?view=search&q=<검색어>&page=<n>` | 이름·메타데이터 검색 |
| `/spaces/<id>/trash` | 휴지통 |

그 밖의 경로는 `/`와 같다. 한 앱 세션 안에서 공간 목록과 공통 레이아웃을 유지한다. 파일 목록은 공간과 폴더에 해당하는 화면을 조회하고, 검색 주소를 다시 열면 그 주소의 결과를 불러온다. 업로드·파일 변경·묶음 다운로드 중이거나 대화상자에 입력이 있으면 `runtime.setNavigationState({dirty: true})`로 알린다.

## API

[OpenAPI 명세](docs/openapi.json)는 실제 Drive 라우터에서 생성한다. 요청·응답을 변경하면 `pnpm api:generate`로 명세와 [웹 타입](web/src/api.generated.ts)을 갱신한 뒤 앱 검사를 실행한다.

UI는 `runtime.api('/v1/...')`로 부르고 게이트웨이가 `/api/v1/...`로 전달한다. 바이트 경로는 `runtime.fileApi('/<경로>')`가 `/api/files/<경로>`로 전달한다. 요청·응답 키는 AI-DO와 같은 snake_case다(공유 편집 경로만 SDK의 camelCase). 시각은 naive UTC(`2026-11-22T01:02:03`)다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `files_folder_not_found`).

### 공간

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/spaces` | `{user_id, spaces:[{id,name,role,can_manage,can_upload,...}]}` 열 수 있는 공간(이름순) |
| `POST /v1/spaces` | 공간 만들기 `{name}`(201). 앱을 쓸 수 있는 누구나. 만든 사람이 소유자 |
| `PATCH /v1/spaces/{id}` | 이름 바꾸기 `{name}`. 공간 관리자 |
| `DELETE /v1/spaces/{id}` | 공간과 모든 폴더·파일·휴지통 삭제(204). 커밋 뒤 코어 파일을 해제한다. 공간 관리자 |
| `GET·PUT /v1/spaces/{id}/sharing` | 멤버(SDK ACL 스냅숏), 바꾸기 `{expectedRevision, requestId, entries}`. 공간 관리자. 추가된 사람·그룹에 `share` 알림, 빠진 사람의 알림 회수 |
| `GET /v1/spaces/{id}/sharing/requests/{requestId}` | 응답을 잃은 바꾸기의 결과 |

### 파일과 폴더 (AI-DO `domains/files/router.py`)

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/spaces/{id}/files?folder_id=` | AI-DO 탐색 응답: `space, current_folder, breadcrumbs, folders, files, all_folders, can_ingest, ingest_block_reason` |
| `POST /v1/spaces/{id}/files/search` | AI-DO 검색 요청·응답. `strategy`는 `keyword`만(이름·소유자·폴더·형식), `semantic`·`hybrid`는 422 `files_search_strategy_unsupported` |
| `POST /v1/spaces/{id}/files/folders` | 폴더 만들기 `{name, parent_id?, visibility}`(201). 하위 폴더는 상위 권한을 따른다 |
| `PATCH /v1/spaces/{id}/files/folders/{folderId}` | 이름 바꾸기 `{name}`, 이동 `{parent_id}`(보강, `null` = 루트). 권한 변경은 422 `files_visibility_change_not_allowed` |
| `DELETE /v1/spaces/{id}/files/folders/{folderId}` | 폴더와 하위 항목을 휴지통으로(204) |
| `PATCH /v1/spaces/{id}/files/{fileId}` | 이름 바꾸기 `{filename}`, 이동 `{folder_id}`(보강) |
| `DELETE /v1/spaces/{id}/files/{fileId}` | 휴지통으로(204) |
| `POST /v1/spaces/{id}/files/bulk-delete` | `{file_ids, folder_ids}`(각 500개까지) 휴지통으로(204) |
| `POST /v1/spaces/{id}/files/move` | `{file_ids, folder_ids, target_folder_id}` 이동(204, 보강) |
| `POST /v1/spaces/{id}/files/archive` | 묶음 다운로드 계획 `{entries:[{path,file_id,size_bytes}], file_count, total_size}`(500개까지). 파일을 한 건씩 받아 전송을 확인한다. 브라우저가 자동 다운로드를 막거나 전송을 확인하지 못하면 해당 파일에서 일시정지한다. 전송만 다시 확인하거나, 해당 파일을 다시 다운로드하거나, 건너뛰어 계속할 수 있다 |
| `GET /v1/spaces/{id}/files/{fileId}/download` | 5분짜리 다운로드 링크 `{url, expires_at}` → `runtime.download(url)` |
| `GET /v1/spaces/{id}/files/{fileId}/versions` | 버전 목록(보강) |
| `GET /v1/spaces/{id}/files/{fileId}/versions/{versionId}/download` | 그 버전의 다운로드 링크 |
| `GET /files/items/{fileId}/preview` | (fileApi) 이미지 바이트, 형식 서명 확인, 10MB까지. 그 밖은 415·413 |

### 업로드 (대용량 전송)

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /v1/spaces/{id}/files/uploads` | 예약 `{request_id, filename, content_type, size_bytes, folder_id?, visibility, file_id?}`(201; 같은 `request_id`면 같은 업로드 200). 10MiB까지 `single`, 그보다 크면 `parts`(2GiB까지). `file_id`는 새 버전 |
| `GET /v1/spaces/{id}/files/uploads/{uploadId}` | 상태와 커밋된 `received`(이어 보낼 위치) |
| `PUT /files/uploads/{uploadId}/content` | (fileApi) `single` 바이트 |
| `PUT /files/uploads/{uploadId}/parts?offset=N` | (fileApi) `parts`의 한 조각(8MiB까지). 화면은 요청별 전송 진행값을 표시하며 다시 보낼 위치는 서버가 확정한 `received`를 쓴다. `offset`이 `received`와 다르면 409 `files_part_offset` |
| `POST /v1/spaces/{id}/files/uploads/{uploadId}/complete` | 확정. `files_upload_finalizing`(409)이면 같은 업로드 ID로 다시 확정한다. 잠금 뒤 권한을 다시 확인하고 커밋한 다음 코어 파일을 연결한다. 파일 항목을 돌려준다 |
| `DELETE /v1/spaces/{id}/files/uploads/{uploadId}` | 끝나지 않은 업로드 취소(204). 완료된 업로드는 409 |

### 휴지통 (보강)

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/spaces/{id}/trash` | 지운 항목(내가 관리할 수 있는 것, 공간 관리자는 전부): 원래 위치·지운 사람·지운 시각 |
| `POST /v1/spaces/{id}/trash/restore` | `{file_ids, folder_ids}` 복원(204). 원래 폴더가 없거나 휴지통에 있으면 루트로 |
| `POST /v1/spaces/{id}/trash/delete` | 영구 삭제(204). 앱 행을 지우고 커밋한 뒤 코어 파일을 해제(연결된 파일은 `files_detach`, 아니면 `files_cancel`). 실패한 해제는 정리 작업으로 남아 다음 요청이 다시 한다 |
| `POST /v1/spaces/{id}/trash/empty` | 내가 지울 수 있는 휴지통 항목 전부 영구 삭제(204) |

### 플랫폼 → 앱 (읽기 전용, 플랫폼 호출만, 아니면 403)

| 메서드·경로 | 설명 |
| --- | --- |
| `POST /v1/file-access` | `{fileId, reference}` → `{allowed}`. 다운로드 링크의 바이트를 보내기 전 화면과 같은 규칙으로 다시 판단한다(휴지통·공간 역할·공개 범위·상위 폴더) |
| `POST /v1/search` | 통합 검색: 열 수 있는 파일 이름(최대 `limit`), 경로는 그 폴더 |
| `POST /platform/notification-check` | `space:<id>` 알림을 그 공간을 아직 열 수 있는 사람에게만 남긴다 |

플랫폼 호출의 그룹은 서명된 `identity.context.groups`로 판단한다.

## 개발과 검사

```
cargo build && cargo test                     # DB 시험은 #[ignore]
DRIVE_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored             # 일회용 PostgreSQL에서 HTTP·DB 시험
cargo fmt --package miy-app -- --check
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
```

화면 문구는 [scripts/gen-i18n-resources.py](scripts/gen-i18n-resources.py)가 AI-DO `resources.ts`에서 생성한다.
