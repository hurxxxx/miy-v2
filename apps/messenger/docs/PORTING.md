# AI-DO DM → 메신저 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 기본 제공 앱 `messenger`(이름 `메신저`), 번들 0.12.0, Rust SDK 0.4.0(알림 포함), 공용 UI 2.0.0
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M2 플랫폼–앱 연결과 알림", "M3 메신저", §2.2 메신저 행과 "메신저의 메시지 수정·삭제 기준"
- 원칙: AI-DO 구현(화면·상호작용·REST 경로와 모양·테이블·규칙·문구·라이브러리)을 그대로 옮기고, miy 경계(신원·사용자 디렉터리·Storage·알림·앱 프레임·앱별 DB) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳과 보강은 아래에 모두 적었다. 프로젝트는 플래너와 같이 CLI(`miy app init apps/messenger --id messenger --name 메신저`, 오프라인)로 만들었다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 1:1 대화(있으면 그 대화, 빠진 참여자 되살리기), 그룹 대화(이름, 초대, 내보내기, 나가기, 소유자 승계, 1:1에서 새 그룹 만들기), 메시지(본문 120,000자, 첨부 10개, 인용 답장, 링크 미리보기, 날짜 구분선, 안 읽은 사람 수, "여기까지 읽음"), 읽음, 사용자 검색, 첨부(이미지 미리보기·확대·내려받기, 붙여넣기·끌어놓기), 대화 이동 기록(이전/다음 대화), 대화별 초안·스크롤 위치, 신고(분류·메모·차단 함께, 멱등, 시간당 10·하루 30, 중복 신고), 1:1 차단(목록·해제, 그룹에서 차단한 사람 메시지 접기), 관리자 가림, 금칙어, 이용규칙 동의, 관리자 신고 검토 화면(목록·상세·증거·처리·가림/되돌림·금칙어).

**보강(AI-DO에 없던 것, §2.2):** 작성자 메시지 수정·삭제(§3.10), 이전 메시지 이어 불러오기(§3.11), 대화별 알림 끄기(§3.11). 멘션·반응·메시지 검색은 하지 않았다(§6).

**뺀 것:**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 근태 앱 DM 중계 | `domains/commute/dm_relay_service.py`, `DmMessageClientKey`(`dm_message_client_keys`), `source='commute_app'` 경로 | 계획에서 제외. 테이블의 `source` 제약은 원본 그대로 두었지만 값은 `web`만 쓴다 |
| 실시간 이벤트(대화 스냅숏을 실은 웹소켓) | `realtime_events.py`, `realtime_payloads.py`, `realtime_event_types.py`, 화면의 `useRealtimeEvent` 처리기 | 옮기지 않음. M4에서 플랫폼 실시간 연결의 짧은 신호와 API 재조회로 바꿨다(§3.5). 원본 처리기 코드는 남아 있지만 그 이벤트는 오지 않는다 |
| 모바일 푸시, 봇 알림 DM, 업무 승인 DM | `notifications/mobile_push_*`, `notifications.dm_delivery`, `legacy_issues` | 계획에서 제외. 메시지 알림은 플랫폼 알림함으로(§3.4) |
| AI 기능, 데스크톱 앱 연동 | — | 계획에서 제외 |
| 떠 있는 DM 패널·셸 배지 | `FloatingDmWidget.tsx`(+spec), `useFloatingDmUnreadCount`, `personal-widget-registry` | 포털 전역 위젯은 M5. 대신 manifest `panels`의 좁은 화면(§3.7)과 홈 카드 |
| 사용자 전환 세션 차단 | `session_guard.py`(`dm.impersonation_not_allowed`) | M6 전환 세션은 플랫폼이 발급·회수한다. 메신저는 게이트웨이가 확인한 대상 사용자의 현재 앱 참여 권한을 적용한다. [코어 권한 계약](../../../docs/CORE-PERMISSIONS.md) |
| 워크스페이스 범위 사용자 검색 | `GET /dm/users?workspace_key` | 메신저는 환경 전체 범위다(워크스페이스 없음). 매개변수는 무시한다 |
| 서명 첨부 주소 | `attachment_links.py`, `attachment_content.py`, `GET /dm/attachments/{id}/content?expires&signature` | 프레임은 쿠키·서명 URL 없이 `runtime.fileApi`로 읽는다(§3.3) |
| 50 MB 첨부 | `DM_MAX_ATTACHMENT_SIZE` | 플랫폼 Storage 한도 10 MiB. 큰 첨부는 M10 뒤 |
| AI-DO 셸 연동 | `dm/index.ts`, `manifest.ts` | miy는 manifest·포털이 담당 |

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>` → `web/src/<경로>`, 계약 패키지 `packages/contracts/src/dm*.ts` → `web/src/contracts/`에 같은 이름으로 두었다. "그대로"는 import 경로 외 변경이 없다는 뜻이다(`@/src/…`·`@ai-do/contracts/dm`은 상대 경로, `@ai-do/ui`는 `@miy/components/ui/lib/…` — 보호 파일인 미리보기 빌드 설정이 `@miy/*` 외 별칭을 모른다).

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/dm/views/DMView.tsx` | 같은 경로 | 수정: 실시간 신호 재조회·입력 중 표시(M4), 주기 조회 페이지 병합, 첨부 객체 URL, 수정·삭제·이전 메시지·알림 끄기, 관리자 화면 링크, 이동 확인 보고, 오류 코드 `_` |
| `…/views/DmSafetyDialogs.tsx`, `dm-composer-attachments.ts`, `dm-composer-attachment-upload.ts`, `dm-conversation-session-model.ts`, `dm-group-thread-model.ts`, `dm-message-content.ts`, `dm-read-receipts.ts`, `dm-time.ts`, `useDmUserSearch.ts` | 같은 경로 | 그대로 |
| `…/views/dm-conversation-runtime-model.ts` | 같은 경로 | 원본 + `mergeDmMessagePages`·`prependOlderDmMessages` |
| `…/views/dm-thread-list-model.ts` | 같은 경로 | 원본 + 삭제된 메시지 미리보기 문구 |
| `…/views/dm-attachment-url.ts`, `dm-attachment-action-workflow.ts` | 같은 경로 | 수정: 앱 파일 경로, 객체 URL로 열고 해제 |
| (없음) | `…/views/DmAttachmentImage.tsx` | 이미지 첨부를 `runtime.fileApi`로 읽어 보여 줌 |
| `app-modules/dm/api/dm-api.ts` | 같은 경로 | 수정: 첨부 예약+바이트 전송, `before`, 수정·삭제·알림 끄기·첨부 취소·`/me` |
| `packages/contracts/src/dm.ts`, `dm-routes.ts` | `contracts/` | 수정: 경로 접두 `/v1/dm`, 경로 ID 256자, 10 MiB, 첨부 주소 검증, `edited_at`·`deleted` 정규화 |
| `packages/contracts/src/api.ts`(OpenAPI 생성 타입) | `contracts/api.ts` | 재작성: DM 스키마를 손으로 적은 타입(생성기 없음) |
| `platform/admin/admin-dm-safety-section.tsx`, `admin-dm-safety-api.ts` | 같은 경로 | 수정: `/v1/dm-safety`, 증거 첨부는 `runtime.fileApi`, 알림 경로로 신고 바로 열기, 수정·삭제 이력 표시 |
| (없음) | `platform/admin/DmSafetyPage.tsx` | 신고 검토 화면 + 관리자 지정 + 이용규칙 설정 + 첫 관리자 맡기 |
| `platform/users/*`, `platform/format/byte-size.ts`, `platform/browser/browser-download.ts`, `components/date/UserDateTime.tsx` | 같은 경로 | 그대로 |
| `platform/time/*`, `platform/i18n/{i18n,index,locales}.ts`, `platform/runtime/*`, `platform/api/api-error.ts` | 같은 경로 | 플래너 포팅본 재사용(저장 실패 시 메모리 보관, `runtime.settings.locale`, 이동 확인) |
| `platform/api/client.ts` | 같은 경로 | 재작성: `token`은 표시용 표지이고 요청은 묶인 `runtime.api`/`fileApi`로(§3.6) |
| `platform/auth/auth-provider` | 같은 경로 | 재작성: `GET /v1/dm/me`의 호출자 + `runtime.settings` |
| `platform/realtime/realtime-provider` | 같은 경로 | 재작성: 플랫폼 실시간 연결 + 끊긴 동안 주기 조회(§3.5) |
| (없음) | `app-modules/dm/views/dm-live-model.ts` | 실시간 신호 해석, 입력 중 상태 |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: `common`·`apps.dm`·`apps.admin.console.dmSafety`·`apps.pms.taskDetail.me` 한/영 원문 + miy 키 + `errors`([scripts/gen-i18n-resources.py](../scripts/gen-i18n-resources.py)) |
| (없음) | `AppRoot.tsx`, `platform/runtime/app-location.ts` | miy 연결(경로·공급자) |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/…` | miy `src/…` |
| --- | --- |
| `dm/router.py`, `conversation_application.py`, `conversation_membership.py`, `conversation_lifecycle.py`, `conversation_settings.py`, `conversation_read_receipts.py`, `participants.py`, `user_directory.py` | `conversations.rs` |
| `dm/message_application.py`, `message_flow.py`, `message_delivery.py`, `message_history.py` | `messages.rs`(+ 수정·삭제) |
| `dm/realtime_events.py`(발행 시점만) | `live.rs`(M4 신호 허브와 `GET /live`) |
| `dm/attachment_*.py` | `attachments.rs` |
| `dm/serialization.py`, `thread_projection.py`, `conversation_display.py`, `read_state.py`(투영) | `views.rs` |
| `dm/conversation_queries.py`, `models.py` | `store.rs`, `migrations/0002_messenger.sql` |
| `dm/safety.py`, `safety_application.py` | `safety.rs`, `safety_admin.rs`(사용자 경로) |
| `dm/safety_admin.py`, `admin/dm_safety_router.py` | `safety_admin.rs` |
| `ugc_policy/terms.py`, `content_filter.py`, `guard.py`, `models.py`, `schemas.py` | `ugc.rs`, `schemas.rs` |
| `dm/request_normalization.py`, `schemas.py` | `request.rs`, `schemas.rs` |
| `auth.access.record_audit_log` | `audit.rs`(앱 DB) |
| `core/i18n.localized_http_exception` | `error.rs` |
| (없음) | `notify.rs`(플랫폼 알림·재확인·홈 카드), `users.rs`(디렉터리) |

## 3. 원본과 달라진 점과 이유

### 3.1 데이터 모델

테이블 이름·컬럼·제약·인덱스 이름은 AI-DO 모델 그대로다(`migrations/0002_messenger.sql`, 0001은 번들 보호 파일이라 그대로 둠).

| 테이블 | 달라진 점 |
| --- | --- |
| 모든 `*_user_id`·`*_by_id`·`sender_id`·`uploader_id`·`created_by_id` | miy 사용자 ID `text`, FK 없음(사용자는 코어 DB) |
| `dm_conversations.direct_key` | `varchar(96)` → `text`(사용자 ID가 UUID보다 길 수 있음) |
| `dm_messages` | `edited_at`, `deleted_at` 추가(§3.10) |
| `dm_message_attachments` | `storage_key` 대신 `storage_request_id`·`storage_file_id`·`storage_status`(reserved/uploaded/attached/released/cancelled)·`attach_request_id`·`released_at`(§3.3) |
| `dm_message_client_keys` | 없음(근태 앱 중계 전용) |
| (추가) `dm_terms_settings` | 이용규칙 설정 한 행(AI-DO 기본값: `2026-11-ugc-v1`, 필수 아님) |
| (추가) `dm_safety_managers` | 앱 관리 역할(§3.8) |
| (추가) `dm_audit_logs` | AI-DO가 플랫폼 감사 로그에 남기던 행위(본문·메모·파일 이름 없음) |
| (추가) `dm_message_revisions` | 수정·삭제 이력 |
| (추가) `dm_user_directory_cache` | 디렉터리 이름 캐시(§3.2) |

시각은 AI-DO처럼 naive UTC(`timestamp`)이고 응답도 Pydantic의 naive ISO 문자열이다. ID는 UUID v4 문자열이다.

### 3.2 신원과 사용자

- 호출자는 게이트웨이가 서명한 `Identity::user_id`다. 화면에는 세션 사용자가 없어서 `GET /v1/dm/me`를 더했다(이름·관리자 여부, 표시 전용).
- AI-DO의 `users` 조인 대신 플랫폼 디렉터리(`platform:directory:read`)를 쓴다. `DmUserItem`은 `full_name`=디렉터리 이름, `primary_org_unit_name`=주 소속, `email`=`""`, `display_name`·`job_title`=`null`. 화면의 이메일 자리(대화 머리글·참여자)는 비어 있으면 주 소속을 보인다.
- **활성 사용자가 필요한 쓰기**(새 1:1, 그룹 생성·초대, 차단, 관리자 지정)는 매번 디렉터리에 묻고, 실패하면 502 `dm_directory_unavailable`로 거절한다(fail closed). 디렉터리가 나열하지 않는 사람(비활성·없음)은 AI-DO의 `status != active`와 같이 다룬다.
- **표시 이름**은 `dm_user_directory_cache`에서 읽고 5분이 지난 것만 디렉터리에서 갱신한다. 디렉터리에서 빠진 사람도 마지막 이름이 남는다(AI-DO는 사용자 행이 남았다). 캐시는 표시 전용이며 권한 판단에 쓰지 않는다. 플랫폼 → 앱 호출에는 추가 호출 토큰이 없어 캐시만 쓴다.
- 사용자 검색은 디렉터리 검색(이름·주 소속, 최대 100)이다. AI-DO는 이메일·직함도 검색했다.
- 사용자 ID는 플랫폼 subject라 경로 ID 길이 상한을 36 → 256자로 올렸다(패턴 `[A-Za-z0-9_-]`는 그대로, 서버·화면 같음).
- 범위: 워크스페이스 없음. 같은 환경(dev/prod) 설치 안의 사용자 전체다.

### 3.3 첨부(플랫폼 Storage)

AI-DO는 multipart로 받아 자체 저장소에 쓰고 서명 URL을 돌려줬다. miy는 Storage API(`Identity::files_*`, 파일당 10 MiB)로 옮겼다.

1. `POST /v1/dm/conversations/{id}/attachments`(JSON `{request_id, filename, content_type, size_bytes}`): 참여자·1:1 차단·이용규칙·파일 이름 금칙어·크기(0이면 422 `dm_attachment_empty`, 10 MiB 초과 413)를 확인하고 **예약 의도(행)를 먼저 저장한 뒤** `files_create`. 결과가 불확실하면 같은 요청 ID로 `files_by_request`를 조회한다(재전송 없음).
2. 바이트는 `runtime.fileApi('/dm/attachments/{id}/content', {method:'PUT'})` → `PUT /files/dm/attachments/{id}/content`(업로드한 사람, 아직 참여자, 1:1 열림, `Content-Length` 일치) → `files_upload`. 처음 512바이트로 AI-DO와 같이 형식을 판별한다(선언만 이미지면 `application/octet-stream`).
3. 보내기는 메시지 연결을 커밋한 뒤 `files_attach(file, "dm-attachment:{id}", attach_request_id)`. 실패하면 파일은 ready로 남고(보존), 다음 내려받기에서 같은 요청 ID로 다시 붙인다.
4. 내려받기 `GET /files/dm/attachments/{id}/content`는 매번 ACL을 확인한다. **AI-DO보다 엄격하게** 보내지 않은 첨부는 업로드한 사람만, 보낸 첨부는 그 메시지를 볼 수 있는 참여자만(들어온 뒤, 가림·삭제 아님). 응답의 형식·파일 이름은 앱 행 기준이다. 화면은 객체 URL로 보여 주고 해제한다(`DmAttachmentImage`, 확대 뷰어, 내려받기).
5. 작성창에서 뺀 첨부는 `DELETE /v1/dm/attachments/{id}`로 취소한다(miy, `files_cancel`). AI-DO는 참조 없는 파일로 남겼다.
6. 메시지 삭제는 커밋 뒤 `files_detach`(붙은 파일)·`files_cancel`(붙지 못한 파일)로 해제한다. 해제되지 않은 행(삭제된 메시지 + 상태 uploaded/attached + 열린 신고 없음)이 남은 작업의 기록이며, 이후 삭제·신고 처리 때 최대 10건씩 다시 시도한다. 백그라운드 작업자는 서비스 주체가 생긴 뒤로 미룬다.
7. 동시 전송은 화면에서 3개로 제한한다(파일 게이트웨이 단계당 4개).

### 3.4 플랫폼 알림(M2)

AI-DO 웹 DM은 알림 행을 만들지 않았고(실시간 이벤트와 모바일 푸시뿐) 메신저는 플랫폼 알림함으로 보낸다. 모든 호출은 요청을 처리하는 동안, 업무 커밋 뒤에 하며 5초 상한이다. 실패는 로그만 남기고 업무 저장을 되돌리지 않으며 다시 보내지 않는다.

| 시점 | 호출 |
| --- | --- |
| 보내기 | `Identity::notify(Notification { request_id: 새 UUID, recipients, kind: "message", group_key: 대화 ID, title: "<보낸 사람> · 새 메시지 N개", path: "/conversations/{id}", resource_id: 가장 최근 안 읽은 메시지 ID })`. 활성 참여자별 알림 설정과 차단을 확인한다. N은 삭제·가림·보낸 사람 차단을 적용한 뒤 알림에 포함할 미확인 메시지 수다. 같은 원본과 건수의 수신자를 묶어 호출한다 |
| 수정 | 그 메시지를 아직 안 읽은 사람에게 같은 방식으로 다시 발행(교체) |
| 삭제, 관리자 가림·되돌림 | 아직 안 읽은 사람에게 남은 안 읽은 수로 다시 발행, 남은 것이 없으면 `retract_notifications(대화, Some(&[그 사람]))` |
| 읽음(`PATCH …/read`) | 읽음 위치가 실제로 움직였을 때만 `mark_notifications_read(대화)`(화면은 보이는 동안 매 조회마다 읽음을 보낸다) |
| 나가기, 내보내기 | `retract_notifications(대화, Some(&[그 사람]))` |
| 알림 끄기 | 켜 둔 상태에서 끌 때 본인 줄을 회수 |
| 신고 접수 | 신고 당사자를 뺀 메신저 관리자에게 `kind: "report"`, `group_key: "report:{id}"`, 제목 "<분류> 신고가 접수됐습니다."(AI-DO 관리자 알림 문구, 본문·메모·이름 없음), `path: "/safety/reports/{id}"`. 처리하면 그 줄을 모두 회수 |

- **제목 언어:** 플랫폼은 제목을 하나만 저장하므로 AI-DO 기본 언어인 한국어로 만든다(영어 화면 사용자도 한국어 제목). 보낸 사람 이름은 200자에 맞춰 줄인다.
- **재확인**(manifest `platformEndpoints.notificationCheck` = `/platform/notification-check`): 플랫폼 호출만 받는 읽기 전용 경로다. 항목 500개까지, 그룹과 원본 ID가 일치하고 현재 접근 가능한 대화나 신고를 돌려준다. 보낸 사람 차단, 대화 알림 끄기, 메시지 삭제·가림도 적용한다.
- **홈 카드**(`platformEndpoints.homeCard` = `/platform/home-card`): 플랫폼 호출만, 읽기 전용. `{"title":"안 읽은 대화","count":안 읽은 대화 수,"items":[최대 5개, 최근 순 {title: 대화 이름, subtitle: "<보낸 사람> · 새 메시지 N개", path, at: 마지막 안 읽은 메시지 시각(RFC 3339 UTC)}]}`. 본문은 넣지 않는다.

### 3.5 화면 갱신: 즉시 갱신(M4)과 주기 조회

AI-DO는 자체 웹소켓으로 대화 스냅숏을 실은 이벤트(`dm.message.created` 등)를 보냈다. 메신저는 M4에서 플랫폼 실시간 연결(템플릿 CONTRACT "Live connections")로 **신호만** 보내고, 읽기·쓰기는 모두 `runtime.api`에 둔다.

- **서버(`src/live.rs`):** `GET /live`(SDK가 `/api` 아래에 둠, axum `ws`)가 `Extension<Identity>`로 연결을 받는다. 플랫폼 재확인 신원(`context.platform`)은 403. 사용자 ID별 프로세스 안 허브에 연결을 등록하고, 연결당 대기 64개를 넘는 신호는 버린다(클라이언트가 다시 받는다). 플랫폼이 5분 동안 조용한 연결을 닫으므로 120초마다 `heartbeat`를 보낸다.
- **신호를 보내는 때:** 커밋된 쓰기 뒤 — 보내기, 수정, 삭제, 읽음(읽음 위치가 움직였을 때만), 대화 만들기·되살리기, 그룹 이름, 초대, 내보내기, 나가기, 관리자 가림·되돌림 — 에 `{"type":"conversation.changed","conversationId"}`를 그 대화의 **현재 활성 참여자**에게 보낸다. 참여자는 커밋 뒤 DB에서 다시 읽는다(전달 시점 확인). 예외로 이번 쓰기로 참여가 끝난 사람(내보내진 사람, 나간 사람)에게도 그 신호 하나를 보낸다. 대화 ID만 담고 있어 이미 아는 것 외에 알려 주는 것이 없고, 보내지 않으면 그 사람의 목록에 열 수 없는 대화가 다음 안전 갱신까지 남는다. 알림 끄기는 본인의 다른 화면에만 보낸다.
- **입력 중 표시:** 클라이언트가 `{"type":"open","conversationId"}`로 보고 있는 대화를 알리면 서버가 그때 참여 여부를 DB로 확인하고(구독 시점 확인), 참여자가 아니면 무시한다. `{"type":"typing","conversationId","active"}`는 보낼 때마다 보낸 사람과 받는 사람이 현재 활성 참여자인지 다시 확인하고, 그 대화를 열어 둔 다른 참여자에게만 `{"type":"typing","conversationId","userId","active"}`로 전달한다. 시작 신호는 대화·연결당 1초에 한 번, 끝 신호(`active:false`)는 항상 통과한다. 화면은 입력할 때 3초에 한 번 보내고, 초안을 비우거나 보내거나 다른 대화로 옮기면 끝 신호를 보낸다. 받은 쪽은 6초 동안 신호가 없으면 지운다. 문구는 `apps:dm.typingOne/Two/Many`. **AI-DO `0002af062`에는 DM 입력 중 표시가 없었다**(화면·서버·문구 모두 없음, 저장소 기록 검색). 그래서 원본을 옮긴 것이 아니라 계획 M4("입력 중 표시를 넣는다")에 따라 새로 만들었고, 원본 디자인 요소(작성창 위 보조 문구, `app-text-caption`, `text-app-ink/55`)를 따랐다.
- **화면(`platform/realtime/realtime-provider.tsx`):** 프레임마다 연결 하나(`runtime.live`, `AppRoot`가 처음 받은 것을 씀). 열려 있는 동안 신호를 `useRealtimeEvent` 구독자에게 나눠 주고, (재)연결될 때마다 `reconnectSeq`를 올려 놓친 변경을 다시 받는다. 큐가 넘쳐 버려진 신호나 앱 재시작에 대비해 연결 중에도 60초마다 안전 갱신한다. 런타임이 `retrying: true`로 끊김을 알리면 M3의 주기 조회(보이는 동안 5초, 다시 보일 때 즉시)로 돌아가고, `retrying: false`(세션 끝, 권한 회수, 릴리스 변경, 인증 거부)면 조회도 멈춘다(플랫폼이 프레임을 다시 띄우거나 막는다). `runtime.live`가 없는 호스트에서는 주기 조회만 한다.
- **신호 처리(`DMView`):** `conversation.changed`는 대화별로 120ms 안에 온 것을 하나로 묶어 목록을 다시 받고, 그 대화가 열려 있으면 메시지를 다시 받은 뒤 보이는 상태면 읽음 처리한다. 다시 받는 함수는 사용자 수정의 늦은 응답 확인(다른 대화로 옮겼으면 버림)과 초안 보존을 그대로 쓴다. 최신 페이지 밖의 이미 불러온 이전 메시지는 신호로 다시 받지 않고, (재)연결·안전 갱신·주기 조회 때 묶음으로 갱신한다(아래 문단).
- **한 프로세스 전제:** 허브는 앱 프로세스 안에만 있다. 여러 프로세스를 띄우지 않는다고 가정한다. 프로세스가 다시 시작되면 그 사이 신호는 사라지지만, 연결이 끊겨 플랫폼이 `4502`로 닫고 런타임이 다시 연결하면서 `reconnectSeq`가 올라 다시 받는다. 그 전까지는 주기 조회가 이어 준다.

주기 조회(연결이 없을 때)와 (재)연결·안전 갱신 때는 `reconnectSeq`가 올라 대화 목록과 열린 대화를 다시 받는다. 최신 페이지와 이미 불러온 이전 메시지 최대 50개를 그때마다 갱신한다. 이전 메시지가 많으면 다음 묶음으로 순환한다.

### 3.6 API 호출과 오류

- 화면 코드의 `token` 인자는 그대로 두고 표지 문자열로 쓴다. 실제 요청은 `AppRoot`가 묶은 `runtime.api`/`runtime.fileApi`로 간다(`platform/api/client.ts`). 덕분에 원본 모델·명세의 시그니처가 그대로다. 프레임의 자격증명은 앱 코드에 없다.
- 경로는 앱 기준 `/v1/dm/…`이며 게이트웨이를 거쳐 AI-DO와 같은 `/api/v1/dm/…`가 된다. 관리자 경로는 `/api/v1/admin/dm-safety/…` → `/v1/dm-safety/…`. AI-DO의 숨은 별칭 `/v1/dm/threads…`도 옮겼다.
- 오류는 `{"error":{"code","message"}}`, 코드는 AI-DO 코드의 `.` → `_`(예: `ugc_terms_acceptance_required`). 화면은 AI-DO 서버 카탈로그의 한/영 문장(`errors`)으로 번역한다. 매개변수는 상수로 넣는다(차단 500명, 첨부 10 MB, 금칙어 50자·1,000개). 요청 검증 실패는 422 `validation_request_invalid`.
- miy 코드: `dm_directory_unavailable`, `dm_attachment_not_uploaded`, `dm_attachment_already_uploaded`, `dm_attachment_size_mismatch`, `dm_attachment_request_conflict`, `dm_message_edit_forbidden`, `dm_message_not_editable`, `dm_safety_manager_required`, `dm_safety_manager_already_assigned`, `dm_safety_manager_not_found`, `dm_safety_last_manager`, `platform_call_required`.
- 화면의 `crypto.randomUUID()`(신고 ID·첨부 요청 ID)는 공용 UI `requestId()`로 바꿨다(HTTP 내부 주소의 비보안 문맥, `app check` 지적).

### 3.7 화면 경로·패널·표시 설정

- AI-DO DM은 셸의 떠 있는 패널(`FloatingDmWidget` → `DMEmbeddedView embeddedPersistentList`)로만 열렸다. miy 앱은 프레임 전체를 쓰므로 같은 `DMEmbeddedView`를 `/`·`/conversations/{id}`에 넣고(넓으면 목록+대화), 대화 선택은 `runtime.navigate`로 경로를 바꾼다. 알림은 `/conversations/{id}`로 연다.
- manifest `panels: [{id: "conversations", title: "메신저", path: "/widgets/conversations"}]`: 같은 화면의 좁은 폭(목록 ↔ 대화, `embeddedPersistentList=false`). 패널 안 이동은 `/widgets/conversations/{id}`.
- 언어·시간대·날짜 형식은 `runtime.settings`(AI-DO는 사용자 프로필). 프레임 테마는 플랫폼이 붙인 `.dark`를 AI-DO `app-*` 토큰이 따른다.
- 이동 확인(miy 계약): 대화별 새 메시지·전송 실패 초안과 별도의 수정 입력, 첨부·전송 진행 및 신고·관리 화면 입력을 `runtime.setNavigationState({dirty})`에 반영한다(문구 `apps:dm.unsavedChanges`). 전송 실패 응답은 새 초안을 보존하며, 앞 문구는 작성창이 비면 불러올 수 있다. 이전·다음 대화 이력은 포털이 새 경로를 돌려줄 때 반영한다.

### 3.8 신고 검토와 관리 역할

- AI-DO는 플랫폼 관리자(`platform_admin`)가 관리 콘솔에서 검토했다. 계획(§2.2)대로 검토 화면은 앱 안(`/safety`)에 두고 앱이 선언한 관리 역할(`dm_safety_managers`)을 가진 사람만 쓴다. 플랫폼 관리자 표시(`Identity::admin`)는 쓰지 않는다.
- **첫 관리자:** 역할이 비어 있을 때만 `POST /v1/dm-safety/managers/claim`으로 첫 사람이 맡는다(표 잠금, 감사 행 `dm_safety.manager_claimed`). 이후에는 관리자만 관리자를 지정·해제하고 마지막 한 명은 해제할 수 없다. 첫 관리자 지정이 가능하거나 현재 관리자이면 메신저의 "신고 검토" 링크에서 `/safety`를 연다. 설치한 운영 담당자가 dev·prod 설치 직후 각각 맡는 절차와 초기 지정 정책은 인수·결정 항목이다(§6).
- 규칙은 원본 그대로: 목록은 분류·상태만, 상세에 메모·신원, 증거는 신고한 메시지와 그 첨부뿐, 당사자는 검토 불가(403 `dm_safety_review_conflict`), 처리는 `expected_status: open`일 때만(409), 열람·처리·가림은 매번 감사 행(앱 DB).
- 증거에는 miy 수정·삭제 이력(`revisions`, `edited_at`, `deleted_at`)을 더했다. 신고가 열린 동안 삭제된 메시지는 보존한 본문 사본을 보인다.

### 3.9 이용규칙 설정

AI-DO의 `AI_DO_UGC_TERMS_VERSION`·`_REQUIRED`·`_DOCUMENT_URL`·`AI_DO_UGC_CONTACT_*` 서버 설정은 앱에 없어서 `dm_terms_settings` 한 행(기본값 동일)으로 두고 관리자가 `/safety`에서 바꾼다(`PUT /v1/dm-safety/terms-settings`, 감사 행). 동의 원장·막는 쓰기(보내기, 그룹 이름, 첨부 파일 이름)·409 판 불일치는 원본 그대로다.

### 3.10 메시지 수정·삭제(보강)

계획 "메신저의 메시지 수정·삭제 기준"을 따랐다.

- 작성자 본인만(403 `dm_message_edit_forbidden`), 아직 참여 중이고 들어온 뒤의 메시지만. 가린·삭제된 메시지는 수정할 수 없다(422 `dm_message_not_editable`). 수정에도 이용규칙·금칙어·1:1 차단 검사를 적용한다.
- 수정: 바뀐 본문을 저장하고 `edited_at`을 남기며 이전 본문은 `dm_message_revisions`(`edit`)에 둔다. 화면은 시각 앞에 "수정됨". 작성창이 수정 모드가 된다.
- 삭제: 대화에는 "삭제된 메시지" 자리표시만 남고(본문 `""`, `deleted: true`, 첨부 없음, 인용 미리보기도 비움), 커밋 뒤 첨부를 해제한다. 이력은 `delete` 행만 남기고 이전 수정 본문도 지운다.
- 신고가 열린 메시지를 삭제하면 본문 사본을 `delete` 이력에 보존하고 첨부 해제를 미룬다. 그 메시지의 열린 신고가 모두 처리되면 사본을 지우고 첨부를 해제한다. 삭제된 메시지는 새로 신고할 수 없다(404).
- 안 읽은 수와 홈 카드는 삭제된 메시지를 세지 않는다(가린 메시지는 AI-DO처럼 센다).
- 수정·삭제 감사 행(`dm.message_edited`, `dm.message_deleted`)에는 본문이 없다. 플랫폼 감사 로그에는 업무 본문을 넣지 않는다.

### 3.11 이전 메시지·알림 끄기(보강)

- 이전 메시지: 화면은 대화 내 순번 커서 `before_sequence`로 50개씩 조회하므로 시각이 같은 메시지도 이어서 받는다. "이전 메시지 더 보기" 뒤에는 읽던 위치를 유지한다. 주기 조회는 위 §3.5의 방식으로 이미 열린 이전 메시지도 갱신한다.
- 알림 끄기: AI-DO 참여 행의 `muted_at`(근태 앱 중계가 쓰던 값)을 `PUT /v1/dm/conversations/{id}/mute`로 바꾼다. 끈 대화는 플랫폼 알림을 받지 않는다. 대화 머리글의 종 단추.

### 3.12 그 밖의 서버 세부

- 참여자 변경과 메시지 보내기·수정·삭제, 읽음 위치 변경은 대화 행을 같은 순서로 잠근 뒤 현재 참여 권한을 확인한다. 보내기 순번은 `UPDATE … RETURNING`으로 받고, 읽음 위치는 앞으로만 움직인다. 새 신고와 삭제는 같은 메시지 잠금 뒤 상태를 확인한다.
- 1:1 동시 생성은 고유 키 위반 시 먼저 만든 대화를 돌려준다.
- 신고 요청 해시는 Python `json.dumps(sort_keys, ensure_ascii=False, separators)`와 같은 바이트(serde_json 정렬 맵)로 만든다. 같은 사람의 신고·차단은 advisory lock으로 한 줄로 세운다.
- 신고와 함께 하는 차단이 대상 때문에 실패(404·422, miy: 디렉터리 실패 502)하면 savepoint만 되돌리고 신고는 남긴다.
- 금칙어 정규화의 Python `casefold`는 유니코드 소문자 + `ß→ss`로 근사했다. `\s`에는 `\x1c-\x1f`도 포함했다.
- 사람만 신고한 경우 AI-DO는 대상 사용자 행의 존재를 먼저 봤다. miy는 그 대화의 (과거) 참여자인지로 확인한다(참여자는 반드시 존재했던 사람).

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 범위는 환경 전체(워크스페이스 없음) | 계획: 메신저는 플랫폼 기본 제공 앱 |
| 알림 제목은 한국어 하나 | 플랫폼이 제목 하나만 저장, AI-DO 기본 언어 |
| 알림 N은 받는 사람별 앱 안 읽은 수, 같은 값끼리 묶어 호출 | "새 메시지 N개" 정확성과 호출 수 |
| 수정은 교체, 삭제·가림은 교체 또는 회수 | 계획 M2 6단계(원본 수정·삭제 시 회수·교체) |
| 읽음 알림은 읽음 위치가 움직일 때만 | 주기 조회가 매번 읽음을 보내므로 |
| 즉시 갱신은 신호 + API 재조회, 끊기면 주기 조회 5초, 연결 중 안전 갱신 60초 | 계획 M4. 권한 판단과 내용은 API에 두고, 신호를 잃어도 결과가 맞게 |
| 신호 허브는 프로세스 안, 사용자별 | 앱 프로세스는 하나. 재시작으로 잃은 신호는 재연결 재조회와 주기 조회가 채움 |
| 참여가 끝난 사람에게도 그 변경 신호 하나 | 목록에서 열 수 없는 대화를 바로 지우기 위해(대화 ID 외 정보 없음) |
| 첫 관리자는 빈 역할을 맡는 방식 | 플랫폼 관리자 표시를 쓰지 않는 가장 단순한 시작. 운영 담당자가 설치 직후 맡는다 |
| 이용규칙 설정은 앱 DB 한 행 | 앱에 서버 설정이 없음 |
| 디렉터리 이름 캐시 5분 | 매 조회마다 디렉터리를 부르지 않기 위해. 쓰기의 활성 확인은 캐시를 쓰지 않음 |
| 경로 ID 256자 | 플랫폼 사용자 ID |
| 보내지 않은 첨부는 취소, 받은 파일은 앱 행 형식으로 내려줌 | Storage 할당량과 AI-DO의 형식 판별 유지 |
| 의존성은 AI-DO·플래너 버전 | i18next 26.0.8, react-i18next 17.0.6, lucide-react 0.546.0, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library(react 16.3.0, dom 10.4.0). React·Vite·Tailwind·Radix는 템플릿·공용 UI 버전 |
| DB 테스트는 `#[ignore]` + `MESSENGER_TEST_DATABASE_URL`, 플랫폼은 SDK 미리보기 전송의 대역 서버 | 서버 빌드의 `cargo test`에는 DB·코어가 없다. 디렉터리·Storage·알림 호출까지 실제 SDK 경로로 확인 |
| 공용 UI는 `registry/components/ui/2.0.0`을 `component add` 형태로 복사하고 lock 계산(레지스트리 `http://localhost:18000` 자리표시) | 오프라인 init 뒤 `component sync`에는 로그인 필요. 플래너와 같은 방식, 같은 digest |
| 번들 0.12.0 보호 파일 3개(`docs/CONTRACT.md`, `sdk/browser/index.ts`, `sdk/preview/main.tsx`)와 lock을 현재 CLI 내장본에 맞춤 | init 뒤 작업본의 템플릿이 같은 판 안에서 바뀌어(`runtime.live`, 패널·홈 카드 문서) `app check`가 `bundle_lock_modified`로 거부. `app upgrade`는 커밋되지 않은 소스와 같은 판 때문에 거부. 플래너 lock도 같은 방식으로 맞춰져 있다 |
| i18n 원문은 스크립트로 생성 | 손으로 옮기다 생기는 차이를 막음 |

## 5. 테스트 대응

| AI-DO | miy | 결과 |
| --- | --- | --- |
| `test_dm.py`: 전역 대화·메시지, 본문 최대 길이, 첨부 참여자 전용, 위장 이미지 미리보기 불가, 첨부 내려받기 disposition, 그룹 관리 수명주기, 요청 모양 검증, 사용자 검색 | `api_tests.rs` `direct_conversation_send_read_and_notifications`, `request_shapes_are_validated`, `group_membership_lifecycle`, `attachments_use_platform_storage_and_stay_private`, `notification_check_mute_search_and_older_messages` | 통과. 알림 행이 없음을 보던 단언은 플랫폼 알림 발행 단언으로 바꿈. 웹소켓·워크스페이스 검색 테스트는 기능 제외 |
| `test_dm_conversation_membership.py`: 되살리기, 그룹 생성·누락 수신자, 초대 시 읽음 위치, 내보내기·나가기 소유자 승계, 차단 쌍 1:1·되살리기·그룹 | `group_membership_lifecycle`, `report_with_block_and_one_to_one_blocks` | 통과(savepoint·commit 내부 동작 테스트는 구조가 달라 해당 없음) |
| `test_dm_message_flow.py`, `test_dm_message_delivery.py`: 본문 또는 첨부 필수, 첨부만, 같은 대화 답장, 다른 대화 답장 거부, 남의·이미 보낸 첨부 거부 | `request_shapes_are_validated`, `attachments_…`, `group_membership_lifecycle` | 통과 |
| `test_dm_read_state.py`, `test_dm_serialization.py`, `test_dm_conversation_display.py` | `views.rs` 단위 4개 + HTTP 단언 | 통과 |
| `test_dm_safety_db.py`: 신고+차단 멱등·양방향 1:1 차단, 잘못된 대상, 관리자 검토·가림, 당사자 검토 불가, 시간당 10건, 감사 페이로드에 본문·메모 없음 | `report_with_block_and_one_to_one_blocks`, `invalid_report_targets`, `report_rate_limit`, `manager_reviews_and_hides_the_reported_message` | 통과. 근태 앱 폴링 재전송 테스트 8개는 중계와 함께 제외 |
| `test_dm_safety.py`(금칙어·이용규칙·재시도 시간·해시) | `ugc.rs`·`safety.rs` 단위 + `terms_and_banned_words_guard_user_content` | 통과 |
| `test_dm_attachment_policy.py`, `test_dm_attachment_upload.py`, `test_dm_attachment_records.py` | `attachments.rs` 단위 3개 + HTTP | 통과. 서명·저장소 테스트(`attachment_links/content/storage/persistence`, `test_dm_settings.py`)는 서명 URL·자체 저장소와 함께 해당 없음 |
| `test_dm_realtime_*.py` | — | 스냅숏 이벤트는 옮기지 않음 |
| (miy M4) | `live.rs` 단위 5개(허브 전달·열린 대화 거르기·큐 넘침·클라이언트 메시지·입력 중 제한), `api_tests.rs` `live_hints_follow_current_participation`(실제 웹소켓: 커밋 뒤 신호, 외부인 미수신, 열기 전 입력 중 미전달, 외부인 구독·입력 거부, 내보낸 사람은 그 신호 하나 뒤 미수신, 두 번째 읽음은 조용, 플랫폼 신원 403) | 통과 |
| (miy 추가) | 수정·삭제·이력·신고 중 삭제 사본, 알림 발행·교체·회수·읽음, 알림 끄기, 재확인·홈 카드, 관리자 맡기·마지막 관리자, `/me`, 이전 페이지 | 통과 |
| 화면 `*.spec` 27개 파일(DM 모델·화면·API 14, 계약 2, 사용자 검색 2, 바이트·내려받기·날짜 표시 3, 시간 5(플래너 포팅본), 관리자 신고 화면 1) | 같은 경로 | 통과. 경로 접두·첨부 주소·10 MiB·오류 코드 `_` 기대값만 바꿈. `DMView.spec`은 첨부 객체 URL을 흉내 냄 |
| (miy 추가) | `app-location.spec.ts`, `api-error.spec.ts`, 페이지 병합, miy API(첨부 예약·전송, 수정·삭제·끄기·취소) | 통과 |
| (miy M4) | `realtime-provider.spec.tsx`(연결 하나, 신호 전달, 재연결 재조회, 끊김 시 조회, 최종 종료 시 멈춤, 연결 없는 호스트), `dm-live-model.spec.ts`, `DMView.spec.tsx` 2개(신호로 목록·열린 대화 재조회, 입력 중 표시) | 통과 |

옮기지 않은 화면 테스트: `FloatingDmWidget.spec.tsx`(기능 제외).

## 6. 남은 일

- **보강 중 미구현(후속):** 멘션(작성창 자동완성 + `mention` 알림 종류), 반응(원본의 빈 리액션 단추는 그대로 둠), 메시지 검색. 계획 §2.2 "보강" 항목이다.
- **실시간(M4) 실제 플랫폼 확인:** 미리보기 빌드에서 `/__miy/live`를 백엔드 `/api/live`로 서명 신원과 함께 잇고(플랫폼 중계와 같은 `miy.ready` 포함) 두 사용자로 확인했다(새 대화 즉시 표시, 입력 중 표시, 보낸 메시지 약 0.2초, 수정 반영). 실제 코어 중계(`/_miy/live`, 30초 재확인, 종료 코드)와는 확인하지 않았다.
- **대화 만들기 중 이동 확인(해결):** 사용자 수정(cdf1f1e)으로 `creatingGroup`이 이동 확인 상태에 들어가, 새 대화를 만들고 그 대화로 이동할 때 "보내지 않은 내용…" 확인 창이 떴다(미리보기에서 재현). 2026-11-22 포털이 같은 앱 안의 이동에는 묻지 않도록 바꿨다(앱 프레임과 입력이 그대로 남는다). 앱을 떠날 때만 묻는다. SDK 미리보기는 자체 라우터라 이 동작과 다를 수 있다.
- **첫 관리자 맡기 창(남은 결정):** 지금은 역할이 빌 때 처음 연 사람이 맡는다. 플랫폼이 "앱 역할 지정"이나 설치자 신원을 앱에 줄 수 있으면 그것으로 바꾼다.
- **첨부 해제 재시도:** 서비스 주체가 생기면 남은 해제(삭제된 메시지의 첨부)를 주기 작업으로 정리한다. 지금은 다음 삭제·신고 처리 때 10건씩 재시도한다.
- **실제 플랫폼 확인(계획 절차 9):** SDK 미리보기 빌드를 로컬 게이트웨이·플랫폼 대역과 실제 Rust 서버·일회용 DB로 띄워 Chromium에서 확인했다(1:1 시작, 보내기, 이미지 첨부, 수정, 삭제, 다른 사용자 읽기·답장·알림 끄기·신고, 관리자 맡기·검토, 380px 패널). 실제 프레임 CSP, 실제 코어 Storage·알림함·재확인·홈 카드, 포털 패널 열기, 영어 화면은 확인하지 않았다.
- 카탈로그 공식 릴리스·dev 설치·인수·prod 승격, AI-DO 기준 화면과의 나란한 비교.
