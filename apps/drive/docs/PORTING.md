# AI-DO 파일(드라이브) 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 공식 앱 `drive`(이름 `드라이브`), 번들 0.12.0(2026-11-22 개정 SDK: 대용량 전송·다운로드 링크, 플랫폼 호출의 `identity.context.groups`, `notify_with_groups`), Rust SDK 0.4.0, 공용 UI 2.0.1
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M7~M11 공식 업무 앱"(M10), §2.2 드라이브 행, §2.3 파일 저장, "M2", "M12"
- 플랫폼 계약: [CORE-PERMISSIONS 대용량 전송(M10)](../../../docs/CORE-PERMISSIONS.md#대용량-전송m10), [docs/CONTRACT.md](CONTRACT.md) "Small attachments"(큰 파일 문단 포함), "Resource ACL and sharing", "Platform notifications", "Side panels, home cards and search"
- 원칙: AI-DO 구현을 그대로 옮기고, miy의 경계(바이트는 플랫폼 Storage, 신원·디렉터리, 앱 프레임, 앱별 DB) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다. 계획의 보강(이동·이름 변경, 휴지통, 대용량 전송)과 §2.3의 버전도 이번에 넣었고 따로 표시했다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 파일 관리자 화면(머리글·이동 경로·업로드 권한 선택, 새 폴더·폴더 수정·폴더 삭제, 끌어 놓기 업로드, 선택 막대(전체 선택·선택 크기·다운로드·삭제), 폴더·파일 타일, 이미지 미리보기(확대·축소)), 왼쪽 폴더 트리, 업로드 진행 패널(배치 하나씩, 접기·닫기, 완료·실패 알림), 파일 검색 화면(검색어·결과 순위·강조·점수·페이지·다운로드), 서버의 탐색·폴더·업로드·다운로드·미리보기·묶음 다운로드·일괄 삭제·삭제 규칙(개인용·워크스페이스 공유, 상위 폴더 권한 상속, 폴더 권한 고정, 소유자·워크스페이스 관리자 관리, 상위 폴더 소유자만 그 안에 추가, 상위 체인이 보여야 보임, 파일 이름 정리, 묶음 경로 계획).

**보강(AI-DO에 없던 것):** 업로드 취소와 실패 파일부터 다시 올리기(공통 컴포넌트 후보 "파일 업로드·다운로드 UI"), 대용량 전송(2GiB, 8MiB 조각 이어 올리기), 다운로드 링크(`runtime.download`), 파일·폴더 이동과 파일 이름 변경, 휴지통(복원·영구 삭제·비우기), 버전(새 버전 올리기·버전 목록·버전별 다운로드), 공간과 이름 기반 공유(공용 `ShareEditor`), 공간 공유 알림, 플랫폼 엔드포인트(`fileAccess`, `search`, `notificationCheck`).

**뺀 것(화면·API 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 본문·의미·하이브리드 검색(BM25, 벡터, 재순위화) | `search.py`의 `query_files`·`resolve_file_search_runtime`, `rag_*`, `search_projection.py`, `search_hooks.py`, `retrieval_contract.py` | 계획 §2.2: 본문·의미 검색은 M13. 이름·메타데이터 검색만 옮김(§3.8) |
| RAG 색인 상태와 폴링 | `rag_status.py`, `rag_sync.py`, `FILE_RAG_STATUS_PRESENTATION`, 타일의 RAG 배지 | 같은 이유 |
| 본문 추출 | `extraction_bootstrap.py`, `content_signature.py`(PDF 서명), `purge_file_retrieval_artifact` | 같은 이유 |
| 파일 코퍼스·일괄 적재·업로드/편집 허용 명단 | `corpora` 경로 전부, `bulk_ingest.py`, `FileCorpusIngressAccessDialog`, `FileCorpusUploadAccessSettingsView`, `file-corpus-upload-access-model`, `files-upload-grants-api` | RAG 자료실 기능. 드라이브 범위 밖 |
| 외부 원본 연동(mcloudoc 등 문서중앙화) | `external_*.py`, `source_access.py`, `FileManagerFileSourceMetadata`, `FileManagerFileAccessGrant` | 계획 §2.2 제외(문서중앙화 연계) |
| 문서 AI 대화 | `chat_retrieval.py`, `conversation_scope.py`, `FilesChatView`, `FilesRagSourcesArtifact` | M13 |
| AI-DO 셸 연동 | `index.ts`, `manifest.ts`(+spec), `routes.ts`(+spec), `upload-access-route.ts`, `app_catalog.py`, `workspace_app_gate` | miy는 manifest·포털이 담당 |
| `@pierre/trees` 폴더 트리 | `sidebar-folders.tsx`의 `FilesSidebarPierreTree` | §3.10 |
| 서버 ZIP 묶음 | `build_archive`, `POST /files/archive`의 ZIP 스트림 | §3.5 |

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>`를 앱 `web/src/<경로>`로 옮겼다. "그대로"는 import 경로 외 변경이 없다는 뜻이다.

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/files/views/FileManagerView.tsx` | 같은 경로 | 수정: RAG 배지·코퍼스 대화상자 제거, 이름 바꾸기·이동·버전 버튼과 대화상자, 선택 막대의 이동 |
| `app-modules/files/views/useFileManagerController.ts` | 같은 경로 | 수정: 런타임(`runtime.api`, 공간 경로, `?folder=`), 다운로드 링크, Blob 미리보기, 미저장 보고, 보강 동작. RAG 폴링 제거 |
| `app-modules/files/views/FileManagerWorkflow.ts`(+spec) | 같은 경로 | 그대로 |
| `app-modules/files/views/file-manager-view-model.ts`(+spec) | 같은 경로 | RAG 부분 제거 |
| `app-modules/files/views/FileImagePreviewDialog.tsx`, `components/media/FullscreenImageDialog.tsx` | 같은 경로 | 그대로(이미지는 Blob URL, 닫을 때 해제) |
| `app-modules/files/views/FileSearchView.tsx`(+spec) | 같은 경로 | 수정: 공간·런타임 클라이언트, 검색 방식 선택은 방식이 하나라 숨김, 방식 라벨 `filename`·`metadata` 추가 |
| `app-modules/files/views/useFileSearchController.ts`(+spec) | 같은 경로 | 입력만 수정(`client`·`openDownload` 필수, `token` 자리에 설치 ID) |
| `app-modules/files/views/file-search-view-model.ts`(+spec) | 같은 경로 | 방식은 `keyword`만, 기본값 `keyword` |
| `app-modules/files/file-upload-session.ts`(+spec) | 같은 경로 | 수정: 업로드 핸들(요청·업로드 ID), 실패 위치에서 재개, 취소 상태 |
| `app-modules/files/file-upload-provider.tsx` | 같은 경로 | 수정: 취소·다시 시도 버튼, 대용량 전송 클라이언트, 미저장 보고 |
| `app-modules/files/folder-tree-model.ts`(+spec) | 같은 경로 | 그대로 |
| `app-modules/files/folder-tree-row.tsx`, `sidebar-folders.tsx` | 같은 경로 | 수정: 링크 → `runtime.navigate`, 항상 행 목록(§3.10), 늦은 응답 무시 |
| `app-modules/files/sidebar.tsx` | 같은 경로(`DriveSidebar`) | 재작성: 프레임 왼쪽에 공간 선택·전체 파일·파일 검색·휴지통·폴더 트리, 공간 관리 대화상자 |
| `app-modules/files/api/files-api.ts`(+spec) | 같은 경로 | 수정: `token`→런타임 클라이언트, 워크스페이스 → 공간 경로, 코퍼스 API 제거, 보강 API 추가 |
| (없음) | `api/file-transfer.ts`(+spec), `views/FileActionDialogs.tsx`, `views/TrashView.tsx`, `views/file-downloads.ts`, `platform/drive/drive-context.tsx`, `platform/runtime/app-location.ts`(+spec), `AppRoot.tsx` | miy 연결과 보강 |
| `platform/api/client.ts`, `platform/i18n/*`, `platform/time/*`, `platform/runtime/{runtime-context,navigation-state}.tsx`, `lib/utils.ts`, `test-setup.ts` | 같은 경로 | 게시판 포팅본을 그대로 사용 |
| `platform/format/byte-size.ts`(+spec) | 같은 경로 | PMS 포팅본을 그대로 사용 |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: [scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py) |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/files/…` | miy `src/…` |
| --- | --- |
| `router.py` | `main.rs`(경로), `files.rs`, `uploads.rs`, `search.rs` |
| `service.py`: `list_accessible_folders`·`list_accessible_files`·`require_file_access`·`_record_visible`·`_ensure_can_manage_record`·`_ensure_can_delete_rows`·`_ensure_can_ingest_under_parent`·`resolve_file_ingress_policy`·`_lock_tree_delete_scope`·`_normalize_name`·`normalize_visibility` | `tree.rs` |
| `service.py`: `create_folder`·`update_folder`·`delete_folder`·`delete_items`·`delete_file`·`list_archive_entries`·`build_archive` | `files.rs` |
| `service.py`: `upload_file`, `router._read_upload_to_spooled_file` | `uploads.rs` |
| `service.py`: `remove_storage_objects`·`enqueue_storage_cleanup_job`, `storage_adapter.py` | `cleanup.rs`, `versions.rs` |
| `browse_projection.py` | `tree.rs`(직렬화), `files::browse` |
| `archive_planner.py` | `archive_planner.rs` |
| `content_access.py`(미리보기 형식·서명) | `files::preview` |
| `search.py`(요청·응답 모양) | `search.rs` |
| `models.py` | `migrations/0003_drive.sql` |
| `auth` 워크스페이스·역할(`app_space_roles.py`, `WorkspaceUserBinding`) | `access.rs`, `spaces.rs`(SDK ACL) |
| `users` 표 이름 조회 | `people.rs`(플랫폼 디렉터리) |
| `core/i18n.localized_http_exception` | `error.rs` |
| (없음) | `platform.rs`(`fileAccess`·`notificationCheck`), `notify.rs`, `trash.rs` |

## 3. 원본과 달라진 점과 이유

### 3.1 데이터 모델

`migrations/0002_acl.sql`은 SDK `acl.sql` 그대로다. `migrations/0003_drive.sql`:

| 표 | AI-DO | miy |
| --- | --- | --- |
| `file_manager_folders` | `id, workspace_id(FK), retrieval_partition_id, corpus_id, parent_id(FK self, cascade), owner_id(FK users), name(255), visibility, created_at, updated_at, deleted_at` | `workspace_id`는 `drive_spaces` 참조(공간 삭제 시 cascade), `owner_id`는 `text`, RAG 열 없음. 휴지통 열 `trash_id`·`trash_root`·`trashed_by` 추가 |
| `file_manager_files` | `…, folder_id(FK, set null), filename(512), content_type(160), size_bytes(int), storage_key(unique), visibility, extraction_*, deleted_at` | `storage_key`·추출 열 없음. `current_version_id`(현재 버전), `size_bytes`는 `bigint`(2GiB), 휴지통 열 추가 |
| (없음) | — | `file_manager_file_versions`: 버전마다 코어 파일 ID(`storage_file_id`), 예약 요청 ID(`request_id`, 예약 전에 기록), 연결 요청 ID, 전송 방식(`single`·`parts`), 상태(`reserving`·`uploading`·`ready`·`cancelled`), 연결 여부, SHA-256 |
| `file_manager_storage_cleanup_jobs` | `id, storage_key, status, attempts, last_error, next_retry_at` | 키가 코어 파일 ID, 연결 참조 보관. 성공하면 행을 지운다 |
| (없음) | — | `drive_spaces`(공간) |

- 사용자 ID는 miy 사용자 ID(`Identity::user_id`)다. 사용자 표는 코어 DB에 있어 FK를 둘 수 없다.
- 시각은 AI-DO처럼 naive UTC(`timestamp`), ID는 UUID v4 문자열이다.
- 파일 행은 첫 버전이 확정될 때 보인다(`current_version_id`가 생김). 끝나지 않은 첫 업로드를 취소하면 행은 `deleted_at`만 있고 휴지통 ID가 없어 어디에도 보이지 않는다.

### 3.2 공간과 권한 (워크스페이스 대응)

| AI-DO | miy |
| --- | --- |
| 파일은 워크스페이스에 속함. 사용자는 현재 워크스페이스에서 연다 | 파일은 **공간**(`drive_spaces`)에 속함. 경로에 공간 ID(`/v1/spaces/{id}/files/...`) |
| 워크스페이스 역할 `viewer`·`member`·`admin`·`owner`(`app_space_roles.py`) | 공간 역할(SDK 자원 ACL, kind `space`): `viewer`(`space.read`), `member`(+`files.write`), `admin`(+`space.admin`, `acl.manage`). `owner`는 보호된 만든 사람(`admin`)으로 합침 |
| 워크스페이스 멤버 바인딩 | ACL 사용자·그룹 부여. 공용 `ShareEditor`로 이름을 검색해 추가(공통 컴포넌트 후보 "이름 기반 공유 대상 선택") |
| 워크스페이스 관리자: 남의 개인 파일 보기, 모든 항목 관리 | 공간 `admin` 같음 |
| 멤버 이상: 루트에 추가. 폴더 안은 그 폴더 소유자나 관리자 | 같음(`viewer`는 추가 못 함 → `workspace_access_required`) |
| 플랫폼 관리자(`legacy_shared` 워크스페이스 예외) | 플랫폼 관리자 표시(`Identity::admin`)는 아무 권한도 주지 않는다(시험으로 확인) |
| 워크스페이스는 관리 콘솔에서 만듦 | 앱을 쓸 수 있는 누구나 공간을 만들고 소유자가 된다. 이름 바꾸기·멤버·삭제는 공간 관리자 |

- **역할 판단:** 프레임 요청은 그 사람의 현재 그룹을 디렉터리에서 한 번 읽고(`memberships`) 부여 표에서 가장 높은 역할을 SQL로 고른다(SDK ACL과 같은 술어). 디렉터리가 답하지 않으면 503으로 닫는다. 쓰기는 공간 행(`FOR SHARE`)과 자기 행(`FOR UPDATE`, id 순서)을 먼저 잠근 뒤 SDK `require_in`(ACL 공유 잠금)으로 다시 확인하고 바꾼다(리뷰 지적 "잠금 뒤 현재 권한 재확인").
- **플랫폼 호출:** `fileAccess`·`search`·`notificationCheck`는 디렉터리를 부를 수 없으므로 코어가 서명해 보내는 현재 그룹(`identity.context.groups`)과 직접 부여로 판단한다. `platform:directory:read`가 승인되지 않은 설치에서는 직접 부여만 센다.
- 공간 삭제는 그 공간의 폴더·파일·버전·휴지통을 함께 지우고(cascade), 커밋 뒤 코어 파일을 해제한다.

### 3.3 업로드와 대용량 전송

AI-DO는 `POST /files/upload` 한 번에 multipart로 250MB까지 받아 MinIO에 넣었다(XHR 진행률). miy는 바이트를 플랫폼 Storage에만 두고 JSON 한도(4MiB)를 넘지 않게 나눈다.

1. **예약** `POST /v1/spaces/{id}/files/uploads`: 화면이 만든 요청 ID(`requestId()`)로 파일·버전 행을 먼저 커밋하고(`reserving`) 코어를 예약한다. 10MiB까지는 `files_create`(`single`), 그보다 크면 `files_create_parts`(`parts`, 2GiB까지). 응답을 잃으면 같은 요청 ID로 다시 보내 같은 업로드를 받는다(서버는 `files_by_request`로 코어 결과를 찾는다). 코어가 거절하면(한도 409 → `files_storage_quota_exceeded`, 저장 공간 부족 503 → `files_storage_unavailable`) 버전을 취소하고 거절을 알린다.
2. **바이트**(`runtime.fileApi`): `single`은 `PUT /files/uploads/{id}/content` 한 번(SDK `files_upload` 스트림). `parts`는 `PUT /files/uploads/{id}/parts?offset=N`에 8MiB 이하 조각 하나씩. 서버가 코어의 `received`와 `offset`을 비교해 다르면 409 `files_part_offset`, 같으면 `files_upload_part`로 쓰고 새 `received`를 돌려준다. 실패하면 화면이 `GET .../uploads/{id}`로 커밋된 위치를 읽고 거기서 이어 보낸다(진전 없이 5번 실패하면 중단). 파일 게이트웨이는 `Content-Type`·`Content-Length`와 쿼리만 전달하므로 위치는 쿼리로 보낸다. 바이트는 올린 사람만 보낼 수 있다.
3. **확정** `POST .../uploads/{id}/complete`: `parts`는 `files_complete`로 Core에 SHA-256 준비를 요청한다. 준비 중에는 `files_upload_finalizing`을 반환하고 화면이 같은 업로드로 확정을 다시 요청한다. `single`은 상태를 확인한다. 파일·버전 행과 상위 폴더를 잠그고 권한을 다시 확인한 뒤 버전을 현재 버전으로 커밋하고, **그 다음에** `files_attach(코어 파일, "drive:version:<id>", 연결 요청 ID)`를 부른다. 연결 응답이 불확실하면 다음 다운로드 링크 요청이 같은 연결 요청 ID로 마저 연결한다.
4. **취소** `DELETE .../uploads/{id}`: 앱 참조가 없으므로 버전을 취소로 커밋한 뒤 `files_cancel`. 완료된 업로드는 409(파일은 삭제로). 끝나지 않은 예약은 코어에서 24시간 뒤 만료된다.

- 화면(업로드 패널)은 배치를 한 번에 하나 돌리고(AI-DO와 같음), **취소**하면 진행 중 파일의 예약을 해제하고 이미 올린 파일은 남긴다. **다시 시도**하면 실패한 파일부터 같은 요청 ID로 이어 간다(리뷰 지적 "같은 요청 ID로 복구, 삭제하지 않음"). 실패한 배치를 닫으면 그 파일의 끝나지 않은 예약을 해제한다. 진행 중 업로드는 미저장 작업으로 보고한다.
- `runtime.fileApi`는 `fetch`라 업로드 진행 이벤트가 없다. 진행률은 조각 단위로 오르고, 10MiB 이하 파일은 끝날 때 100%가 된다(AI-DO는 바이트 단위).
- 파일 이름은 AI-DO `safe_filename` 그대로 정리한다. 크기 한도는 AI-DO의 250MB 대신 플랫폼의 2GiB(`files_file_size_limit_exceeded`, 413).

### 3.4 다운로드와 `fileAccess`

AI-DO는 서명된 프록시 URL(`/files/content/{id}?expires&signature`)을 주고 열 때 워크스페이스 멤버십을 다시 확인했다. miy는 코어의 다운로드 링크를 쓴다.

- `GET .../files/{id}/download`(버전은 `.../versions/{versionId}/download`): AI-DO `require_file_access`와 같은 판단 뒤 `files_download_link(코어 파일, 참조)`의 `url`을 돌려주고 화면이 `runtime.download(url)`을 부른다. 바이트는 브라우저로 바로 흐르고 메모리 Blob을 만들지 않는다. 링크는 5분이다.
- `fileAccess`(`POST /v1/file-access`): 코어가 링크의 바이트(이어 받기 포함)를 보내기 직전에 묻는다. 코어 파일 → 버전 → 파일을 찾고, 참조가 그 버전의 것인지, 파일이 휴지통에 없는지, 그 사람이 공간 역할을 아직 갖는지(직접 또는 서명된 현재 그룹), 공개 범위와 상위 폴더 체인이 보이는지를 화면과 같은 규칙으로 판단한다. AI-DO 시험 `test_signed_file_content_rechecks_workspace_membership`을 이 경로로 옮겼다.
- 로컬 미리보기에는 `runtime.download`가 없다. 화면은 "이 화면에서는 다운로드를 시작할 수 없습니다"를 보인다.

### 3.5 묶음 다운로드

AI-DO는 선택(파일·폴더 하위)을 서버에서 ZIP으로 묶어(최대 500개·512MB) 한 번에 내려보냈다. 플랫폼 파일 게이트웨이는 응답 10MiB·20초 한도이고 앱이 쓸 수 있는 스트리밍 묶음 경로가 없어, **파일을 하나씩 내려받는다**(지시: "한도 안에서 스트리밍할 수 없으면 하나씩").

- `POST .../files/archive`가 AI-DO `list_archive_entries`·`plan_archive_entries` 그대로 항목(경로·파일 ID·크기)을 계산한다. 접근 확인·폴더 펼치기·중복 제거·경로 정리·충돌 번호가 원본과 같다.
- 화면은 파일마다 링크를 받아 `runtime.downloadAndWait`로 Core의 서버 전송을 확인한 뒤 다음 파일을 시작한다. 상태 확인이 끊기면 같은 링크에서 서버 전송 결과만 다시 확인한다. Core가 실패를 확인하면 새 링크로 다운로드를 다시 시작할 수 있다. 확인할 수 없는 파일은 건너뛰거나 대기열을 중지할 수 있다. 브라우저에서 진행 중인 다운로드는 브라우저에서 관리한다. 브라우저가 여러 파일의 자동 다운로드를 차단하거나 로컬 저장에 실패할 수 있다. 폴더 구조는 브라우저 다운로드에 남지 않는다.
- 한도는 파일 500개만 남겼다(`files_archive_limit_exceeded`, 413). 512MB 총량은 서버 압축 때문이었으므로 없앴다.

### 3.6 휴지통과 Storage 해제 (보강)

- 삭제(단건·일괄·폴더)는 AI-DO처럼 `deleted_at`을 적되, 선택한 항목마다 휴지통 항목(`trash_id`)을 만들어 그 하위 전체를 묶는다. 선택한 폴더 안에서 선택한 파일은 그 폴더 항목에 들어간다. 삭제 권한은 AI-DO `_ensure_can_delete_rows`(관리자, 아니면 하위 전체가 내 것).
- 휴지통 목록은 내가 관리할 수 있는 항목(공간 관리자는 전부)과 원래 위치·지운 사람을 보인다. 복원은 항목 전체를 되돌리고, 원래 폴더가 없거나 휴지통에 있으면 루트로 돌린다.
- 영구 삭제·비우기는 행을 지우고 같은 트랜잭션에서 그 파일의 모든 버전을 정리 작업으로 적는다. **커밋한 뒤** 코어 파일을 해제한다: 연결된 파일은 `files_detach(참조)`, 연결되지 않은 파일은 `files_cancel`. 실패하면 작업이 남고(시도 수·다음 시각) 다음 휴지통·공간 요청이 다시 한다. 먼저 휴지통에 간 하위 항목은 폴더와 함께 사라지지 않고 루트로 옮겨 자기 항목으로 남는다.
- AI-DO는 삭제 즉시 저장소 객체를 지웠다. miy는 휴지통에 있는 동안 바이트와 한도를 유지한다.
- 자동 비우기(보존 기간)는 없다. 요청 밖 작업에는 서비스 주체가 필요하다(§6).

### 3.7 버전 (§2.3 보강)

- 같은 파일에 새 버전을 올리면(`file_id`를 준 예약) 새 코어 파일이 생기고, 확정 때 그 버전이 현재 버전이 된다. 파일 이름은 그대로 두고 버전마다 원래 이름·크기·형식·SHA-256·올린 사람을 남긴다. 새 버전은 파일을 관리할 수 있는 사람(소유자·공간 관리자)이 올린다.
- 버전 목록과 버전별 다운로드는 파일을 읽을 수 있는 사람에게 열려 있다. 이전 버전으로 되돌리기는 없다(필요하면 내려받아 새 버전으로 올린다).

### 3.8 검색

- AI-DO `POST /files/search`의 요청·응답 모양(`query, strategy, page, page_size`, 메타데이터 필터, `hits[rank, file_id, filename, snippet{text, highlights}, score, methods, …], has_more, max_ranked_results(100), latency_ms`)과 검증(쿼리 1~2000자, 페이지 크기 1~50, 상위 100개 창, 날짜 범위 순서, 알 수 없는 필드 거절)을 그대로 둔다.
- `keyword`만 답한다: 검색어의 모든 낱말이 파일 이름이나 메타데이터(소유자 이름, 폴더 경로, 형식)에 있어야 한다. 순위는 이름 일치 → 이름 시작 → 이름에 모든 낱말 → 메타데이터로만 일치, 같으면 최근 수정순. 강조 범위는 브라우저 문자열(UTF-16) 기준이다. `author`는 디렉터리 이름 검색, `document_type`은 형식 접두어, `authored_from/to`는 올린 시각에 대응한다. `department`·`source_kind`(`files` 외)는 대응 값이 없어 결과가 없다. `semantic`·`hybrid`는 422 `files_search_strategy_unsupported`(본문·의미 검색은 M13). 화면의 기본 방식도 `keyword`이고 선택 단추는 숨겼다.
- 범위는 AI-DO처럼 현재 공간이다. 결과는 목록과 같은 접근 규칙을 지난 파일만이다.
- **통합 검색**(`platformEndpoints.search`, M12): 열 수 있는 공간 전체에서 파일 이름이 모든 낱말을 포함하는 파일(최근순, `LIKE`의 `%`·`_`·`\` 이스케이프)을 공개 범위·폴더 체인으로 거른 뒤 `limit`개까지 돌려준다. 제목은 파일 이름, 보조 문구는 `공간 · 폴더 경로`, 경로는 그 폴더(`/spaces/<id>?folder=<id>`), 종류는 `파일`/`File`.

### 3.9 알림과 플랫폼 엔드포인트

| 종류 | 언제 | 받는 사람 | 묶음 키 | 제목 |
| --- | --- | --- | --- | --- |
| `share` | 공간 멤버 바꾸기에서 사람·그룹이 추가됨 | 추가된 사람(본인 제외)과 추가된 그룹의 현재 멤버(`notify_with_groups`) | `space:<id>` | `<이름>님이 "<공간>" 공간을 공유했습니다` |

- 경로는 `/spaces/<id>`. 공간을 열면(루트 탐색) 읽음 처리한다. 사람이 빠지면 그 사람에게서 회수, 공간을 지우면 모두에게서 회수한다. 그룹이 빠졌을 때는 앱이 그 구성원을 알 수 없어 회수하지 못하고, `notificationCheck`가 다음 열기 때 숨긴다.
- 제목에 파일 내용은 없다. 받는 사람의 언어를 앱이 모르므로 한국어로 보낸다(게시판과 같은 한계).
- 발행은 업무 커밋 뒤 같은 요청 안에서 하고, 실패해도 업무를 되돌리지 않는다. 결과가 불확실하면 같은 요청 ID로 조회만 한다.
- `notificationCheck`: `space:<id>`이고 `resourceId`가 같으며 그 사람이 지금 그 공간을 열 수 있으면 허용(읽기 전용, 플랫폼 호출만).
- 홈 카드는 두지 않았다(AI-DO 파일 모듈에 없음).

### 3.10 화면 경로와 구성

- AI-DO `/w/:workspaceSlug/files`(+`?folder=`, `?view=search&q=&strategy=&page=`)를 `/spaces/<id>`로 옮기고 쿼리는 그대로 쓴다. `/`는 마지막(없으면 첫) 공간을 연다(브라우저 저장소는 편의용, 실패해도 동작). 휴지통은 `/spaces/<id>/trash`. react-router 대신 `runtime.location`·`runtime.navigate`.
- AI-DO는 포털 사이드바에 `files-all`·`files-search`·`files-chat`과 폴더 트리를 그렸다. 프레임에는 포털 사이드바가 없어 앱 왼쪽에 같은 항목(`sidebar-submenu-item`)과 공간 선택, 휴지통을 그린다. 문서 AI 대화는 뺐다.
- 폴더 트리: AI-DO는 경로가 겹치지 않으면 `@pierre/trees`(섀도 DOM 웹 컴포넌트, 약 2MB)로, 겹치면 자체 `FolderTreeRow` 목록으로 그렸다. 프레임 CSP에서 확인되지 않은 웹 컴포넌트를 들이지 않고 AI-DO에 이미 있는 목록 경로만 쓴다. 의존성은 늘지 않았다.
- 이미지 미리보기는 서버가 형식(PNG·JPEG·GIF·WebP)과 파일 서명을 확인한 바이트를 파일 게이트웨이로 받아 Blob URL로 보인다(AI-DO는 `inline` URL). 백엔드 다운로드 한도 때문에 10MB까지이고, 넘으면 413 `files_preview_too_large`로 다운로드를 안내한다. 눈 단추 표시 규칙(`image/*`)은 AI-DO 그대로다.
- 확인은 AI-DO처럼 `window.confirm`(프레임 sandbox에 `allow-modals` 있음). 문구 "…삭제할까요?"는 그대로이며 실제로는 휴지통으로 간다.
- **미저장 보고(miy 계약):** 진행 중 업로드, 폴더·이름·이동·공간 대화상자 입력, 휴지통 작업, 공간 멤버 편집을 `runtime.setNavigationState`로 합쳐 보고한다. AI-DO에는 이동 확인이 없었다.
- 늦은 응답: 탐색은 요청 순번으로, 폴더 트리·휴지통·공간 목록은 요청 공간과 순번으로 늦은 응답을 버린다. 대화상자(폴더 수정·이동·이름 바꾸기)와 휴지통 작업은 연 순간의 대상으로 실행한다(리뷰 지적 "늦은 응답이 보이는 항목·작업 대상을 바꾸지 않음").
- 날짜·시간 표시는 `runtime.settings`(언어·시간대·날짜 형식)를 따른다.

### 3.11 오류 응답과 문구

- 게시판과 같은 방식이다: `{"error":{"code","message"}}`, 코드는 AI-DO 코드의 `.`을 `_`로, `message`는 AI-DO 영어 카탈로그 문장, 화면은 `errors` 네임스페이스로 번역한다. 요청 검증 실패는 422 `validation_request_invalid`.
- 상태 코드와 검증(이름 1~255자 후 공백 제거, 파일 이름 512자, 권한 값, 선택 500개, 선택 없음 422)은 AI-DO와 같다.
- 새 코드: `files_space_not_found`, `files_workspace_access_required`, `files_move_into_itself`, `files_scope_changed`, `files_trash_item_not_found`, `files_download_frame_required`, `files_preview_too_large`, `files_storage_quota_exceeded`, `files_storage_unavailable`, `files_upload_*`, `files_part_*`, `files_search_strategy_unsupported`, `files_platform_only`, `acl_*`(SDK).
- 화면 문구는 [생성 스크립트](../scripts/gen-i18n-resources.py)가 AI-DO `resources.ts`의 `common`과 `apps.files`를 그대로 복사한다(코퍼스·RAG 키도 따라오지만 쓰지 않는다). 워크스페이스를 말하는 두 문장(`visibility.workspace`, `ingestBlocked.workspaceAccessRequired`)만 공간으로 바꿨다. 탐색 라벨은 AI-DO `shell.nav.files-all`·`files-search`에서 가져왔다. miy 문구는 `apps.drive`와 `errors`에 있다.

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 워크스페이스 → 앱 공간 + SDK ACL(역할 `viewer`·`member`·`admin`) | AI-DO 워크스페이스 역할 순위를 그대로 옮길 수 있고 그룹 공유가 된다. `owner`는 보호된 만든 사람으로 충분 |
| 누구나 공간을 만든다 | AI-DO 사용자는 늘 워크스페이스가 있었다. 플랫폼 관리자 표시를 쓰지 않고 시작할 수 있는 가장 단순한 방법 |
| 프레임 요청은 그룹을 한 번 읽고 SQL로 역할 판단, 쓰기는 SDK `require_in`으로 잠금 뒤 재확인 | 읽기마다 역할별 ACL 호출을 여러 번 하지 않으면서 쓰기의 경합을 막는다 |
| 플랫폼 호출은 서명된 `identity.context.groups` | 2026-11-22 개정 SDK. 그룹으로 들어온 사람도 다운로드·통합 검색·알림 재확인이 된다 |
| 10MiB까지 `single`, 그보다 크면 `parts`(8MiB) | 계약(CORE-PERMISSIONS M10)과 지시 |
| 이어 올리기 위치는 쿼리(`offset`) | 파일 게이트웨이가 `Content-Range`를 전달하지 않는다 |
| 묶음 다운로드는 파일마다 링크 | 서버 ZIP을 플랫폼 한도 안에서 스트리밍할 수 없다(§3.5) |
| 휴지통: 선택 항목마다 묶음, 영구 삭제 때 커밋 뒤 해제 | 지시("커밋 뒤 detach, 그 전에는 안 함"). 해제 실패는 작업으로 남겨 복구 |
| 버전은 파일 행 + 버전 행(코어 파일 ID) | §2.3 "버전은 새 파일 UUID를 가리키는 행" |
| 미리보기는 서명 확인한 10MB 이하 래스터 | AI-DO 안전 형식·서명 규칙 + 백엔드 다운로드 한도 |
| `@pierre/trees` 대신 AI-DO의 행 목록 | §3.10 |
| 홈 카드 없음 | AI-DO 파일 모듈에 없다 |
| 의존성은 AI-DO·템플릿 버전 | i18next 26.0.8, react-i18next 17.0.6, lucide-react 0.546.0, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library 10.4.0/16.3.0. React·Vite·Tailwind·Radix는 템플릿·공용 UI 버전. Rust는 템플릿 구성에 `chrono`·`uuid`(시험에 `sha2`·`hex`·`jsonwebtoken`·`reqwest`) |
| 공용 UI 설치는 플래너·게시판과 같은 후보 소스 동기화 | `registry/components/ui/2.0.1`의 바이트와 SHA-256을 고정한다. 게시 후 실제 서버에서 `miy component sync`로 digest를 확인한다 |
| 보호 파일은 새 `app init` 결과로 맞춤 | 작업 중 저장소 번들 0.12.0의 CONTRACT·SDK가 개정되어(d7fbdfe) 새 init의 보호 파일과 `miy.bundle.json`을 그대로 복사했다 |
| DB 시험은 `#[ignore]` + `DRIVE_TEST_DATABASE_URL`, 가짜 코어 | 서버 빌드의 `cargo test`에는 DB가 없다. 디렉터리·알림·Storage(`single`·`parts`·확정·연결·해제·다운로드 링크)는 시험 안의 가짜 코어가 SDK의 실제 HTTP 경로로 받는다 |

## 5. 테스트 대응

| AI-DO | miy | 비고 |
| --- | --- | --- |
| `test_file_manager.py::test_file_manager_upload_download_and_delete_with_rag_job` | `api_tests::upload_download_and_delete` | RAG 작업 확인 대신 다운로드 링크·코어 연결 참조·휴지통 |
| `…::test_file_manager_image_preview_uses_inline_same_origin_content_url` | `image_preview_returns_inline_raster_bytes_only` | + SVG 415, 서명 불일치 415 |
| `…::test_workspace_shared_file_is_readable_but_not_deletable_by_member` | 같은 이름 | |
| `…::test_child_folder_and_upload_inherit_parent_visibility` | 같은 이름 | |
| `…::test_folder_visibility_changes_are_rejected` | 같은 이름 | |
| `…::test_private_file_is_hidden_from_workspace_member` | 같은 이름 | + 개인 폴더 안 파일, 관리자는 봄 |
| `…::test_signed_file_content_rechecks_workspace_membership` | `download_access_is_rechecked_when_the_link_is_used` | `fileAccess`로. + 참조·다른 사람·휴지통 |
| `…::test_file_manager_archive_and_bulk_delete_selected_items` | `archive_plan_and_bulk_delete_selected_items` | ZIP 내용 대신 계획 경로 |
| `…::test_upload_rejects_stream_once_file_size_limit_is_exceeded` | `upload_rejects_files_over_the_size_limit` | 예약 크기로(2GiB) |
| `…::test_archive_sanitizes_windows_paths_and_rejects_large_expansions` | `archive_sanitizes_windows_paths` | 한도 부분은 상수를 바꿀 수 없어 옮기지 않음 |
| `…::test_workspace_member_cannot_write_inside_owner_shared_folder` | 같은 이름 | + `viewer`는 루트에도 못 올림 |
| `…::test_delete_queues_storage_cleanup_job_when_minio_delete_fails` | `permanent_delete_releases_storage_after_commit_and_retries_failures` | 영구 삭제 커밋 뒤 해제, 실패 작업의 재시도 |
| `…` RAG·추출 시험 5개(`reports_rag_status…`, `adopted_file_ready…`, `corpus_workspace_move…`, `active_files_gate…`, `extraction_result…`, `upload_pdf_signature…`) | 옮기지 않음 | 기능 제외(§1) |
| `test_file_archive_planner.py`(5) | `archive_planner.rs` 5개 + 2개(이름 정리, 폴더 번호) | 같은 입력에 같은 결과 |
| `test_files_search.py`, 코퍼스·업로드 허용·일괄 적재·외부 원본 시험 | 옮기지 않음 | 기능 제외. 검색 모양은 `name_and_metadata_search_returns_only_readable_files` |
| (miy 추가) | `large_upload_sends_parts_from_the_committed_offset`, `upload_is_recovered_by_its_request_id_and_cancel_releases_it`, `new_versions_keep_history_and_download_by_version`, `trash_restores_items_to_their_folder_or_the_root`, `rename_and_move_follow_the_manage_and_upload_rules`, `name_and_metadata_search_returns_only_readable_files`(통합 검색 포함), `sharing_a_space_notifies_and_retracts_and_is_rechecked`, `group_grants_apply_and_the_admin_flag_grants_nothing`(그룹 알림·서명 그룹 포함), `deleting_a_space_releases_its_files_after_commit`, 단위 시험(역할, 이미지 서명, 휴지통 루트, 강조 오프셋, LIKE 이스케이프, 알림 제목, 오류 코드, 시각) | |
| 화면 `FileManagerWorkflow.spec.ts`, `folder-tree-model.spec.ts` | 같은 경로, 그대로 | |
| `file-manager-view-model.spec.ts` | 같은 경로 | RAG 사례 제외 |
| `file-upload-session.spec.ts` | 같은 경로 + 2개 | 클라이언트·공간 입력, 실패 파일부터 재개, 취소 |
| `file-search-view-model.spec.ts`, `useFileSearchController.spec.ts`, `FileSearchView.spec.tsx` | 같은 경로 | 방식은 `keyword` |
| `api/files-api.spec.ts` | 같은 경로(2 + 3) | `fetch` 대신 `runtime.api`·`fileApi` |
| (miy 추가) | `api/file-transfer.spec.ts`(조각·응답 유실 재개·요청 ID 재사용·취소·재시도 분류), `platform/runtime/app-location.spec.ts`, `platform/api/api-error.spec.ts`, 플래너·PMS 포팅의 `time`·`byte-size` 시험 | |

옮기지 않은 화면 시험: `manifest.spec.ts`, `routes.spec.tsx`, `sidebar.spec.tsx`(셸 연동 제외), `FilesChatView.spec.tsx`, `FilesRagSourcesArtifact.spec.tsx`, `FileCorpus*.spec.tsx`, `file-corpus-upload-access-model.spec.ts`, `files-upload-grants-api.spec.ts`(기능 제외).

## 6. 남은 일과 필요한 플랫폼 변경

- **실제 프레임 확인(계획 절차 9):** 단위·HTTP·jsdom 시험과 프로덕션 빌드까지 확인했다. 실제 플랫폼 프레임에서 `runtime.fileApi` PUT 본문(10MiB·8MiB 조각), `runtime.download` 링크와 `fileAccess` 왕복, 실제 코어의 `files_create_parts`·`files_complete`·한도 거절, 2GiB에 가까운 파일, 여러 파일 연속 다운로드의 브라우저 동작, 포털 이동 확인을 확인해야 한다.
- AI-DO 기준 화면(가상 계정·가상 데이터)과의 나란한 비교, 카탈로그 공식 릴리스·dev 설치·권한 인수·prod 승격.
- **플랫폼 변경 필요(이 앱에서 하지 않음):**
  1. SDK `CallError`가 코어 오류 코드를 버리고 상태만 준다. 예약 409를 한도 초과로 간주했다(`file_quota_exceeded`와 다른 409를 구분 못 함). 코어 코드를 노출하면 정확해진다.
  2. `runtime.fileApi`에 업로드 진행 콜백이 없다(`fetch`). 바이트 단위 진행률이 필요하면 런타임 지원이 필요하다.
  3. 여러 파일을 한 번에 받는 스트리밍 묶음(ZIP) 다운로드 링크가 없다. 생기면 AI-DO의 한 파일 묶음을 되살릴 수 있다.
  4. 10MB를 넘는 이미지 미리보기: 다운로드 링크는 `attachment`이고 `files_download`는 10MiB 한도다. 이미지용 `inline` 링크가 필요하다.
  5. 받는 사람 언어별 알림 제목(또는 플랫폼이 번역하는 키+인자). 그룹 단위 알림 회수(빠진 그룹 구성원에게서 회수).
  6. 요청 밖 작업(서비스 주체): 휴지통 자동 비우기, 실패한 Storage 해제의 주기 재시도, 만료된 예약의 앱 행 정리. 지금은 다음 사용자 요청이 처리한다.
  7. 서버 빌드가 이 앱의 npm·crate 고정본을 받을 수 있어야 한다(`pnpm-lock.yaml`·`Cargo.lock`).
  8. 카탈로그(`registry/apps/catalog.json`)의 `drive` 항목에 릴리스를 기록하는 일(소스 고정 뒤).
