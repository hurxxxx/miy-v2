# AI-DO 문서 포팅 기록

2026-11-24 추가. 0.2.0부터 앱 서버가 리소스별 Yjs 공동편집과 커서를 제공한다. 아래 표와 제외 범위는 최초 포팅 시점의 기록이다. 현재 동작은 [앱 README](../README.md)에서 확인한다.

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 공식 앱 `docs`(이름 `문서`), 번들 0.12.0(2026-11-22 개정본으로 재동기화), Rust SDK 0.4.0, 공용 UI 2.0.0
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M7~M11 공식 업무 앱"(M9), "M2 플랫폼–앱 연결과 알림", "M5", "M12 통합 검색", §2.2 문서 행
- 원칙: AI-DO 구현을 그대로 옮기고, miy의 경계(인증·저장·앱 프레임·앱별 DB·디렉터리) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다. 계획 §2.2의 보강(휴지통 복원·버전 기록·댓글·템플릿)은 AI-DO에 없던 기능이라 포팅 뒤에 더했고 결정은 §8에 있다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 문서 허브(전체·내 문서·공유받은 문서·비공개·최근 페이지·보관됨 보기, 검색, 제목·수정일 정렬, 페이지), 문서 만들기(공간 문서/비공개), 문서 이름 변경·복제·보관(삭제)·인쇄·링크 복사·새 탭, 페이지 트리(추가·삭제·끌어서 이동·펼침), BlockNote 블록 편집기(AI-DO 스키마: 콜아웃·구분선·멘션·태스크 참조 인라인, 이미지 업로드), 페이지 첨부, Markdown 가져오기·내보내기, 전체 보기(발표 모드: 페이지 이동·레이저·펜·지우기), 사용자 공유(읽기·편집), 공개 범위(공간 문서 ↔ 비공개), 즐겨찾기·최근 페이지(사이드바, 모든 공간), 열람 기록.

**바꾼 것(miy가 강제):** 실시간 공동편집 → 한 사람 편집 + revision 충돌 알림(§3.3), 워크스페이스 → 앱 공간(§3.2), 객체 저장소 → 코어 Storage(§3.4), 토큰·라우터 → 프레임 런타임(§3.8).

**miy가 더한 것:** 공유 알림(§3.5), 플랫폼 재확인·홈 카드("최근 문서")·통합 검색(§3.6), 문서 관리자와 공간 설정 화면(§3.2), 보강: 휴지통·버전 기록·댓글과 멘션 알림·공간 템플릿(§8).

**뺀 것(화면·API 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 실시간 공동편집·커서 | `collab.py`, `collab_codec.py`, `realtime_protocol.py`, `docs_collab_documents`, `/docs/collab/pages/{ref}/session·snapshot`, WS 경로, `CollaborativeBlockEditor`, `useDocsCollabSnapshotSaveController`(+spec), 페이지 트리 실시간 이벤트(`docsPagesChanged`) | 계획 §2.2 제외(후속). §3.3의 단일 편집으로 대체 |
| HTML 페이지 | `content_format='html'`, `docs-html-*`, `DocsHtmlRenderPage`, `/:docId/html/:pageId`, HTML 업로드·편집·렌더링 | 계획 §2.2 제외(프레임 `frame-src 'none'`) |
| 링크 공유(내부 링크) | `docs_native_doc_link_shares`, `/sharing/link`, `/shared-links/{token}`, `public_router`의 `share_token` 경로, `/docs/shared/:token` | 계획 §2.2 "익명 링크" 제외. AI-DO 링크는 로그인한 내부 사용자 누구나 토큰으로 여는 bearer 권한이라 기본 거부·명시 승인 모델과 맞지 않는다. 사용자 공유만 옮김 |
| PMS 태스크 연결·PMS 공간 대상 | `/items/{id}/pms-tasks`, `pms_task_doc_links`, `TaskPickerModal`, `space:<id>` 위치(`pms/space` 대상), 허브 `space_id` 필터, `pms_rehome.py` | 태스크에서 기존 문서를 연결하는 흐름은 PMS가 제공한다(§7). 문서 화면에서 태스크를 연결하는 흐름과 PMS 공간을 문서 저장 위치로 쓰는 방식은 남아 있다 |
| 회의록·회의 노트 | `minutes.py`, `docs_meeting_access`, `access_grants.py`, 회의 노트 보기 | 회의 앱 데이터(회의 앱 미운영). `meeting_notes` 보기 값은 API에 남지만 항상 비어 있고 사이드바에서 뺐다 |
| AI 도구·RAG·검색 투영 | `tools.py`, `rag_sync.py`, `search_projection.py`, `search_hooks.py`, `source_access.py`, `partitioning.py`, `rag_scope`·`retrieval_partition_id` 컬럼, "AI에게 묻기" 버튼 | M13 범위. 통합 검색은 §3.6의 SQL 검색 |
| 컬렉션 | `docs_collections`, `/docs/collections*` | AI-DO 화면에서 쓰지 않던 API 전용 기능. 응답의 `collection`은 `null`, 요청의 `collection_id`는 404 |
| 다른 앱용 뷰어·피커 | `DocsViewerModal`, `DocsHubPickerModal`, `docs-hub-picker-*`, `public-api.ts` | 다른 앱 화면에 끼워 넣던 컴포넌트. 앱 간 연결 이후 |
| 공간 선택기 최근 문서 | `space-chooser-recent.tsx`(+spec) | 포털 런처의 공간 선택기 확장. miy 포털에는 없다 |
| 레거시 경로 | `legacy_routes.py`, `/resources/docs/...`, `removeLegacyDocsSearchParams` 외 레거시 처리 | 계획 제외 |
| SVG 이미지 | `media/router.py` `image/svg+xml` | miy 파일 게이트웨이·공용 미리보기는 래스터만 |

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>`를 앱 `web/src/<경로>`로 옮겼다. "그대로"는 import 경로 외 변경이 없다는 뜻이다.

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/docs/views/DocsView.tsx` | 같은 경로 | 수정: 런타임 경로·API, 공간 `?space=`, 단일 편집 저장·충돌, 제외 기능 제거(HTML·링크 공유·PMS·AI·실시간), 링크·클립보드(§3.8), 늦은 응답 무시, 미저장 보고. 마크업·클래스·문구는 원본 |
| `…/views/DocsBlockContentSurface.tsx` | 같은 경로 | 재작성: 협업 분기 대신 원본의 단독 `BlockEditor` 경로 + 충돌·저장 실패 알림 |
| `…/views/useDocsPageContentSaveController.ts`(+spec) | 같은 경로 | 재작성: revision·직렬 저장·충돌·실패 보존(§3.3). 시험도 새로 씀 |
| `…/views/useDocsHubController.ts`(+spec) | 같은 경로 | 수정: `runtime.api`, 공간, 늦은 응답 무시(+시험 1개) |
| `…/views/useDocsViewState.ts` | 같은 경로 | 수정: PMS·링크 공유 상태 제거 |
| `…/views/DocsPageAttachments.tsx` | 같은 경로 | 수정: `runtime.api`·`fileApi`, 같은 요청 ID 재시도·"다시 시도", 10MiB 사전 확인, 미저장 보고 |
| `…/views/DocsFullscreenReadModal.tsx` | 같은 경로 | 수정: HTML 분기(iframe·휠 브리지) 제거 |
| `…/views/docs-view-model.ts`(+spec), `DocsViewParts.ts`, `DocsPageTreeNode.tsx`, `LocationPicker.tsx`, `DocsContentFormatBadge.tsx`, `docs-content-renderers.tsx`, `docs-markdown-file.ts`, `docs-page-attachments-model.ts`(+spec) | 같은 경로 | 그대로 |
| `…/api/docs-api.ts` | 같은 경로 | 수정: `runtime.api`, 공간 질의, 제외 API 제거, `revision`·`space_id`, 첨부 2단계 업로드 |
| `…/api/docs-page-reorder.ts`(+spec) | 같은 경로 | 그대로(시험 픽스처에 `revision`만 추가) |
| `…/api/docs-url-state.ts`(+spec) | 같은 경로 | 수정: HTML 경로 함수와 그 시험 제거 |
| `…/sidebar.tsx` | 같은 경로 | 수정: `DocsSidebarExtras`는 원본 마크업, 링크는 `runtime.navigate`. 포털 셸이 그리던 공간 선택·라이브러리·새 문서는 `DocsSidebar`로 프레임 왼쪽에 그림 |
| `…/docs-sidebar-model.ts`(+spec) | 같은 경로 | 재작성: 문서 경로 `/<docId>?space=&page=` |
| `packages/ui/src/lib/editor/*`(collaborative 제외) | `components/editor/*` | 그대로(PMS 포팅과 같은 방식). `compactSchema` 제거 |
| `platform/format/byte-size.ts`, `platform/ordering/ordered-reorder.ts`(+spec), `platform/i18n/resource-location-label.ts` | 같은 경로 | 그대로 |
| `platform/media/use-media-upload.ts` | 같은 경로 | 게시판 포팅본(재시도 포함) + `linkUploadsToPage`(AI-DO `createLinkedUploadFile`) + Blob URL 캐시·해제 |
| `platform/api/*`, `platform/i18n/*`, `platform/runtime/*`, `platform/time/*`, `lib/utils.ts`, `test-setup.ts` | 같은 경로 | 게시판·플래너 포팅본 |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: [scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py) |
| `packages/ui/styles.css`의 읽기 모드·레이저 토큰과 `.ui-block-editor` 규칙, `index.css`의 인쇄 규칙 | `web/src/styles/docs.css` | 그대로 |
| (없음) | `app-modules/docs-settings/DocsSettingsView.tsx`, `platform/docs/*`, `views/docs-link.ts`(+spec), `platform/runtime/app-location.ts`(+spec), `AppRoot.tsx` | miy 연결 |
| (없음) | `app-modules/docs-trash/DocsTrashView.tsx`, `views/DocsPageVersionsModal.tsx`, `views/DocsPageComments.tsx`(+spec), `views/docs-comments-model.ts`(+spec), `views/docs-comment-drafts.ts`, `views/DocsTemplatePicker.tsx` | 보강(§8) |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/…` | miy `src/…` |
| --- | --- |
| `docs/router.py`(허브·문서·대상·즐겨찾기·최근·열람·공유), `service.py`(`list_hub`·필터·정렬), `hub_projection.py`, `registry.py`(배지), `timestamps.py` | `items.rs` |
| `docs/page_mutations.py`, 페이지 경로 | `pages.rs` |
| `docs/access_context.py`, `source_access/targets.py`의 docs 어댑터 | `access.rs` |
| `docs/attachments.py`, 첨부 경로 | `attachments.rs` |
| `media/router.py`·`service.py`·`resource_access.py`의 docs 부분 | `media.rs` |
| 저장소 정리(`_remove_docs_storage_objects`, `cleanup_media_for_resource`) | `files.rs` |
| `docs/content_text.py`, `search/service.py::_extract_blocks_text` | `content_text.rs` |
| `auth/app_global_spaces.py`, `app_access.py`, `app_space_roles.py`(문서 공간) | `spaces.rs`(SDK ACL) |
| 앱 관리자(`app_role_may_create_space`) | `managers.rs`(문서 관리자) |
| `users` 표 이름 조회 | `people.rs`(플랫폼 디렉터리) |
| `core/i18n.localized_http_exception` | `error.rs` |
| (없음) | `notify.rs`(공유 알림·재확인·홈 카드), `search.rs`(통합 검색) |
| (없음, 보강) | `trash.rs`(휴지통), `versions.rs`(버전 기록), `comments.rs`(댓글·멘션 알림), `templates.rs`(공간 템플릿) |

## 3. 원본과 달라진 점과 이유

### 3.1 데이터 모델

`migrations/0002_acl.sql`은 SDK `acl.sql` 그대로다. `migrations/0003_docs.sql`:

| 표 | AI-DO | miy |
| --- | --- | --- |
| `docs_native_docs` | `id, workspace_id(FK), retrieval_partition_id, owner_id(FK users), collection_id, title, doc_type, source_app, source_kind, source_ref, generation_kind, rag_scope, created_at, updated_at, trashed_at` | `workspace_id`는 `docs_spaces` 참조, `owner_id`는 `text`. `retrieval_partition_id`·`rag_scope`(RAG)·`collection_id`(컬렉션) 없음 |
| `docs_doc_targets` | 같음 | 같음(문서 FK cascade) |
| `docs_native_doc_pages` | `…, content_format IN ('block','html'), content_blocks JSON, content_text, …, created_by_id(FK)` | `content_format IN ('block')`, `created_by_id`는 `text`, **`revision`**(§3.3)·**`search_text`**(§3.6) 추가 |
| `docs_page_attachments` | `…, storage_key, uploaded_by_id(FK)` | `storage_key` 대신 `request_id`(예약 의도)·`file_id`·`status` |
| `docs_native_doc_user_shares` | 같음 | `user_id`·`created_by_id`는 `text`, `access_level` 검사 제약 |
| `docs_user_item_prefs` | 같음 | `user_id`는 `text` |
| `media_files` | AI-DO media 도메인 | 게시판 포팅과 같은 형태(`request_id`·`file_id`·`status`) |
| `docs_collections`, `docs_native_doc_link_shares`, `docs_meeting_access`, `docs_collab_documents` | 있음 | 없음(§1) |
| (없음) | — | `docs_spaces`(전체 문서 공간 `global` 시드), `docs_managers` |
| (없음, 보강 `0004_docs_extras.sql`) | — | 문서·페이지 `trashed_by_id`, `docs_page_versions`, `docs_page_comments`, `docs_comment_notifications`, `docs_templates`; 첨부의 페이지 FK를 `ON DELETE SET NULL`로(§8) |

- 사용자 ID는 miy 사용자 ID다. 사용자 표가 코어 DB에 있어 FK를 둘 수 없다. 시각은 naive UTC, ID는 UUID v4 문자열이다.
- 전체 문서 공간은 AI-DO가 설치 때 만들던 앱 전역 공간(`seed_app_global_spaces`)이라 migration에 넣었다.
- `content_blocks`는 AI-DO처럼 `json` 타입이다. serde_json이 객체 키를 정렬해 저장하므로 키 순서는 원본과 다를 수 있다(내용 비교와 BlockNote에는 영향 없음).

### 3.2 공간·문서 관리자·권한 (워크스페이스 대응)

| AI-DO | miy |
| --- | --- |
| 앱 전역 공간(`app_global`, 소유 앱 `docs`): 앱 사용자 누구나 바인딩 없이 `member` | **전체 문서**(`docs_spaces.id = 'global'`): 앱을 쓸 수 있는 모든 사람이 `member`. 앱 사용 권한은 플랫폼이 이미 확인했다 |
| 전역 공간의 명시 `admin` 바인딩 | **문서 관리자**(`docs_managers`)가 전역 공간의 `admin` |
| 앱 공간(`app_space`)과 바인딩 역할 `viewer`·`member`·`admin`·`owner` | 공간(`docs_spaces`, kind `app_space`) + SDK ACL(kind `space`) 역할 `viewer`(`space.read`)·`member`(+`doc.write`)·`admin`(+`space.admin`, `acl.manage`). `owner`는 ACL의 보호된 생성자(`admin`) |
| 공간 만들기: 앱 관리자(`space_creation_role = "admin"`) | 문서 관리자(잠금 뒤 다시 확인) |
| 플랫폼 관리자 표시 | 아무 권한도 주지 않는다(시험으로 확인) |
| 요청마다 URL의 워크스페이스에 묶고 그 워크스페이스 문서만 로드 | 공간 범위 경로(허브·만들기·즐겨찾기 `scope=space`)만 `?space=`. 문서·페이지·첨부 경로는 문서의 공간을 스스로 찾는다(AI-DO의 공유 토큰·리소스 경로와 같은 동작) |

- **문서 접근(`resolve_native_doc_access`) 그대로:** 소유자는 모든 권한. 그 밖에는 사용자 공유(`read`/`edit`)와 문서 대상(`docs/workspace_sidebar/<문서의 공간>`)을 통한 공간 역할 투영 중 높은 것. 대상 투영: `viewer` → 읽기, `member` → 편집, `admin` → 편집·관리·공유. 관리·공유는 대상의 `admin`만(소유자 제외). 보이지 않는 문서와 관리할 수 없는 보관 문서는 404, 페이지 읽기 실패는 403 `docs_page_access_required`, 편집 실패는 403 `docs_doc_edit_access_required`.
- **대상 변경과 복제:** 공개 범위 변경(`PUT/DELETE …/target`)은 문서 관리 권한이 필요하다. 공간의 `viewer`도 그 공간에 개인 문서를 만들 수 있다. 공유받은 사람은 그 문서가 있는 공간에 들어가지 않아도 문서를 연다(목록에는 공간에 들어갈 때만 보인다, AI-DO ADR 0027). 공간 밖에서 공유받아 복제하면 전역 공간에 비공개 문서로 저장한다. 공간 공개 대상은 원본 공간에서 쓰기 역할을 가진 경우에만 복제본에 유지한다.
- **SDK ACL을 쓴 곳과 안 쓴 곳:** 공간 멤버는 ACL을 쓴다(그룹 부여·감사·revision·`ShareEditor` 재사용). 문서별 사용자 공유는 AI-DO 모델(`docs_native_doc_user_shares`)을 그대로 옮겼다. AI-DO 공유는 사람 한 명씩 `PUT/DELETE …/sharing/users/{id}`로 바꾸고, 소유자는 목록에 없으며, 공간 관리자도 목록과 무관하게 공유할 수 있다. ACL의 전체 교체(`replace_in`, 생성자가 항상 관리자 항목으로 남아야 함)는 이 REST 형태와 화면에 맞지 않는다.
- **공유 대상 확인:** AI-DO는 같은 워크스페이스 멤버만 공유 대상으로 받았다(`docs.shared_users_workspace_required`). miy는 디렉터리에서 지금 활성인 사람이면 받고, 앱을 쓸 수 없는 사람은 플랫폼이 알림에서 빼고 앱도 열 수 없다. 그룹 공유는 AI-DO에 없어 두지 않았다.
- **잠금 뒤 재확인(리뷰 반영):** 모든 쓰기는 문서 행을 `FOR UPDATE`로 잡은 뒤 같은 트랜잭션에서 공유·대상·공간 역할을 다시 판정한다. 공간 역할은 ACL `require_in`(자원 공유 잠금) 또는 문서 관리자 행 `FOR SHARE`로 판정해 커밋까지 회수가 끼어들지 못한다. 페이지 추가·이동·삭제는 문서 행 잠금으로 직렬화된다(AI-DO의 형제 수 기반 `sort_order`).
- **문서 관리자 재판정:** 관리자를 해제하면 다음 요청부터 전체 문서 공간의 관리 권한이 사라진다(시험 `global_space_docs_follow_member_and_manager_roles`). 관리자 추가·해제·부트스트랩은 게시판과 같은 표 잠금을 쓴다.
- **공간 삭제:** 살아 있는 문서가 있으면 409 `docs_space_not_empty`. 휴지통의 문서는 공간과 함께 지우고, 그 페이지의 첨부와 이미지를 같은 트랜잭션에서 `detaching`으로 적은 뒤 커밋 후 코어에서 떼어 낸다(시험 2개: 정상 보관 뒤, 해제가 확인되지 않은 채 남은 경우). 전체 문서 공간은 바꾸거나 지울 수 없다(400 `docs_global_space_protected`). AI-DO는 워크스페이스를 휴지통으로 보냈고 miy에는 공간 휴지통이 없다.

### 3.3 실시간 공동편집 대체: 한 사람 편집 + revision

AI-DO의 블록 페이지는 「편집 시작」 뒤 Yjs 공동편집기(y-websocket, 서버 relay)로 편집했고, 편집기가 250ms마다 스냅샷(`PUT /docs/collab/pages/{ref}/snapshot`, `content_blocks`+`yjs_state`)을 저장했다. 협업이 꺼진 페이지(공유 토큰, `realtime_collab=false`)는 단독 `BlockEditor`가 800ms 디바운스로 `PATCH /docs/pages/{id}`에 `content_blocks`를 보냈다(마지막 저장이 이김). 바꾼 것:

1. **서버:** 페이지에 `revision`(1부터)을 두었다. 본문 저장(`PATCH`의 `content_blocks`)에 `expected_revision`을 받는다. 값이 있고 현재 revision과 다르며 내용이 실제로 바뀌는 저장은 409 `docs_page_revision_conflict`로 거절하고 아무것도 바꾸지 않는다. 바뀐 저장은 revision을 1 올리고 응답에 새 revision을 준다. 같은 내용 저장은 변경이 아니므로 충돌도 아니다. 제목·부모·순서는 메타데이터라 검사하지도 올리지도 않는다(트리 이동과 이름 변경이 본문 편집과 충돌하지 않게). `expected_revision`이 없는 저장은 AI-DO처럼 마지막 저장이 이긴다(화면은 항상 보낸다). 페이지 행은 `FOR UPDATE`로 잡고 비교한다.
2. **화면:** 모든 편집 가능한 페이지에 원본의 단독 `BlockEditor` 경로를 쓴다(「편집 시작」 버튼과 협업 편집기 없음). 저장 제어기는 800ms 디바운스를 유지하고, 한 페이지의 저장을 겹치지 않게 순서대로 보내며, 저장 중에 쓴 내용은 그 응답의 새 revision으로 이어 보낸다. 여러 페이지의 미저장 초안은 차례로 보낸다.
3. **충돌:** 409를 받으면 그 페이지의 자동 저장을 멈추고 초안을 편집기에 그대로 둔다. 「다시 불러오기」는 서버 페이지를 받아 초안을 버리고 편집기를 새로 그리고, 「내 변경 유지」는 최신 revision을 받아 초안을 그 위에 저장한다. 충돌 중 계속 쓴 내용도 초안에 남는다.
4. **실패:** 네트워크·5xx 실패는 초안을 남기고 「다시 저장」을 보인다. 다음 편집도 다시 보낸다.
5. **미저장 보고:** 초안·저장 중·충돌·실패 상태는 `runtime.setNavigationState`로 보고한다(문서 목록으로 돌아가기·다른 문서 열기·새 탭에서는 남은 초안을 먼저 보낸다).
6. **늦은 응답:** 저장 응답이 오는 사이 더 쓴 경우 서버 응답의 본문으로 화면을 덮지 않고 메타데이터만 받는다. 다른 문서로 이동한 뒤 도착한 문서·페이지·공유·생성 응답은 버린다. 제목 저장과 트리 이동 응답은 본문을 바꾸지 않는다.
7. **Markdown 가져오기:** 그 페이지의 대기 초안을 버리고 현재 revision으로 저장한다. 충돌이면 충돌 문구를 알린다.
8. **버전 복원(보강):** 초안을 먼저 저장하고, 저장되지 않으면(실패·충돌) 복원하지 않는다(§8.2).

**하지 않은 것:** 편집 잠금(누가 편집 중인지 표시), 페이지 트리의 즉시 갱신(AI-DO의 `docsPagesChanged` 실시간 이벤트), 자동 병합. 다른 사람이 추가·이동한 페이지는 문서를 다시 열 때 보인다. 이것들은 업무 실시간 전송(`runtime.live`, M4) 뒤 실시간 공동편집과 함께 다룬다.

### 3.4 첨부와 본문 이미지 (`platform:files:*`)

- **첨부:** AI-DO는 multipart로 50MB까지 받아 객체 저장소에 넣었다. miy는 계약 순서대로 ① `POST …/pages/{id}/attachments`가 요청 ID를 먼저 앱 DB에 적고 코어 파일을 예약하고, ② 프레임이 `runtime.fileApi(PUT /docs/attachments/{id}/content)`로 바이트를 보내면 서버가 저장하고 코어 파일을 `docs_attachment:<id>`에 붙인다(attach 요청 ID = 첨부 ID라 재시도가 같은 호출). 한도는 파일당 10MiB(413 `docs_attachment_size_limit_exceeded`, 문구의 `{limit_mb}`는 10). 파일 이름 정리는 AI-DO `normalize_page_attachment_filename` 그대로다.
- **복구(리뷰 반영):** 예약·전송 응답이 불확실하면 화면이 같은 요청 ID로 한 번 더 보내고, 그래도 안 되면 「다시 시도」로 같은 요청 ID를 다시 쓴다. 같은 요청 ID에 다른 파일은 409 `docs_attachment_request_conflict`. 실패한 업로드를 자동으로 지우지 않는다(끝내 올리지 못한 예약은 코어가 24시간 뒤 만료).
- **내려받기:** 페이지 읽기 권한을 확인한 뒤 코어에서 받아 항상 `application/octet-stream`으로 보낸다(AI-DO와 같은 이유: 올린 HTML·SVG를 그리지 않음). 프레임이 Blob으로 받아 파일 이름으로 저장한다.
- **정리:** 첨부 삭제·휴지통의 영구 삭제·공간 삭제는 첨부 행을 같은 트랜잭션에서 `detaching`으로 적고 커밋 뒤 코어에서 떼어 낸다(붙어 있으면 detach, 아니면 cancel). 페이지·문서를 휴지통으로 보낼 때는 복원할 수 있도록 파일을 그대로 둔다(§8). 확인되지 않은 작업은 다음 정리 요청이 끝낸다. 목록에는 바이트가 저장된 첨부만 보인다.
- **본문 이미지:** BlockNote `uploadFile`은 게시판과 같은 2단계(`POST /v1/media/upload` → `fileApi PUT /media/{id}/content`)로 `media:<id>`를 돌려주고, AI-DO `createLinkedUploadFile`처럼 곧바로 `POST /v1/media/link`로 페이지에 잇는다. 본문 저장은 AI-DO `sync_embedded_media`처럼 본문의 `media:<id>`를 다시 훑어 잇는다. 본문에서 빠진 이미지는 버전 기록이 계속 가리키므로 끊지 않고 페이지를 영구 삭제할 때 떼어 낸다(§8). 보기는 `resolveFileUrl`이 `fileApi GET /media/{id}/content`(페이지 읽기 권한 확인)로 받아 Blob URL로 바꾼다. 같은 이미지는 한 번만 받고 화면을 떠날 때 Blob URL을 해제한다. PNG·JPEG·GIF·WebP만(SVG 제외). 페이지를 영구 삭제하면 이미지도 코어에서 떼어 낸다. 복제한 문서는 AI-DO처럼 같은 `media:<id>`를 가리키므로 원본 페이지를 지우면 복제본의 이미지도 보이지 않는다(AI-DO도 같았다).

### 3.5 알림 (miy)

- AI-DO 문서에는 알림이 없었다. miy는 사용자 공유 하나만 알린다: 새로 공유하면 그 사람에게 `share`, 묶음 키 `doc:<id>`, 경로 `/<docId>`, 자원 ID 문서 ID, 제목 `<공유한 사람>님이 "<문서 제목>" 문서를 공유했습니다`(받는 사람 언어를 몰라 한국어, 본문 없음). 권한 변경은 다시 알리지 않는다.
- 회수: 공유 해제 → 그 사람에게서, 문서를 휴지통으로 → 모두에게서. 읽음: 받은 사람이 문서를 열면(열람 기록).
- **댓글·멘션(보강):** AI-DO 문서 편집기는 멘션 인라인 형식을 스키마에만 갖고 입력 메뉴를 열지 않았고, 댓글도 없었다. 보강의 페이지 댓글이 `comment`·`mention`을 보낸다(§8.3). 본문 편집기의 멘션 입력은 여전히 열지 않는다(원본 그대로).
- 문서 공유는 사람 한 명씩이고 댓글 알림의 받는 사람도 특정 사람이라 `notify_with_groups`(그룹 받는 사람)는 쓰지 않는다.
- 발행은 커밋 뒤 같은 요청 안에서 하고 실패해도 업무를 되돌리지 않는다. 결과가 불확실하면 같은 요청 ID로 조회만 한다.

### 3.6 플랫폼 → 앱 엔드포인트

세 엔드포인트 모두 플랫폼 호출(`identity.context.platform`)이 아니면 403 `docs_platform_only`이고 데이터를 바꾸지 않는다. 읽기 가능 여부는 결과 제한·건수와 같은 문장에서 정하도록 앱 DB만으로 **한 SQL 문 안에서** 판정한다(`notify::READABLE_BY_CALLER`): 살아 있는 문서이고, 소유자이거나, 사용자 공유가 있거나, 문서의 공간 대상이 있고 그 공간이 전체 문서이거나 그 사람 또는 그 사람의 그룹이 공간에 부여되어 있을 때. 플랫폼 호출에는 디렉터리 토큰이 없으므로 그룹은 코어가 호출에 서명해 넣은 현재 그룹(`identity.context.groups`, 앱이 `platform:directory:read`를 승인받았을 때만; 번들 0.12.0 개정, d7fbdfe)을 쓴다(`notify::signed_groups`). 승인이 없어 그룹이 없으면 사용자 부여만 센다. 같은 규칙이 플랫폼 호출의 공간 역할(`spaces::direct_roles_of`)에도 쓰인다.

- `notificationCheck` → `POST /platform/notification-check`: `doc:<id>` 중 `resourceId`가 같고 지금 읽을 수 있는 것, `comment:<id>`·`mention:<id>` 중 `resourceId`가 같고 댓글이 지워지지 않았으며 살아 있는 페이지에 있고 그 문서를 지금 읽을 수 있는 것만 허용.
- `homeCard` → `POST /v1/home-card`: "최근 문서"(영어 `Recent docs`). 내가 최근 연 페이지 중 문서를 지금 읽을 수 있는 것 최대 5개(제목은 페이지 제목, 보조 문구는 문서 제목, 경로 `/<docId>?page=<pageId>`, 시각은 열람 시각). 건수는 보내지 않는다.
- `search` → `POST /v1/search`: 지금 읽을 수 있는 문서 중 제목·살아 있는 페이지의 제목·본문 텍스트가 맞는 것(최신순, `limit`개). 결과는 `{title: 문서 제목, snippet: 페이지 제목 · 본문 일부, path, at, kind: 문서|Doc}`. `LIKE`의 `%`·`_`·`\`는 그대로 찾는다. 본문 텍스트는 저장할 때 AI-DO `_extract_blocks_text`(모든 `text` 값)로 `search_text`에 적는다. AI-DO는 OpenSearch/RAG 투영으로 찾았다(제외).

### 3.7 오류 응답과 문구

- 게시판·플래너와 같은 방식이다: `{"error":{"code","message"}}`, 코드는 AI-DO 코드의 `.`을 `_`로, `message`는 AI-DO 영어 카탈로그 문장, 화면은 `errors` 네임스페이스로 번역한다. 요청 검증 실패는 422 `validation_request_invalid`.
- 상태 코드와 검증 규칙(제목 1~200자, 페이지 크기 1~200, 최근 1~20, 공유 수준 `read|edit`, 대상 길이)은 AI-DO와 같다. `content_format: "html"`은 422, `content_text`·`first_page_content_text`는 400 `docs_content_format_mismatch`.
- 새 코드: `docs_page_revision_conflict`, `docs_space_not_empty`, `docs_global_space_protected`, `docs_manager_*`, `docs_last_manager`, `docs_attachment_request_conflict`, `docs_platform_only`, `media_*`(게시판과 같음), `acl_*`(SDK). 보강: `docs_not_in_trash`(409), `docs_version_not_found`, `docs_comment_not_found`, `docs_comment_request_conflict`(409), `docs_comment_author_required`(403), `docs_comment_reply_not_resolvable`(400), `docs_mention_unknown_user`(400), `docs_template_not_found`, `docs_template_manage_access_required`(403).
- 화면 문구는 [생성 스크립트](../scripts/gen-i18n-resources.py)가 AI-DO `resources.ts`의 `common`, `apps.docs`와 화면이 빌려 쓰는 단일 키(`apps.planner.title`, `apps.pms.creating`·`name`·`filter.filters`·`bulk.remove`, `apps.ai.search.metadataLocation`, `apps.home.actionNewDoc`)를 그대로 복사한다. miy 문구는 `apps.docsApp`과 `errors`에 있다. "워크스페이스"라는 AI-DO 문구(위치 선택)는 그대로 두었다. AI-DO의 앱 공간 화면도 같은 문구를 썼다.

### 3.8 화면 경로·구성·프레임 제약

- react-router 대신 `runtime.location`·`runtime.navigate`(`useAppSearchParams`, `useAppNavigate`). AI-DO `/w/:slug/docs[/:docId]` → `/[?space=]`, `/:docId?space=&page=`. 편집기 화면은 AI-DO 라우트처럼 지연 로드한다(BlockNote·Mantine 청크 약 1MB).
- AI-DO의 포털 셸이 그리던 공간 선택기·라이브러리 항목·「새 Doc」은 프레임 왼쪽 열에 그린다(`DocsSidebar`). 회의 노트 항목은 뺐다. 「AI에게 묻기」 버튼은 뺐다.
- 새 문서·새 페이지 창의 "문서 형식/페이지 형식" 선택은 블록 하나만 남아 뺐다(HTML 제외). 형식 배지는 원본대로 "블록 에디터"를 보인다.
- 위치 선택은 "워크스페이스 · <공간 이름>"과 "비공개" 둘이다(PMS 공간 항목 제외).
- 공유 창의 사람 검색은 입력할 때마다 디렉터리를 찾는다(AI-DO는 워크스페이스 멤버 30명을 한 번 받아 걸렀다). 디렉터리에 이메일이 없어 이름만 보인다. 내부 링크 섹션은 뺐다.
- **링크:** 프레임의 `window.location.origin`은 포털이 아니라 앱 출처다. 링크는 브라우저가 알려 주는 포털 출처(`location.ancestorOrigins[0]`, 부모 접근 아님) + `runtime.basePath` + 앱 경로로 만든다. 출처를 알 수 없으면(Firefox) 포털 기준 경로만 쓴다. 「새 탭에서 열기」도 같은 주소를 `window.open`으로 연다(sandbox `allow-popups`).
- **클립보드:** 포털 iframe에 `allow` 위임이 없어 Chromium은 `navigator.clipboard.writeText`를 권한 정책으로 거절한다. 실패하면 임시 텍스트 상자 + `document.execCommand('copy')`로 복사하고, 그것도 안 되면 "복사하지 못했습니다" 안내와 함께 공유 창의 링크 칸(선택 가능)을 쓴다. 실측은 §6.
- **전체 보기(발표 모드):** AI-DO도 브라우저 Fullscreen API를 쓰지 않고 화면 전체를 덮는 `dialog`를 썼다. 프레임에서는 앱 작업 영역 전체를 덮으며 위임이 필요 없다. HTML 확대 컨트롤·iframe 휠 브리지는 HTML 제외로 뺐다.
- 인쇄는 원본처럼 `window.print()`이고 프레임 문서를 인쇄한다(sandbox `allow-modals`). 실측하지 않았다.
- **미저장 보고(miy 계약):** 본문 초안·저장 중·충돌·실패, 새 문서/새 페이지 창의 입력과 진행 중 생성, 첨부 업로드 중·재시도 대기, 공간 만들기·멤버 편집·관리자 선택, 보내지 않은 댓글(모든 페이지), 버전 복원·휴지통 작업 중을 `runtime.setNavigationState`로 합쳐 보고한다. 문구는 `apps:docsApp.unsavedChanges`.
- 날짜 표시는 `runtime.settings`(언어·시간대·날짜 형식)를 따른다.

### 3.9 서버 동작의 세부

- `touch_native_doc`처럼 페이지가 바뀌면 문서 `updated_at`을 이전 값보다 반드시 크게 올린다. 페이지 `updated_at`은 실제로 값이 바뀐 수정에서만 바꾼다.
- 허브는 AI-DO처럼 공간의 문서를 모두 읽어 접근을 판정한 뒤 보기·검색·정렬·페이지를 적용한다(정렬은 Python `sorted`와 같은 안정 정렬). 대상·페이지 수·공유 수·내 공유·선호·소유자 이름은 묶어서 읽는다.
- 최근 페이지는 AI-DO처럼 열람 기록을 50개씩 읽어 지금 읽을 수 있는 것이 `limit`개가 될 때까지 모은다. 제목은 저장된 제목 대신 살아 있는 페이지의 현재 제목을 쓰고, 페이지가 지워졌으면 문서로 연결한다(AI-DO는 열람 때 제목을 보였다).
- 응답의 `created_by_name`은 플랫폼 디렉터리 이름이고, 읽지 못하면 빈 문자열이다(요청은 실패시키지 않는다).
- 허브 항목에 miy 필드 `space_id`(문서의 공간)를 더했다. 페이지 항목의 `realtime_collab`은 형태를 위해 남기고 항상 `false`, `revision`을 더했다.

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 공간 멤버는 SDK ACL, 문서 공유는 AI-DO 모델 | §3.2 |
| 전체 문서 공간 시드와 문서 관리자 | AI-DO의 앱 전역 공간과 앱 관리자 대응. 게시판의 관리자 방식(한 번 맡기·마지막 관리자 보호) |
| 링크 공유 제외 | 계획 §2.2와 기본 거부 모델(§1) |
| revision은 본문에만 | 트리 이동·이름 변경이 편집을 충돌로 만들지 않게(§3.3) |
| 단독 편집기를 바로 보임(「편집 시작」 없음) | 협업 없는 AI-DO 경로 그대로 |
| 검색 텍스트를 저장 때 계산 | `json` 본문에 `ILIKE`를 쓰면 키 이름까지 맞는다 |
| 플랫폼 호출은 사용자 부여 + 코어가 서명한 그룹 | 디렉터리 토큰이 없어 코어가 서명한 현재 그룹을 쓴다. 그룹이 없으면(승인 없음) 사용자 부여만(닫는 쪽) |
| 공간 삭제는 빈 공간만 | 문서를 조용히 지우지 않는다. 휴지통 문서의 파일은 회수 |
| 링크는 `ancestorOrigins`, 클립보드는 `execCommand` 대체 | 플랫폼 변경 없이 프레임에서 동작하는 방법(§3.8, §7) |
| 의존성은 AI-DO 고정 버전 | `@blocknote/core`·`react`·`mantine` 0.52.1, `@mantine/core`·`hooks` 8.3.18, `@dnd-kit/core` 6.3.1·`sortable` 10.0.0·`utilities` 3.2.2, `motion` 12.38.0, `i18next` 26.0.8, `react-i18next` 17.0.6, `@tailwindcss/typography` 0.5.19, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library. React·Vite·Tailwind·Radix는 템플릿·공용 UI 버전(PMS 포팅과 같음) |
| 편집기는 앱에 복사(`components/editor`) | 공용 UI 2.0.0에 편집기가 없다. PMS 포팅과 같은 방식(§7에서 공통 컴포넌트 후보로) |
| 공용 UI 설치는 게시판·플래너와 같은 오프라인 복사 | `registry/components/ui/2.0.0`과 같은 파일과 잠금. 실제 서버에서 `miy component sync`로 digest를 확인해야 한다 |
| DB 시험은 `#[ignore]` + `DOCS_TEST_DATABASE_URL`, 가짜 코어 | 서버 빌드의 `cargo test`에는 DB가 없다 |

## 5. 테스트 대응

| AI-DO | miy | 비고 |
| --- | --- | --- |
| `test_docs_hub.py::test_docs_native_docs_and_direct_user_share_grant_docs_access` | `native_docs_and_direct_user_share_grant_docs_access` | + 공유 권한·소유자·없는 사용자·편집 공유·해제 |
| `…::test_native_doc_page_patch_rejects_cycle` | `native_doc_page_patch_rejects_cycle` + `pages.rs` 단위 | |
| `…::test_duplicate_doc_via_read_share_creates_private_copy_for_recipient` | 같은 이름 | |
| `…::test_workspace_scoped_shareable_users_stay_in_requested_docs_workspace` | (해당 없음) | 공유 대상은 디렉터리(§3.2) |
| `…::test_docs_hub_reuses_pms_acl…`, `test_space_docs_filter…`, `test_internal_shared_links…` | (제외) | PMS·링크 공유 |
| `test_docs_page_mutations.py`(update metadata, delete subtree, doc-prefixed parent, format mismatch) | `page_mutations_update_metadata_and_trash_subtrees`, `native_doc_page_patch_rejects_cycle` | HTML 페이지 생성·`approved_call_id`(AI 도구) 시험은 제외 |
| `test_docs_access_context.py`(ID 정규화 2개) | `access.rs` 단위 | 워크스페이스 문맥 시험은 해당 없음 |
| `test_docs_page_attachments.py`(round trip, delete, size limit, trash page, not readable elsewhere, HTML never renderable) | `attachments_round_trip_with_page_access` | share-link 업로드 시험은 제외 |
| `test_docs_attachment_files.py`(파일 이름 4개) | `attachments.rs` 단위 | 스트리밍 lane 시험은 해당 없음(코어 Storage) |
| `test_docs_cross_space_prefs.py`(2개) | `hub_views_favorites_and_recent_pages` | |
| `test_docs_content_formats.py::test_block_doc_rejects_initial_content_text`, 블록 텍스트 추출 | `page_mutations…`, `content_text.rs` 단위 | HTML 부분 제외 |
| (miy 추가) | revision 충돌, 공간 역할(그룹 포함)·공간 삭제와 파일 회수(2), 문서 관리자 재판정·부트스트랩, 첨부 요청 ID 복구, 본문 이미지, 공유 알림, 플랫폼 검색·재확인·홈 카드(지금 권한만), 플랫폼 호출의 서명된 그룹(`platform_calls_count_group_members_through_signed_groups`: 그룹 멤버 포함, 서명 없으면 제외, 그룹을 떠나면 제외), 신원 없음·위조·관리자 표시 무시 | |
| (보강) | `trash_lists_restores_and_purges_docs_and_pages`, `versions_record_each_saved_revision_and_restore_as_new_ones`, `comments_threads_mentions_and_notifications`, `templates_are_saved_from_pages_and_start_new_pages_and_docs` | §8 |
| 화면 `docs-view-model.spec.ts`, `docs-page-reorder.spec.ts`, `docs-url-state.spec.ts`(HTML 경로 제외), `docs-page-attachments-model.spec.ts`, `ordered-reorder.spec.ts`, 편집기 `clipboard.spec.ts`·`markdown.spec.ts` | 같은 경로, 그대로 | |
| `useDocsHubController.spec.ts` | 같은 경로 + 늦은 응답 1개 | |
| `useDocsPageContentSaveController.spec.ts` | 새로 작성(7개) | revision·직렬·충돌·실패·여러 페이지, 복원 전 정착 확인(보강) |
| `docs-sidebar-model.spec.ts` | 새 경로 규칙으로 | |
| (miy 추가) | `docs-link.spec.ts`, `app-location.spec.ts`, `api-error.spec.ts`, `use-media-upload.spec.tsx`(게시판), 시간 유틸 | |
| (보강) | `docs-comments-model.spec.ts`(멘션·스레드·초안·요청 ID), `DocsPageComments.spec.tsx`(늦은 응답, 실패 뒤 같은 요청 ID 재전송과 초안 유지, 페이지를 떠났다 와도 초안 유지), `docs-sidebar-model.spec.ts`의 휴지통 경로 | |

옮기지 않은 화면 시험: `useDocsCollabSnapshotSaveController.spec.ts`(협업), `docs-html-frame-srcdoc.spec.ts`(HTML), `docs-hub-picker-model.spec.ts`(피커), `space-chooser-recent.spec.tsx`(런처).

## 6. 프레임 CSP와 발표 모드 확인 (2026-11-22, 격리 하네스)

**방법:** 앱의 실제 `AppRoot`를 메모리 API로 감싼 하네스를 빌드해(앱 밖, 임시 디렉터리), 프레임과 같은 CSP 헤더(`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; worker-src 'none'; frame-src 'none'; frame-ancestors <부모>`)로 `127.0.0.1` 출처에서 내보내고, `localhost` 출처의 부모 문서가 포털과 같은 속성(`sandbox="allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-popups allow-popups-to-escape-sandbox"`, `referrerpolicy="no-referrer"`, `allow` 없음)의 iframe으로 띄웠다. Playwright 1.64.1(Chromium headless shell, Firefox, WebKit)로 조작했다. 실제 포털·프레임 런타임·Module Federation 적재는 거치지 않았다.

| 항목 | Chromium | Firefox | WebKit |
| --- | --- | --- | --- |
| BlockNote 적재·기존 블록 표시 | PASS | PASS | PASS |
| 입력 → 800ms 뒤 `expected_revision`과 함께 저장 | PASS | PASS | PASS |
| `/` 블록 메뉴 | PASS | PASS | PASS |
| 409 충돌 알림 → 「내 변경 유지」로 최신 revision 저장 | PASS | PASS | PASS |
| CSP 위반(`securitypolicyviolation`) | 0건 | 0건 | 0건 |
| 미저장 보고(`dirty` true → false) | PASS | PASS | PASS |
| `navigator.clipboard.writeText` | `NotAllowedError`(권한 정책으로 차단) | 성공 | 성공 |
| 링크 복사(공유 창) | `execCommand` 대체로 성공, 클립보드에 포털 URL | 성공 | 성공 |
| `location.ancestorOrigins` | 부모 출처 | 지원(값 `"null"`: no-referrer로 가림) → 경로만 복사 | 부모 출처 |
| `document.fullscreenEnabled` | false | false | false |
| 전체 보기(오버레이가 프레임 1280×800 전체, 레이저 선택, Esc 닫기) | PASS | PASS | PASS |

- 빌드 산출물에 `eval`·`new Function`·Worker·WebAssembly가 없고 글꼴(Inter)은 같은 출처 자산이다. BlockNote·Mantine의 인라인 스타일은 `style-src 'unsafe-inline'`로 허용된다.
- **발표 모드:** Fullscreen API를 쓰지 않으므로 위임 없이 동작한다. 브라우저 전체 화면은 되지 않는다(프레임 작업 영역 전체만).
- **클립보드:** 비동기 API는 Chromium에서 막히고 `execCommand('copy')` 대체가 동작했다. `execCommand`는 폐지 예정 API라 장기적으로는 iframe 위임이 필요하다(§7).
- 실행하지 않은 것(`NOT_RUN`): 실제 포털 프레임(프레임 런타임의 CSS·자산 적재, `runtime.fileApi` 실경로, 포털 이동 확인), 이미지 업로드·첨부 내려받기의 브라우저 경로, 인쇄, 새 탭 열기, 모바일 폭.

## 7. 남은 일과 필요한 플랫폼 변경

- **실제 프레임 확인(계획 절차 9):** 위 하네스는 실제 포털을 거치지 않았다. 설치 프레임에서 편집기·첨부·이미지·링크·인쇄를 확인해야 한다. AI-DO 기준 화면과의 나란한 비교, 카탈로그 공식 릴리스·dev 설치·권한 인수·prod 승격도 남았다.
- 공간 이름 바꾸기는 API(`PATCH /v1/spaces/{id}`)만 있고 화면이 없다(게시판과 같음).
- 다른 사람이 바꾼 페이지 트리는 문서를 다시 열어야 보인다(§3.3).
- **플랫폼 변경 필요(이 앱에서 하지 않음):**
  1. **클립보드 위임:** `web/platform/src/app/AppFrame.tsx`의 iframe에 `allow="clipboard-write"`(또는 앱별 선언). 없으면 Chromium에서 폐지 예정인 `execCommand` 대체에 기댄다.
  2. **포털 주소:** 프레임이 포털 링크를 만들 수단(예: 부트스트랩의 포털 출처를 `runtime`에 노출하거나 `runtime.portalUrl(path)`). Firefox는 `ancestorOrigins`를 no-referrer로 가려 지금은 포털 기준 경로만 복사된다.
  3. **다른 사람의 읽기 권한(그룹):** 플랫폼 호출의 그룹 문제는 서명된 그룹(d7fbdfe)으로 풀렸다(§3.6). 남은 것은 댓글 알림의 받는 사람 판정이다. 디렉터리는 호출자 자신의 그룹만 알려 주므로 앱이 다른 사람이 그룹으로 페이지를 읽을 수 있는지 알 수 없어, 그룹으로만 읽는 사람은 `comment`·`mention` 알림에서 빠진다(§8.3). 특정 사용자의 그룹 소속 확인(예: `is_member(user, group)`)이나 받는 사람마다 앱에 읽기 가능 여부를 묻는 발송 계약이 있으면 풀린다.
  4. **받는 사람 언어별 알림 제목**(게시판과 같음). 지금은 한국어로만 보낸다.
  5. **PMS 연결:** PMS 앱이 태스크의 연결을 소유한다. 문서의 `GET /v1/docs/items/{id}/reference`는 현재 호출자가 읽을 수 있는 활성 문서의 ID·제목·공간을 돌려준다. 문서 UI의 역방향 연결은 앱 간 권한과 호출 계약을 추가로 확정해야 한다. [포팅 대조 및 검증](../../../docs/releases/docs-whiteboard-port-parity-2026-11-23.md)
  6. **실시간 공동편집·트리 갱신:** `runtime.live`(M4) 위에 공동편집을 올리는 설계(계획의 후속).
  7. **공용 편집기 컴포넌트:** PMS와 문서가 같은 BlockNote 래퍼(`components/editor`)를 각자 갖고 있다. 공용 UI 후보([공통 컴포넌트 후보](../../../docs/PLATFORM-COMPONENT-CANDIDATES.md))로 검토.
  8. 서버 빌드가 npm 패키지(BlockNote·Mantine·dnd-kit·motion)를 받을 수 있어야 한다(`pnpm-lock.yaml`에 고정). 편집기 화면 청크가 약 1MB(gzip 302KB)다.
  9. 카탈로그(`registry/apps/catalog.json`)의 `docs` 항목에 릴리스를 기록하는 일(소스 고정 뒤).
  10. **미리 보기의 이동 확인:** 번들 `sdk/preview/main.tsx`는 작업이 보고된 동안 앱 자신의 `runtime.navigate`에도 확인 창을 띄운다(포털은 묻지 않음, 577bef5·4be4d65). 예를 들어 새 문서를 만든 직후 문서로 가는 이동에서 묻는다. 미리 보기도 포털과 같게 맞추는 것이 좋다(§8.5).

## 8. 보강: 휴지통·버전 기록·댓글·템플릿

계획 §2.2 문서 행의 보강이다. 네 기능 모두 AI-DO `0002af062`에 없어(AI-DO는 보관 목록만 있고 복원·버전·댓글·템플릿이 없었다) 화면은 원본 컴포넌트와 클래스(`app-*` 토큰, 원본 모달·메뉴 마크업)로 새로 만들었다. 쓰기는 모두 문서 행을 잠근 뒤 권한을 다시 판정하고(§3.2), 플랫폼 호출은 지금 읽을 수 있는 것만 답한다(§3.6).

### 8.1 휴지통

- **무엇이 들어가나:** 문서 삭제(`DELETE /v1/docs/items/{id}`)와 페이지 삭제(하위 포함)는 지우지 않고 `trashed_at`·`trashed_by_id`를 적는다(AI-DO와 같은 보관 + 누가 지웠는지). 화면의 "보관됨" 보기 대신 왼쪽 열의 **휴지통**(`/trash?space=`)이 그 공간의 항목을 보인다: 관리 권한이 있는 휴지통 문서, 편집 권한이 있는 살아 있는 문서의 휴지통 페이지. 페이지는 **삭제 묶음**(같은 시각에 함께 지워진 부분 트리)마다 한 줄이고 하위 페이지 수를 보인다. 문서와 함께 지워진 페이지는 문서 한 줄에 들어간다.
- **복원:** 문서 복원은 관리 권한, 문서와 같은 시각에 지워진 페이지를 함께 되살린다(그 전에 따로 지운 페이지는 휴지통에 남는다). 페이지 복원은 편집 권한, 같은 묶음의 하위를 함께 되살린다.
- **결정 — 상위가 아직 휴지통인 페이지:** 복원한 페이지를 **문서 맨 위(루트)의 끝으로 옮긴다**(`parent_id = NULL`, 응답 `moved_to_root: true`, 화면 문구로 알림). 상위를 함께 되살리는 방식은 사용자가 고르지 않은 페이지까지 살리고 그 상위의 다른 하위와의 관계가 모호해서 택하지 않았다.
- **영구 삭제:** 문서·페이지 모두 관리 권한(문서 소유자 또는 공간 `admin`). 페이지는 모든 하위가 휴지통에 있어야 한다(아니면 409 `docs_not_in_trash`). 같은 트랜잭션에서 첨부·이미지 행을 `detaching`으로 적고 행을 지운 뒤, **앱 커밋 뒤에** 코어 파일을 detach한다(`files_detach`). 첨부 행의 페이지 FK를 `ON DELETE SET NULL`로 바꿔(0004) 페이지 행이 지워져도 detach 기록이 남아 확인되지 않은 작업을 다음 요청이 끝낸다. 버전·댓글·댓글 알림 기록은 FK cascade로 함께 지워지고, 댓글 알림은 휴지통으로 보낼 때 이미 회수했다.
- **보존:** 자동으로 비우지 않는다(수동). 휴지통에 있는 동안 파일은 그대로 둔다(복원 뒤 첨부·이미지가 그대로 보이도록; 코어 사용량은 영구 삭제 때 줄어든다). 공간 삭제는 여전히 살아 있는 문서가 없을 때만 되고 휴지통 문서를 함께 정리한다.
- **화면:** 늦은 응답(다른 공간의 목록)은 버리고, 작업 중에는 미저장으로 보고한다. 실패하면 목록을 다시 읽는다.

### 8.2 버전 기록

- **기록:** 본문 revision이 바뀌는 저장(본문 저장·Markdown 가져오기·버전 복원·새 페이지·복제)마다 같은 트랜잭션에서 `docs_page_versions`에 그 revision의 본문·그때 제목·저장한 사람·시각을 남긴다. 제목·부모·순서 변경은 revision을 바꾸지 않으므로 버전도 남기지 않는다. 0004가 기존 페이지의 현재 본문을 첫 버전으로 채운다.
- **결정 — 보존 한도:** 페이지마다 **최근 50개 revision + 그보다 오래된 날(UTC 날짜)마다 그날의 마지막 revision 하나**. 같은 트랜잭션에서 정리한다. 800ms 자동 저장이라 revision이 빨리 늘어 전부 두면 행이 많아지고, 하루 하나씩이면 오래된 이력도 날짜 단위로 남는다.
- **목록·미리 보기:** 읽기 권한이면 목록(최신 먼저, 본문 없이)과 버전 하나의 본문을 받는다. 화면은 문서 메뉴의 「버전 기록」에서 시각·저장한 사람·revision·"현재"·"N번째 저장에서 복원"을 보이고, 고른 버전을 읽기 전용 `BlockViewer`로 미리 본다(늦게 온 미리 보기 응답은 버림).
- **복원:** 편집 권한, `{expected_revision}`을 받아 다르면 409(저장과 같은 규칙). 그 버전의 본문을 **새 revision으로 저장**하고 새 버전 행에 `restored_from_revision`을 적는다. 옛 행은 고치지 않는다. 제목은 바꾸지 않는다(제목은 메타데이터라 버전과 따로 움직인다). 화면은 먼저 초안을 저장하고, 저장되지 않으면(실패·충돌) "먼저 저장하라"고 알리고 복원하지 않아 초안을 잃지 않는다. 성공하면 편집기를 새 본문으로 다시 그린다.
- **이미지:** 옛 버전이 가리키는 `media:<id>`를 볼 수 있도록 본문에서 빠진 이미지를 끊지 않는다(§3.4). 이미지는 페이지 영구 삭제 때 떼어 낸다.

### 8.3 댓글

- **모델:** 페이지마다 스레드(첫 댓글 + 한 단계 답글), 해결/다시 열기는 첫 댓글에만(답글은 400 `docs_comment_reply_not_resolvable`). 페이지를 읽을 수 있으면 누구나 쓰고, 쓴 사람만 고치며(403 `docs_comment_author_required`), 쓴 사람이나 문서 관리 권한이 지운다(소프트 삭제; 답글이 남은 첫 댓글은 "삭제된 댓글"로 보인다). 해결은 편집 권한이나 스레드를 연 사람. 본문 1~10,000자.
- **요청 ID:** 쓰기는 `request_id`(UUID)를 받는다. 같은 ID·같은 내용이면 200으로 이미 만든 댓글을, 다른 내용이면 409 `docs_comment_request_conflict`. 화면은 응답을 못 받은 같은 댓글을 다시 보낼 때 같은 ID를 쓰고, 내용을 바꾸면 새 ID를 쓴다.
- **멘션:** 입력란에서 `@`를 치면 플랫폼 디렉터리(`/v1/docs/shareable-users`)에서 이름을 찾아 `@이름`을 넣고, 보낼 때 본문에 `@이름`이 남은 사람만 `mention_ids`로 보낸다(최대 20명, 디렉터리에 없는 사람은 400 `docs_mention_unknown_user`). 페이지를 읽을 수 없는 사람도 고를 수는 있지만 알림은 받지 않는다(아래).
- **알림:** `comment`(문서 소유자와 스레드 참여자, 쓴 사람·멘션된 사람 제외), `mention`(멘션된 사람, 쓴 사람 제외), 묶음 키 `comment:<id>`·`mention:<id>`, 경로 `/<docId>?page=<pageId>&comment=<id>`(화면이 그 댓글을 펼쳐 강조). 받는 사람마다 지금 그 페이지를 읽을 수 있는지 한 SQL 문으로 확인한다(`notify::user_can_read_page`). **다른 사람의 그룹은 앱이 알 수 없어**(디렉터리는 호출자 자신의 그룹만 답한다) 그 사람의 사용자 부여만 센다. 그룹으로만 읽는 사람은 빠진다(닫는 쪽, §7). 실제 보낸 사람은 `docs_comment_notifications`에 적어 회수 범위로 쓴다.
- **회수:** 댓글 삭제 → 두 묶음 모두, 멘션을 빼는 수정 → 기록된 그 사람, 페이지·문서를 휴지통으로 → 그 페이지들의 댓글 알림 모두, 공유 해제·공개 범위 변경·공간 멤버 변경 → 기록된 받는 사람 중 지금 읽을 수 없는 사람. 페이지의 댓글을 열면 내 댓글 알림(최근 20개)을 읽음 처리한다. 알림 재확인은 §3.6.
- **초안:** 입력란(새 댓글·답글·수정)마다 보내지 않은 글을 앱을 쓰는 동안 기억한다(페이지를 바꾸거나 실패하거나 늦은 응답이 와도 남는다). 남은 글이 있으면 앱 루트가 미저장으로 보고한다. 「취소」는 답글·수정 초안을 버린다. 다른 페이지의 늦은 목록 응답은 버린다.

### 8.4 템플릿

- **결정 — 기본 템플릿 없음:** AI-DO는 예전에 새 문서 창에 카드 3개(프로젝트 개요·회의록·위키)를 두었으나 제목만 채웠고 커밋 `77a44b5cf`에서 지워 `0002af062`에는 템플릿이 없다. 옮길 기본 템플릿이 없어 두지 않았다.
- **공간 템플릿:** 공간의 멤버 이상(`member`·`admin`; 전체 문서 공간은 모든 사용자)이 읽을 수 있는 페이지를 그 페이지가 있는 공간의 템플릿으로 저장한다(문서 메뉴 「템플릿으로 저장」, 이름 입력). 본문은 문서 잠금 뒤 저장된 본문을 복사한다(화면은 초안을 먼저 저장하고, 저장되지 않으면 만들지 않는다). 공간 역할이 있으면 누구나 목록을 보고 새 문서·새 페이지 창의 「템플릿」에서 골라 시작한다(`template_id`, 서버가 잠금 안에서 템플릿이 그 문서의 공간에 있고 호출자에게 공간 역할이 있는지 확인). 만든 사람이나 공간 관리자가 지운다.
- 템플릿 안의 이미지는 원본 페이지의 `media:<id>`를 가리키므로 원본 페이지를 읽을 수 있는 사람에게만 보인다(복제와 같은 한계, §3.4).

### 8.5 확인 (2026-11-22, 격리)

- **서버:** 일회용 PostgreSQL 컨테이너에서 DB 시험 48개 전체 통과(보강 시험 4개와 서명된 그룹 시험 포함). `cargo fmt --check` 통과.
- **화면:** 번들의 미리 보기(`sdk/preview`, 실제 `AppRoot`)를 앱 프레임 CSP로 내보내는 격리 하네스(가짜 코어: 디렉터리·알림·파일, 앱은 일회용 DB)에서 Chromium(Playwright 1.64.1)으로 조작: 휴지통 링크, 문서 만들기, 입력 저장, 버전 목록·읽기 전용 미리 보기, 복원(3 → 4개, "N번째 저장에서 복원", 편집기 본문 교체), `@Col` → "Colleague" 멘션 댓글과 `mention` 알림 발행, 답글·해결, 템플릿으로 저장 → 템플릿으로 새 페이지(본문 복사), 페이지 삭제 → 휴지통 복원, 문서 삭제 → 영구 삭제 → 빈 휴지통. 10단계 모두 PASS, CSP 위반 0건, 페이지 오류 0건.
- 미리 보기는 보고된 작업이 있으면 앱의 `runtime.navigate`에도 이동 확인을 띄운다(포털은 앱 스스로의 이동에는 묻지 않는다, 577bef5). 하네스는 그 확인(2번)을 수락했다.
- 실행하지 않은 것(`NOT_RUN`): 실제 포털 프레임과 Module Federation 적재, Firefox·WebKit, 알림 패널에서 `?comment=`로 여는 경로, 그룹 멤버의 댓글 알림, 모바일 폭.

### 8.6 하지 않은 것

- 휴지통 자동 비우기(보존 기간), 버전 사이 비교(diff), 버전 이름 붙이기, 댓글의 본문 위치 고정(블록에 다는 댓글), 댓글 반응·첨부, 템플릿 이름 바꾸기·공간 간 복사.

