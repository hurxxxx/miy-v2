# 메신저 (`messenger`)

1:1·그룹 대화를 다루는 miy 기본 제공 앱이다. AI-DO `0002af062`의 `dm` 도메인(서버)과 DM 화면(`app-modules/dm`), 관리자 DM 신고 검토 화면을 옮겼다. 원본과의 차이와 결정은 [docs/PORTING.md](docs/PORTING.md)에 있다.

- 범위: 1:1·그룹 대화, 첨부(파일당 10 MiB, 플랫폼 Storage), 인용 답장, 읽음 표시, 사용자 검색(플랫폼 디렉터리), 신고·1:1 차단·금칙어·이용규칙 동의, 신고 검토 화면(앱 관리 역할).
- 보강(AI-DO에 없던 것): 작성자 메시지 수정("수정됨")·삭제("삭제된 메시지"), 이전 메시지 이어 불러오기, 대화별 알림 끄기.
- 알림: 새 메시지를 플랫폼 알림함에 발행한다(본문 없이 "보낸 사람 · 새 메시지 N개"). 읽으면 읽음 처리, 수정·삭제·가림 때 교체·회수, 나가기·내보내기·알림 끄기 때 회수한다.
- 화면 갱신은 주기 조회(보이는 동안 5초)다. 즉시 갱신·입력 중 표시는 M4 뒤.
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

| 메서드·경로 | 설명 |
| --- | --- |
| `GET /v1/dm/me` | (miy) 호출자와 관리자 여부 |
| `GET /v1/dm/users?q&limit&include_current` | 사용자 검색(플랫폼 디렉터리, 기본 30, 최대 100) |
| `GET /v1/dm/conversations` | 내 대화 목록 + `safety`(신고·차단 지원, 이용규칙 상태, `manager`) |
| `POST /v1/dm/conversations` | `recipient_user_id`(1:1, 있으면 그 대화) 또는 `participant_user_ids`(그룹, 본인 외 2명 이상)+`title`. 201 |
| `PATCH /v1/dm/conversations/{id}` | 그룹 이름(소유자·관리자) |
| `POST /v1/dm/conversations/{id}/participants` | 그룹 초대(참여자 누구나) |
| `DELETE /v1/dm/conversations/{id}/participants/me` | 그룹 나가기(204) |
| `DELETE /v1/dm/conversations/{id}/participants/{user_id}` | 내보내기(소유자·관리자) |
| `GET /v1/dm/conversations/{id}/messages?limit&before` | 최근 메시지(기본 50), `before`보다 이전 |
| `POST /v1/dm/conversations/{id}/messages` | 보내기 `{body, attachment_ids, reply_to_message_id}` |
| `PATCH /v1/dm/conversations/{id}/messages/{mid}` | (miy) 작성자 수정 `{body}` |
| `DELETE /v1/dm/conversations/{id}/messages/{mid}` | (miy) 작성자 삭제 |
| `PATCH /v1/dm/conversations/{id}/read` | 읽음 |
| `PUT /v1/dm/conversations/{id}/mute` | (miy) 알림 끄기 `{muted}` |
| `POST /v1/dm/conversations/{id}/attachments` | 첨부 예약 `{request_id, filename, content_type, size_bytes}`(201) |
| `PUT /files/dm/attachments/{id}/content` | 첨부 바이트(파일 게이트웨이, 예약한 사람만) |
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
| `POST /platform/notification-check` | 플랫폼 → 앱 재확인(manifest `platformEndpoints.notificationCheck`, 읽기 전용) |
| `POST /platform/home-card` | 포털 홈 "안 읽은 대화" 카드(manifest `platformEndpoints.homeCard`, 읽기 전용) |

## 데이터

`migrations/0002_messenger.sql`: AI-DO의 `dm_*`, `ugc_terms_acceptances`, `ugc_filter_terms` 테이블과 miy 추가 테이블(`dm_terms_settings`, `dm_safety_managers`, `dm_audit_logs`, `dm_message_revisions`, `dm_user_directory_cache`). 사용자 참조는 miy 사용자 ID(`text`, FK 없음)다.

## 검사

```sh
cargo test                                   # 단위 테스트
MESSENGER_TEST_DATABASE_URL=postgres://… cargo test -- --include-ignored   # 일회용 PostgreSQL로 HTTP 테스트
pnpm install --frozen-lockfile && pnpm exec tsc --noEmit && pnpm test && pnpm build
.miy-tools/miy app check
python3 scripts/gen-i18n-resources.py <AI-DO resources.ts> web/src/platform/i18n/resources.ts
```
