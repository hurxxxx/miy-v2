# 메신저 (`messenger`)

1:1·그룹 대화를 다루는 miy 기본 제공 앱이다. AI-DO `0002af062`의 `dm` 도메인(서버)과 DM 화면(`app-modules/dm`), 관리자 DM 신고 검토 화면을 옮겼다. 원본과의 차이와 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 1:1·그룹 대화, 첨부(파일당 10 MiB, 플랫폼 Storage), 인용 답장, 읽음 표시, 사용자 검색(플랫폼 디렉터리), 신고·1:1 차단·금칙어·이용규칙 동의, 신고 검토 화면(앱 관리 역할).
- 보강(AI-DO에 없던 것): 작성자 메시지 수정("수정됨")·삭제("삭제된 메시지"), 이전 메시지 이어 불러오기, 대화별 알림 끄기, 멘션(`@` 선택, `mention` 알림), 반응(정해진 7가지), 메시지 검색(대화 안·내 모든 대화, 메시지로 이동)과 포털 통합 검색.
- 알림: 새 메시지를 플랫폼 알림함에 발행한다(본문 없이 "보낸 사람 · 새 메시지 N개"). 읽으면 읽음 처리, 수정·삭제·가림 때 교체·회수, 나가기·내보내기·알림 끄기 때 회수한다.
- 즉시 갱신(M4): 플랫폼 실시간 연결(`runtime.live` → `GET /api/live`)로 "대화가 바뀜"·"입력 중" 신호만 받고, 내용은 `runtime.api`로 다시 받는다. 연결이 끊긴 동안(`retrying`)은 주기 조회(보이는 동안 5초)로 돌아가고, 연결 중에는 60초마다 안전 갱신한다. 대화 창 아래에 "○○ 님이 입력 중…"을 보인다.
- 참여자만 읽고 쓴다. 나중에 들어온 사람은 들어온 뒤의 메시지만 본다. 호출자는 게이트웨이가 서명한 신원(`Identity::user_id`)이며 요청 본문의 사용자 값은 신원으로 쓰지 않는다.
- 요청 권한: `database`, `platform:directory:read`, `platform:files:read`, `platform:files:write`, `platform:notifications:write`.

## 화면 경로

| 경로 | 화면 |
| --- | --- |
| `/`, `/conversations/{id}` | 대화 목록과 대화(넓은 폭에서 나란히). 알림은 `/conversations/{id}`로 연다 |
| `/widgets/conversations[/{id}]` | 같은 화면의 좁은 폭(목록 ↔ 대화). manifest `panels`의 `conversations`로 포털 보조 패널에 뜬다 |
| `/safety`, `/safety/reports/{id}` | 신고 검토·금칙어·이용규칙 설정·관리자 지정(메신저 관리자만). 관리자가 없으면 첫 사람이 맡는다 |

## API

UI는 `runtime.api('/v1/...')`, 파일 바이트는 `runtime.fileApi('/dm/...')`로 부른다. 게이트웨이가 각각 `/api/v1/...`, `/api/files/...`로 Rust 서버에 전달한다. 시각은 naive UTC 문자열이다. 오류는 `{"error":{"code","message"}}`이며 코드는 AI-DO 코드의 `.`을 `_`로 바꾼 것이다(예: `dm_thread_not_found`).

[OpenAPI 계약](docs/openapi.json)은 앱 라우터와 같은 Rust 등록에서 생성한다. 기존 `/v1/dm/threads` 목록·생성·메시지·읽음 별칭은 호환을 위해 남겨 둔다. 새 호출에는 `/v1/dm/conversations`를 사용한다. API를 변경하면 `pnpm api:generate`로 명세와 웹 타입을 갱신한다.

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/dm/me` | (miy) 호출자와 관리자 여부 |
| `GET /live` (WebSocket) | (miy, M4) 플랫폼이 `runtime.live`를 중계하는 실시간 연결. 서버 → `{"type":"conversation.changed","conversationId"}`, `{"type":"typing","conversationId","userId","active"}`, `{"type":"heartbeat"}`. 클라이언트 → `{"type":"open","conversationId"}`(보고 있는 대화), `{"type":"typing","conversationId","active"}`. 신호만 오가고 대화 내용은 없다 |
| `GET /v1/dm/users?q&limit&include_current` | 사용자 검색(플랫폼 디렉터리, 기본 30, 최대 100) |
| `GET /v1/dm/conversations` | 내 대화 목록 + `safety`(신고·차단, 이용규칙, `manager`, `manager_claimable`) |
| `POST /v1/dm/conversations` | `recipient_user_id`(1:1, 있으면 그 대화) 또는 `participant_user_ids`(그룹, 본인 외 2명 이상)+`title`. 201 |
| `PATCH /v1/dm/conversations/{id}` | 그룹 이름(소유자·관리자) |
| `POST /v1/dm/conversations/{id}/participants` | 그룹 초대(참여자 누구나) |
| `DELETE /v1/dm/conversations/{id}/participants/me` | 그룹 나가기(204) |
| `DELETE /v1/dm/conversations/{id}/participants/{user_id}` | 내보내기(소유자·관리자) |
| `GET /v1/dm/conversations/{id}/messages` | 최근 메시지(기본 50). `before_sequence`로 이전 페이지, `ids`로 쉼표로 나눈 기존 ID 최대 50개를 다시 읽는다. 날짜 커서 `before`도 지원한다. |
| `POST /v1/dm/conversations/{id}/messages` | 보내기 `{body, attachment_ids, reply_to_message_id}` |
| `PATCH /v1/dm/conversations/{id}/messages/{mid}` | (miy) 작성자 수정 `{body}` |
| `DELETE /v1/dm/conversations/{id}/messages/{mid}` | (miy) 작성자 삭제 |
| `PUT/DELETE /v1/dm/conversations/{id}/messages/{mid}/reactions/{reaction}` | (miy) 내 반응 켜기·끄기(`thumbs_up`, `heart`, `laugh`, `surprised`, `sad`, `pray`, `check`). 답은 메시지 |
| `GET /v1/dm/search?q&conversation_id&cursor&limit` | (miy) 메시지 본문 검색(1~200자, 기본 20·최대 50, `next_cursor`). 지금 참여 중인 대화의 볼 수 있는 메시지만, 지운·가린 메시지 제외 |
| `PATCH /v1/dm/conversations/{id}/read` | 읽음 |
| `PUT /v1/dm/conversations/{id}/mute` | (miy) 알림 끄기 `{muted}` |
| `POST /v1/dm/conversations/{id}/attachments` | 첨부 예약 `{request_id, filename, content_type, size_bytes}`(201) |
| `PUT /files/dm/attachments/{id}/content` | 첨부 바이트(파일 게이트웨이, 예약한 사람만). 응답 유실 시 같은 예약의 완료 상태를 복구한다. |
| `GET /files/dm/attachments/{id}/content` | 첨부 내려받기(참여자, 들어온 뒤의 메시지, 가림·삭제 아님) |
| `GET /v1/dm/attachments/{id}/download`, `/preview` | 파일 경로 `{url}`(미리보기는 이미지만, 아니면 415) |
| `DELETE /v1/dm/attachments/{id}` | (miy) 보내지 않은 첨부 취소 |
| `POST /v1/dm/reports` | 신고(`client_report_id` 멱등, 시간당 10·하루 30) |
| `GET /v1/dm/blocks`, `PUT/DELETE /v1/dm/blocks/{user_id}` | 1:1 차단 목록·차단·해제 |
| `GET /v1/dm/terms`, `POST /v1/dm/terms/accept` | 이용규칙 상태·동의 |
| `GET /v1/dm-safety/me` | (miy) 관리자 여부·맡을 수 있는지 |
| `POST /v1/dm-safety/managers/claim` | (miy) 관리자가 없을 때만 첫 관리자 맡기 |
| `GET/POST /v1/dm-safety/managers`, `DELETE /v1/dm-safety/managers/{user_id}` | (miy) 관리자 목록·지정·해제(마지막 한 명은 남는다) |
| `GET/PUT /v1/dm-safety/terms-settings` | (miy) 이용규칙 판·필수 여부·문서·문의 |
| `GET /v1/dm-safety/reports?status&before&limit` | 신고 목록(분류·상태만) |
| `GET /v1/dm-safety/reports/{id}`, `/evidence` | 신고 상세, 신고한 메시지(수정·삭제 이력 포함) |
| `GET /files/dm-safety/reports/{id}/evidence/attachments/{aid}/content` | 증거 첨부 |
| `POST /v1/dm-safety/reports/{id}/resolve` | 처리(`expected_status: open`) |
| `POST /v1/dm-safety/reports/{id}/message-visibility` | 가림·되돌림 |
| `GET/POST /v1/dm-safety/filter-terms`, `DELETE .../{id}` | 금칙어 |
| `POST /platform/notification-check` | 플랫폼 → 앱 재확인(manifest `platformEndpoints.notificationCheck`, 읽기 전용). 그룹·원본 ID와 현재 접근을 함께 확인한다. |
| `POST /platform/home-card` | 포털 홈 "안 읽은 대화" 카드(manifest `platformEndpoints.homeCard`, 읽기 전용) |
| `POST /platform/search` | 포털 통합 검색(manifest `platformEndpoints.search`, 읽기 전용, 최대 `limit`(1~10)개, 메시지로 가는 경로) |

### 실시간 프로토콜

프레임의 `runtime.live` 요청을 게이트웨이가 `GET /api/live`로 중계한다. 서명된 앱 호출의 WebSocket 핸드셰이크는 `101 Switching Protocols`를 반환한다. 각 프레임은 JSON 텍스트다. OpenAPI `GET /live`의 `x-miy-websocket` 확장에 양방향 메시지 타입이 있다.

| 방향 | 메시지 | 동작 |
| --- | --- | --- |
| 클라이언트 → 서버 | `{"type":"open","conversationId":"<ID>"}` | 그 대화의 현재 참여자라면 이 연결에 입력 중 신호를 구독한다. ID를 생략하거나 `null`로 보내면 구독을 비운다. |
| 클라이언트 → 서버 | `{"type":"typing","conversationId":"<ID>","active":true}` | 입력 중 상태를 알린다. `active`의 기본값은 `true`다. 시작 신호는 연결·대화별 초당 1회로 제한되고 종료는 즉시 보낸다. |
| 서버 → 클라이언트 | `{"type":"conversation.changed","conversationId":"<ID>"}` | 변경이 커밋된 대화를 다시 읽는다. 지금 참여 중인 사람과 방금 나간 사람에게 필요한 갱신 신호를 보낸다. |
| 서버 → 클라이언트 | `{"type":"typing","conversationId":"<ID>","userId":"<ID>","active":true}` | 그 대화를 연 다른 현재 참여자에게 입력 중 상태를 전달한다. 서버가 보낼 때마다 참여를 확인한다. |
| 서버 → 클라이언트 | `{"type":"heartbeat"}` | 120초마다 연결을 유지한다. |

서버가 받는 메시지는 최대 4 KiB다. 이 연결은 갱신 신호를 보내고, 모든 내용 조회와 변경은 `runtime.api`를 사용한다. 연결이 복구되면 프레임이 현재 대화와 목록을 다시 읽는다.

## 데이터

`migrations/0002_messenger.sql`: AI-DO의 `dm_*`, `ugc_terms_acceptances`, `ugc_filter_terms` 테이블과 miy 추가 테이블(`dm_terms_settings`, `dm_safety_managers`, `dm_audit_logs`, `dm_message_revisions`, `dm_user_directory_cache`). `migrations/0003_messenger_mentions_reactions.sql`: `dm_message_mentions`, `dm_message_reactions`. 사용자 참조는 miy 사용자 ID(`text`, FK 없음)다.

## 검사

```sh
cargo test                                   # 단위 테스트
MESSENGER_TEST_DATABASE_URL=postgres://… cargo test -- --include-ignored   # 일회용 PostgreSQL로 HTTP 테스트
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
python3 scripts/gen-i18n-resources.py <AI-DO resources.ts> web/src/platform/i18n/resources.ts
```
