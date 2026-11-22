# AI-DO 게시판 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 공식 앱 `board`(이름 `게시판`), 번들 0.12.0(2026-11-22 개정 CONTRACT, 플래너와 같은 잠금), Rust SDK 0.4.0, 공용 UI 2.0.0
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M7~M11 공식 업무 앱", "M2 플랫폼–앱 연결과 알림", §2.2 게시판 행
- 원칙: AI-DO 구현을 그대로 옮기고, miy의 경계(인증·저장·앱 프레임·앱별 DB·디렉터리) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다. 보강(공지·댓글 알림, 금칙어)은 계획상 포팅 인수 뒤 항목이지만 계약이 이미 있어 이번에 함께 넣었고, 따로 표시했다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 커뮤니티 채널(기본 채널 `suggestions`, 순서·활성, 읽기 전용, 완전 익명, 완전 비공개(건의함), 글 제목·본문 템플릿)과 채널 관리 화면, 글 목록(번호·제목·댓글 수·작성자·작성일, 읽음/안읽음 굵기, 페이지 크기 10~100, 이전/다음), 번호·제목·작성자 검색, 글 작성·수정·삭제, 익명 글, 비밀글(비밀번호 열기), 댓글·답글(한 단계)·수정·삭제(본문을 비우는 삭제), 익명 댓글 번호, 읽음 표시, 채널별 고정 글 번호(재사용하지 않음, 잠금 대기 없이 409), 본문 Markdown 편집기(Milkdown Crepe)·보기(react-markdown)와 이미지 첨부, 공지(공간·전사, 상단 고정, 작성·수정·삭제·상세).

**보강(AI-DO에 없던 것):** 공지 알림과 댓글 알림을 플랫폼 알림함으로(§3.6), 플랫폼 재확인·홈 카드 엔드포인트(§3.7), 글·댓글 금칙어(§3.8), 게시판 관리자와 공간 화면(§3.2).

**뺀 것(화면·API 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 좋아요 등 반응 | (AI-DO에도 없음) | 계획 §2.2에서 요구 확인 뒤 |
| 댓글 알림의 봇 DM과 실시간 이벤트 | `community/notifications.py` `deliver_notification_as_bot_dm`, `publish_comment_notification`, `dm/realtime_events` | 메신저 앱 데이터. 알림은 플랫폼 알림함으로 보낸다(§3.6) |
| 이용규칙 수락 | `ugc_policy/terms.py`, `ugc_terms_acceptances`, `UgcTermsStatus` 등 | 이번 보강 범위는 금칙어만. 이용규칙은 메신저 범위(§2.2) |
| 공지 관리 위임(이관 공지) | `AnnouncementManagementGrant`, `migrated_from_workspace_id` | AI-DO 데이터 이관용. miy에는 이관할 공지가 없다 |
| 홈 화면의 공지 카드 | `home/views/WorkspaceHomeView/WorkspaceHomeView.tsx` | 포털 홈은 M5. 대신 읽기 전용 `GET /v1/board/notices/recent`와 `homeCard`를 둔다(§3.7) |
| AI 도구·검색 연동·포털 전역 위젯 | (커뮤니티에는 없음) | 계획의 제외 범위 |
| AI-DO 셸 연동 | `community/index.ts`, `manifest.ts`(+spec), `routes.ts`, `announcements/manifest.ts`, `public-api.ts`, 관리 콘솔 `admin-sections.ts` | miy는 manifest·포털이 담당 |
| 감사 로그 | `writeAuditActions`(`community.*`), 금칙어 `record_audit_log` | 플랫폼 감사 로그에는 업무 본문을 넣지 않는다(§2.2 메신저 기준과 같음). 앱 감사 표는 두지 않았다 |
| SVG 이미지 | `media/router.py` `ALLOWED_IMAGE_TYPES`의 `image/svg+xml` | miy 파일 게이트웨이·공용 미리보기는 래스터 이미지만 다룬다 |

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>`를 앱 `web/src/<경로>`로 옮겼다. "그대로"는 import 경로 외 변경이 없다는 뜻이다.

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/community/views/CommunityView.tsx` | 같은 경로 | 수정: 앱 경로·`runtime.api`·`runtime.fileApi`, 관리자 → 게시판 관리자, 미저장 보고, 금칙어 거절 문구 |
| `app-modules/community/api/community-api.ts` | 같은 경로 | 수정: `runtime.api`, 앱 기준 경로, 이미지 해석은 파일 게이트웨이와 Blob URL, 오류 코드 보관 |
| `app-modules/community/community-url.ts`, `community-constants.ts` | 같은 경로 | 그대로 |
| `app-modules/community/sidebar.tsx` | 같은 경로(`BoardSidebar`) | 수정: 프레임 왼쪽에 그림, 링크 → `runtime.navigate`, 공지·게시판 관리 항목 추가 |
| `platform/community/CommunityMarkdownEditor.tsx`, `community-markdown-editor.css`, `community-channel-events.ts` | 같은 경로 | 편집기·CSS·이벤트는 그대로. 보기의 이미지는 Blob URL을 쓰고 없앨 때 해제(§3.5) |
| `platform/media/use-media-upload.ts` | 같은 경로 | 재작성: 코어 파일 예약 → `fileApi` 업로드 → `media:<id>` |
| `components/date/UserDateTime.tsx` | 같은 경로 | 수정: 사용자 프로필 대신 `runtime.settings` |
| `platform/admin/admin-community-section.tsx` | `app-modules/board-admin/views/admin-community-section.tsx` | 수정: `runtime.api`, 미저장 보고 |
| `platform/admin/community-channel-form-model.ts`(+spec) | `app-modules/board-admin/` | 그대로 |
| `platform/admin/admin-api.ts`(커뮤니티 부분), `admin-dm-safety-api.ts`(금칙어 부분) | `app-modules/board-admin/api/board-admin-api.ts` | 수정: 앱 기준 경로 |
| `platform/admin/admin-dm-safety-section.tsx` `DmSafetyFilterTerms` | `app-modules/board-admin/views/BoardAdminView.tsx` `FilterTerms` | 마크업 그대로, 적용 범위 문장만 게시판용 |
| `platform/admin/admin-shared-*.ts(x)`의 `FORM_FIELD_CLASS`·`getErrorMessage`·`SectionMessage` | `app-modules/board-admin/views/admin-shared.tsx` | 그대로 |
| `app-modules/announcements/api/announcements-api.ts`(+spec) | 같은 경로 | 수정: `runtime.api`, 작업 공간 경로 재작성 대신 공간 경로, 단건 조회 추가 |
| `app-modules/home/views/WorkspaceHomeView/AnnouncementsBoard.tsx`(+spec) | `app-modules/announcements/views/AnnouncementsBoard.tsx` | 수정: 공간·권한 입력, 알림 링크로 열기, 미저장 보고 |
| (없음) | `app-modules/announcements/views/NoticesView.tsx`, `app-modules/board-admin/views/BoardAdminView.tsx`, `platform/board/*`, `platform/runtime/app-location.ts`, `AppRoot.tsx` | miy 연결과 보강 |
| `platform/api/client.ts`, `platform/i18n/*`, `platform/time/*`, `lib/utils.ts`, `test-setup.ts` | 같은 경로 | 플래너 포팅본을 그대로 사용(플래너 PORTING §2) |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: [scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py) |
| `index.css`의 앱 공통 클래스, `@plugin "@tailwindcss/typography"` | 공용 UI 2.0.0 `app.css` + `web/src/app.css`의 typography 플러그인 | 앱에 복사하지 않음 |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/…` | miy `src/…` |
| --- | --- |
| `community/router.py`, `service.py`, `schemas.py`, `models.py` | `community.rs` |
| `community/notifications.py` | `notify.rs`(플랫폼 알림) |
| `announcements/router.py`, `service.py`, `schemas.py`, `models.py` | `announcements.rs` |
| `ugc_policy/content_filter.py`, `admin/dm_safety_router.py`(금칙어 경로) | `ugc.rs` |
| `media/router.py`(업로드), `service.sync_embedded_media`, `community/service.resolve_post_media_urls` | `media.rs` |
| `auth/security.py` `hash_password`·`verify_password` | `password.rs` |
| `auth/access.is_platform_admin_user` | `managers.rs`(게시판 관리자) |
| 작업 공간과 그 멤버(`Workspace`, `WorkspaceUserBinding`, `resolve_workspace_role`) | `spaces.rs`(SDK ACL) |
| `users` 표 이름 조회 | `people.rs`(플랫폼 디렉터리) |
| `core/i18n.localized_http_exception` | `error.rs` |
| (없음) | `notify.rs` `notification_check`·`home_card`, `announcements.rs` `recent` |

## 3. 원본과 달라진 점과 이유

### 3.1 데이터 모델

`migrations/0002_acl.sql`은 SDK `acl.sql` 그대로다(계약: 번호를 붙인 앱 migration으로 적용). `migrations/0003_board.sql`:

| 표 | AI-DO | miy |
| --- | --- | --- |
| `community_channels` | `id, key(unique), name, description, position, active, read_only, force_anonymous, admin_only_content, template_title, template_body, last_post_number, created_at, updated_at` | 같음 |
| `community_posts` | `…, author_id(FK users), …, password_hash, …` + `uq_community_posts_channel_number` | `author_id`는 `text`·FK 없음. 나머지 같음 |
| `community_post_reads` | `post_id(FK, cascade), user_id(FK), read_at` | `user_id`는 `text` |
| `community_comments` | `…, author_id(FK), parent_comment_id(FK self, cascade), is_anonymous, is_deleted, …` | `author_id`는 `text` |
| `announcements` | `id, workspace_id(FK, not null), migrated_from_workspace_id, author_id(FK), scope, title(200), body, is_pinned, created_at, updated_at` | `workspace_id`는 `board_spaces` 참조이고 전사 공지는 비움(`ck_announcements_workspace_scope`), `migrated_from_workspace_id` 없음 |
| `ugc_filter_terms` | `id, term(50), normalized(50, unique), created_by_id(FK), created_at` | `created_by_id`는 `text` |
| `media_files` | `id, storage_key, filename, content_type, size_bytes, uploaded_by_id(FK), resource_type, resource_id, created_at` | `storage_key` 대신 `request_id`·`file_id`(코어 파일), `status` 추가 |
| (없음) | — | `board_spaces`(공간), `board_managers`(게시판 관리자) |

- 사용자 ID는 miy 사용자 ID(`Identity::user_id`)다. 사용자 표는 코어 DB에 있어 FK를 둘 수 없다.
- 시각은 AI-DO처럼 naive UTC(`timestamp`), ID는 UUID v4 문자열이다.
- AI-DO의 `ensure_default_channels`(요청 때 기본 채널 생성)는 그대로 요청 때 한다. migration에 시드를 넣지 않았다.
- 익명 글·댓글도 작성자 ID는 저장한다(관리자 확인용). 다른 사람에게는 이름도 ID도 돌려주지 않는다(API 응답에 `authorId`가 없다). 시험 `force_anonymous_channel_masks_names_except_for_admin`이 응답에 ID가 없음을 확인한다.

### 3.2 공간·게시판 관리자·권한 (워크스페이스 대응)

| AI-DO | miy |
| --- | --- |
| 커뮤니티: 전사 범위(`resourceScope: 'company'`), 활성 채널은 모든 사용자에게 보임 | 그대로: 앱을 쓸 수 있는 모든 사람에게 보임. 채널에는 멤버가 없다 |
| 플랫폼 관리자(`is_platform_admin_user`): 채널 관리, 완전 비공개 글·익명 작성자 열람, 읽기 전용 채널 쓰기, 남의 글·댓글 수정·삭제, 전사 공지 관리 | **게시판 관리자**(`board_managers`). 플랫폼 관리자 표시(`Identity::admin`)는 아무 권한도 주지 않는다(시험으로 확인) |
| 워크스페이스(`Workspace`)와 멤버 역할 | **공간**(`board_spaces`). 멤버는 SDK 자원 ACL(kind `space`)의 사용자·그룹 부여 |
| 워크스페이스 공지 작성: 워크스페이스 `admin`·`owner` | 공간 역할 `admin`(ACL 동작 `notice.manage`, `acl.manage`) |
| 워크스페이스 공지 읽기: 워크스페이스 멤버 | 공간 역할 `member` 이상(`space.read`) |
| 워크스페이스 만들기: 플랫폼 관리자 | 공간 만들기: 게시판 관리자. 만든 사람이 보호된 소유자(`admin`) |

- **채널에 ACL을 쓰지 않은 이유:** AI-DO 채널은 멤버 개념이 없고 공개 범위를 채널 플래그(읽기 전용·완전 익명·완전 비공개)로 정한다. ACL을 붙이면 원본에 없는 비공개 채널이 생긴다. 멤버가 필요한 것은 공간 공지뿐이라 공간에만 ACL을 썼다.
- **채널 관리자:** 비밀글·완전 비공개 글을 볼 수 있는 "관리자"는 AI-DO의 플랫폼 관리자이고, miy에서는 채널을 관리하는 게시판 관리자다. 채널별 관리자는 두지 않았다.
- **첫 게시판 관리자(부트스트랩):** 관리자가 한 명도 없을 때 앱을 쓸 수 있는 사람이 `POST /v1/board/managers/bootstrap`으로 한 번 맡는다(표 잠금으로 한 명만). 화면은 그 상태에서 모두에게 안내를 보인다. 이후 관리자 추가·해제는 관리자만 하고, 마지막 관리자는 해제할 수 없다. 앱 사용 권한은 플랫폼이 승인한 사람에게만 있으므로 설치 직후 운영자가 먼저 맡는 것을 전제로 한다. 플랫폼 관리자 표시를 쓰지 말라는 지시에 따라 다른 자동 지정은 두지 않았다.
- 공간 화면(공간 고르기·만들기·멤버 관리·삭제)은 AI-DO에 없던 miy 화면이다. 멤버 관리는 공용 UI `ShareEditor`를 그대로 쓴다. 공간 삭제는 그 공간 공지도 지운다(FK cascade). 전사 공지는 공간을 갖지 않으므로 공간 삭제로 지워지지 않는다.

### 3.3 익명·비밀·완전 비공개 규칙 (그대로)

AI-DO 규칙을 바꾸지 않았다. `post_locked_reason`·`post_out`·`comment_out`·`anon_seq_map`과 라우터의 검사 순서는 원본을 한 줄씩 옮겼다.

- 비밀글은 글쓴이·게시판 관리자에게 열려 있고, 그 밖의 사람은 **비밀번호로 열 수 있다**(AI-DO `unlock`). 지시의 "글쓴이와 관리자만"은 AI-DO 규칙을 그대로 지키라는 조건과 함께 와서, 원본의 비밀번호 열기를 유지했다. 비밀번호는 AI-DO와 같은 `pbkdf2_sha256$600000$salt$digest`로 저장한다(Python `hashlib`과 같은 결과, 시험으로 확인).
- 완전 비공개 채널의 글은 게시판 관리자만 본다. 글쓴이 자신도 제목·본문을 보지 못한다(원본과 같음). 이 채널에서는 비밀글 설정을 무시한다.
- 글 작성 응답은 AI-DO처럼 `viewer_is_admin=False`로 만든다. 그래서 관리자가 완전 비공개 채널에 글을 써도 응답은 잠겨 있다(화면은 이어서 상세를 다시 불러온다).
- 익명 댓글 번호는 삭제된 댓글을 포함한 작성 순서로 매긴다(원본과 같음).

### 3.4 이름과 검색

- AI-DO는 `users` 표를 조인해 표시 이름(`display_name`, 없으면 `full_name`)을 얻고 작성자 검색을 `ILIKE`로 했다. miy는 플랫폼 디렉터리(`platform:directory:read`)로 바꿨다. 이름은 ID로 최대 100명씩 읽고, 디렉터리를 읽지 못하면 이름만 비운다(AI-DO의 이름 없음 표시 "알 수 없음"). 요청은 실패시키지 않는다.
- 작성자 검색은 디렉터리 이름 검색(`q`, 최대 5쪽 500명)으로 ID를 얻어 `author_id = ANY(...)`로 거른다. 디렉터리 검색 규칙(부분 일치 여부 등)은 코어를 따른다. 목록의 제목·작성자 표시 권한을 검색과 건수에도 적용한다. 일반 사용자는 완전 비공개 채널에서 번호만 검색하며, 완전 익명도 적용된 채널에서는 검색이 닫힌다.
- 제목 검색 패턴은 원본처럼 `%`·`_`를 이스케이프하지 않는다.
- 공지 작성자 이름(`authorName`)은 AI-DO처럼 이름이 없으면 작성자 ID다.

### 3.5 본문 이미지 (`platform:files:*`)

AI-DO는 이미지를 자체 저장소에 올리고 `media:<id>`를 본문에 넣은 뒤, 볼 때 `POST /community/posts/{id}/media/resolve`로 서명된 프록시 URL을 받았다. miy에는 공개 파일 URL이 없다.

1. `POST /v1/media/upload`가 요청 ID를 먼저 앱 DB에 적고 코어 파일을 예약한다. 응답이 불확실하면 같은 요청 ID로 조회한다. 응답의 `id`, `url`(`media:<id>`), `status`는 같은 예약을 가리킨다.
2. 화면이 `runtime.fileApi('/media/<id>/content', {method:'PUT'})`로 바이트를 보낸다. 올린 사람만 보낼 수 있다. 응답을 받지 못하면 화면은 기존 예약 ID와 파일 경로를 다시 사용한다.
3. 글·댓글을 저장할 때 본문의 `media:<id>` 중 올린 사람이 같은 연결 안 된 이미지를 그 글·댓글에 연결한다(AI-DO `sync_embedded_media`). 코어 연결을 확인하지 못하면 다음 저장에서 같은 요청 ID로 완료한다. 본문에서 빠진 이미지는 연결만 끊는다(원본과 같음, 바이트는 남는다).
4. 보기: 글·댓글 이미지는 `POST /files/community/posts/{postId}/media/{id}`(비밀글은 본문에 비밀번호)로, 편집 중인 내 이미지나 비밀번호 없이 읽을 수 있는 글의 이미지는 `GET /files/media/{id}/content`로 프레임 안에서 내려받아 Blob URL로 보인다. 검사는 AI-DO resolve와 같다(완전 비공개 → 관리자, 비밀글 → 글쓴이·관리자·비밀번호, 삭제된 댓글의 이미지 제외). Blob URL은 이미지가 사라질 때 해제한다. 해석되지 않은 `media:` 주소는 그리지 않는다.
5. 글·댓글을 지우면 연결된 이미지를 같은 트랜잭션에서 `detaching`으로 적고, 커밋 뒤 코어에서 떼어 낸다(붙어 있으면 detach, 아니면 cancel). 확인되지 않은 작업은 다음 삭제 요청이 이어서 끝낸다(`app check` 하네스 지적 반영). AI-DO는 글을 지워도 파일을 지우지 않았다. miy는 앱 쿼터(설치당 256MiB)를 돌려받기 위해 지운다.
- 형식은 PNG·JPEG·GIF·WebP, 파일당 10MiB(AI-DO와 같은 크기 제한, SVG 제외).
- 프레임 CSP는 `img-src 'self' data: blob:`라 본문의 외부 이미지 링크는 표시되지 않는다.

### 3.6 알림 (보강)

| 종류 | 언제 | 받는 사람 | 묶음 키 | 제목 |
| --- | --- | --- | --- | --- |
| `comment` | 댓글·답글 작성 | 글쓴이. 본인 댓글이면 없음. 완전 비공개 채널이면 글쓴이가 게시판 관리자일 때만(AI-DO `create_comment_notification` 규칙) | `post:<id>` | `<이름>님이 "<글 제목>"에 댓글을 남겼습니다` / 익명이면 `익명 사용자가 …` |
| `notice` | 공지 등록 | 전사: 디렉터리의 모든 사람(최대 100쪽, 1만 명), 공간: 공간에 **직접** 추가된 사람. 작성자 제외 | `notice:<id>` | `[전사 공지] <제목>` / `[<공간> 공지] <제목>` |

- 경로는 앱 기준이다: 댓글은 AI-DO `_community_post_url`과 같은 `/community/posts/<id>[?channel=<key>]`, 공지는 `/notices/<id>[?space=<id>]`.
- 제목에 댓글 본문을 넣지 않는다(계약). AI-DO 문구의 앞부분을 쓰고 미리보기는 뺐다. 받는 사람의 언어를 앱이 모르므로 한국어로 보낸다(AI-DO는 받는 사람 언어별로 골랐다).
- 발행은 업무 커밋 뒤 같은 요청 안에서 하고, 실패해도 업무를 되돌리지 않는다. 결과가 불확실하면 같은 요청 ID로 조회만 한다. 500명씩 나눠 보낸다.
- 회수: 글 삭제 → `post:<id>`, 공지 삭제 → `notice:<id>`, 공간 삭제 → 그 공간 공지 전부, 공간에서 빠진 사람 → 그 공간 공지(최근 200개)를 그 사람에게서.
- 읽음: 글쓴이가 글을 열면 `post:<id>`, 누가 공지를 열면(`GET …/announcements/{id}`) `notice:<id>`.
- 공지를 고쳐도 다시 알리지 않는다(알림 줄의 제목은 등록 때 것이다). 원본에 공지 알림이 없어 기준이 없으므로, 수정마다 모두에게 다시 안읽음을 만들지 않는 쪽을 골랐다.
- **한계:** 디렉터리는 다른 사람의 그룹 소속을 알려 주지 않는다(`memberships`는 본인만). 그래서 그룹으로만 공간에 들어간 사람에게는 공간 공지 알림을 보내지 못하고, 플랫폼 재확인·홈 카드도 그 사람의 공간 공지를 보이지 않는다(안전한 쪽으로 닫음). 앱 안에서는 그룹 멤버도 공지를 읽고 쓴다(요청마다 ACL을 확인). §6의 플랫폼 변경이 필요하다.

### 3.7 플랫폼 → 앱 엔드포인트와 최근 공지

- `notificationCheck` → `POST /platform/notification-check`: 묶음 키와 `resourceId`가 같은 항목을 가리켜야 한다. `post:<id>`는 글과 활성 채널을 그 사람이 비밀번호 없이 열 수 있을 때, `notice:<id>`는 전사 공지이거나 그 사람이 공간에 직접 들어 있을 때 허용한다. 공지와 공간 권한을 한 SQL 문에서 읽는다. 플랫폼 호출에는 다른 플랫폼 호출용 토큰이 없어 앱 DB만으로 판단한다. 플랫폼 호출(`identity.context.platform`)이 아니면 403이다. 응답은 읽기 전용이다.
- `homeCard` → `POST /platform/home-card`: 읽을 수 있는 공지 최대 5개(고정 → 최신), `{"title":"공지","items":[{"title","subtitle":"<공간 이름>|전사","path","at"(RFC 3339 UTC)}]}`. `count`는 읽음을 기록하지 않아 뺐다. 본문의 `locale`이 `en-US`이면 `Announcements`·`Company`로 답한다. 플랫폼 호출이 아니면 403.
- `GET /v1/board/notices/recent?limit=`(기본 5, 최대 20): 프레임·앱 호출용 읽기 전용 목록(최신순, 공간 이름·경로 포함). M5 홈 공지 카드가 앱 호출로 쓸 수 있게 남겼다. 이 경로는 ACL로 공간을 고르므로 그룹 멤버도 포함한다.

### 3.8 금칙어 (보강)

- AI-DO `content_filter.py`를 옮겼다: NFKC → 대소문자 접기(Python `casefold`와 같은 전체 접기, `caseless` crate) → 공백(Python `\s`, U+001C~001F 포함)·보이지 않는 글자·구분 기호 제거 뒤 포함 여부. 걸리면 422 `ugc_content_rejected`이고 어느 낱말인지 알리지 않는다. 등록 규칙(1~50자, 정규화 뒤 50자 초과 거절, 최대 1000개, 정규화 값 중복 409, 알 수 없는 필드 422)도 같다.
- 적용: 글 제목·본문(작성·수정), 댓글 본문(작성·수정). 게시판 관리자도 예외가 아니다. 공지·채널 설명·이미지 파일 이름에는 적용하지 않았다(지시 범위가 글·댓글).
- 목록은 앱마다 따로다(앱별 DB). AI-DO는 DM·업무와 한 목록을 공유했다. 관리 화면은 게시판 관리 > 금칙어(AI-DO DM 안전 화면의 금칙어 부분 마크업).
- AI-DO 게시판 화면은 실패하면 고정 문구만 보였다. 금칙어로 거절되면 이유를 알 수 있게 카탈로그 문장("운영 정책상 보낼 수 없는 표현이 포함되어 있습니다.")을 보이고, 그 밖의 실패는 원본 문구 그대로다(`contentErrorMessage`).

### 3.9 오류 응답과 문구

- 플래너와 같은 방식이다: `{"error":{"code","message"}}`, 코드는 AI-DO 코드의 `.`을 `_`로, `message`는 AI-DO 영어 카탈로그 문장, 화면은 `errors` 네임스페이스로 번역한다. 요청 검증 실패는 422 `validation_request_invalid`.
- 상태 코드와 검증 규칙(제목 1~240자 후 공백 제거, 본문 1~20000자, 댓글 1~10000자, 비밀번호 200자, 채널 key 패턴, 공지 제목 1~200자·본문 20000자, 목록 limit 1~100, page_size 1~100, q 120자)은 AI-DO와 같다.
- 문구를 바꾼 서버 메시지: `announcements.admin_required`(워크스페이스 관리자 → 공간 관리자), `announcements.company_admin_required`(플랫폼 관리자 → 게시판 관리자), `admin.platform_admin_required` → `board.manager_required`(게시판 관리자 권한).
- 새 코드: `announcements_space_required`, `board_manager_*`, `board_last_manager`, `board_space_not_found`, `board_platform_only`, `media_*`, `acl_*`(SDK).
- 화면 문구는 [생성 스크립트](../scripts/gen-i18n-resources.py)가 AI-DO `resources.ts`의 `common`, `apps.community`, `apps.home`의 공지 키, `apps.admin.console.community`, `apps.admin.console.dmSafety`의 금칙어 키를 그대로 복사한다. miy 문구는 `apps.board`와 `errors`에 있다.

### 3.10 화면 경로와 구성

- AI-DO 경로 `/community`, `/community/posts/:postId`(+`?channel=`)를 그대로 쓴다. `/`도 커뮤니티다. react-router 대신 `runtime.location`·`runtime.navigate`(`useAppSearchParams`, `useAppNavigate`).
- AI-DO는 포털 보조 사이드바에 채널 목록을 그렸다. 프레임에는 포털 사이드바가 없어 앱 왼쪽에 같은 마크업(`sidebar-submenu-item`)으로 그리고, 공지와 게시판 관리 항목을 더했다.
- 공지 화면 `/notices`는 AI-DO 워크스페이스 홈의 두 공지 보드(공간·전사)를 나란히 놓은 것이다. 보드 컴포넌트는 원본이고, 알림 링크(`/notices/<id>`)로 그 공지를 연다.
- 게시판 관리 `/admin`은 AI-DO 관리 콘솔의 커뮤니티 채널 관리와 금칙어를 가져오고 관리자 탭을 더했다.
- 커뮤니티·관리 화면은 편집기(Milkdown, 약 1.4MB)를 쓰므로 지연 로드한다(AI-DO 라우트도 지연 로드).
- **미저장 보고(miy 계약):** 글 작성 창(템플릿과 다를 때), 비밀번호 입력, 글·댓글 수정, 작성 중인 댓글, 진행 중인 작성, 공지 편집 창, 채널 편집, 공간 만들기·멤버 편집을 `runtime.setNavigationState`로 합쳐 보고한다. AI-DO에는 이동 확인이 없었다. 문구는 `apps:board.unsavedChanges`.
- 날짜·시간 표시는 `runtime.settings`(언어·시간대·날짜 형식)를 따른다.

### 3.11 서버 동작의 세부

- `updated_at`은 SQLAlchemy `onupdate`처럼 값이 실제로 바뀐 수정에서만 갱신한다. 읽음 판정(`read_at >= updated_at`)이 이에 기대므로 같은 내용으로 저장해도 다른 사람의 읽음이 풀리지 않는다.
- 글 번호는 AI-DO `_next_post_number`와 같이 채널 행을 `FOR NO KEY UPDATE NOWAIT`로 잡고 늘린다. 잠겨 있으면 기다리지 않고 409 `community_post_number_busy`, 거절된 글은 번호를 가져가지 않는다. 비밀번호 해시 등 느린 일은 번호를 잡기 전에 한다. 채널 `updated_at`은 바꾸지 않는다.
- 요청 본문은 camelCase 별칭과 snake_case 필드 이름을 모두 받는다(Pydantic `populate_by_name`). 알 수 없는 필드는 무시한다(금칙어 추가만 원본처럼 거절).
- PBKDF2 60만 회는 비동기 작업자를 막지 않도록 `spawn_blocking`에서 한다. 디버그 빌드 시험이 느려지지 않게 `sha2`·`hmac`·`pbkdf2`는 개발 프로필에서도 최적화한다.

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 채널은 전사 범위 그대로, ACL은 공간에만 | 원본 채널에 멤버가 없다(§3.2) |
| 게시판 관리자 표 + 한 번만 가능한 부트스트랩 | 플랫폼 관리자 표시를 쓰지 않는 가장 단순한 시작 방법 |
| 공간 역할은 `member`·`admin` 둘 | AI-DO 공지 권한이 "멤버 읽기, admin·owner 쓰기"뿐이다. `member`가 SDK 기본 역할이 된다 |
| 비밀글 비밀번호 열기 유지 | "AI-DO 규칙 그대로" 조건 |
| 알림 수신자는 직접 멤버, 재확인은 DB만 | 디렉터리가 다른 사람의 그룹 소속을 주지 않고, 플랫폼 호출에는 디렉터리 토큰이 없다. 열어 두는 쪽보다 닫는 쪽을 골랐다 |
| 전사 공지 알림은 디렉터리 전체에 | 플랫폼이 앱 사용 권한 없는 사람을 걸러 준다(계약) |
| 공지 수정은 다시 알리지 않음 | §3.6 |
| 이미지 바이트는 글·댓글 삭제 때 지움 | 앱 쿼터 회수. 연결만 끊긴 이미지는 원본처럼 남긴다 |
| 의존성은 AI-DO 고정 버전 | `@milkdown/crepe`·`@milkdown/kit` 7.21.2, `react-markdown` 10.1.0, `rehype-highlight` 7.0.2, `remark-gfm` 4.0.1, `@tailwindcss/typography` 0.5.19, i18next 26.0.8, react-i18next 17.0.6, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library. React·Vite·Tailwind·Radix 등은 템플릿·공용 UI 버전. Rust는 플래너 구성에 `pbkdf2`·`sha2`·`hex`·`unicode-normalization`·`caseless`를 더함 |
| 공용 UI 설치는 플래너와 같은 오프라인 복사 | `registry/components/ui/2.0.0`과 같은 파일(`component.json` 제외)과 플래너의 잠금 파일. 실제 서버에서 `miy component sync`로 digest를 확인해야 한다 |
| `docs/CONTRACT.md`와 `miy.bundle.json`의 그 해시 | 작업 중 저장소의 번들 0.12.0 CONTRACT가 개정되어(사이드 패널·홈 카드) 플래너와 같이 새 문서와 해시로 맞췄다. 그 밖의 보호 파일은 init 그대로다 |
| DB 시험은 `#[ignore]` + `BOARD_TEST_DATABASE_URL`, 가짜 코어 | 서버 빌드의 `cargo test`에는 DB가 없다. 디렉터리·알림·파일 호출은 시험 안의 가짜 코어가 SDK의 실제 HTTP 경로로 받는다 |
| 프런트 시험은 플래너와 같은 설정(jsdom, `TZ=Asia/Seoul`) | |

## 5. 테스트 대응

| AI-DO | miy | 비고 |
| --- | --- | --- |
| `tests/test_community.py`(33) | `src/api_tests.rs` 28개 + `community.rs` 단위 6개 | 같은 입력에 같은 결과. 서비스 직접 호출 시험은 HTTP로 옮김 |
| ― 봇 DM 시험 3개(`…bot_dm…`, `test_community_comment_dm_body.py`) | 댓글 알림 시험 4개(링크·익명·본인·완전 비공개) | 봇 DM 대신 플랫폼 알림 |
| ― `test_post_number_lock_never_waits_in_the_request_pool` | `post_number_lock_never_waits` | 요청 스레드 한도 대신 응답 시간(1.5초 미만)과 동시 작성 번호 연속성 |
| `tests/test_announcements.py`(5) | `api_tests.rs` 3개 | 공간 경로로. 이관 공지 시험(`test_announcement_migration_access.py`)은 기능과 함께 제외 |
| `tests/test_ugc_policy.py`의 금칙어 3개 | `ugc.rs` 단위 4개 | 이용규칙 시험은 제외 |
| `tests/test_ugc_policy_db.py`의 금칙어 시험 | `banned_words_reject_posts_and_comments_without_naming_the_term` | 게시판 적용 범위로 |
| (miy 추가) | 이미지 업로드·연결·보기·삭제·검증, 미확인 detach 재시도, 공지 알림·회수·읽음, 플랫폼 재확인·홈 카드·최근 공지, 공간 권한(그룹 포함), 게시판 관리자 부트스트랩, 신원 없음·위조·플랫폼 관리자 표시 무시, 검증 규칙, 비밀번호 해시 형식(RFC 7914 벡터) | |
| 화면 `community-url.spec.ts`, `community-channel-form-model.spec.ts` | 같은 경로, 그대로 | |
| `community-api.spec.ts`(5) | 같은 경로 5개 + 1개 | `fetch` 대신 `runtime.api`·`fileApi` 모의 |
| `CommunityView.spec.tsx`(9) | 같은 경로 9개 + 2개(미저장 보고, 금칙어 문구) | 라우터 대신 프레임 런타임 모의 |
| `announcements-api.spec.ts`(2) | 같은 경로 2개 + 1개 | 공간 경로 |
| `AnnouncementsBoard.spec.tsx`(4종) | `announcements/views/` 4종 + 알림 링크 1개 | 홈 역할 대신 공간 관리 표시·게시판 관리자 |
| (miy 추가) | `app-location.spec.ts`, `api-error.spec.ts`, 플래너에서 가져온 `platform/time/*.spec.ts` | |

옮기지 않은 화면 시험: `manifest.spec.ts`(셸 연동 제외), `admin-dm-safety-section.spec.tsx`(신고 검토 부분은 메신저 범위, 금칙어 부분은 서버 시험으로).

## 6. 남은 일과 필요한 플랫폼 변경

- **실제 프레임 확인(계획 절차 9):** 이 포팅은 단위·HTTP·jsdom 시험과 프로덕션 빌드까지 확인했다. 실제 플랫폼 프레임이나 SDK 미리보기에서 화면을 띄워 보지 않았다. 특히 Milkdown Crepe 편집기가 프레임 CSP(`script-src 'self'`, `worker-src 'none'`)에서 동작하는지, `runtime.fileApi`의 PUT·POST 본문 전달, 포털 이동 확인을 확인해야 한다.
- AI-DO 기준 화면(가상 계정·가상 데이터)과의 나란한 비교, 카탈로그 공식 릴리스·dev 설치·권한 인수·prod 승격.
- 공간 이름 바꾸기는 API(`PATCH /v1/spaces/{id}`)만 있고 화면이 없다.
- **플랫폼 변경 필요(이 앱에서 하지 않음):**
  1. 그룹 멤버 알림: 디렉터리에 그룹 구성원 조회를 더하거나(권한 필요), 알림 수신자로 그룹 ID를 받게 하기. 지금은 공간에 그룹으로 들어간 사람이 공간 공지 알림을 받지 못한다.
  2. 플랫폼 호출의 그룹 확인: `notificationCheck`·`homeCard` 호출의 서명된 신원에 그 사람의 현재 그룹 소속을 넣거나 디렉터리 소속 조회용 범위 제한 토큰을 주기. 지금은 그룹 멤버의 공간 공지가 알림 패널·홈 카드에서 숨는다.
  3. 받는 사람 언어별 알림 제목(또는 플랫폼이 번역하는 키+인자 형식). 지금은 한국어로만 보낸다.
  4. 금칙어 공용 목록이 필요하면 플랫폼 기능으로(지금은 앱마다 따로).
  5. 서버 빌드가 새 Rust crate(`pbkdf2`, `sha2`, `hex`, `unicode-normalization`, `caseless`)와 npm 패키지(Milkdown 등)를 받을 수 있어야 한다(`Cargo.lock`·`pnpm-lock.yaml`에 고정).
  6. 카탈로그(`registry/apps/catalog.json`)의 `board` 항목에 릴리스를 기록하는 일(소스 고정 뒤).
