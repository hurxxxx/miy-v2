# v1(miy) 검토와 v2 적용 계획

- 작성일: 2026-11-03 (같은 날 원문 대조 재검증·보완, §8)
- 검토 대상: 공식 v1 저장소 `/projects/miy` main `188a8cef`(2026-10-12, VERSION 0.1.1). `origin/dev`(`7d845743`, 10-14)는 main보다 API 코드가 오래됐고 Workbench와 Hermes 런타임 업그레이드(v0.21.7)만 앞서 있다. 그 차이가 판단에 영향을 주는 곳만 따로 적었다.
- 보조 조사 이력: `../ai-do/dev` `dfc7fb9e16e`(2026-11-03). 이미 구현한 기본 UI의 참조 이력이며, 이후 기능·권한·공간 설계의 기준은 miy v1이다.
- 비교 기준: v2 main `3876d43` (CLI/core 0.7.0, 번들 0.8.0)
- 상태: **검토·재검증 완료, 적용 항목은 미착수.** 착수 순서는 [추진 계획](PLAN.md)이 관리한다. 사용자 결정이 필요한 사항은 §5에 모았다.

v1은 Python API·Nx 모노레포·공유 DB 위에 공식 업무 앱 12종, Hermes 챗봇, RAG, Workbench까지 구현했다. 자체 dev/prod 환경은 운영했지만 고객 출시 전 단계다([PRD](PRD.md)). v2는 Rust 코어·앱별 컨테이너/DB·CLI/MCP 중심으로 새로 만든 후속 제품이다. 이 문서는 v1의 **계약·운영 규칙·실패 교훈**을 v2 구조에 맞춰 어떤 순서로 가져올지 정한다. v1 코드나 데이터 모델을 그대로 옮기는 계획이 아니다.

**사용자 지시에 따라 이후 적용 기준은 miy v1으로 통일한다.** 이미 완료한 기본 UI가 `../ai-do/dev`를 참고했다는 사실과 검증 증거는 [UI 이식 계획](UIUX-MIGRATION.md)에 보존한다. 후속 공용 UI·다국어·알림·편집기와 조직·그룹·권한은 v1을 검토 원본으로 삼고 v2 계약에 맞춰 구현한다. ai-do의 변경을 자동으로 채택하지 않는다.

v1 ADR 0012는 **플랫폼 워크스페이스를 제거하고 앱이 업무 컨테이너와 ACL을 소유**하도록 정했다. PMS의 공간처럼 앱 내부 데이터 분리 단위는 남아 있다. v2도 플랫폼에서 사용자·그룹별 앱 사용 권한을 관리하고, 필요한 앱이 자기 DB에서 공간·멤버·리소스 권한을 관리한다. 모든 앱에 공간이나 숨김 글로벌 공간을 강제하지 않는다. 제품 요구사항은 [PRD](PRD.md)의 앱 사용 권한·앱 내부 데이터 권한 계약이 관리한다.

v1 근거는 `miy:<경로>`, ai-do 근거는 `ai-do:<경로>`로 적는다. 각각 `/projects/miy`, `/projects/ai-do/dev`를 뜻한다. 이 문서는 2026-10-20 [RESEARCH](RESEARCH.md)에 있던 "기존 miy에서 참고할 계약" 절을 흡수했다. 당시 링크는 더 이상 없는 `../miy/dev` 경로를 가리켰다.

## 1. 결론

1. **v1에서 가져올 핵심은 기능 코드보다 경계 규칙이다.** 해당 규칙은 다음과 같다. 모두 v2의 기본 거부·명시 승인 방향과 맞는다.
   - 원본 ACL을 다시 확인한다.
   - 파생 권한을 저장하지 않는 그룹 모델을 쓴다.
   - AI 도구의 신원은 실행에 묶고, 쓰기 승인은 인자에 묶는다.
   - 자동 전환을 하지 않는다.
   - 불확실한 커밋 결과를 근거로 보상 삭제를 하지 않는다.
   - AI 없이도 업무가 동작하게 한다.
2. **v1이 폐기한 구조는 다시 들이지 않는다.** 플랫폼 전역 워크스페이스(ADR 0012로 폐기), 앱 간 공유 DB 외래키, 셸형 AI 터미널, 대리 로그인, 범용 검증 엔진, 무거운 hook·skill 체계가 그 대상이다.
3. **v2에 들어갈 자리가 없는 기술 공백은 세 가지다.** 감사 기록 조회·보존(V1-A1), 서비스 계정용 범위 제한 API 키(V1-A2), 앱이 플랫폼을 통해 LLM을 호출하는 등록 workload(V1-I7)다. [추진 계획](PLAN.md)의 작업표에 넣었다.
4. **공식 업무 앱은 "이전"이 아니라 "재구현 후 인수"다.**
   - v1 공식 앱 12종은 소유 이전과 대표 경로 검사까지 진행했다.
   - 이슈 대장에 오른 10종은 전체 업무 인수가 "검증 대기"로 남았다. Community는 "의심/미확인"이다(`miy:platform-redesign/APP_ISSUES.md`).
   - v2는 앱을 적게 골라 하나씩 인수 완료까지 진행한다.
5. **대외 소개 자료의 제품 요구 일부가 v2 PRD에 없다.**
   - v1은 회사 소개 자료(2026-10-02)를 기준으로 다음을 요구사항으로 정리했다(`miy:MIY-SOLUTION-GAP-REVIEW.md`): 앱 수명주기 기록, 비용·재사용·패치 관리, 납품 패키지, 채용 인수 사례, 웹 Studio.
   - v2 PRD에는 이 항목들이 없다. v2가 범위를 의도적으로 좁힌 것인지 확인이 필요하다.
   - 그래서 적용 항목이 아니라 결정 항목으로 두었다(§3.8, §5).

## 2. 적용 원칙 (v1이 기록한 실패에서)

| 원칙 | 근거 |
| --- | --- |
| 검색·알림·색인 후보나 숨긴 버튼은 권한이 아니다. 제목·요약·건수를 내보내기 전에 원본 ACL을 다시 확인한다 | `miy:docs/domains/notifications/README.md`, `miy:adr/0004-retrieval-rag-boundary-policy.md`, `miy:docs/product/core-platform-user-acceptance.md` |
| 그룹에서 파생된 권한은 행으로 저장하지 않고 계산한다. 저장했던 v1 PMS에서는 회수 후에도 접근이 남았다 | `miy:WORKSPACE_ACL_REVIEW.md` 41행 |
| 비동기 작업은 가져올 때, 외부 호출 전, 응답을 저장하기 전에 현재 권한을 다시 확인한다. v1 메일 작업은 정지·회수 후에도 계속됐다 | `miy:WORKSPACE_ACL_REVIEW.md` 42행 |
| 계정을 정지하면 세션 연결을 닫기 전에 권한 변경 이벤트를 먼저 보내 열린 화면을 가린다. v1은 연결을 먼저 닫아 정지된 사용자의 화면이 남았다 | `miy:WORKSPACE_ACL_REVIEW.md` 53행 |
| 코어와 업무 앱은 AI 서비스 없이 시작·동작해야 한다. AI 장애는 AI 기능만 실패시킨다. v1 운영 구성은 API·Worker가 Hermes 정상 상태에 의존했다 | `miy:MIY-SOLUTION-GAP-REVIEW.md` §5.3 |
| 자동 전환 금지: 로컬↔외부 모델, 모델 대체, Hermes 실패 대체, 다른 계정 과금으로 넘어가지 않는다. 모르는 비용은 0이 아니라 "미확인"으로 표시한다 | `miy:docs/domains/ai/gateway.md`, `miy:docs/domains/integrations/README.md` |
| 앱 간 연결은 API와 ID로만 한다. v1은 공유 DB의 교차 외래키 때문에 분리에 수십 개의 경계 문서가 필요했다 | `miy:platform-redesign/OFFICIAL_APPS.md`, `miy:platform-redesign/FILES_SOURCE_*.md` |
| 이름 변경으로 Compose 프로젝트·볼륨 이름이 바뀌면 빈 볼륨이 새로 생긴다. baseline을 교체할 때 이전 스키마를 stamp하거나 삭제하지 않는다. v1처럼 데이터를 리셋하지 않는다 | `miy:docs/domains/release/README.md` |
| smoke 통과는 전달 증거가 아니다. mock이 저장소 장애를 숨겼다. AI 기능 인수는 브라우저→실행→MCP→서비스 전체 경로로 확인한다 | `miy:docs/domains/ai/hermes.md`, `miy:docs/domains/ai/capabilities.md` |
| 지침은 행동을 안내하고, 강제할 제한은 checker·CI·hook에 둔다. 범용 검증 엔진, 자동 재시도, 과도한 skill·hook은 만들지 않는다. v1은 skill을 14개에서 7개로 줄이고 hook은 PreToolUse 패치 보호만 남겼다 | `miy:CLAUDE.md`, `miy:platform-redesign/POLICY.md`, `miy:platform-redesign/VALIDATION.md` 469행 |
| 진행 로그형 문서는 계속 커진다(`platform-redesign/VALIDATION.md` 2,423줄). 담당 문서마다 현재 상태 하나만 두고 이력은 결과 문서나 증거 파일로 보낸다 | `miy:docs/README.md`, `miy:platform-redesign/` |
| 공용 UI 패키지는 i18n 라이브러리에 의존하지 않고 번역된 문구를 props로 받는다. 서버와 SDK의 길이 제한 계산을 일치시킨다. 편집기 스키마는 한 곳에서만 정의한다 | `miy:platform-redesign/OFFICIAL_FILES_WORKSPACE.md`, `miy:platform-redesign/SDK_FILES.md`, `miy:packages/ui/src/lib/editor/schema.ts` |

## 3. 적용 항목

각 항목은 **적용**(규칙을 그대로 가져옴) 또는 **변형 적용**(v2 모델에 맞게 재설계) 중 하나로 판정했다. 우선순위는 P1(해당 단계의 선행 조건), P2(같은 단계 안에서 함께 처리), P3(필요해질 때) 순이다. 배치는 해당 항목이 속한 코어 차수나 UI 후속 번호이며, 실제 순서는 [추진 계획](PLAN.md)의 작업표를 따른다.

### 3.1 개발 절차·품질 검사

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-Q1 | 플랫폼 개발용 루트 `AGENTS.md`와 `CLAUDE.md` 연결. 커밋·push·배포 권한, CI·생성 계약·릴리스 게이트 우회 금지, 비밀 취급, 실행/미실행 검사 보고를 담는다 | `miy:AGENTS.md`, `miy:CLAUDE.md` | 없음. 생성 앱용 `templates/rust-react/AGENTS.md`만 있음 | 변형 적용: Rust 코어·CLI·운영 서버 안전 규칙 중심 | P1 |
| V1-Q2 | 변경 유형별 최소 검사 지도(Rust 코어, CLI, 웹, migration, 설치 파일, 템플릿) | `miy:docs/agents/vibe-coding-harness.md` | [CORE-TESTING](CORE-TESTING.md)·[VALIDATION](VALIDATION.md)에 흩어져 있음 | 변형 적용 | P1 |
| V1-Q3 | 웹 CI 작업(`pnpm typecheck` + `pnpm build`). `scripts/check.sh`를 CI에서 돌릴 수 있는 부분과 실행 중 스택이 필요한 부분으로 분리 | `miy:package.json`(`check:web-architecture`) | GitHub Actions는 Rust·CLI만 검사. 웹 검사는 로컬 `check.sh`에서만 실행 | 적용 | P1 |
| V1-Q4 | 버전 일치 검사: Cargo, 번들·템플릿 버전, 릴리스 증거 | `miy:scripts/check-project-version.mjs` | 릴리스 태그와 Cargo 버전만 대조 | 변형 적용 | P2 |
| V1-Q5 | 의존성·비밀 검사(`cargo deny`/`cargo audit`, secret scan) | v1 설정에도 없음 | 없음 | 신규 | P2 |
| V1-Q6 | 하드코딩 경로 검사: v1 경로, 절대 `/projects/` 경로 | `miy:scripts/check-path-hardcoding.mjs` | 없음 | 변형 적용 | P2 |
| V1-Q7 | 앱 계약에서 빠진 규칙(중복 ID, route 소유)을 Rust 단위 테스트로 추가 | `miy:scripts/app-contract-validation.mjs` | `runtime_contract.rs`·`component_contract.rs`가 대부분 담당 | 변형 적용 | P2 |
| V1-Q8 | env 키 계약: 폐기된 키를 차단하고 모든 키가 문서화됐는지 확인 | `miy:scripts/check-env-contract.py` | `scripts/check-server-config.py`가 일부 담당 | 변형 적용 | P2 |
| V1-Q9 | 앱 개발 지침 평가: 같은 모델·설정으로 기준안과 후보안을 3회 이상 반복하고, 읽은 문서가 아니라 산출물로 판정 | `miy:scripts/agent-guidance-eval.mjs` | `scripts/run-agent-acceptance.mjs`, `check-cli-guidance.mjs` | 변형 적용: 기존 인수 도구에 규칙만 추가 | P3 |

### 3.2 코어 운영 기반 (코어 3·4·5차)

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 | 배치 |
| --- | --- | --- | --- | --- | --- | --- |
| V1-O1 | Docker 저장소 사전 점검(v1 기준은 15GiB 이상이면서 15% 이상 여유), 현재·직전 롤백 이미지 보존. BuildKit 캐시 GC는 이미지 정리와 분리 | `miy:docs/domains/release/README.md` §Build and test storage, `miy:scripts/docker-storage.mjs` | [CAPACITY](CAPACITY.md)에 설계만 있음 | 변형 적용: v2 작업·복구 필요량에 맞춰 수치 확인 | P1 | 3차 중 먼저, 변경 범위의 최소 백업·복원 확인과 함께 |
| V1-O2 | DB 연결 예산: dev/prod 합계를 서버 한도 아래로 두고 `application_name`으로 구분해 관찰. 오래된 env 값이 새 기본값을 덮어쓰지 않게 함 | release README §Database connection budgets | CAPACITY 표에 한 줄 | 적용 | P2 | 3차(먼저) |
| V1-O3 | 깨지는 변경의 전환·롤백: 불변 이미지 digest, 해당 리비전의 설정 스냅샷과 0600 env 백업, prepare/deploy/rollback 상호 배제 잠금, 실패 시 이전 이미지 복구 | release README §Incompatible database and configuration cutovers, `miy:scripts/prod-app.sh` | 배포마다 쓰는 `deploy.py`의 `restore()`뿐 | 적용: 호스트 updater의 설계 원칙으로 | P1 | 4차 |
| V1-O4 | 불확실한 커밋 결과로 보상 삭제를 하지 않음. 확정된 거부 SQLSTATE만 근거로 인정하고, class 40 전체는 제외(completion_unknown 40003 포함) | `miy:apps/api/src/miy_api/core/transaction_outcome.py` | 1차에서 DB 오류를 타입별로 분류 | 적용: 파일 저장소·정리 작업에도 적용 | P2 | 4차·§7-3 |
| V1-O5 | 공개 런타임 설정 스키마: 모르는 키나 잘못된 값이면 시작 실패, 값은 출력하지 않음. 설정 migration은 0600으로 백업하고, 새 키가 남아 있으면 downgrade 차단 | release README §Public runtime configuration, `miy:scripts/runtime-config-migrate.py` | `render-server-config.py`, `runtime.env` | 변형 적용 | P2 | 4·5차 |
| V1-O6 | 호스트 진단 항목: Docker data-root 이전, 메모리·swap 제한, 클러스터 생성 전 PostgreSQL 로케일 | `miy:docs/domains/release/installation-operations.md` | 없음 | 적용 | P2 | 5차 |
| V1-O7 | 빈 설치 공개 전 점검: 격리 프로세스에서 초기 관리자 생성, 공개 최초 사용자 설정이 닫혔는지 확인, 업무 앱은 관리자가 켜고 대상을 정할 때까지 꺼진 상태로 시작 | release README(빈 배포 노출 전 절차) | `scripts/bootstrap.py` | 적용: 인수 체크 항목 | P2 | 5차 |
| V1-O8 | 검증 자원 분리: 빌드와 브라우저 검증을 같은 호스트에서 겹쳐 돌리지 않음. v1 공유 호스트에서 OOM이 늘었다 | `miy:WORKSPACE_ACL_REVIEW.md` 122행 | 없음 | 적용 | P3 | 5차 |

### 3.3 조직·그룹 권한 (UI §7-2)

§7-2의 설계 원본은 `miy:adr/0012-company-app-access-without-workspaces.md`와 `miy:docs/domains/organization/README.md`다. **플랫폼은 앱 사용 권한, 앱은 내부 데이터 권한을 관리한다.** 앱을 사용할 수 있어도 앱 내부 모든 공간·문서를 볼 수 있는 것은 아니다. 공간·프로젝트·폴더의 존재, 멤버와 역할은 앱이 소유하며, 실제 API·작업·검색·알림·AI에서도 해당 앱의 현재 권한을 확인한다. 앱 간 공유는 공간 공동 소유 대신 기존 API·설치 승인·원래 호출자 보존 계약을 사용한다.

초기 그룹 기본안도 v1을 따른다. 사용자와 단일 그룹을 권한 주체로 쓰며, 인사 출처 그룹은 현재 주 소속의 정확 일치로, 수동 그룹은 직접 배정한 사용자로 멤버십을 계산한다. 하위 부서·책임자·중첩 그룹의 권한을 자동 상속하지 않는다. 하위 부서 포함이나 부서들을 묶는 동적 그룹이 필요해지면 별도 기능으로 설계한다. 이 차이는 표시 방식이나 테이블 수만의 차이가 아니다.

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-G1 | 사용자·단일 그룹을 앱 권한 주체로 사용. 인사 주 소속 정확 일치·수동 사용자 배정, 계층·책임자·하위 부서의 자동 권한 상속 없음. 계층 변경은 잠금 후 순환 검사 | `miy:docs/domains/organization/README.md`, `miy:adr/0012-company-app-access-without-workspaces.md`, `miy:WORKSPACE_ACL_REVIEW.md` 43행 | 사용자 단위 `grants`(use/develop/deploy)만 있음 | 변형 적용: 기본 거부·기존 개발/배포 권한 유지, 파생 권한은 현재 멤버십으로 계산. 그룹 정본·동기화는 §5 | P1 |
| V1-G2 | 앱 입장 대상(전체/선택, 사용자∪그룹)과 앱별 회사 중지 스위치. 설정이 없거나 대상이 비면 거부. 관리자는 대상 선택만 우회하고 중지 스위치는 우회 못 함 | `miy:docs/domains/app-platform/README.md` §App Admission | 없음. 앱 비활성화는 [PRD](PRD.md)에 요구만 있음 | 변형 적용: "전체"는 명시적으로 선택할 때만. 입장 판정과 앱 내부 ACL을 분리하며 v1의 관리자 업무 데이터 조회 정책을 자동 도입하지 않음 | P1 |
| V1-G3 | 그룹·계정 변경 시 영향받는 사용자만 무효화하고 대기 작업을 다시 확인 | `miy:WORKSPACE_ACL_REVIEW.md` 42행 | 사용자 단위 위임 재확인은 있음([CORE-PERMISSIONS](CORE-PERMISSIONS.md)) | 적용 | P1 |
| V1-G4 | 디렉터리 선택기 DTO: 활성 대상만, 페이지 처리, 보안 필드 제외. 디렉터리 조회 가능 여부와 콘텐츠 권한은 별개. SDK와 공용 UI에 사용자·그룹 선택기 제공 | `miy:packages/platform-web/src/directory/DirectoryPicker.tsx`, organization README | `connections.rs` `directory_data`가 활성 사용자 전체의 id·이름을 반환 | 변형 적용 | P2 |
| V1-G5 | 필요 앱만 자기 DB에서 공간·멤버·역할·리소스 ACL을 관리. 플랫폼 워크스페이스 테이블·전역 선택기·필수 숨김 공간 없음. 공간 ID는 앱·환경 범위 안에서만 해석 | ADR 0012, `miy:WORKSPACE_ACL_REVIEW.md` 29~30행 | 앱별 DB는 있음. 공간 권한과 공용 선택 UI·SDK 지원은 후속 | 변형 적용: 플랫폼은 신원·그룹 조회와 재사용 UI/SDK를 제공하며 데이터 소유권은 앱에 유지 | P1 |
| V1-G6 | 실사용 인수 LIVE-01~17을 v2에 맞게 재작성. 같은 앱에 들어가는 두 사용자의 서로 다른 공간 접근, 공간 없는 앱, 그룹 회수 후 직접 URL·API·대기 작업 거부, 앱/환경 간 공간 ID 오용을 확인 | `miy:docs/product/core-platform-user-acceptance.md` | 없음 | 변형 적용: 대표 앱의 실제 서버 판정으로 인수 | P2 |

### 3.4 운영 관리 공백

| ID | 항목 | v1 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-A1 | 구조화된 감사 기록과 관리자 조회: 대상 종류·결과·요약, 기간·행위자·동작 필터, 권한 변경과 같은 트랜잭션에 기록, 보존 기간 | `miy:apps/api/src/miy_api/domains/auth/models.py`, admin `/audit-logs` | `audit` 테이블은 actor/action/target/request_id뿐. 요청 미들웨어는 `METHOD:status`로 기록하고 실패해도 로그만 남김. 위임·앱 호출 거부·상태 점검은 별도 action으로 기록. 조회 API 없음 | 변형 적용 | P2 |
| V1-A2 | 범위가 제한된 플랫폼 API 키(서비스 계정 기반): `miy_pk_` opaque 토큰, SHA-256 해시로 인증, 코드로 고정한 scope, 사용자 토큰과 섞어 쓸 수 없음, 감사 기록 | `miy:adr/0010-platform-api-key-directory-integration.md`, `miy:docs/domains/integrations/README.md` | 서비스 계정 미구현([CORE-HARDENING](CORE-HARDENING.md) 2차 한계) | 변형 적용: 다시 보여 주기 위한 암호문 저장은 제외 | P2 |
| V1-A3 | 사용량·비용 집계: dev/prod 분리, `complete:false`, 보고되지 않은 비용을 0으로 처리하지 않음 | integrations README, `miy:MIY-SOLUTION-GAP-REVIEW.md` §5.4 | 없음 | 변형 적용: V1-R2를 채택하면 그 일부로 처리 | P3 |

### 3.5 공용 UI·테마·언어·SDK (UI §7-1)

| ID | 항목 | 근거(miy v1 기준) | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-U1 | 업무 UI 원칙: 밀도 높은 화면, 평면 표, 한 줄 도구 모음, 모바일 전역 메뉴와 현재 앱 메뉴 분리. 템플릿 지침에도 넣는다 | `miy:docs/product/ui-design-principles.md` | 규칙 문서 없음 | 적용 | P1 |
| V1-U2 | 다크/시스템 테마: SDK `AppRuntime.theme`를 `"light" \| "dark"`로 확장, `--miy-*` 다크 토큰, 강조색 전경 누락 검사 | `miy:scripts/check-web-dark-mode-tokens.mjs`, `miy:packages/platform-web/src/theme/document-theme.ts` | `theme: "light"` 고정 | 변형 적용: 기존 앱·불변 UI 버전 호환 확인 | P1 |
| V1-U3 | 한국어/영어 메시지 목록, 런타임 `locale`, 하드코딩 문자열 검사 | `miy:scripts/check-web-i18n-messages.mjs`, `miy:packages/platform-web/src/i18n/resources.ts` | 한국어만 | 변형 적용 | P1 |
| V1-U4 | 공용 primitive: dialog, confirm/prompt, data-table, 빈 상태·skeleton, 검색 필드, 필터 바, 탭, 상세 drawer, split pane. 새 불변 registry 버전(1.1.x)으로 게시하고 i18n 의존성은 넣지 않음 | `miy:packages/ui/src/lib/` | registry에는 `Panel`·`Button`만 있음 | 변형 적용 | P1 |
| V1-U5 | 날짜·시간대 유틸과 한국 공휴일(정적 데이터, 지원 범위 밖이면 null) | `miy:packages/platform-web/src/time/korean-holidays.ts`, `miy:docs/product/korean-holidays.md` | 없음 | 적용 | P2 |
| V1-U6 | SDK 확장: 테마·언어 변경 알림, 다른 앱으로 이동 제안(사용자 확인 필수, 자동 이동 없음), 저장 대기 신호 | `miy:platform-redesign/SDK_NAVIGATION.md`, `miy:apps/official-suite/README.md` | 앱 내부 `navigate`, `registerNavigationGuard` | 변형 적용 | P2 |

### 3.6 공통 업무 기능 (UI §7-3)

| ID | 항목 | 근거 | v2 현재 | 판정 | 우선 |
| --- | --- | --- | --- | --- | --- |
| V1-F1 | Storage API를 먼저 만들고 그다음 파일 앱. 선택한 파일 읽기는 플랫폼 선택기, 짧은 읽기 권한, 10MiB 상한, `files:read-selected` 권한으로 제공 | `miy:platform-redesign/SDK_FILES.md`, `miy:packages/official-suite-web/src/files/` | PRD에 계획(MinIO 제외)만 있음 | 변형 적용 | P1(§7-3 첫 항목) |
| V1-F2 | 콘텐츠 전달 증표: 15분 이내, fragment로 전달, `private, no-store`, 스트리밍 직전 재확인, 전용 서명 키 | `miy:docs/domains/content-access/README.md` | 없음 | 변형 적용 | P2 |
| V1-F3 | 알림은 원본 앱 이벤트의 읽기 projection. 목록·건수·읽음 처리마다 원본 ACL을 다시 확인하고, 건수는 필터링한 뒤 계산 | `miy:docs/domains/notifications/README.md`, `miy:apps/web/src/components/layout/NotificationPanel.tsx` | 없음(UI-4에서 메뉴 제외) | 변형 적용: 앱이 발행하고 코어가 원본 앱에 재확인 | P2 |
| V1-F4 | 업로드 관리자 UX: 오른쪽 아래 고정, 탭 단위, 재개·복원 없음 | V1-U1의 UI 원칙 | 없음 | 적용(F1과 함께) | P2 |
| V1-F5 | BlockNote 편집기와 협업 codec. 스키마는 한 곳에서 정의하고 Rust 서버의 Yjs 처리 방식은 별도 결정 | `miy:packages/ui/src/lib/editor/`, `miy:scripts/blocknote-collab-codec.mjs` | 없음 | 변형 적용 | P3 |
| V1-F6 | 공식 업무 앱(PMS·문서·플래너·회의·녹음)을 독립 앱으로 재구현. 캘린더는 v1 Planner의 UI 소유 범위이며 별도 공식 앱 ID가 아님. 교차 외래키 대신 API와 링크 사용. 녹음의 시도·버전 fencing 재사용 | `miy:packages/official-suite-web/src/*`, `miy:packages/contracts/app-contracts.json`, `miy:docs/apps/recording/README.md` | 없음 | 변형 적용: 아래 범위표에 따라 한 번에 하나씩 인수까지 | P3 |
| V1-F7 | 실시간 연결: 권한 종료 코드(1008/4401/4403)를 받으면 화면을 가리고 신원 재조회, WS URL에 토큰 금지. DM은 별도 앱으로 | `miy:docs/domains/app-platform/README.md` §Revoked realtime sessions, `miy:apps/web/src/app-modules/dm` | 없음(폴링으로 회수 처리) | 변형 적용: WS를 도입할 때 | P3 |
| V1-F8 | 앱이 선언한 레코드 검색. 처음에는 앱마다 검색하고, 앱 간 projection은 원본 ACL 재확인을 갖춘 뒤 도입 | `miy:packages/app-sdk/README.md` | 없음 | 변형 적용 | P3 |

v1 공식 앱 12종의 범위는 `miy:packages/contracts/app-contracts.json`의 `official_app_ids`를 기준으로 추적한다. 적용 후보는 이미 구현했다는 뜻이 아니며, 각 앱 착수 시 업무 범위와 인수 기준을 정한다.

| 공식 앱 ID | v2 처리 |
| --- | --- |
| `files` | F1·F2·F4로 적용. Storage API 다음에 구현 |
| `pms`, `docs`, `planner`, `meeting`, `recording` | F6 적용 후보. 첫 앱과 이후 순서는 기능 의존성에 따라 결정 |
| `mail`, `community`, `whiteboard`, `video-chat` | 채택 여부 보류. 이번 재구현 완료 범위에 자동 포함하지 않음 |
| `diagrams`, `bento` | 초기 적용 제외(§4). 필요해질 때 별도 실행 출처와 권한 계약 검토 |

### 3.7 내부 AI·챗봇·RAG ([AI 계획](AI.md), PLAN 4-1·5단계)

v2에는 아직 AI 실행 코드가 없다. MCP는 개발 도구만 제공한다. 따라서 아래 항목은 기존 v2 코드를 바꾸는 것이 아니라 계획된 자리를 채운다.

| ID | 항목 | v1 근거 | 판정 | 우선 |
| --- | --- | --- | --- | --- |
| V1-I14 | AI 독립 운영: 코어·worker·업무 앱의 시작과 동작이 AI 서비스 상태에 의존하지 않음. AI 장애는 AI 기능만 실패시키고, 업무 데이터 전달 경로는 설치 구성으로 분리 확인 | `miy:MIY-SOLUTION-GAP-REVIEW.md` §5.3(Type A/B) | 적용 | P1 |
| V1-I1 | miy↔Hermes 책임 구분과 내구성 있는 실행. 대화 안에서는 오래된 실행부터 처리. v1의 전역 동시 실행 상한은 기본 10개이며 pending 대기 요청은 그 수에 포함하지 않음. v2 대기열 크기·초과 정책은 별도 결정. SSE를 잃어도 5초마다 상태를 폴링해 복구. 중지 요청은 먼저 저장한 뒤 전달 | `miy:docs/domains/ai/hermes.md`, `miy:apps/api/src/miy_api/domains/hermes/{execution,repository,models}.py` | 변형 적용(Rust) | P1 |
| V1-I2 | 실행에 묶인 도구 신원: 내부 MCP가 프로필 HMAC과 실행 ID를 요구하고 모델은 신원을 지정할 수 없음. 호출마다 계정·앱·도구 범위·원본 ACL 재확인 | `miy:docs/domains/ai/hermes.md`, `miy:ops/hermes/plugins/miy_runtime/` | 적용 | P1 |
| V1-I3 | 인자에 묶인 쓰기 승인: 쓰기 도구는 기본 숨김. 승인은 서버+도구+정규화한 인자 SHA-256에 묶이고 1회용이며 5분 후 만료. 재개 시 고정된 스냅샷을 쓰며 범위는 좁힐 수만 있고 ACL을 다시 확인 | `miy:docs/domains/ai/hermes.md`(206행), `miy:docs/domains/ai/write-policy.md`, `execution.md` | 적용 | P1 |
| V1-I4 | 앱이 AI 도구를 manifest에 명시적으로 선언. 전용 입력 스키마, 하나의 등록에서 MCP와 모델 스키마를 함께 생성. 목록에서 숨긴 도구는 실행도 차단. 앱·REST·DB 모델을 만드는 것만으로 도구가 생기지 않음 | `miy:adr/0002-mcp-capability-platform.md`, `miy:docs/domains/ai/capabilities.md` | 변형 적용(manifest 선언·코어 집행) | P1 |
| V1-I5 | 관리자 모델 연결 목록: 이름 붙은 연결마다 암호화된 키(조회 시 `has_api_key`만 반환), 활성화된 카탈로그 모델만 사용. 정책은 DB에만 두고 실패하면 닫힘. 자동 전환 없음. 처음부터 DB에 저장(v1은 env→DB 이전 단계가 필요했음) | `miy:docs/domains/ai/gateway.md`, `miy:adr/0005-registered-llm-workload.md` | 변형 적용(Codex/Claude Code 구독 어댑터 추가) | P1 |
| V1-I6 | Codex App Server 연동 규칙: 사용하는 RPC 스키마 버전을 고정하고 호환되지 않는 CLI는 거부. 결과를 모르는 turn은 thread 이력을 읽고 자동 재전송 금지. `thread/resume` 때 sandbox·cwd를 다시 지정하고 기록보다 넓은 권한은 거부. v1 dev에서도 프로토콜 버전 대응 수정이 이어졌다 | `miy:apps/codex-console-api/src/codex_console/{runtime,rpc,protocol_contract}.py` | 적용(4-1 공통 연결 실험) | P1 |
| V1-I7 | 앱이 LLM을 부를 때 쓰는 등록 workload: 안정적인 `workload_id`, 출력 상한·결과 계약·경로·감사·외부 전송 정책. 접수·검증된 업무 결과가 있어야 성공하며 모델의 완료 알림으로 대체하지 않음. 종료 시 접수·검증된 결과가 없으면 `invalid_output`. 앱은 공급자 자격증명을 갖지 않음 | ADR 0005, `miy:apps/api/src/miy_api/domains/hermes/workloads.py`, `miy:templates/independent-app/AGENTS.md.template` | 변형 적용(작은 코어 endpoint + SDK, manifest 선언). Decisions API 재도입과 별개 | P2 |
| V1-I8 | 외부 전송 데이터 정책: 처음에는 원본별 허용/차단 + 감사만, 마스킹은 후속. 차단은 실패로 닫히고 다른 경로로 돌리지 않음 | `miy:apps/api/src/miy_api/domains/ai/security_policy.py` | 변형 적용 | P2 |
| V1-I9 | Hermes 이미지 digest 고정과 업그레이드마다 smoke 실행(`check_*.py` 7종). upstream 공백 9건(main 기준, dev의 v0.21.7에서 8건)을 자체 어댑터로 메운 비용을 예산에 반영. 일부 어댑터는 비공개 API를 사용 | `miy:docs/domains/ai/hermes.md` §Pinned upstream gaps, `miy:ops/hermes/check_*.py` | 적용(코드 실행 sandbox는 P3) | P2 |
| V1-I10 | 챗봇 UX: 이벤트 순번으로 중복 제거하며 재개, 서버의 종료 이벤트를 받은 뒤 중지 표시, 한국어 IME 조합 중 Enter로 전송하지 않음, 대화 전환 뒤 늦게 온 응답 무시, 실행 중 업로드 차단 | `miy:docs/apps/chatbot/hermes-desktop/README.md`, `miy:packages/platform-web/src/chatbot/` | 적용 | P2 |
| V1-I11 | RAG 권한 규칙: 후보 파티션은 범위를 좁힐 뿐 권한을 주지 않음, 인용·모델 입력 전에 원본 ACL 확인, 백엔드 간 원점수 대신 RRF(k=60), 명시한 원본을 쓸 수 없으면 실패로 닫힘, outbox 버전 관리, 생성→검증→전환 방식의 색인 교체, 품질 코퍼스. 문서·벡터 ID namespace는 한 번 정하면 바꾸지 않음 | `miy:adr/0004-retrieval-rag-boundary-policy.md`, `miy:adr/0009-retrieval-partition-projection-generations.md`, `miy:docs/domains/{retrieval,rag}/README.md` | 변형 적용(pgvector 단일 백엔드) | P2 |
| V1-I12 | 중첩 생성 교착 방지: 도구가 부른 생성이 부모 실행 뒤에서 기다리면 즉시 실패(`nested_capacity`). 열린 SSE가 DB 연결을 잡고 있지 않게 함. 공급자 어댑터마다 무시되는 파라미터를 명시적으로 거부 | `miy:docs/domains/ai/hermes.md` | 적용 | P2 |
| V1-I13 | 임베딩·OCR·음성 인식 등 LLM 외 추론은 역할별 어댑터를 거침. 앱은 호스트·장치를 가정하지 않음 | `miy:docs/domains/inference-gateway/README.md` | 변형 적용(계약 한 줄) | P3 |

2026-10-20 조사에서 확인한 v1 경로도 유지한다.
- **챗봇 실행 경로:** `miy:packages/platform-web/src/chatbot/api/chatbot-api.ts`에서 시작한다. `miy:apps/api/src/miy_api/domains/hermes/router.py`의 `create_run`과 `miy:apps/worker/src/miy_worker/tasks/hermes.py`를 거쳐 내부 Hermes의 `/v1/runs`로 이어진다.
- **가져올 것:** 이 경로의 책임 구분이다. Python API·워커·큐 제품·런타임 버전은 가져오지 않는다.
- **구독 연결:** 구독 인증을 쓰는 Workbench(`codex app-server`, 소유자 1인)와 제품 내장 Hermes는 별도 경로다. v1에는 다중 사용자 구독 연결이나 Claude Code 구독을 Hermes 백엔드로 쓰는 구현이 없었다. v2의 공통 연결과 사용자별 세션은 [AI 계획](AI.md)에 따라 새로 설계한다.

### 3.8 대외 소개 자료의 제품 요구 (채택 여부 결정 필요)

v1은 회사 소개 자료 `MIY 소개 및 사례_20261002.pdf`를 v1 구현과 비교해 아래 공백을 정리했다(`miy:MIY-SOLUTION-GAP-REVIEW.md` §3·§5). v2 [PRD](PRD.md)에는 이 항목들이 없다. 채택하면 [추진 계획](PLAN.md)에 배치한다.

| ID | 요구 | 내용 | v2 현재 | 채택 시 배치 |
| --- | --- | --- | --- | --- |
| V1-R1 | 앱 수명주기 기록 | 앱 목적·요구사항·소유 부서·책임자, 필요한 권한·AI 기능, 검사 결과와 검토자·승인 기록, 설치 버전과 배포·복구 결과, 변경 요청·장애·패치 이력. 별도 레지스트리를 만들지 않고 기존 앱 ID·릴리스·작업 기록에 연결 | 릴리스·설치·작업·등록 승인 기록은 있음. 책임자·검토/승인 기록·패치 이력은 없음 | 2(관리 화면) 또는 신규 |
| V1-R2 | 앱 관리 센터 | 개발/운영 AI 사용량 분리, 앱·프로젝트·부서별 비용·예산·초과 알림, 개발 전 기존 앱 검색·재사용 검토(유사도는 AI가 후보만 제시하고 판단은 담당자), 문제→담당자 통지→수정→검토→배포 연결. V1-A3 포함 | 없음 | 5 이후 |
| V1-R3 | 고객 납품 패키지 | 지원 OS·버전·하드웨어 기준, 포트·통신 경로, 검증된 이미지·라이선스 고지·구성요소 목록, 인증서·키 관리, DB·파일·암호화 키 백업과 실제 복원 검사, 장애 알림·지원 책임, 폐쇄망 반입물 | 설치·백업·갱신은 코어 4·5차. 라이선스 고지·구성요소 목록·폐쇄망 반입물은 없음 | 6(코어 5차 인수) |
| V1-R4 | 대표 인수 사례: 채용 평가 | 공고·평가 기준 버전, 지원서 근거 위치·하이라이트, AI 의견과 사람 최종 평가 분리, 심사관별 권한, 서버 제출 규칙, 엑셀 출력. 합성 지원서 10건으로 기획→개발→검토→배포→심사→제출→패치까지 확인 | PRD 대표 시나리오는 자산 관리·기준정보·구매 요청 앱 | 4 이후 인수 시나리오 |
| V1-R5 | 웹 앱 개발 Studio | 현업이 자연어로 앱을 만들고 모델 선택·BYOK를 쓰는 웹 화면. 개발 사용자별 작업 공간·자격 증명 격리, 개발·검토·배포 권한 분리 | v2는 CLI와 사용자 자신의 에이전트(MCP) 방식. Studio는 PRD 범위 밖 | 결정 전 미배치 |

## 4. 적용하지 않는 것

| 대상 | 이유 |
| --- | --- |
| 플랫폼 전역 워크스페이스·테넌트 범위(ADR 0006·0007·0011) | 세 ADR 모두 "Superseded by ADR 0012"다. 이전 AI ADR(0002·0005)의 scope 정책도 0012로 대체됐으므로 가져오지 않음 |
| 공유 DB와 앱 간 외래키, 같은 트랜잭션 ACL | v2의 앱별 DB와 반대 구조이며 v1 분리 비용의 원인 |
| iframe/PKCE 기반 SDK 연결, 사용자별 범용 JSON 레코드 저장 | v2는 같은 출처의 Module Federation과 앱별 DB를 사용. 신뢰할 수 없는 제3자 앱을 받을 때 별도 origin 실행 방식을 다시 검토 |
| Workbench(1인용 웹 Codex 콘솔, worktree·GitLab MR) | v2는 사용자 자신의 Codex/Claude Code와 CLI/MCP를 사용. 연동 규칙은 V1-I6만 가져옴. 현업용 웹 Studio 요구는 V1-R5에서 따로 결정 |
| 셸형 AI 터미널(PTY) | v1이 폐기했지만 종료를 위한 broker·egress 정리 부담이 남았음 |
| 대리 로그인, 임시 비밀번호·재설정 | 대리 로그인은 v1이 폐기했으나 컬럼·감사 코드가 남았음(`WORKSPACE_ACL_REVIEW.md` 197행). 나머지는 Keycloak이 담당 |
| LangGraph 기반 실행, Decisions API, 직접 completion 우회, 사용자별 모델 선택 | 현재 v2 범위 밖. 보고서 산출물을 추가할 때 산출물 불변 규칙만 재검토 |
| OpenSearch·Qdrant 이중 검색 백엔드 | v2는 pgvector 단일 백엔드로 시작([STACK](STACK.md)) |
| GitLab MR·runner 게이트, 자체 Codex 리뷰 CI, MR 증거 스크립트 | GitLab·자체 runner에 종속되고 비용이 큼. PR 템플릿의 "실행/미실행 검사·남은 위험"만 가져옴 |
| Python 소스 무결성·alembic 상태 검사, 웹 모듈 import 경계 검사 | 기술 스택이 다름. 적용된 migration 변경은 sqlx가 checksum 불일치로 실행 시 거부함. v2 웹은 아직 작아 import 경계 검사가 필요 없음 |
| Codex `apply_patch` hook | v2의 `miy app check` 번들 무결성 검사가 더 강함. v1도 hook을 보안 경계로 보지 않음 |
| Beat heartbeat | v2 worker 소유권 감시(1차)가 대신함 |
| Bento·draw.io 기반 다이어그램 | 필요할 때 별도 origin 원칙으로 다시 검토 |

## 5. 결정이 필요한 사항

후속 설계 원본은 miy v1, 앱 입장과 앱 내부 데이터 권한은 분리하는 방향으로 정리했다. 플랫폼 공통 워크스페이스 도입 여부를 다시 선택하는 표가 아니다. 아래에는 그 방향 안에서 남은 범위를 적는다.

| # | 항목 | 현재 기준 | 남은 설계·권고 |
| --- | --- | --- | --- |
| 1 | 작업 순서와 첫 업무 앱 | [추진 계획](PLAN.md#다음-우선-작업): 변경 범위의 최소 백업·복원 확인과 디스크·DB 보호 → 사용자·그룹 앱 권한과 공용 UI → 공통 기능·업무 앱. 전체 백업 제품·호스트 updater·종합 인수는 베타 전 | 최소 보호의 완료 기준을 먼저 좁히고 각 업무 앱을 하나씩 인수한다. 전체 코어 4차를 기능 개발의 선행 조건으로 두지 않음 |
| 2 | 그룹 정본·동기화 | v1식 단일 그룹, 주 소속 정확 일치와 수동 사용자 배정. 하위 부서 자동 상속·중첩 그룹 제외 | 코어 DB를 정본으로 시작하고 Keycloak은 인증에 사용하는 안을 우선 검토. 외부 인사 연동이 필요하면 출처·동기화·회수 계약을 먼저 정의. ai-do식 동적 조직 묶음은 자동 도입하지 않음 |
| 3 | 앱 내부 공간의 상세 계약 | 공간이 필요한 앱만 자체 DB·API에서 데이터·멤버·역할을 관리. 플랫폼은 사용자·그룹 조회와 공용 UI/SDK를 제공 | 대표 앱에서 생성·조회·공유·마지막 소유자 보호·권한 회수·데이터 보존을 정한다. 공용 선택 UI를 재사용해도 공간 소유권은 앱에 유지 |
| 4 | 대외 자료 요구와 보류 앱 | V1-R1~R5 및 §3.6의 보류 앱은 채택 / 보류 / 범위 밖으로 구분 | R1(수명주기 기록)·R3(납품 패키지)은 기존 v2 방향과 맞아 채택을 권고. R2·R4·R5와 보류 앱은 제품 범위 결정이 필요. 특히 R5 웹 Studio는 별도 결정 |
| 5 | Hermes 채택 유지 | PRD 합의대로 유지 / 4-1 실험 후 재검토 | 유지하되 4-1 실험에서 통합 비용(upstream 공백 9건, 버전마다 재검증)과 v2 Rust 코어와의 경계를 측정한 뒤 확정 |

## 6. 진행 방법

1. 착수 순서는 [추진 계획](PLAN.md)의 작업표가 관리한다. 이 문서는 항목 ID와 근거만 관리한다.
2. 각 구현 묶음은 시작할 때 해당 ID의 v1 근거와 현재 v2 구현을 다시 읽고 설계를 확정한다. 과거 ai-do 참조는 구현 이력으로 보존하며 후속 설계 정본으로 사용하지 않는다.
3. 항목을 마치면 이 문서에 ID별 상태(`완료`·`보류`·`제외로 변경`)와 v2 commit을 기록하고, 검증 증거는 해당 담당 문서에 남긴다. v1의 테스트 통과를 v2의 검증 결과로 대신하지 않는다.

## 7. 적용 상태

완료한 항목만 적는다. 표에 없는 항목은 미착수다. 검증 증거는 담당 문서에 있다.

| ID | 상태 | 반영 | 확인 |
| --- | --- | --- | --- |
| V1-Q1 | 완료 | 루트 `AGENTS.md`, `CLAUDE.md` | 운영 경계·비밀·Git·검증 보고 규칙 |
| V1-Q2 | 완료 | [변경별 검사 지도](CHECKS.md) | 각 명령의 전제(Docker·로컬 스택·이전 CLI)를 스크립트에서 확인 |
| V1-Q3 | 완료(GitHub 실행은 push 후) | `scripts/check-static.sh`, `.github/workflows/web.yml`, `check.sh` origin 가드, 루트 Playwright 설정 축소 | 로컬에서 `scripts`·`rust`·`web` 모두 PASS. `check.sh`가 운영 `.local`을 거부함을 확인 |

## 8. 재검증 기록 (2026-11-03)

작성 직후 v1·ai-do·v2 원문을 대조했고, 후속 리뷰와 사용자 지시에 따라 적용 기준을 miy v1으로 바로잡았다. 아래는 문서 검토 결과이며 기능 구현이나 운영 반영을 뜻하지 않는다.

- **확인한 것:**
  - 인용한 v1 경로 50여 개가 모두 존재한다.
  - 수치: v1 저장 공간 기준 15GiB·15%, 쓰기 승인 5분·인자 SHA-256·1회, 전역 동시 실행 상한 10(pending 제외), 5초 폴링, 파일 10MiB, 콘텐츠 증표 15분, 종료 코드 1008/4401/4403, class 40 제외, RRF k=60, skill 14→7, `VALIDATION.md` 2,423줄
  - 권한 사례: `WORKSPACE_ACL_REVIEW.md` 29·30·41·42·43·53·122·197행
  - ADR 상태(0006·0007·0011은 0012로 대체), 빈 설치 공개 전 절차
  - v2 현재 상태: grants, 감사 기록, 디렉터리, SDK, registry, AI 코드 부재, 검사 스크립트
- **바로잡은 것:**
  - Hermes upstream 공백 수: 8 → main 9건
  - 공식 앱 인수 상태 표현: 12종 중 대장에 오른 10종
  - v1 출시 상태 표현
  - 계정 정지 시 이벤트 순서
  - DB 연결 예산의 SQLAlchemy 전용 표현
  - migration 수정 금지 검사가 필요 없는 이유(sqlx가 거부)
- **보완한 것:**
  - 후속 UI·기능·권한 설계의 원본은 miy v1로 통일. ai-do 기본 UI 참조 이력은 보존
  - 플랫폼 앱 사용 권한과 앱 내부의 선택적 공간·리소스 권한 분리, 그룹의 정확 일치·상속 제외 명시
  - 공식 앱 12종의 적용 후보·보류·제외 범위, AI 결과 성공 판정 계약
  - 대외 자료 제품 요구(§3.8)
  - AI 독립 운영(V1-I14)
  - `origin/dev`와의 차이
