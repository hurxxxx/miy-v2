# AI-DO 화이트보드 포팅 기록

- 원본: `/projects/ai-do/prod` main `0002af062f3147198ee6e6ffef9b30b069122f45` (읽기 전용으로 참조, 수정하지 않음)
- 대상: miy 공식 앱 `whiteboard`(이름 `화이트보드`), 번들 0.12.0(작업 중 개정된 템플릿으로 보호 파일을 다시 맞춤), Rust SDK 0.4.0, 공용 UI 2.0.0, Excalidraw 0.18.1(AI-DO와 같은 버전)
- 절차: [COMMON-FEATURES-PLAN](../../../docs/COMMON-FEATURES-PLAN.md) "앱 포팅 절차", "M7~M11 공식 업무 앱"(M11), "M2 플랫폼–앱 연결과 알림", "M12 통합 검색", §2.2 화이트보드 행
- 원칙: AI-DO 구현을 그대로 옮기고, miy의 경계(인증·저장·앱 프레임·앱별 DB·디렉터리·프레임 CSP) 때문에 그대로 둘 수 없는 곳만 바꾼다. 바꾼 곳은 아래에 모두 적었다. 보강(이미지를 Storage로 분리)은 계획상 포팅 인수 뒤 항목이지만 지시에 따라 함께 넣었다.

## 0. 프레임 CSP에서 Excalidraw 확인 (M11 선행 조건)

**결론: 동작한다.** 글꼴을 앱 자산으로 자체 호스팅하면 그리기·저장·PNG/SVG 내보내기·이미지 넣기가 프레임 CSP에서 된다. 끈 기능은 웹 임베드 하나이고, SVG 글꼴 부분 집합 추출은 Excalidraw가 스스로 대체 경로로 넘어간다.

확인 방법(격리): Excalidraw 0.18.1을 Vite로 빌드해 프레임과 같은 CSP 헤더(`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; worker-src 'none'; frame-src 'none'`, `crates/miy/src/app_frames.rs`와 같은 값)를 붙인 로컬 정적 서버로 열고 Chromium headless(Playwright)로 조작했다. 먼저 Excalidraw만 있는 탐침 페이지로, 이어 이 앱의 SDK 미리보기 빌드를 같은 CSP와 실제 Rust 백엔드(일회용 PostgreSQL, 가짜 코어)로 확인했다. 실제 플랫폼 프레임에서는 아직 열지 않았다(§6).

| 항목 | 결과 | 조치 |
| --- | --- | --- |
| 글꼴 | `EXCALIDRAW_ASSET_PATH`가 없으면 https://esm.sh에서 받으려다 `font-src`로 막힌다 | 패키지 글꼴(`dist/prod/fonts`, 234개·약 13.6MB)을 빌드가 `excalidraw-assets/fonts/`에 원래 이름으로 복사하고(`vite.config.ts`), Excalidraw를 불러오기 전에 그 경로를 지정한다(`platform/excalidraw/self-hosted-assets.ts`, AppRoot가 먼저 import). Excalifont·Xiaolai(한글) 표시 확인 |
| 글꼴 대체 주소 | 자체 글꼴이 로드돼도 Excalidraw가 FontFace마다 덧붙이는 esm.sh 두 번째 주소 때문에 위반 보고가 약 230건 나온다 | FontFace 생성자에서 esm.sh 대체 주소만 뺀다(같은 파일). 이후 `font-src` 위반 0건 |
| 그리기·텍스트(한글)·선택·삭제 | 정상 | — |
| PNG 내보내기 | 정상(`exportToBlob`) | — |
| SVG 내보내기 | 동작. 글꼴 부분 집합 추출용 워커가 `worker-src 'none'`으로 막히고(첫 내보내기에 위반 3건), 주 스레드 대체 경로의 WebAssembly(embind `new Function`)가 `script-src`로 막힌다(위반 1건). Excalidraw가 잡아서 글꼴 조각 전체를 SVG에 넣는다 | 그대로 둔다. SVG가 커진다(측정: 글꼴 6개 포함 약 446KB). 부분 집합 추출을 다시 하려면 플랫폼 CSP 변경이 필요하다(§6) |
| 이미지 넣기(끌어 놓기) | 동작. 큰 이미지 축소(pica)의 WebAssembly가 막혀(위반 1건) JS 경로로 넘어간다 | 그대로 둔다 |
| Mermaid → Excalidraw | 미리보기까지 정상(eval 없음) | — |
| 웹 임베드(YouTube 등 iframe) | `frame-src 'none'`으로 항상 막힌다 | `validateEmbeddable={false}`: 링크를 임베드로 바꾸지 않는다 |
| 다운로드 | 프레임 sandbox에 `allow-downloads`가 있어 Blob 링크 다운로드가 된다(포털 `AppFrame.tsx`) | — |

남는 콘솔 위반(최종 미리보기 빌드 실측): `worker-src`(subset-worker) 3건과 `script-src eval` 1건은 SVG 내보내기 때, `script-src wasm-eval`(pica) 1건은 이미지를 처음 넣을 때. 플랫폼 CSP에는 보고 주소가 없어 콘솔에만 남는다.

## 1. 옮긴 범위와 뺀 범위

**옮긴 것:** 보드 허브(보기 전체·내 보드·최근·즐겨찾기·휴지통, 검색, 정렬 4종, 카드·목록 보기와 그 저장, 비공개·워크스페이스 공개 구역, 지연 미리보기), 보드 만들기(비공개·워크스페이스 공개), 편집기(제목, 저장 상태, "편집 시작", 즐겨찾기, 그리드, 선택 요소 삭제, PNG·SVG·Excalidraw JSON 내보내기, 공유, 공개 범위 메뉴, 다시 불러오기, 휴지통·복원·영구 삭제, 닫기 전 저장), 라이브러리 5종, 사용자 공유(읽기·편집), 대상(워크스페이스 공개) 정렬 순서, 저장 일괄 처리(160ms 모음, 800ms 지연, 하나씩 저장).

**보강·miy 추가:** 이미지를 플랫폼 Storage로 분리(§3.5), 리비전 기반 단독 저장과 충돌 처리(§3.4), 공유 알림(§3.6), 플랫폼 재확인·통합 검색(§3.7), 공간 화면(§3.2).

**뺀 것(화면·API 모두 가져오지 않음):**

| AI-DO 기능 | 원본 위치 | 이유 |
| --- | --- | --- |
| 실시간 공동편집, 협업자 커서·선택 표시 | `collab.py`, `router.py` collab 경로·WebSocket, `scene_state.py`의 Yjs 상태, `whiteboard_collab_documents`, 화면 `whiteboard-collab-runtime*.ts`, `whiteboard-collab-scene.ts`의 Yjs 부분 | 계획 §2.2 제외(후속). 리비전 저장으로 대체(§3.4) |
| AI 연동 | (`aiEnabled={false}`) | 계획 제외 |
| 링크 공유와 공유 링크 화면 | `WhiteboardLinkShare`, `sharing.py` 링크 함수, `/shared-links/*`, `/whiteboard/shared/:token` | 지시: 익명·공개 링크 제외. CONTRACT도 익명 공유 URL을 두지 않는다 |
| PMS·회의 연결(대상, 컨텍스트 슬롯, 선택 창) | `registry.py`의 PMS·회의 판정, `/contexts/slot`, `WhiteboardPickerModal`, `WhiteboardContextSlotPanel`, `public-api.ts` | PMS 앱이 화이트보드가 확인할 API를 아직 제공하지 않고, 다른 앱 화면 안에 끼워 넣는 구성은 프레임에서 불가능. PMS 대상은 403(AI-DO의 알 수 없는 대상과 같음). 일반 링크 필드는 넣지 않았다(§4) |
| 사용 통계·감사 | `record_usage_event`, legacy routes, `app_catalog.py`, `manifest.ts`, `routes.ts` | miy는 manifest·포털이 담당 |
| 웹 임베드 | Excalidraw 기본값 | 프레임 CSP(§0) |

## 2. 파일 대응

화면은 AI-DO `apps/web/src/<경로>`를 앱 `web/src/<경로>`로 옮겼다. "그대로"는 import 경로 외 변경이 없다는 뜻이다.

| AI-DO | miy | 상태 |
| --- | --- | --- |
| `app-modules/whiteboard/views/WhiteboardView.tsx` | 같은 경로 | 수정: 프레임 경로·`runtime.api`·공간, 공유 링크·리소스 진입·임베드 제거, 늦은 목록 응답 무시 |
| `views/WhiteboardViewParts.tsx` | 같은 경로 | 수정: 미리보기가 앱 API와 이미지 내려받기를 사용 |
| `views/WhiteboardEditorSurface.tsx` | 같은 경로 | 수정: 공동편집 제거, 단독 편집·충돌 배너, 이미지 Storage, 링크 공유 제거, 웹 임베드 끔, 늦은 응답·대상 보드 확인 |
| `views/useWhiteboardScenePersistence.ts` | 같은 경로 | 재작성 부분: 공동편집 게시 제거, 리비전·충돌·실패 유지·이미지 저장 |
| `views/whiteboard-collab-scene.ts` | `views/whiteboard-scene.ts` | Yjs 부분 제거, 서명에서 이미지 바이트 제외 |
| `views/whiteboard-editor-session.ts`(+spec) | 같은 경로 | 그대로 |
| `views/whiteboard-editor-utils.ts` | 같은 경로 | 공동편집 도우미 제거 |
| `views/whiteboard-hub-model.ts`(+spec) | 같은 경로 | 수정: 프레임 경로(`/<id>`), 공유 링크 상태 제거 |
| `views/whiteboard-preview-loader.ts`(+spec) | 같은 경로 | 수정: 앱 API, 이미지 내려받은 뒤 렌더 |
| `views/whiteboard-share-model.ts`(+spec) | 같은 경로 | 링크 URL 제거 |
| `views/whiteboard-colors.ts`, `libraries/*.excalidrawlib.json`, `api/whiteboard-scene-codec.ts` | 같은 경로 | 그대로 |
| `api/whiteboard-api.ts`, `api/whiteboard-routes.ts`(+spec) | 같은 경로 | 수정: `runtime.api`, 공간 접두, 제거 기능의 호출 삭제, `base_revision`, 이미지 호출 |
| `sidebar.tsx`, `manifest.ts`의 메뉴 | `app-modules/whiteboard/sidebar.tsx` | 프레임 왼쪽에 그림, 공간 선택 추가 |
| `platform/browser/browser-download.ts` | 같은 경로 | Blob 다운로드만 |
| `platform/i18n/resource-location-label.ts` | 같은 경로 | 그대로 |
| `platform/api/client.ts`, `platform/i18n/*`, `platform/time/*`, `platform/runtime/*`, `lib/utils.ts`, `test-setup.ts` | 같은 경로 | 게시판·플래너 포팅본 사용 |
| `platform/i18n/resources.ts` | 같은 경로 | 생성: [scripts/gen-i18n-resources.mjs](../scripts/gen-i18n-resources.mjs) |
| (없음) | `api/whiteboard-files.ts`, `platform/excalidraw/self-hosted-assets.ts`, `platform/whiteboard/spaces-api.ts`, `AppRoot.tsx` | miy 연결과 보강 |

서버는 FastAPI(Python) → Axum 0.8·SQLx 0.8(Rust)이다.

| AI-DO `apps/api/src/ai_do_api/domains/whiteboard/…` | miy `src/…` |
| --- | --- |
| `router.py`, `access.py`, `hub.py`, `item_mutations.py`, `service.py`, `sharing.py`(사용자 공유), `registry.py`(라벨·대상 판정) | `whiteboard.rs` |
| `scene_state.py` `scene_for_compare`·`scene_matches` | `scene.rs` |
| `models.py` | `migrations/0003_whiteboard.sql` |
| 워크스페이스(`app_global`, `app_space`)와 바인딩 | `spaces.rs`(SDK ACL) |
| `users` 표 조회 | `people.rs`(플랫폼 디렉터리) |
| `core/i18n.localized_http_exception` | `error.rs` |
| (없음) | `files.rs`(이미지 Storage), `notify.rs`(공유 알림·재확인), `search.rs`(통합 검색) |

## 3. 원본과 달라진 점과 이유

### 3.1 데이터 모델

`migrations/0002_acl.sql`은 SDK `acl.sql` 그대로다. `migrations/0003_whiteboard.sql`:

| 표 | AI-DO | miy |
| --- | --- | --- |
| `whiteboards` | `id, workspace_id(FK), owner_id(FK), title, source_app, source_kind, source_ref, generation_kind, scene(JSON), created_at, updated_at, trashed_at` | `workspace_id`는 `'global'` 또는 `whiteboard_spaces.id`(FK 없음), `owner_id`는 miy 사용자 ID, `scene`은 JSONB, `revision` 추가 |
| `whiteboard_targets` | 같은 열과 고유 색인 | 같음(회의·PMS 태스크 리스트 단일 슬롯 색인은 기능과 함께 제외) |
| `whiteboard_user_shares` | `user_id(FK)`, `access_level` | `user_id`는 `text`, 수준 검사 제약 추가 |
| `whiteboard_user_item_prefs` | 같음 | 같음(`user_id`는 `text`) |
| `whiteboard_link_shares`, `whiteboard_collab_documents` | 있음 | 없음(기능 제외) |
| (없음) | — | `whiteboard_spaces`(공간), `whiteboard_files`(저장된 이미지) |

- 시각은 AI-DO처럼 naive UTC, ID는 UUID v4 문자열이다. 대상·공유·선호는 보드 삭제에 따라 지워진다(FK cascade). AI-DO는 영구 삭제 때 하나씩 지웠다.
- 장면 비교는 AI-DO `scene_for_compare`와 같다: 요소, `appState`의 `gridModeEnabled`·`gridSize`·`viewBackgroundColor`, 파일. 바뀌지 않은 저장은 `updated_at`·`revision`을 바꾸지 않는다. 바뀐 장면은 받은 그대로(편집기 상태 포함) 저장한다(원본과 같음).

### 3.2 공간·권한 (워크스페이스 대응)

| AI-DO | miy |
| --- | --- |
| 앱 소유 모드의 전역 공간(`app_global`): 앱 접근이 있으면 바인딩 없이도 `member` | 예약 공간 `global`(화면 "전사 공간"): 앱 사용 권한이 있는 모든 사람이 `member`. API 경로 `/v1/whiteboard/...` |
| 앱 공간(`app_space`)과 멤버 역할 | `whiteboard_spaces` + SDK 자원 ACL(kind `space`), 역할 `member`(`space.read`, `whiteboard.edit`)·`admin`(+`whiteboard.manage`, `acl.manage`). API 경로 `/v1/workspaces/{spaceId}/whiteboard/...` |
| 전역 공간의 `admin` 바인딩(플랫폼 관리자가 부여) | 없음. 플랫폼 관리자 표시는 아무 권한도 주지 않는다(시험으로 확인) |
| 공간 만들기: 앱 설정에 따라 멤버 또는 관리자 | 앱 사용자 누구나. 만든 사람이 보호된 `admin` |
| 보드 접근(`resolve_whiteboard_access`) | 그대로: 소유자 전체 권한, 직접 공유와 대상 수준 중 높은 쪽, 공유·관리는 대상 관리 권한(소유자 제외). 대상은 그 보드 공간의 `workspace_sidebar`만 판정하고 나머지는 권한 없음 |
| 현재 워크스페이스 필터 | 그대로: 항목 경로는 그 공간의 보드만, 목록·만들기·공유 대상 목록은 공간 멤버만(아니면 403 `whiteboard_workspace_context_required`) |

- **잠금 뒤 재판정:** 쓰기 요청(수정·휴지통·복원·영구 삭제·대상·즐겨찾기·열람·공유·이미지 예약)은 보드 행을 `FOR UPDATE`로 잠그고 직접 공유·대상을 다시 읽으며, 공간 역할은 같은 트랜잭션에서 ACL 공유 잠금(`require_in`)으로 확인한다. 공간에서 빠진 사람은 다음 요청부터 바로 막힌다(시험).
- 공간 삭제는 보드(휴지통 포함)가 남아 있으면 409 `whiteboard_space_not_empty`다. 보드 이미지·공유를 함께 정리하는 일을 공간 삭제가 떠맡지 않도록 했다.
- 공간 화면(선택·만들기·멤버·삭제)은 AI-DO 포털 셸의 워크스페이스 전환 대신 앱 왼쪽 메뉴에 둔 miy 화면이다. 멤버 관리는 공용 UI `ShareEditor`.

### 3.3 공유

- AI-DO 사용자 공유를 그대로 옮겼다: `PUT/DELETE …/sharing/users/{userId}`, 검사 순서(볼 권한 404 → 공유 권한 403 → 본인 409 → 사용자 없음 404 → 워크스페이스 멤버 아님 409). SDK ACL로 바꾸지 않은 이유는 §4.
- 사용자 확인·이름·검색은 플랫폼 디렉터리다. 디렉터리에는 이메일이 없어 `email`은 빈 문자열이고, 화면은 그 자리에 `primary_group`(소속)을 보인다. 디렉터리 조회는 보드 잠금 전에 하고 결과는 접근 검사 뒤에 쓴다(잠금을 잡은 채 외부 호출을 기다리지 않음).
- 앱 공간 보드의 공유 대상은 그 공간의 **직접** 멤버여야 한다. 공유 대상은 요청한 사람이 아닌 다른 사람이고, 디렉터리는 다른 사람의 그룹 소속을 알려 주지 않아 그룹으로만 들어온 사람은 확인할 수 없다(닫는 쪽). 그 사람은 공유 없이도 공간 공개 보드를 그룹 멤버로서 이미 열 수 있다. 전사 공간은 디렉터리의 모든 사람(앱 사용 권한은 플랫폼이 거른다).

### 3.4 공동편집 대신 단독 저장(리비전)

AI-DO는 "편집 시작"으로 Yjs 협업 세션을 열고, 여러 사람의 변경을 요소 버전으로 합치며 스냅샷을 저장했다. miy에는 협업 서버가 없으므로:

- "편집 시작"은 최신 저장본을 다시 읽어(리비전이 바뀌었으면 편집기에 반영) 그 리비전에서 편집을 켠다. 그 순간의 편집기 장면을 저장 기준으로 삼아 Excalidraw의 불러오기 정규화가 곧바로 저장되지 않게 한다(실측으로 발견한 거짓 충돌 방지).
- 저장 일괄 처리와 지연은 AI-DO와 같고, 저장마다 `base_revision`을 보낸다. 서버는 장면이 실제로 바뀌었는데 리비전이 다르면 409 `whiteboard_revision_conflict`로 거절한다. 같은 장면이면 리비전과 무관하게 성공(변화 없음)이다. 이름 변경은 리비전과 무관하다.
- 충돌하면 자동 저장을 멈추고 내 장면을 보관한 채 배너를 보인다: **최신 내용 불러오기**(내 변경 버림, 최신 장면을 편집기에 반영) 또는 **내 변경 유지**(최신 리비전을 읽어 그 위에 내 장면을 저장, 상대 변경은 덮어씀). 닫기·휴지통 이동은 충돌이 풀릴 때까지 진행하지 않는다.
- 저장 실패(네트워크·서버)는 장면을 버리지 않고 다시 대기시킨다. "저장 실패" 표시를 누르거나 다음 변경·닫기에서 다시 보낸다. 저장되지 않은 변경·저장 중·실패·충돌·제목 편집은 `runtime.setNavigationState`로 보고한다(AI-DO에는 이동 확인이 없었다).
- 휴지통의 보드는 편집을 켜지 않는다(AI-DO 화면은 관리자에게 "편집 시작"을 보였다). 이미지 예약도 휴지통 보드에는 404다. 복원한 뒤 편집한다.
- 다른 보드로 옮긴 뒤 도착한 저장·불러오기·작업 응답은 현재 보드와 작업 대상을 바꾸지 않는다(요청 순번과 보드 ID 확인).

### 3.5 이미지 (`platform:files:*`, 보강)

AI-DO는 Excalidraw의 `files`(이미지 data URL)를 장면 JSON에 그대로 저장했다. miy는 바이트를 코어 Storage에 둔다.

1. 저장 직전에 새 이미지를 저장한다: `POST …/items/{id}/files`(편집 권한, 요청 ID를 앱 DB에 먼저 기록 → 코어 예약) → `runtime.fileApi(PUT /whiteboard/items/{id}/files/{row})`. 응답을 잃으면 같은 요청 ID로 다시 보내 같은 행과 상태를 받는다(바이트가 이미 있으면 다시 올리지 않음). 실패해도 지우지 않는다.
2. 장면의 파일 항목은 `{id, mimeType, created, storage: "<행 ID>"}`다. 서버는 `dataURL`이 있거나 이 보드의 저장된 행을 가리키지 않는 항목을 422 `whiteboard_scene_file_invalid`로 거절한다.
3. 장면이 저장된 뒤 그 장면이 가리키는 `ready` 행을 코어에 붙인다(요청 ID = 행 ID라 반복해도 같은 요청, 실패하면 `ready`로 남아 다음 저장이 이어서 붙임).
4. 보드를 열면 저장된 이미지를 `runtime.fileApi(GET …)`로 내려받아 data URL로 편집기에 준다. 내려받지 못한 이미지는 자리표시로 남고, 저장할 때 그 항목을 잃지 않는다.
5. 휴지통은 이미지를 유지하고, 영구 삭제는 같은 트랜잭션에서 `detaching`으로 적은 뒤 커밋 후 코어에서 떼어 낸다(붙어 있으면 detach, 아니면 cancel). 확인되지 않은 작업은 다음 영구 삭제가 이어서 끝낸다.
- 형식은 PNG·JPEG·GIF·WebP, 파일당 10MiB. SVG 등 다른 형식은 저장할 때 PNG로 한 번 그려서 저장한다(gateway·공용 미리보기가 래스터만 다룸). 모든 파일 호출은 보드의 현재 권한을 먼저 확인한다(올리기는 편집 권한과 올린 사람, 내려받기는 볼 권한).
- 화면 서명은 이미지를 Excalidraw 파일 ID(내용 해시)와 형식으로만 비교해, data URL과 저장 항목이 같은 장면으로 취급된다.

### 3.6 알림 (miy)

| 종류 | 언제 | 받는 사람 | 묶음 키 | 제목 |
| --- | --- | --- | --- | --- |
| `share` | 사용자 공유를 새로 추가 | 공유받은 사람 | `whiteboard:<id>` | `<이름>님이 화이트보드 "<제목>"을(를) 공유했습니다` |

- 경로는 `/<id>`(앱 공간이면 `?space=<id>`). 수준 변경은 다시 알리지 않는다. 공유 해제 → 그 사람에게서 회수, 휴지통 이동·영구 삭제 → 묶음 전체 회수, 보드 열기(`…/view`) → 읽음.
- 발행은 커밋 뒤 같은 요청 안에서 하고, 실패해도 업무를 되돌리지 않는다. 결과가 불확실하면 같은 요청 ID로 조회만 한다. 받는 사람의 언어를 앱이 모르므로 한국어로 보낸다.
- 그룹 수신자(`Identity::notify_with_groups`)는 쓰지 않는다. 알림을 보내는 곳은 사용자 공유뿐이고 공유 대상은 항상 사용자다(AI-DO 공유에 그룹이 없음). 공간 멤버 변경과 워크스페이스 공개 전환은 AI-DO처럼 알리지 않는다.

### 3.7 플랫폼 → 앱 엔드포인트

- `notificationCheck` → `POST /platform/notification-check`: `whiteboard:<id>`이고 `resourceId`가 같은 ID인 항목만 본다. 보드가 휴지통에 없고, 소유자·직접 공유·(공간 공개이면서 전사 공간이거나 그 공간의 멤버)일 때 허용한다. 플랫폼 호출에는 디렉터리 토큰이 없어 앱 DB로 판정하고, 공간 멤버는 직접 사용자 부여와, 플랫폼이 서명해 넣은 그 사람의 현재 그룹(`identity.context.groups`, 디렉터리 읽기 승인이 있을 때)에 대한 그룹 부여로 확인한다. 서명된 그룹이 없으면 직접 부여만 센다. 형식이 잘못된 그룹 목록은 503(SDK ACL과 같음). 데이터를 바꾸지 않는다. 플랫폼 호출이 아니면 403. 같은 플랫폼 호출로 보드·파일을 읽을 때는 SDK ACL이 서명된 그룹을 스스로 쓴다.
- `search` → `POST /v1/search`: 같은 판정으로 지금 열 수 있는 보드 중 제목이 맞는 것(`%`·`_`·`\` 이스케이프), 최근 수정순 최대 `limit`(1~10). 항목은 `{title, snippet(공간 이름|워크스페이스 화이트보드|비공개), path, at, kind: "화이트보드"}`. 숨긴 보드는 이름도 건수도 내지 않는다.

### 3.8 오류 응답과 문구

- 게시판과 같은 방식이다: `{"error":{"code","message"}}`, 코드는 AI-DO 코드의 `.`을 `_`로, `message`는 AI-DO 영어 카탈로그 문장, 화면은 `errors` 네임스페이스로 번역한다. 요청 검증 실패는 422 `validation_request_invalid`. 길이·범위 검증(제목 1~200자, 대상 64/64/128자, `page_size` 1~200, 보기·정렬 방향 값)은 AI-DO와 같다.
- 새 코드: `whiteboard_revision_conflict`(409), `whiteboard_scene_file_invalid`(422), `whiteboard_space_not_empty`(409), `whiteboard_directory_unavailable`(503), `whiteboard_platform_only`(403), `media_*`, `acl_*`(SDK).
- 화면 문구는 [생성 스크립트](../scripts/gen-i18n-resources.mjs)가 AI-DO `resources.ts`의 `common`, `shell.nav.whiteboard-*`, `apps.whiteboard`, 그리고 화면이 빌려 쓰는 `apps.docs.share.access.*`·`apps.pms.searchUser`·`apps.pms.bulk.remove`·`apps.pms.actions.favorite`·`apps.ai.search.metadataPeople`을 그대로 복사한다(줄 번호 대신 키로 고름). miy 문구는 `apps.whiteboardApp`과 `errors`에 있다. AI-DO의 `count_plural`은 i18next 26에서 쓰이지 않아 원본처럼 단수형이 보인다.

### 3.9 화면 경로와 구성

- AI-DO 경로 `/w/:workspaceSlug/whiteboard`, `/w/:workspaceSlug/whiteboard/:whiteboardId`는 프레임 안에서 `/`, `/<id>`이고 워크스페이스는 `?space=`다. react-router 대신 `runtime.location`·`runtime.navigate`.
- AI-DO가 포털 보조 사이드바에 그린 보기 메뉴와 "새 화이트보드" 동작을 앱 왼쪽에 그리고, 그 위에 공간 선택을 더했다. 만들기 동작은 AI-DO처럼 `whiteboard:create` 문서 이벤트로 허브에 전달한다.
- Excalidraw 화면(편집기와 미리보기)은 지연 로드한다(AI-DO 라우트도 지연 로드). Excalidraw UI 언어는 AI-DO처럼 지정하지 않는다(영어).
- 날짜·시간 표시는 `runtime.settings`(언어·시간대·날짜 형식)를 따른다.

## 4. 결정한 것

| 결정 | 이유 |
| --- | --- |
| 프레임 CSP를 바꾸지 않고 앱에서 해결(글꼴 자체 호스팅, esm.sh 대체 주소 제거, 웹 임베드 끔) | §0. 플랫폼 정책 변경 없이 동작 |
| 전역 공간은 예약 ID `global`, 앱 공간은 SDK ACL | AI-DO 앱 소유 모드의 두 종류 공간을 그대로 대응 |
| 사용자 공유는 AI-DO 표와 경로 그대로(SDK ACL로 바꾸지 않음) | 지시: REST 경로·형태 유지. AI-DO 공유는 읽기/편집 두 수준의 사용자 단위라 그대로 옮길 수 있다. 그룹 공유는 원본에 없다 |
| 링크 공유 제외 | 지시(익명·공개 링크 제외), CONTRACT |
| PMS 연결 제외, 일반 링크 필드도 두지 않음 | PMS 앱 API가 없고, 링크 필드는 AI-DO에 없는 새 화면이라 "사소한 경우만"에 해당하지 않는다고 판단 |
| 단독 저장: 장면 변경에만 리비전, 충돌 시 자동 저장 중지 + 불러오기/유지 | 지시. 이름 변경까지 막으면 거짓 충돌이 늘어난다 |
| "편집 시작" 유지 | AI-DO 상호작용 유지. 편집 직전에 최신본을 읽을 시점이 된다 |
| 이미지 Storage 분리, 영구 삭제 때만 바이트 해제 | 보강 지시. 휴지통은 복원할 수 있어야 하고 Excalidraw 실행 취소가 이미지를 되살릴 수 있다 |
| SVG 이미지는 PNG로 저장 | 지시: 래스터만 |
| 공간 삭제는 빈 공간만 | §3.2 |
| 공유 알림 제목은 한국어 | 받는 사람 언어를 모른다(게시판과 같음) |
| 의존성은 AI-DO 고정 버전 | `@excalidraw/excalidraw` 0.18.1, i18next 26.0.8, react-i18next 17.0.6, 시험용 vitest 4.1.2·jsdom 22.1.0·Testing Library. React·Vite·Tailwind·Radix 등은 템플릿·공용 UI 버전. Yjs는 넣지 않음 |
| 공용 UI 설치는 플래너와 같은 오프라인 복사 | `registry/components/ui/2.0.0`과 같은 파일(`component.json` 제외)과 플래너의 잠금 파일. 실제 서버에서 `miy component sync`로 digest를 확인해야 한다 |
| 번들 보호 파일 재동기화 | 0.12.0 템플릿이 바뀔 때마다 새 `app init`의 바뀐 보호 파일과 `miy.bundle.json`을 복사했다. 첫 번째(Storage 대용량 전송: CONTRACT·api.gen.ts·index.ts·preview·storage.rs)는 앱 코드를 바꾸지 않았고, 두 번째(플랫폼 호출의 서명된 그룹과 `notify_with_groups`: CONTRACT·acl.rs·acl_postgres_tests.rs·lib.rs·notifications.rs)에 맞춰 알림 재확인·검색이 서명된 그룹을 쓰게 했다 |
| DB 시험은 `#[ignore]` + `WHITEBOARD_TEST_DATABASE_URL`, 가짜 코어 | 서버 빌드의 `cargo test`에는 DB가 없다 |

## 5. 테스트 대응

| AI-DO | miy | 비고 |
| --- | --- | --- |
| `test_whiteboard_hub.py::test_whiteboard_create_update_reload_and_archive` | `api_tests.rs::create_update_reload_and_archive` | 공간에서 실행, `base_revision`·리비전 확인 추가. collab 세션 확인 제외 |
| `test_whiteboard_hub.py` collab 4개, `test_whiteboard_rest_scene_patch_invalidates_collab_room` | 없음 | 공동편집 제외 |
| `test_whiteboard_access.py::test_private_whiteboard_is_owner_only` | `private_whiteboard_is_owner_only` | 플랫폼 관리자 표시·신원 없음·위조 서명 추가 |
| `test_whiteboard_access.py::test_whiteboard_user_and_link_shares_grant_access` | `user_shares_grant_access_and_notify` | 사용자 공유 부분. 링크 공유 제외, 알림·회수·읽음 추가 |
| `test_whiteboard_access.py::test_whiteboard_reuses_pms_space_acl` | `space_members_edit_and_admins_manage` | PMS 공간 대신 앱 공간 ACL(직접·그룹 멤버), 멤버 해제 즉시 차단, 공간 삭제 규칙 |
| `test_whiteboard_item_mutations.py` 2개 | `scene.rs` 단위 시험 + 위 HTTP 시험 | 변화 없는 장면, 비공개 보드 수정. 링크 부분 제외 |
| `test_whiteboard_target.py::…filters_and_sort_order` | `target_filters_sort_order_and_unlink` | 공간 대상으로. PMS 대상은 403 확인 |
| `test_whiteboard_target.py` 단일 슬롯, `test_whiteboard_context_owner.py` | 없음 | PMS·회의 연결 제외 |
| (miy 추가) | `older_revision_conflicts_and_keep_resends_on_the_current_one`, `images_are_stored_attached_read_and_released`, `failed_attach_is_recovered_by_the_next_save`, `unconfirmed_detach_is_finished_later`, `platform_check_and_search_follow_current_access`(서명된 그룹 포함·없음·다른 그룹·형식 오류), `hub_views_are_per_person`, 단위 시험(접근 판정, 라벨, 대상 판정, ACL 역할, LIKE 이스케이프) | |
| 화면 `whiteboard-api.spec.ts`, `whiteboard-hub-model.spec.ts`, `whiteboard-share-model.spec.ts`, `whiteboard-editor-session.spec.ts`, `whiteboard-preview-loader.spec.ts`, `whiteboard-collab-scene.spec.ts`(장면 부분) | 같은 경로(장면은 `whiteboard-scene.spec.ts`) | `runtime.api` 모의, 프레임 경로, 링크 제거 |
| `whiteboard-collab-runtime*.spec.ts`, `WhiteboardPickerModal.spec.tsx`, `WhiteboardContextSlotPanel.spec.tsx`, `whiteboard-picker-model.spec.ts`, `whiteboard-context-slot-panel-model.spec.ts`, `WhiteboardEditorSurface.scope.spec.tsx`(컨텍스트 범위) | 없음 | 기능 제외 |
| (miy 추가) | `useWhiteboardScenePersistence.spec.tsx`(리비전·충돌·실패 유지·늦은 응답), `whiteboard-files.spec.ts`(저장·같은 요청 재개·누락 이미지 보존·크기), `self-hosted-assets.spec.ts`, `app-location.spec.ts` | |

브라우저 확인(격리): SDK 미리보기 빌드를 프레임 CSP로 열고 실제 Rust 백엔드(일회용 PostgreSQL, 가짜 코어)와 함께 보드 만들기 → 편집 시작 → 사각형·한글 텍스트 → PNG 끌어 놓기(Storage 저장 확인) → 자동 저장 → PNG·SVG 내보내기 → 새로고침 후 이미지 내려받기 → 편집 시작만으로 저장되지 않음 → 충돌 후 내 변경 유지 → 충돌 후 최신 불러오기 → 닫기와 미리보기 카드를 확인했다. 결과는 작업 보고에 있다.

## 6. 남은 일과 필요한 플랫폼 변경

- **실제 프레임 확인(계획 절차 9):** 설치된 플랫폼 프레임(다른 출처, 실제 CSP·sandbox·`runtime.fileApi` gateway)에서 Excalidraw·글꼴·이미지 업로드/내려받기·다운로드·이동 확인을 다시 확인해야 한다. 이번 확인은 같은 CSP를 붙인 로컬 미리보기였다.
- AI-DO 기준 화면(가상 계정·가상 데이터)과의 나란한 비교, 카탈로그 공식 릴리스·dev 설치·권한 인수(특히 공유 회수), prod 승격.
- 공간 이름 바꾸기는 API(`PATCH /v1/spaces/{id}`)만 있고 화면이 없다. 공간 멤버 편집기는 현재 사용자를 "본인"으로 표시하지 않는다(앱에 `/me`가 없음).
- **플랫폼 변경 필요(이 앱에서 하지 않음):**
  1. 다른 사람의 그룹 소속 확인(디렉터리의 그룹 구성원 조회 등): 지금은 그룹으로만 앱 공간에 든 사람을 사용자 공유 대상으로 고를 수 없다. 플랫폼 호출의 그룹 판정(알림 재확인·통합 검색)은 플랫폼의 서명된 그룹으로 해결되었다(`d7fbdfe`).
  2. SVG 글꼴 부분 집합 추출을 원하면 프레임 CSP에 `'wasm-unsafe-eval'`(주 스레드 경로, embind의 `new Function`은 여전히 `'unsafe-eval'` 필요) 또는 `worker-src 'self'`가 필요하다. 지금은 글꼴 조각 전체가 SVG에 들어간다.
  3. 받는 사람 언어별 알림 제목(또는 플랫폼이 번역하는 키+인자 형식).
  4. 서버 빌드가 새 npm 패키지(`@excalidraw/excalidraw` 0.18.1과 그 의존성)를 받을 수 있어야 한다(`pnpm-lock.yaml`에 고정). UI 산출물이 글꼴 때문에 약 23MB(426개 파일)로 이미지 한도(64MiB/파일, 256MiB·10,000개) 안이다.
  5. 카탈로그(`registry/apps/catalog.json`)의 `whiteboard` 항목에 릴리스를 기록하는 일(소스 고정 뒤).
  6. PMS 연결을 되살리려면 PMS 앱이 공간·태스크 리스트의 현재 권한을 답하는 앱 간 API(`app:pms:read`)를 제공해야 한다.
