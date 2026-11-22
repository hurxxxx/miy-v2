# 게시판 (`board`)

사내 커뮤니티 게시판과 공지를 다루는 miy 공식 업무 앱이다. AI-DO `0002af062`의 `community`·`announcements` 도메인과 화면을 옮기고, 공지 알림·댓글 알림과 금칙어를 더했다(보강). 원본과의 차이와 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 채널(읽기 전용·완전 익명·완전 비공개(건의함)·글 템플릿), 글·댓글·대댓글, 익명 글·댓글(익명 번호), 비밀글(비밀번호), 글 번호·검색·페이지, 읽음 표시, 본문 이미지(Markdown 편집기), 공간 공지와 전사 공지(상단 고정), 공지·댓글 알림, 금칙어.
- 권한 판단은 모두 서버가 한다. 호출자는 게이트웨이가 서명한 신원(`Identity::user_id`)이며 요청 본문의 사용자 값과 플랫폼 관리자 표시는 쓰지 않는다.
- AI-DO의 플랫폼 관리자 역할은 **게시판 관리자**(앱 역할)가 맡는다. 관리자가 없을 때는 앱을 쓸 수 있는 사람 한 명이 한 번 맡을 수 있다(화면 위 안내).
- 요청 권한: `database`, `platform:directory:read`, `platform:files:read`, `platform:files:write`, `platform:notifications:write`.
- 플랫폼 엔드포인트: `notificationCheck` → `/platform/notification-check`, `homeCard` → `/platform/home-card`.

## 화면 경로

앱 기준 경로(`runtime.location.pathname`)다.

| 경로 | 화면 |
| --- | --- |
| `/`, `/community?channel=<key>` | 채널 글 목록(기본 채널 `suggestions`). 왼쪽에 채널 목록 |
| `/community/posts/<id>?channel=<key>` | 글과 댓글 |
| `/notices?space=<id>` | 공간 공지와 전사 공지. 공간 선택·공간 만들기(게시판 관리자)·멤버 관리(공간 관리자) |
| `/notices/<id>?space=<id>` | 공지 하나를 연 공지 화면(알림 링크) |
| `/admin` | 게시판 관리(채널·금칙어·관리자). 게시판 관리자만 |

그 밖의 경로는 채널 글 목록을 보인다.

## API

UI는 `runtime.api('/v1/...')`로 부르고 게이트웨이가 `/api/v1/...`로 전달한다(AI-DO와 같은 경로). 시각은 naive UTC다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `community_post_not_found`). 요청 본문 키는 camelCase와 snake_case를 모두 받는다(AI-DO `populate_by_name`).

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/community/channels` | 활성 채널(순서 → 이름) |
| `GET·POST /v1/community/admin/channels` | 전체 채널, 채널 추가(201). 게시판 관리자 |
| `PATCH·DELETE /v1/community/admin/channels/{key}` | 채널 수정, 삭제(204; 기본 채널·글 있는 채널은 409) |
| `GET /v1/community/channels/{key}/posts?page&page_size&q` | 글 목록(최신순, 기본 10개·최대 100). `q`는 볼 수 있는 제목·작성자·번호(`37`, `#37`)에만 적용한다. 완전 비공개·완전 익명 채널의 검색은 게시판 관리자만 쓸 수 있다 |
| `POST /v1/community/channels/{key}/posts` | 글 작성(201). 다른 글이 번호를 잡고 있으면 즉시 409 `community_post_number_busy` |
| `GET·PATCH·DELETE /v1/community/posts/{id}` | 글(댓글 포함), 수정, 삭제(204) |
| `POST /v1/community/posts/{id}/read` | 읽음 표시(204) |
| `POST /v1/community/posts/{id}/unlock` | 비밀글 열기 `{password}` |
| `POST /v1/community/posts/{id}/comments` | 댓글·답글 작성(201) `{body, isAnonymous, parentCommentId?, password?}` |
| `PATCH·DELETE /v1/community/posts/{id}/comments/{commentId}` | 댓글 수정, 삭제(본문을 비우는 삭제, 204) |
| `GET·POST /v1/community/admin/filter-terms`, `DELETE …/{id}` | 금칙어 목록·추가·삭제. 게시판 관리자 |
| `POST /v1/media/upload` | 이미지 예약 `{requestId, name, mime, size}` → `{id, url:"media:<id>", status}`(PNG·JPEG·GIF·WebP, 10MiB). 응답이 불확실하면 같은 요청 ID를 사용한다 |
| `PUT·GET /files/media/{id}/content` | `runtime.fileApi`: 이미지 올리기(올린 사람), 내려받기 |
| `POST /files/community/posts/{postId}/media/{id}` | `runtime.fileApi`: 글·댓글 이미지 내려받기 `{password?}` |
| `GET·POST /v1/announcements?scope=company&limit&offset` | 전사 공지 목록·작성. 목록 응답의 `hasMore`로 다음 페이지를 판단한다 |
| `GET·PATCH·DELETE /v1/announcements/{id}` | 전사 공지 |
| `GET·POST /v1/spaces/{spaceId}/announcements?scope&limit&offset` | 공간 공지(`scope=workspace`, 기본) 또는 전사 공지 목록·작성 |
| `GET·PATCH·DELETE /v1/spaces/{spaceId}/announcements/{id}` | 공간 문맥의 공지 |
| `GET /v1/board/notices/recent?limit` | 읽을 수 있는 최근 공지(기본 5·최대 20). 홈 공지 카드(M5)용 읽기 전용 |
| `GET·POST /v1/spaces`, `PATCH·DELETE /v1/spaces/{id}` | 내 공간 목록, 공간 만들기(게시판 관리자), 수정·삭제(공간 관리자) |
| `GET·PUT /v1/spaces/{id}/sharing`, `GET …/sharing/requests/{requestId}` | 공간 멤버(SDK ACL 공유 계약) |
| `GET /v1/board/me` | `{userId, isManager, canBootstrap}` |
| `POST /v1/board/managers/bootstrap` | 관리자가 없을 때 첫 관리자 맡기(201, 이미 있으면 409) |
| `GET·POST /v1/board/managers`, `DELETE …/{userId}` | 게시판 관리자 목록·추가·해제(마지막 관리자는 409) |
| `POST /platform/notification-check` | 플랫폼 호출 전용(읽기 전용) |
| `POST /platform/home-card` | 플랫폼 호출 전용(읽기 전용): 공지 최대 5개, 고정 → 최신 |

## 알림

| 종류 | 언제 | 받는 사람 | 묶음 키 |
| --- | --- | --- | --- |
| `comment` | 내 글에 댓글이 달림 | 글쓴이(본인 댓글 제외, 완전 비공개 채널은 글쓴이가 게시판 관리자일 때만) | `post:<id>` |
| `notice` | 공지가 등록됨 | 전사: 디렉터리의 모든 사람, 공간: 공간에 직접 추가된 사람(작성자 제외) | `notice:<id>` |

글·공지를 지우면 묶음을 회수하고, 공간에서 빠진 사람에게서 그 공간 공지를 회수한다. 글쓴이가 글을 열거나 공지를 열면 읽음 처리한다.

## 데이터

`migrations/0002_acl.sql`(SDK `acl.sql` 그대로), `migrations/0003_board.sql`: `community_channels`, `community_posts`, `community_post_reads`, `community_comments`, `board_spaces`, `announcements`, `board_managers`, `ugc_filter_terms`, `media_files`. `0001_initial.sql`은 번들이 고정한 초기 파일이라 그대로 둔다.

## 검사

```sh
cargo test                                   # DB 없는 단위 테스트
BOARD_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
  cargo test -- --include-ignored            # 일회용 PostgreSQL과 가짜 코어로 HTTP·DB 테스트까지
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
```

`BOARD_TEST_DATABASE_URL`에는 버려도 되는 PostgreSQL만 준다. 테스트가 그 서버에 새 데이터베이스와 `app` 스키마를 만들고 migration을 적용한다. 디렉터리·알림·파일 저장소는 테스트 안의 가짜 코어가 받는다.
