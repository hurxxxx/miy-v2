# v1(miy) 검토와 v2 적용 계획

- 작성일: 2026-11-03
- 검토 대상: 공식 v1 저장소 `/projects/miy` (main `188a8cef`, VERSION 0.1.1, 미출시)
- 비교 기준: v2 main `3876d43` (CLI/core 0.7.0, 번들 0.8.0)
- 상태: **검토 완료 · 적용 항목은 미착수.** 실제 착수 순서는 [추진 계획](PLAN.md)이 관리한다.

v1은 Python API·Nx 모노레포·공유 DB 위에 공식 업무 앱 12종, Hermes 챗봇, RAG, Workbench까지 구현했지만 출시하지 않았다. v2는 Rust 코어·앱별 컨테이너/DB·CLI/MCP 중심으로 새로 만든 후속 제품이다. 이 문서는 v1의 **계약·운영 규칙·실패 교훈**을 v2 구조에 맞춰 어떤 순서로 가져올지 정한다. v1 코드나 데이터 모델을 그대로 옮기는 계획이 아니다.

기본 UI의 원본은 v1이 아니라 `../ai-do/dev`다([UI 이식 계획](UIUX-MIGRATION.md)). UI 항목은 ai-do에 같은 자산이 있는지 먼저 확인하고, 둘이 다르면 ai-do 기준을 따른다.

v1 근거는 `miy:<경로>`로 적는다. `miy:`는 `/projects/miy`를 뜻한다. 이 문서는 2026-10-20 [RESEARCH](RESEARCH.md)에 있던 "기존 miy에서 참고할 계약" 절을 흡수했다. 당시 링크는 더 이상 없는 `../miy/dev` 경로를 가리켰다.

## 1. 결론

1. **v1에서 가져올 핵심은 기능 코드보다 경계 규칙이다.** 원본 ACL 재확인, 파생 권한을 저장하지 않는 그룹 모델, 실행에 묶인 AI 도구 신원, 인자에 묶인 쓰기 승인, 자동 전환 금지, 불확실한 커밋 결과로 보상 삭제를 하지 않는 원칙 등이 해당한다. 모두 v2의 기본 거부·명시 승인 방향과 맞는다.
2. **v1이 폐기한 구조는 다시 들이지 않는다.** 플랫폼 전역 워크스페이스(ADR 0012로 폐기), 앱 간 공유 DB 외래키, 셸형 AI 터미널, 대리 로그인, 범용 검증 엔진, 무거운 hook·skill 체계가 그 대상이다.
3. **v2에 들어갈 자리가 없는 공백은 세 가지다.** 감사 기록 조회·보존, 서비스 계정용 범위 제한 API 키, 앱이 플랫폼을 통해 LLM을 호출하는 등록 workload다. [추진 계획](PLAN.md)에 새 항목으로 추가했다.
4. **공식 업무 앱은 "이전"이 아니라 "재구현 후 인수"다.** v1의 업무 모듈 14종은 모두 전체 앱 인수 전에 멈췄다(`miy:platform-redesign/APP_ISSUES.md`). v2는 앱을 적게 골라 하나씩 인수 완료까지 진행한다.

## 2. 적용 원칙 (v1이 기록한 실패에서)

| 원칙 | v1 근거 |
| --- | --- |
| 검색·알림·색인 후보나 숨긴 버튼은 권한이 아니다. 제목·요약·건수를 내보내기 전에 원본 ACL을 다시 확인한다 | `miy:docs/domains/notifications/README.md`, `miy:adr/0004-retrieval-rag-boundary-policy.md`, `miy:docs/product/core-platform-user-acceptance.md` |
| 그룹에서 파생된 권한은 행으로 저장하지 않고 계산한다. 저장했던 v1 PMS에서는 회수 후에도 접근이 남았다 | `miy:WORKSPACE_ACL_REVIEW.md` (41행) |
| 비동기 작업은 가져올 때, 외부 호출 전, 쓰기 전에 현재 권한을 다시 확인한다 | `miy:WORKSPACE_ACL_REVIEW.md` (42행) |
| 계정 정지 시 열린 화면을 먼저 닫지 말고 권한 이벤트를 먼저 보낸다 | `miy:WORKSPACE_ACL_REVIEW.md` (53행) |
| 자동 전환 금지: 로컬↔외부 모델, 모델 대체, Hermes 실패 대체, 다른 계정 과금으로 넘어가지 않는다. 모르는 비용은 0이 아니라 "미확인"으로 표시한다 | `miy:docs/domains/ai/gateway.md`, `miy:docs/domains/integrations/README.md` |
| 앱 간 연결은 API와 ID로만 한다. v1은 공유 DB의 교차 외래키 때문에 분리에 수십 개의 경계 문서가 필요했다 | `miy:platform-redesign/OFFICIAL_APPS.md`, `miy:platform-redesign/FILES_SOURCE_*.md` |
| 이름 변경으로 Compose 프로젝트·볼륨 이름이 바뀌면 빈 볼륨이 새로 생긴다. baseline을 교체할 때 이전 스키마를 stamp하거나 삭제하지 않는다. v1처럼 데이터를 리셋하지 않는다 | `miy:docs/domains/release/README.md` |
| smoke 통과는 전달 증거가 아니다. mock이 저장소 장애를 숨겼다. AI 기능 인수는 브라우저→실행→MCP→서비스 전체 경로로 확인한다 | `miy:docs/domains/ai/hermes.md`, `miy:docs/domains/ai/capabilities.md` |
| 지침은 행동을 안내하고, 강제할 제한은 checker·CI·hook에 둔다. 범용 검증 엔진, 자동 재시도, 과도한 skill·hook은 만들지 않는다. v1은 skill을 14개에서 7개로 줄이고 hook은 PreToolUse 패치 보호만 남겼다 | `miy:CLAUDE.md`, `miy:platform-redesign/POLICY.md`, `miy:platform-redesign/VALIDATION.md` |
| 진행 로그형 문서는 계속 커진다(`VALIDATION.md` 2,423줄). 담당 문서마다 현재 상태 하나만 두고 이력은 보관 문서나 증거 파일로 보낸다 | `miy:docs/README.md`, `miy:platform-redesign/` |
| 공용 UI 패키지는 i18n 라이브러리에 의존하지 않고 번역된 문구를 props로 받는다. 서버와 SDK의 길이 제한 계산을 일치시킨다. 편집기 스키마는 한 곳에서만 정의한다 | `miy:platform-redesign/OFFICIAL_FILES_WORKSPACE.md`, `miy:platform-redesign/SDK_FILES.md`, `miy:packages/ui/src/lib/editor/schema.ts` |

## 3. 적용 항목

각 항목은 **적용**(규칙을 그대로 가져옴) 또는 **변형 적용**(v2 모델에 맞게 재설계) 중 하나로 판정했다. 우선순위는 P1(해당 단계의 선행 조건), P2(같은 단계 안에서 함께 처리), P3(필요해질 때) 순이다. 배치 위치는 [추진 계획](PLAN.md)의 작업 순서를 따른다.

### 3.1 개발 절차·품질 검사 (즉시)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-Q1 | 플랫폼 개발용 루트 `AGENTS.md`와 `CLAUDE.md` 연결. 커밋·push·배포 권한, CI·생성 계약·릴리스 게이트 우회 금지, 비밀 취급, 실행/미실행 검사 보고를 담는다 | `miy:AGENTS.md`, `miy:CLAUDE.md` | 없음. 생성 앱용 `templates/rust-react/AGENTS.md`만 있음 | 변형 적용: Rust 코어·CLI·운영 서버 안전 규칙 중심 | P1 |
| V1-Q2 | 변경 유형별 최소 검사 지도(Rust 코어, CLI, 웹, migration, 설치 파일, 템플릿) | `miy:docs/agents/vibe-coding-harness.md` | [CORE-TESTING](CORE-TESTING.md)·[VALIDATION](VALIDATION.md)에 흩어져 있음 | 변형 적용 | P1 |
| V1-Q3 | 웹 CI 작업(`pnpm typecheck` + `pnpm build`). `scripts/check.sh`를 CI에서 돌릴 수 있는 부분과 실행 중 스택이 필요한 부분으로 분리 | `miy:package.json` (`check:web-architecture`) | 웹 검사는 로컬 `check.sh`에서만 실행 | 적용 | P1 |
| V1-Q4 | 버전 일치 검사: Cargo, 번들·템플릿 버전, 릴리스 증거 | `miy:scripts/check-project-version.mjs` | 릴리스 태그와 Cargo 버전만 대조 | 변형 적용 | P2 |
| V1-Q5 | 의존성·비밀 검사(`cargo deny`/`cargo audit`, secret scan) | v1에도 없음 | 없음 | 신규 | P2 |
| V1-Q6 | 하드코딩 경로 검사: v1 경로, 절대 `/projects/` 경로 | `miy:scripts/check-path-hardcoding.mjs` | 없음 | 변형 적용 | P2 |
| V1-Q7 | 앱 계약에서 빠진 규칙(중복 ID, route 소유)을 Rust 단위 테스트로 추가 | `miy:scripts/app-contract-validation.mjs` | `runtime_contract.rs`·`component_contract.rs`가 대부분 담당 | 변형 적용 | P2 |
| V1-Q8 | env 키 계약: 폐기된 키를 차단하고 모든 키가 문서화됐는지 확인 | `miy:scripts/check-env-contract.py` | `scripts/check-server-config.py`가 일부 담당 | 변형 적용 | P2 |
| V1-Q9 | 앱 개발 지침 평가: 같은 모델·설정으로 기준안과 후보안을 3회 이상 반복하고, 읽은 문서가 아니라 산출물로 판정 | `miy:scripts/agent-guidance-eval.mjs` | `scripts/run-agent-acceptance.mjs`, `check-cli-guidance.mjs` | 변형 적용: 기존 인수 도구에 규칙만 추가 | P3 |

### 3.2 코어 운영 기반 (코어 3·4·5차에 흡수)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 | 배치 |
| --- | --- | --- | --- | --- | --- | --- |
| V1-O1 | Docker 저장소 사전 점검(15GiB 이상이면서 15% 이상 여유), 현재·직전 롤백 이미지 보존. BuildKit 캐시 GC는 이미지 정리와 분리 | `miy:docs/domains/release/README.md` §Build and test storage, `miy:scripts/docker-storage.mjs` | [CAPACITY](CAPACITY.md)에 설계만 있음 | 적용 | P1 | 3차 |
| V1-O2 | DB 연결 예산: dev/prod 합계를 서버 한도 아래로 두고, `application_name`으로 구분해 관찰. `pool_size=0`(무제한)과 오래된 env 값이 새 기본값을 덮는 문제 차단 | release README §Database connection budgets | CAPACITY 표에 한 줄 | 적용 | P2 | 3차 |
| V1-O3 | 깨지는 변경의 전환·롤백: 불변 이미지 digest, 해당 리비전의 설정 스냅샷과 0600 env 백업, prepare/deploy/rollback 상호 배제 잠금, 실패 시 이전 이미지 복구 | release README §Incompatible cutovers, `miy:scripts/prod-app*.sh` | 배포마다 쓰는 `deploy.py`의 `restore()`뿐 | 적용: 호스트 updater의 설계 원칙으로 | P1 | 4차 |
| V1-O4 | 불확실한 커밋 결과로 보상 삭제를 하지 않음. 확정된 거부 SQLSTATE만 근거로 인정하고, class 40 전체와 completion_unknown은 제외 | `miy:apps/api/src/miy_api/core/transaction_outcome.py` | 1차에서 DB 오류를 타입별로 분류 | 적용: 파일 저장소·정리 작업에도 적용 | P2 | 4차·§7-3 |
| V1-O5 | 공개 런타임 설정 스키마: 모르는 키나 잘못된 값이면 시작 실패, 값은 출력하지 않음. 설정 migration은 기본 dry-run이고 0600으로 백업 | release README §Public runtime configuration, `miy:scripts/runtime-config-migrate.py` | `render-server-config.py`, `runtime.env` | 변형 적용 | P2 | 4·5차 |
| V1-O6 | 호스트 진단 항목: Docker data-root 이전, LXC cgroup의 swap 제한, 클러스터 생성 전 PostgreSQL 로케일 | `miy:docs/domains/release/installation-operations.md` | 없음 | 적용 | P2 | 5차 |
| V1-O7 | 빈 설치 공개 전 점검: 격리 프로세스에서 초기 관리자 생성, 공개 가입 차단 확인, 업무 앱은 꺼진 상태로 시작 | release README | `scripts/bootstrap.py` | 적용: 인수 체크 항목 | P2 | 5차 |
| V1-O8 | 검증 자원 분리: 빌드와 브라우저 검증을 같은 호스트에서 겹쳐 돌리지 않음(v1에서 OOM 발생), 캐시 없는 웹 빌드는 Node heap 3GiB | `miy:WORKSPACE_ACL_REVIEW.md` (122행) | 없음 | 적용 | P3 | 3·5차 |

### 3.3 조직·그룹 권한 (UI §7-2)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-G1 | 단일 `groups` 엔터티(출처: 인사 `hr`, 수동 `local`). 계층·책임자 지정 자체는 권한을 주지 않음. 계층 변경은 잠금 후 순환 검사 | `miy:docs/domains/organization/README.md`, `miy:adr/0012-company-app-access-without-workspaces.md` | 사용자 단위 `grants`만 있음 | 변형 적용: 그룹을 use/develop/deploy의 주체로 추가하고 기본 거부 유지. 파생 권한은 계산 | P1 |
| V1-G2 | 앱 입장 대상(전체/선택, 사용자∪그룹)과 앱별 회사 중지 스위치. 설정이 없거나 대상이 비면 거부. 관리자도 중지 스위치는 우회 못 함 | `miy:docs/domains/app-platform/README.md` §App Admission | 없음. 중지 스위치는 [PRD](PRD.md)에 요구만 있음 | 변형 적용: "전체"는 명시적으로 선택할 때만 | P1 |
| V1-G3 | 그룹·계정 변경 시 영향받는 사용자만 무효화하고, 대기 작업을 다시 확인 | `miy:WORKSPACE_ACL_REVIEW.md` | 사용자 단위 위임 재확인은 있음([CORE-PERMISSIONS](CORE-PERMISSIONS.md)) | 적용 | P1 |
| V1-G4 | 디렉터리 선택기 DTO: 활성 대상만, 페이지 처리, 보안 필드 제외. 디렉터리 조회 가능 여부와 콘텐츠 권한은 별개. SDK와 공용 UI에 사용자·그룹 선택기 제공 | `miy:packages/platform-web/src/directory/DirectoryPicker.tsx` | `connections.rs` `directory_data`가 활성 사용자 전체를 반환 | 변형 적용 | P2 |
| V1-G5 | 업무 컨테이너(스페이스·프로젝트)는 앱이 소유. 플랫폼 전역 워크스페이스 선택기는 다시 들이지 않음 | ADR 0012, `miy:WORKSPACE_ACL_REVIEW.md` (29~30행) | [UI 이식 계획](UIUX-MIGRATION.md) §7-2 방향과 일치 | 적용 | P1 |
| V1-G6 | 실사용 인수 시나리오 LIVE-01~17(권한·공유·회수)을 v2 인수 항목으로 재작성 | `miy:docs/product/core-platform-user-acceptance.md` | 없음 | 변형 적용 | P2 |

### 3.4 운영 관리 공백 (새 항목)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-A1 | 구조화된 감사 기록과 관리자 조회: 대상 종류·결과·요약, 기간·행위자·동작 필터, 권한 변경과 같은 트랜잭션에 기록, 보존 기간 | `miy:apps/api/src/miy_api/domains/auth/models.py`, admin `/audit-logs` | `audit` 테이블은 actor/action/target/request_id뿐. 미들웨어는 `METHOD:status`만 기록하고, 기록 실패는 로그만 남김. 조회 API 없음 | 변형 적용 | P2 |
| V1-A2 | 범위가 제한된 플랫폼 API 키(서비스 계정 기반): opaque prefix, SHA-256 해시만 저장, 코드로 고정한 scope, 사용자 토큰과 섞어 쓸 수 없음, 감사 기록 | `miy:adr/0010-platform-api-key-directory-integration.md`, `miy:docs/domains/integrations/README.md` | 서비스 계정 미구현([CORE-HARDENING](CORE-HARDENING.md) 2차 한계) | 변형 적용: 다시 보여 주기 위한 암호문 저장은 제외 | P2 |
| V1-A3 | 사용량·비용 집계: dev/prod 분리, `complete:false`, 보고되지 않은 비용을 0으로 처리하지 않음 | integrations README, `miy:MIY-SOLUTION-GAP-REVIEW.md` §5.4 | 없음 | 변형 적용 | P3 |

### 3.5 공용 UI·테마·언어·SDK (UI §7-1)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-U1 | 업무 UI 원칙: 밀도 높은 화면, 평면 표, 한 줄 도구 모음, 모바일 메뉴는 런처, 앱 메뉴는 앱이 소유. 템플릿 지침에도 넣는다 | `miy:docs/product/ui-design-principles.md` | 규칙 문서 없음 | 적용(ai-do와 충돌하면 ai-do 우선) | P1 |
| V1-U2 | 다크/시스템 테마: `AppRuntime.theme`를 `"light" \| "dark"`로 확장, `--miy-*` 다크 토큰, 강조색 전경 누락 검사 | `miy:packages/platform-web/src/theme/document-theme.ts`, `miy:scripts/check-web-dark-mode-tokens.mjs` | `theme: "light"` 고정 | 변형 적용 | P1 |
| V1-U3 | 한국어/영어 메시지 목록, 런타임 `locale`, 하드코딩 문자열 검사 | `miy:packages/platform-web/src/i18n/resources.ts`, `miy:scripts/check-web-i18n-messages.mjs` | 한국어만 | 변형 적용 | P1 |
| V1-U4 | 공용 primitive: dialog, confirm/prompt, data-table, 빈 상태·skeleton, 검색 필드, 필터 바, 탭, 상세 drawer, split pane. 새 불변 registry 버전(1.1.x)으로 게시하고 i18n 의존성은 넣지 않음 | `miy:packages/ui/src/lib/` | registry에는 `Panel`·`Button`만 있음 | 변형 적용 | P1 |
| V1-U5 | 날짜·시간대 유틸과 한국 공휴일(정적 데이터, 지원 범위 밖이면 null) | `miy:packages/platform-web/src/time/korean-holidays.ts`, `miy:docs/product/korean-holidays.md` | 없음 | 적용 | P2 |
| V1-U6 | SDK 확장: 테마·언어 변경 알림, 다른 앱으로 이동 제안(사용자 확인 필수, 자동 이동 없음), 저장 대기 신호 | `miy:platform-redesign/SDK_NAVIGATION.md`, `miy:apps/official-suite/README.md` | 앱 내부 `navigate`, `registerNavigationGuard` | 변형 적용 | P2 |

### 3.6 공통 업무 기능 (UI §7-3)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-F1 | Storage API 먼저, 그다음 파일 앱. 선택한 파일 읽기는 플랫폼 선택기, 짧은 읽기 권한, 10MiB 상한, `files:read-selected` 권한으로 제공 | `miy:platform-redesign/SDK_FILES.md`, `miy:packages/official-suite-web/src/files/` | PRD에 계획(MinIO 제외)만 있음 | 변형 적용 | P1(§7-3 첫 항목) |
| V1-F2 | 콘텐츠 전달 증표: 15분 이내, 세션에 묶임, `no-store`, 스트리밍 직전 재확인, 전용 서명 키 | `miy:docs/domains/content-access/README.md` | 없음 | 변형 적용 | P2 |
| V1-F3 | 알림은 원본 앱 이벤트의 읽기 projection. 목록·건수·읽음 처리마다 원본 ACL을 다시 확인 | `miy:docs/domains/notifications/README.md` | 없음(UI-4에서 메뉴 제외) | 변형 적용: 앱이 발행하고 코어가 원본 앱에 재확인 | P2 |
| V1-F4 | 업로드 관리자 UX: 오른쪽 아래 고정, 탭 단위, 재개·복원 없음 | `miy:docs/product/ui-design-principles.md` | 없음 | 적용(F1과 함께) | P2 |
| V1-F5 | BlockNote 편집기와 협업 codec. 스키마는 한 곳에서 정의하고 Rust 서버의 Yjs 처리 방식은 별도 결정 | `miy:packages/ui/src/lib/editor/`, `miy:scripts/blocknote-collab-codec.mjs` | 없음 | 변형 적용 | P3 |
| V1-F6 | 공식 업무 앱(PMS·문서·플래너·회의·녹음·캘린더)을 독립 앱으로 재구현. 교차 외래키 대신 API와 링크 사용. 녹음의 시도·버전 fencing 재사용 | `miy:packages/official-suite-web/src/*`, `miy:docs/apps/recording/README.md` | 없음 | 변형 적용: 한 번에 하나씩 인수까지 | P3 |
| V1-F7 | 실시간 연결: 권한 종료 코드(1008/4401/4403)를 받으면 화면을 가리고 신원 재조회, WS URL에 토큰 금지. DM은 별도 앱으로 | `miy:docs/domains/app-platform/README.md` §Revoked realtime sessions, `miy:apps/web/src/app-modules/dm` | 없음(폴링으로 회수 처리) | 변형 적용: WS 도입 시 | P3 |
| V1-F8 | 앱이 선언한 레코드 검색. 처음에는 앱마다 검색하고, 앱 간 projection은 원본 ACL 재확인을 갖춘 뒤 도입 | `miy:packages/app-sdk/README.md` | 없음 | 변형 적용 | P3 |

### 3.7 내부 AI·챗봇·RAG ([AI 계획](AI.md), PLAN 4-1·5단계)

v2에는 아직 AI 실행 코드가 없다. MCP는 개발 도구만 제공한다. 따라서 아래 항목은 기존 v2 코드를 바꾸는 것이 아니라 계획된 자리를 채운다.

| ID | 항목 | v1 근거 | 판정 | 우선 |
| --- | --- | --- | --- | --- |
| V1-I1 | miy↔Hermes 책임 구분과 내구성 있는 실행: miy가 실행·입력·이벤트·승인·lease·outbox를 저장하고, miy 실행 ID를 멱등 키로 사용. 대화 안에서는 오래된 실행부터 처리하고 전체 대기열에 상한. SSE를 잃어도 5초 상태 폴링으로 복구. 중지 요청은 먼저 저장한 뒤 전달 | `miy:docs/domains/ai/hermes.md`, `miy:apps/api/src/miy_api/domains/hermes/{execution,repository,models}.py` | 변형 적용(Rust) | P1 |
| V1-I2 | 실행에 묶인 도구 신원: 내부 MCP가 프로필 HMAC과 실행 ID를 요구하고 모델은 신원을 지정할 수 없음. 호출마다 계정·앱·도구 범위·원본 ACL 재확인 | `miy:ops/hermes/plugins/miy_runtime/` | 적용 | P1 |
| V1-I3 | 인자에 묶인 쓰기 승인: 쓰기 도구는 기본 숨김. 승인은 서버+도구+인자 SHA-256에 묶이고 1회용이며 5분 후 만료. 재개 시 고정된 스냅샷을 쓰며 범위는 좁힐 수만 있고 ACL을 다시 확인 | `miy:docs/domains/ai/write-policy.md`, `execution.md` | 적용 | P1 |
| V1-I4 | 앱이 AI 도구를 manifest에 명시적으로 선언. 전용 입력 스키마(JSON Schema 부분집합), 하나의 등록에서 MCP와 모델 스키마를 함께 생성. 목록에서 숨긴 도구는 실행도 차단. REST/OpenAPI에서 자동 생성하지 않음 | `miy:adr/0002-mcp-capability-platform.md`, `miy:docs/domains/ai/capabilities.md` | 변형 적용(manifest 선언·코어 집행) | P1 |
| V1-I5 | 관리자 모델 연결 목록: 이름 붙은 연결마다 암호화된 키(조회 시 `has_api_key`만 반환). 발견된 모델은 관리자 승인 후 사용. 정책은 DB에만 두고 실패하면 닫힘. 자동 전환 없음. 처음부터 DB에 저장(v1은 env→DB 이전 단계가 필요했음) | `miy:docs/domains/ai/gateway.md`, `miy:adr/0005-registered-llm-workload.md` | 변형 적용(Codex/Claude Code 구독 어댑터 추가) | P1 |
| V1-I6 | Codex App Server 연동 규칙: 사용하는 RPC 스키마 버전을 고정하고 호환되지 않는 CLI는 거부. 결과를 모르는 turn은 thread 이력을 읽고 자동 재전송 금지. `thread/resume` 때 sandbox·cwd를 다시 지정하고 기록보다 넓은 권한은 거부 | `miy:apps/codex-console-api/src/codex_console/{runtime,rpc,protocol_contract}.py` | 적용(4-1 공통 연결 실험) | P1 |
| V1-I7 | 앱이 LLM을 부를 때 쓰는 등록 workload: 안정적인 `workload_id`, 출력 상한·경로·감사·외부 전송 정책. 앱은 공급자 자격증명을 갖지 않음 | ADR 0005, `miy:templates/independent-app/AGENTS.md.template` | 변형 적용(작은 코어 endpoint + SDK, manifest 선언) | P2 |
| V1-I8 | 외부 전송 데이터 정책: 처음에는 원본별 허용/차단 + 감사만, 마스킹은 후속. 차단은 실패로 닫히고 다른 경로로 돌리지 않음 | `miy:apps/api/src/miy_api/domains/ai/security_policy.py` | 변형 적용 | P2 |
| V1-I9 | Hermes 이미지 digest 고정과 업그레이드마다 smoke 실행. 비공개 API 어댑터 8종이 필요했던 비용을 예산에 반영 | `miy:docs/domains/ai/hermes.md` §Pinned upstream gaps, `miy:ops/hermes/check_*.py` | 적용(코드 실행 sandbox는 P3) | P2 |
| V1-I10 | 챗봇 UX: 이벤트 순번으로 중복 제거하며 재개, 서버의 종료 이벤트를 받은 뒤 중지 표시, 한국어 IME 입력 중 Enter로 전송하지 않음, 대화 전환 뒤 늦게 온 응답 무시, 실행 중 업로드 차단 | `miy:docs/apps/chatbot/hermes-desktop/README.md`, `miy:packages/platform-web/src/chatbot/` | 적용 | P2 |
| V1-I11 | RAG 권한 규칙: 후보 파티션은 범위를 좁힐 뿐 권한을 주지 않음, 인용·모델 입력 전에 원본 ACL 확인, 백엔드 간 원점수를 비교하지 않음(RRF), 명시한 원본을 쓸 수 없으면 실패로 닫힘, outbox 버전 관리, 생성→검증→전환 방식의 색인 교체, 품질 코퍼스. 문서·벡터 ID namespace는 한 번 정하면 바꾸지 않음 | `miy:adr/0004-retrieval-rag-boundary-policy.md`, `miy:adr/0009-retrieval-partition-projection-generations.md`, `miy:docs/domains/{retrieval,rag}/README.md` | 변형 적용(pgvector 단일 백엔드) | P2 |
| V1-I12 | 중첩 생성 교착 방지: 도구가 부른 생성이 부모 실행 뒤에서 기다리면 즉시 실패(`nested_capacity`). 열린 SSE가 DB 연결을 잡고 있지 않게 함. 공급자 어댑터마다 무시되는 파라미터를 명시적으로 거부 | `miy:docs/domains/ai/hermes.md` | 적용 | P2 |
| V1-I13 | 임베딩·OCR·음성 인식 등 LLM 외 추론은 역할별 어댑터를 거침. 앱은 호스트·장치를 가정하지 않음 | `miy:docs/domains/inference-gateway/README.md` | 변형 적용(계약 한 줄) | P3 |

2026-10-20 조사에서 확인한 v1 경로도 유지한다. 사이트 챗봇은 `miy:packages/platform-web/src/chatbot/api/chatbot-api.ts`에서 시작한다. `miy:apps/api/src/miy_api/domains/hermes/router.py`의 `create_run`과 `miy:apps/worker/src/miy_worker/tasks/hermes.py`를 거쳐 내부 Hermes의 `/v1/runs`로 이어진다. v2가 가져올 것은 이 책임 구분이다. Python API·워커·큐 제품·런타임 버전은 가져오지 않는다. 구독 인증을 쓰는 Workbench(`codex app-server`, 소유자 1인)와 제품 내장 Hermes는 별도 경로다. v1에는 다중 사용자 구독 연결이나 Claude Code 구독을 Hermes 백엔드로 쓰는 구현이 없었다. v2의 공통 연결과 사용자별 세션은 [AI 계획](AI.md)에 따라 새로 설계한다.

## 4. 적용하지 않는 것

| 대상 | 이유 |
| --- | --- |
| 플랫폼 전역 워크스페이스·테넌트 범위(ADR 0006·0007·0011) | v1이 ADR 0012로 폐기함. 이전 AI ADR의 scope 개념도 가져오지 않음 |
| 공유 DB와 앱 간 외래키, 같은 트랜잭션 ACL | v2의 앱별 DB와 반대 구조이며 v1 분리 비용의 원인 |
| iframe/PKCE 기반 SDK 연결, 사용자별 범용 JSON 레코드 저장 | v2는 같은 출처의 Module Federation과 앱별 DB를 사용. 신뢰할 수 없는 제3자 앱을 받을 때 별도 origin 실행 방식을 다시 검토 |
| Workbench(1인용 웹 Codex 콘솔, worktree·GitLab MR) | v2는 사용자 자신의 Codex/Claude Code와 CLI/MCP를 사용. 연동 규칙은 V1-I6만 가져옴 |
| 셸형 AI 터미널(PTY) | v1이 폐기했지만 종료를 위한 broker·egress 정리 부담이 남았음 |
| 대리 로그인, 임시 비밀번호·재설정 | 대리 로그인은 v1이 폐기, 나머지는 Keycloak이 담당 |
| LangGraph 기반 실행, Decisions API, 직접 completion 우회, 사용자별 모델 선택 | 현재 v2 범위 밖. 보고서 산출물을 추가할 때 산출물 불변 규칙만 재검토 |
| OpenSearch·Qdrant 이중 검색 백엔드 | v2는 pgvector 단일 백엔드로 시작([STACK](STACK.md)) |
| GitLab MR·runner 게이트, 자체 Codex 리뷰 CI, MR 증거 스크립트 | GitLab·자체 runner에 종속되고 비용이 큼. PR 템플릿의 "실행/미실행 검사·남은 위험"만 가져옴 |
| Python 소스 무결성·alembic 상태 검사, 웹 모듈 import 경계 검사 | 기술 스택이 다름. v2 웹은 아직 작음. 적용된 SQL migration 수정 금지 테스트는 V1-Q7과 함께 검토 |
| Codex `apply_patch` hook | v2의 `miy app check` 번들 무결성 검사가 더 강함. v1도 hook을 보안 경계로 보지 않음 |
| Beat heartbeat | v2 worker 소유권 감시(1차)가 대신함 |
| Bento·draw.io 기반 다이어그램 | 필요할 때 별도 origin 원칙으로 다시 검토 |

## 5. 진행 방법

1. 착수 순서는 [추진 계획](PLAN.md)의 작업표가 관리한다. 이 문서는 항목 ID와 근거만 관리한다.
2. 각 구현 묶음은 시작할 때 해당 ID의 v1 근거를 다시 읽는다. ai-do·v2에 이미 같은 기능이 생겼는지 확인한 뒤 설계를 확정한다.
3. 항목을 마치면 이 문서에 ID별 상태(`완료`·`보류`·`제외로 변경`)와 v2 commit을 기록하고, 검증 증거는 해당 담당 문서에 남긴다. v1의 테스트 통과를 v2의 검증 결과로 대신하지 않는다.
