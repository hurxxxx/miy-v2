# AI-DO 공통 기능의 miy-v2 마이그레이션 대상

- 조사일: 2026-11-14
- 상태: 소스 조사·대상 분류. 이 문서 작성으로 기능 구현이나 데이터 이전을 완료한 것은 아니다.
- 보완(2026-11-14): AI-DO 플랫폼 모듈·API 도메인·관리 콘솔을 전수 대조해 빠진 대상(S12·S13·U14·C12~C14), 관리 콘솔 대응(§4.5), 프레임 CSP 제약과 글꼴 사실을 추가했다(§8 보완 조사).
- 재검토(2026-11-14): 인쇄/내보내기·클립보드·댓글·공통 API 처리(U15~U17·C15)를 추가해 대상은 46개다. 앱 간 화면 이동, 개인 도킹 도구, 파일 전송, 통합 일정, 관리 기능과 프레임 제약을 실제 코드에 맞춰 보완했다. 기존 추가 항목은 유지한다.
- 목적: AI-DO의 익숙한 화면과 사용 흐름을 유지하면서, 여러 업무 앱이 함께 사용할 기능을 miy-v2의 플랫폼·SDK·공용 UI로 옮긴다.

**AI-DO의 화면·상호작용과 재사용 가능한 코드를 가져오고, 인증·API·저장소 연결은 miy-v2 구조에 맞춘다.** 공통 기능은 코어 서버 기능만을 뜻하지 않는다. 로그인·앱 목록은 플랫폼이, 버튼·표·선택기는 공용 UI가, 앱 내부 공간·문서·업무 상태는 각 앱이 맡는다. PLM·PMS·PDM·MES 같은 업무 앱 전체를 코어에 넣지 않는다.

이 문서에 이식 대상, 실제 소스, 현재 v2의 공백, 이식 경계와 인수 기준을 모았다. 제품·권한·실행 계약을 새로 대체하지 않으며 착수 순서는 [PLAN](PLAN.md#다음-우선-작업) 하나를 따른다. 기존 `V1-*` 번호는 작업 연결에 유지하되, **이번 공통 기능·UX의 직접 조사 원본은 사용자가 지정한 AI-DO**다. miy v1의 운영·권한 교훈과 AI-DO의 최신 화면을 같은 리비전으로 취급하지 않는다.

## 1. 기준 소스와 조사 범위

| 구분 | 확인한 기준 | 해석 |
| --- | --- | --- |
| AI-DO 최신 개발 코드 | `../ai-do/dev`, dev `3c7b5d05e1f3f094887511d368db9676494b67a4` | 이번 기능·UX 목록의 직접 원본. 웹 플랫폼·앱 모듈, UI/core-web/contracts 패키지, API 등록·서비스, worker와 관련 테스트를 조사. 보완 조사 시점의 dev는 `822c562702d`로 4커밋 앞섰으나 앱 공간 운영 이전 스크립트·Files 검색 재처리뿐이라 이 목록의 대상과 행 번호에 영향이 없다 |
| AI-DO 운영 checkout | `../ai-do/prod`, main `9178237172df6c9c6861cf0a558a9055a0585ac6` | dev와의 소스 차이 확인용. 실제 서비스 화면·배포 상태를 이번에 검증한 것은 아님 |
| AI-DO 루트 계획 | dev의 `APP_FIRST_WORKSPACE_CONTEXT_PLAN.md`에 기존 미커밋 수정 있음 | 변경하지 않았다. 문서의 “구현 전” 표기보다 현재 라우터·서비스·UI·테스트를 우선해 상태 판정 |
| miy-v2 | main `9c7b4ba2f4779a42a4afd4cbaf245b764255db8a` | 이식 여부를 비교한 소스. 운영 배포 SHA나 게시 번들의 동작을 대신하는 기준은 아님 |

소스 표기는 다음과 같다. `:숫자`는 위 리비전의 시작 행이며 파일 전체의 구현 완료를 의미하지 않는다. 외부 저장소를 이 저장소의 상대 Markdown 링크로 연결하지 않고 경로를 그대로 적었다.

| 접두어 | 기준 경로 |
| --- | --- |
| `web:` | AI-DO `apps/web/src/` |
| `ui:` | AI-DO `packages/ui/` |
| `core-web:` | AI-DO `packages/core-web/src/` |
| `api:` | AI-DO `apps/api/src/ai_do_api/` |
| `ai-do:` | AI-DO dev 루트. contracts·worker·테스트 등에 사용 |
| `v2:` | 이 저장소 루트 |

최신 dev에는 **앱 → 그 앱의 공간 → 업무 자원** 진입과 공간 생성·멤버·보관/복원이 이미 있다. `web:components/layout/AppSpaceBar.tsx:10`, `core-web:app-runtime.ts:41`, `api:api_registry.py:316`에서 확인된다. prod와 비교하면 AppSpaceBar·앱 중심 라우팅이 추가됐고 기존 앱/모바일 메뉴가 새 runtime에 연결됐다. 따라서 과거 운영의 전역 워크스페이스 선택기를 그대로 재현하지 않고, 최신 dev의 앱 중심 흐름과 기존 화면 표현을 이식 기준으로 삼는다.

최신 AI-DO도 앱 접근 판정에서 플랫폼 관리자를 자동으로 데이터 사용자로 취급하지 않는다(`api:domains/auth/app_access.py:261`). 반면 조직 하위 포함, 공유 DB의 공간 테이블, 숨김 `app_global` 등은 v2와 다르다. 최신 코드에 있다는 이유만으로 이 데이터·권한 모델까지 채택하지 않는다.

## 2. 보존할 UX와 기능 소유자

### UX 보존 기준

| 보존할 것 | 이식 기준 |
| --- | --- |
| 화면의 인상 | Pretendard 계열 글꼴, 평면 목록, 밀도·간격·타이포그래피·상태 색·아이콘 배치. AI-DO `ui:styles.css:1`과 `web:index.css:1`의 토큰 및 Tailwind 매핑을 함께 비교. 원본은 Pretendard 글꼴 파일을 포함하지 않고 이름만 지정한다(`ui:styles.css:3`). 사용자 PC에 설치된 경우에만 적용되므로, 같은 인상을 보장하려면 OFL 글꼴을 번들에 포함할지 정한다. 프레임은 자기 출처 글꼴만 쓴다(§5) |
| 탐색 위치와 동작 | 데스크톱 64px 앱 레일, 현재 앱 강조, 즐겨찾기 순서, 앱 내부 서브메뉴. 서브메뉴 기본 240px·180~480px 조절·고정/미리보기 패턴은 `core-web:sub-sidebar-frame.ts:3` 기준 |
| 모바일 사용 흐름 | 전역 앱 선택과 현재 앱 메뉴를 구분한 드로어, 선택 뒤 닫힘, 검색·알림 접근, 포커스 복귀. 레일을 단순 축소한 화면으로 대체하지 않음 |
| 작업의 연속성 | 직접 링크·뒤로/앞으로 가기, query/hash, 미저장 입력 보호, 저장·오류·재시도 상태, 업로드와 작업 진행 표시 |
| 개인화 | 라이트/다크/시스템 테마, 한국어/영어, 시간대·날짜 형식, 앱 순서·즐겨찾기. 계정 설정과 앱별 공간 선호를 구분 |
| 익숙한 공통 조작 | 확인·이름 입력·선택 칩·모달·상세 drawer·표의 키보드 동작·이미지 확대·파일명 다운로드 |

새 디자인 시스템을 만드는 작업으로 확대하지 않는다. 다만 인증은 Keycloak/코어에 연결하고, 업무 앱 모달은 프레임 내부에서 열린다. 포털↔콘솔·prod↔dev 링크는 기존 v2 결정대로 새 탭, 포털 안 앱 이동은 같은 탭이다. 이 차이를 AI-DO의 같은 React 페이지 동작으로 되돌리지 않는다. 화면의 픽셀 단위 동등성은 아래 인수 과정에서 확인한다.

### 책임 배치

| 소유자 | 맡을 기능 | 맡지 않는 것 |
| --- | --- | --- |
| 코어·플랫폼 API | 인증 문맥, 사용자·그룹·앱 접근, 개인 설정, 공통 Storage·알림·연결의 제한된 계약 | 모든 앱의 공간·업무 레코드·도메인 권한을 하나의 DB로 통합 |
| 포털 셸·관리 콘솔 | 앱 런처, 계정 설정, 전역 알림/작업 표시, 관리 화면. 콘솔은 관리 데이터만 표시 | 업무 앱의 React 코드·활성 HTML을 관리 페이지에 직접 실행 |
| SDK·프레임 런타임 | 인증된 자기 앱 API·승인된 연결, 경로·입력 상태·테마/언어 전달, 후속 선택기 계약 | 부모 DOM·쿠키·포털 토큰 공유, 임의 API를 대행하는 메시지 |
| 버전 공용 UI | 디자인 토큰, 폼·표·피드백·선택기·앱 메뉴·편집기 등의 표현과 순수 상태 모델 | 화면 선택값으로 권한 결정, 특정 업무 앱 DB나 플랫폼 내부 모듈 직접 import |
| 업무 앱 | 선택적 공간·멤버·자원 ACL, 문서·프로젝트·폴더·업무 규칙, 검색/알림/작업 provider | 다른 앱 DB 직접 조회, 다른 앱의 자원 권한 자동 상속 |

공통 사이드바·브레드크럼브·공간 선택기는 **앱 프레임 안에서 같은 UI 패키지로 렌더링하는 것을 우선안**으로 삼는다. 앱 코드의 ReactNode·함수 callback을 부모 셸로 넘기지 않는다. 포털 전체를 덮는 선택기가 필요한 기능은 별도의 제한된 메시지·결과 계약을 추가한다. 현재 프레임에 그 계약이 이미 있다고 가정하지 않는다.

## 3. 현재 miy-v2에서 재사용할 기반

| 현재 범위 | 소스 근거 | 이번 이식에서 남은 부분 |
| --- | --- | --- |
| 로그인·세션 상태, 앱 레일·카드, 모바일 메뉴 | `v2:web/platform/src/layout/Shell.tsx:15`, `v2:web/platform/src/pages/Login.tsx:8`, `v2:web/platform/src/pages/Home.tsx:10` | 기본 UI-1~5 기반을 보완. AI-DO 전체 개인 설정·즐겨찾기·서브메뉴까지 이식된 상태는 아님 |
| 플랫폼 내부 UI | `v2:web/platform/src/ui/index.tsx:18` | Button·Panel·Badge·상태 표시·확인 모달 등 있음. 앱이 사용할 공용 패키지와 구분 |
| 게시 공용 UI | `v2:registry/components/ui/1.0.1/index.tsx:2` | Panel·Button만 있음. 플랫폼 내부 컴포넌트를 공용 UI 완료로 세지 않음 |
| 설치별 프레임·SDK·탐색·입력 보호 | `v2:packages/sdk/src/index.ts:27`, `v2:web/frame-runtime/src/main.tsx:322`, `v2:web/platform/src/app/AppFrame.tsx:161` | `api`·`apiBlob`·`appApi`·directory·navigate·setNavigationState 기반 재사용. `theme`은 light 고정, locale·공통 선택기·업무 실시간 계약은 후속 |
| 사용자·그룹 앱 접근과 최소 디렉터리 | `v2:crates/miy/src/policy.rs`, `v2:web/platform/src/pages/AdminAccess.tsx:79`, `v2:crates/miy/src/connections.rs:48` | 활성 사용자 id/name 전체 목록까지. 앱용 그룹·현재 멤버십·페이지 처리·검색·공통 선택기는 G4 확장 |
| 관리 작업·기본 감사 | `v2:web/platform/src/pages/AdminAudit.tsx`, `v2:web/platform/src/pages/ManagedApps.tsx` | 배포 작업과 관리 감사는 있음. 직원의 파일 변환·문서 작업센터와 앱 업무 활동 이력은 별도 |
| 파일·업무 서비스 | `v2:packages/sdk/src/index.ts:37`, `v2:crates/miy/src/app_response.rs:14`, `v2:crates/miy/src/runtime.rs:1081` | 자기 앱의 작은 바이너리 응답은 가능. 요청도 4MiB·본문 읽기 15초 제한이며 응답은 4MiB까지 버퍼링한다. apiBlob는 전체 Blob만 반환하므로 대용량 전송·업로드 진행률·파일명 응답 metadata 계약이 아니다. 공통 ACL·Storage·알림·검색·협업·내장 AI는 후속 |

아래 표의 **기반 있음**은 그 기반만을, **부분**은 대상 UX 중 일부만을 뜻한다. **없음**은 조사한 소스에 해당 공통 제품 기능이 없다는 뜻이며 개별 업무 앱에 비슷한 구현이 있을 가능성까지 부정하지 않는다. `S/U/C/A` 번호는 이 문서의 대상 식별자이고 새 우선순위가 아니다.

## 4. 마이그레이션 대상 목록

### 4.1 포털·계정·앱 탐색

| ID·대상 | AI-DO 구현과 유지할 경험 | v2 상태와 이식 위치 |
| --- | --- | --- |
| S01 로그인·복귀·인증 오류 | `web:platform/auth/login-screen.tsx:59`, `web:platform/auth/login-route.tsx:27`. 로딩/인증 실패/외부 인증 장애를 구분하고 원래 경로로 복귀 | **기반 있음.** 포털·IdP 화면 연결을 보완. 비밀번호 직접 인증·localStorage 토큰 저장은 복사하지 않음 |
| S02 프로필·내 설정·보안 세션 | `web:platform/auth/ProfileSettingsSection.tsx:16`, `web:platform/auth/SecuritySettingsSection.tsx:112`, `api:domains/auth/router.py:872`. 표시명·직함·읽기 전용 ID, 설정 구분, 저장 피드백, 본인 세션 철회 | **부분.** 사용자 표시·CLI 세션은 있으나 통합 개인 설정 없음. 플랫폼 프로필 API·UI로 구현하고 비밀번호는 IdP, 웹/CLI/위임 연결은 종류별로 표시 |
| S03 앱 런처·카테고리·즐겨찾기 | `web:components/layout/AppBarDesktopRail.tsx:175`, `web:components/layout/useAppBarController.ts:397`. 허용 앱만 표시, 고정·순서 편집·초기화, 현재 앱 강조 | **부분.** 앱 목록·64px 레일은 재사용. 설치 metadata와 사용자 선호로 확장하며 하드코딩 앱 registry를 이식하지 않음. U1 |
| S04 서브메뉴·동적 트리·생성 버튼 | `web:components/layout/SubSidebar.tsx:104`, `web:app/shell/sidebar-types.ts:18`, `core-web:sub-sidebar-frame.ts:3`. 폭 조절·접기·고정·생성 메뉴·선택 강조 | **없음(범용 앱 메뉴).** 공용 UI를 앱 프레임 내부에서 사용. 메뉴 데이터·생성 동작·업무 트리는 앱 제공. U1·U4 |
| S05 페이지 헤더·브레드크럼브 | `ui:src/lib/layout/topbar.tsx:3`. 제목·설명·행동 버튼·breadcrumb 슬롯 | **부분.** v2 페이지 제목은 있으나 범용 breadcrumb 없음. 공용 헤더와 앱의 경로/자원 모델을 연결. AI-DO에도 자동 계층 생성 서비스가 있는 것은 아님. U4·U6 |
| S06 경로·딥링크·제목·입력 보호 | `core-web:app-runtime.ts:41`, `web:app/shell/document-title-model.ts:17`, `web:app/shell/app-runtime-route-elements.tsx:140`. 앱/공간/자원 진입, query/hash, 공간 변경 시 이전 자원 경로 제거. 다른 앱 메뉴는 대상 앱 경로를 다시 해석하며 현재 공간은 같은 앱에서만 유지 | **부분.** 자기 앱 navigate·location·setNavigationState는 있음. 다른 앱의 화면/자원 열기는 별도 U6 계약이며 appApi 호출과 다르다. 대상 앱/설치·상대 경로 descriptor, 사용자 확인·현재 접근 재검사로 연결하고 다른 앱에 현재 공간 ID를 자동 전달하지 않음. `/w/...` legacy adapter와 전역 workspace API는 제외 |
| S07 앱별 공간 선택·관리 | `web:components/layout/AppSpaceBar.tsx:10`, `web:platform/app-runtime/AppSpaceSelection.tsx:56`, `web:platform/app-runtime/AppSpaceManagementDialog.tsx:88`. 공간 진입형 앱은 선호/단일 공간 입장, 개요형은 개요 유지·공간에서 개요 복귀. 여러 공간 선택·추가 조회, 생성·멤버·보관/복원 | **없음.** 선택적 공용 UI/템플릿으로 추출하고 데이터·현재 권한은 앱 DB/API가 소유. 공간 없는 앱에 숨김 공간을 만들지 않음. G5·G6 |
| S08 테마 설정 | `web:platform/auth/AppearanceSettingsSection.tsx:17`, `web:app/shell/shell-ui-model.ts:13`. 시스템/라이트/다크·즉시 반영·저장 | **부분.** 라이트 토큰과 theme.changed 기반만 있음. 개인 선호·공용 다크 토큰·SDK 상태를 함께 확장. U2 |
| S09 언어·시간대·날짜 형식 | `web:platform/i18n/locales.ts:10`, `web:platform/i18n/locale-session.ts:108`, `web:platform/time/date-format-preference-store.ts:7`. 한국어/영어·재로그인 후 유지·document lang·일관된 날짜 표시 | **없음(공통 설정).** 플랫폼 선호와 SDK 초기값/변경 이벤트, 공용 포맷 유틸로 이식. 앱 번역은 앱 소유. 브라우저 저장소를 프레임 간 공유하지 않음. U3·U5·U6 |
| S10 모바일 전역/앱 메뉴 | `web:components/layout/AppBarMobileHeader.tsx:43`, `web:app/shell/MobileAppMenuDrawer.tsx:65`, `core-web:mobile-shell-menu.ts:35`. 메뉴 두 종류·경로 변경 후 닫힘·검색/알림 진입 | **부분.** v2 모바일 레일 메뉴를 유지하고 앱 메뉴는 앱 내부 공용 UI로 연결. 프레임 간 포커스·스크롤은 별도 인수. U1·U4 |
| S11 홈·빠른 실행·도움말·새 소식 | `web:app-modules/home/views/WorkspaceHomeView/WorkspaceHomeView.tsx:187`, `web:platform/auth/ReleaseNotesSettingsSection.tsx:80`, `web:app/shell/HelpCenterPage.tsx:24`. 공지는 관리자가 작성해 홈 게시판에 표시한다(`api:domains/announcements/router.py:31`, `web:app-modules/home/views/WorkspaceHomeView/AnnouncementsBoard.tsx:313`). 익숙한 카드 배열·공지·나중에 보기·도움말 | **부분/후보.** 빠른 앱 진입은 포털, 내 업무·최근 문서 집계는 앱 provider 조합. PMS 전용 도움말 연결·대시보드 업무 SQL은 코어로 복사하지 않음. 공통 공지/도움말 상세 범위는 후속 UI에서 확정 |
| S12 배포 후 오래된 화면 복구 | `web:platform/deployment/stale-asset-reload.ts:82`, `web:platform/deployment/client-build-guard.ts:103`, `web:platform/deployment/client-build-guard.ts:194`. 청크/빌드 불일치에 즉시 캐시 무효화 query를 붙여 새로고침하고 60초 안 중복만 억제. fetch·XHR·WS 종료 4409도 처리 | **부분.** v2는 이전 포털 자산 보존, 앱 릴리스 변경 안내(`v2:web/platform/src/app/AppView.tsx:150`)와 dirty/unknown 입력 보호가 이미 있다. 포털·프레임 런타임 build 변경 안내는 별도 후속. 원본의 즉시 새로고침을 복사하지 않고 미저장/미확인 입력을 먼저 보호 |
| S13 개인 할 일·메모·도킹 도구 | `web:platform/personal-widgets/PersonalWidgetHost.tsx:484`, `web:platform/personal-widgets/PersonalWidgetHost.tsx:538`, `web:app/shell/personal-widget-registry.tsx:80`, `api:domains/personal_widgets/router.py:43`. 본인 todo/memo 저장, 메모 800ms 자동저장, 접힘/패널/전체화면, DM·오늘 일정·PMS 패널. 할 일→PMS 업무 생성 후 원본 삭제 확인 | **없음(후보).** 개인 도구 데이터의 소유자(플랫폼 개인 기능/별도 앱)를 정하고 단순 설정값·업무 집계와 구분. 도킹 상태·배지는 공통 후보, DM/일정/업무 콘텐츠·작업 전환은 앱 adapter. 부모에 React 패널·window event를 직접 이식하지 않고 프레임 수명·입력 보호에 맞춤. S11과 함께 범위 확정 |

### 4.2 여러 앱이 공유할 UI와 표현 모델

| ID·대상 | AI-DO 구현과 유지할 경험 | v2 상태와 이식 위치 |
| --- | --- | --- |
| U01 디자인 토큰·기본 입력 | `ui:styles.css:1`, `ui:src/index.ts:1`, `web:index.css:1`. 버튼·입력·배지·탭·툴팁·드롭다운·스크롤·상태 색 | **부분.** 기본 셸 토큰·공용 Panel/Button 위에 새 불변 UI 버전으로 추가. CSS 변수와 Tailwind 매핑·폰트 자산을 함께 이식. V1-U1·U2·U4 |
| U02 Dialog·Drawer·SplitPane·폼 | `ui:src/lib/primitives/dialog.tsx:14`, `ui:src/lib/layout/detail-drawer.tsx:13`, `ui:src/lib/layout/split-pane.tsx`, `web:components/form/FormDialog.tsx:9`. 크기·embedded·중첩 팝오버·포커스 복귀·저장 중 상태 | **부분.** 플랫폼 내부 확인 모달과 분리된 앱용 UI로 제공. 부모 document.body에 렌더링하지 않음. V1-U4 |
| U03 확인·이름 입력·피드백 | `ui:src/lib/feedback/confirm-dialog.tsx:106`, `ui:src/lib/feedback/prompt-dialog.tsx:83`, `ui:src/lib/feedback/feedback-provider.tsx:122`, `ui:src/lib/data-display/content-state.tsx:7`, `web:components/common/NoAccessNotice.tsx:22`. 비동기 confirm/prompt, 토스트·폼 오류·빈 상태·skeleton·재시도, 권한 없음 안내 | **부분.** 내부 UI 일부만 있음. 앱용 provider·순수 상태 모델 추출. 전역 알림함과 일시적 피드백을 구분. v2 앱 화면은 없음과 권한 없음을 한 문구로 보여 주므로, 구분할 때도 존재 여부를 노출하지 않는 범위를 정한다. V1-U4 |
| U04 표·정렬·행 선택 | `ui:src/lib/data-display/data-table.tsx:20`, 사용 예 `web:app-modules/management-tasks/views/HealthCheckupView.tsx:532`. 밀도·정렬·단일 선택·키보드·720px 이하 카드 전환 | **없음(공용 표).** UI 패키지로 추출. 원본에는 서버 페이지 처리·다중 선택·가상 스크롤이 내장돼 있지 않음. V1-U4 |
| U05 필터·페이지 처리·대량 선택·트리 | `ui:src/lib/primitives/filter-bar.tsx:5`, `web:app-modules/pms/views/filter-bar-model.ts:14`, `web:app-modules/files/folder-tree-model.ts:15`, `web:app-modules/management-tasks/views/HealthCheckupPagination.tsx:8`. 선택·접기·현재 경로·결과 범위 표시 | **없음(통합 공용 계약).** 앱별 구현에서 표현·상태만 추출하는 추가 공통화. PMS 업무 상태·Files bulk API·Docs 직접 import는 제외. V1-U4 확장 후보 |
| U06 사용자·그룹 선택기 | `web:platform/users/UserSearchMultiSelect.tsx:25`, `web:platform/users/user-option-picker-model.ts:67`. 검색·선택 칩·본인 우선·잠긴 사용자·복수 선택 | **기반 있음.** G4 directory에 연결하는 UI. 기존 후보 속성 중 실제 제공할 이름·부서 등 공개 DTO를 정하고 대상 앱의 초대 가능 조건을 함께 검사. 그룹 선택까지 기존 사용자 컴포넌트에 구현된 것으로 세지 않음. G4 |
| U07 자원·아이콘 선택기 | `web:components/picker/ResourcePickerDialog.tsx:13`, `web:platform/pickers/resource-picker-session.ts:62`, `web:components/picker/IconPickerDialog.tsx:21`. 검색·로딩·거부·선택 중 중복 방지·오래된 응답 무시 | **없음.** 공통 UI+앱별 provider. 자원 선택과 파일 읽기 권한은 다른 계약이며 포털 소유 선택기가 필요하면 브리지 확장. G4·F1·U6 |
| U08 날짜 입력·달력·공휴일 | `web:components/date/DateInput.tsx:47`, `web:components/calendar/UnifiedCalendar.tsx:79`, `web:lib/korean-holidays.ts:18`. 직접 입력·팝오버·월/주/일/목록·시간대·휴일 마커 | **없음.** 날짜 유틸/입력과 무거운 달력 모듈을 분리. 원본 공휴일은 2018~2027 범위이며 지원 밖을 휴일 없음으로 오인하지 않음. HR 휴일 CRUD·연차 계산은 제외. V1-U5, 달력은 후속 선택 UI |
| U09 차트·지표 | `ui:src/lib/data-display/charts.tsx:36`, `ui:src/lib/data-display/chart-data.ts`. line/bar/donut/composed·복수 Y축·null gap·로딩/빈 상태 | **없음.** 선택 import 가능한 공용 UI. 특허·품질·건강검진 지표의 계산은 앱 소유. V1-U4 확장 후보 |
| U10 파일 전송·첨부 UI | `web:app-modules/files/file-upload-provider.tsx:50`, `web:app-modules/files/file-upload-session.ts:24`, `web:platform/media/use-media-upload.ts:14`, `web:platform/browser/browser-download.ts:46`. 순차 업로드·개별/전체 진행률·접기·실패·화면 종료 시 요청 중단·파일명 다운로드. 사용자 취소 버튼은 원본에 없음 | **기반만 있음.** apiBlob은 재사용하되 공통 전송은 Storage에 연결. token/workspaceSlug/folderId·task/docs 전용 첨부 결합 교체. 사용자 취소와 업로드/첨부 연결 실패 표현은 보완 목표. F1·F2·F4 |
| U11 이미지 미리보기 | `web:components/media/FullscreenImageDialog.tsx:27`, `web:app-modules/files/views/FileImagePreviewDialog.tsx:21`. Esc·제목·확대/축소/리셋·스크롤 | **없음(공용 뷰어).** 앱 프레임 안에서 렌더링하며 인증된 파일 핸들과 연결. 원본을 PDF/Office 미리보기 엔진으로 분류하지 않음. F1 연계 |
| U12 블록 편집·보기·짧은 입력·멘션 | `ui:src/lib/editor/block-editor.tsx:13`, `ui:src/lib/editor/block-viewer.tsx:12`, `ui:src/lib/editor/block-editor-mini.tsx:13`, `ui:src/lib/editor/schema.ts:8`, `ui:src/lib/editor/custom-inline/task-ref.tsx:3`, `ui:src/lib/editor/block-document-codec.ts:37`. 동일 블록 표현·업로드 callback·짧은 입력, Docs가 사용하는 블록↔Markdown 가져오기/내보내기 | **없음.** editor 별도 모듈로 추출. Markdown 변환은 lossy이므로 모든 사용자 블록/멘션의 무손실 백업으로 취급하지 않음. 참조의 추천·삽입·이동·알림은 adapter로 보완. 문서 트리·ACL·버전은 앱 소유. 복사/붙여넣기는 U16, F5 |
| U13 Markdown·타입별 산출물 표시 | `web:components/artifacts/MarkdownContent.tsx:1`, `web:components/artifacts/DocumentArtifact.tsx:8`, `web:app-modules/chatbot/views/chat/ArtifactPanel.tsx:232`. GFM 표·수식·코드 강조·근거 링크. HTML은 미리보기/소스 탭과 srcDoc, SVG는 정화 후 React 노드로 렌더링 | **없음.** 선택 UI 모듈 후보. Markdown의 HTML 비활성 조건을 유지. HTML/SVG는 별도 표시 계약이며 현재 중첩 iframe 제약(§5)으로 원본 HTML 미리보기의 지원을 가정하지 않음. 포털에 활성 콘텐츠를 직접 렌더링하지 않음. F5/AI 후속 |
| U14 공통 유틸 | `web:platform/format/byte-size.ts:1`, `web:platform/ordering/ordered-reorder.ts:1`, `web:platform/network/request-concurrency.ts:1`. 용량 표시, 순서 편집(앱 순서·즐겨찾기), 대량 작업의 동시 요청 제한 | **없음.** 테스트와 함께 순수 함수로 추출한다. 동시 요청 제한은 프레임의 자기 앱 API에만 쓰고 서버 한도(응답 4MiB·동시 읽기)를 대신하지 않는다. V1-U4 |
| U15 인쇄·보고서/표 내보내기 | `web:app-modules/claim-analysis/model/claim-print.ts:129`, `web:app-modules/perf-test/views/CompareReportDownload.tsx:23`, `api:domains/pms/router.py:2750`, `api:domains/claim_analysis/export.py:74`. 인쇄/PDF 저장 진입, 그림 포함 HTML 다운로드, 현재 필터를 반영한 XLSX/CSV와 안전한 셀 처리 | **없음(공용 계약).** 형식 선택·진행/실패·파일명·임시 산출물 정리를 공통화 후보로 추출. 업무 열·쿼리·보고서 배치·ACL은 앱 소유. 기존 프레임의 window.print 지원과 원본의 숨은 인쇄 iframe을 구분해 인수. U4·F1·C08 |
| U16 텍스트·HTML·이미지 클립보드 | `ui:src/lib/editor/clipboard.ts`, `web:app-modules/perf-test/views/CompareReportPanel.tsx:219`, `web:app-modules/data-viz/views/SysBenchPerformancePanel.tsx:125`. 편집기 복사/리치 HTML 우선 붙여넣기, 보고서 복사, 차트 PNG 복사 | **부분/후보.** 편집기 helper는 공용이나 보고서·이미지 복사는 앱별 구현. 데이터 생성은 앱, 사용자 동작·복사 실패 안내·텍스트 fallback은 공용화. iframe 권한·브라우저별 지원은 미검증이며 포털의 광범위 클립보드 대행으로 바꾸지 않음. U4·F5 |
| U17 댓글·답글·활동 표시 | `web:app-modules/pms/views/TaskDetailActivityPanel.tsx:14`, `web:app-modules/community/views/CommunityView.tsx:1370`, `api:domains/community/router.py:455`. 작성자·시각·댓글·답글·수정/삭제·전송 상태·활동 이력 | **없음(공용 UI).** 표시/입력 adapter를 추출할 후보. 저장·수정 권한·익명/비밀 글 규칙은 앱 소유이고 공통 댓글 서버를 새로 강제하지 않음. U12의 짧은 편집기만으로 이 UX를 대체하지 않음. U4·G5·C05·C09 |

### 4.3 공통 서비스와 앱이 구현할 어댑터

| ID·대상 | AI-DO 실제 구현 | v2 상태·이식 경계 |
| --- | --- | --- |
| C01 사용자·그룹·앱 접근·디렉터리 | `api:domains/admin/app_access_router.py:44`, `api:domains/auth/app_access.py:261`, `api:domains/auth/apps_router.py:65`, `api:domains/auth/app_spaces_service.py:631` | **기반 있음.** G1~G3는 재사용, G4 그룹/멤버십/후보 조회를 추가. 조직 하위 자동 포함·everyone 기본값을 v2에 이식하지 않음. 목록 조회와 실제 멤버 지정 때 권한 재검사 |
| C02 자원 ACL·공간·멤버·공유/회수 | `api:domains/source_access/registry.py:8`, `api:domains/source_access/policy.py:86`, `api:domains/auth/app_spaces_service.py:221`, `api:domains/docs/router.py:2532`, `api:domains/docs/router.py:2610`. 사용자 grant와 공유 링크 발급/중지 | **없음(공통 업무 ACL).** SDK·서버 어댑터·공유 UI는 공통, 관계·역할·자원 정책은 앱 DB. 익명 링크는 v2 현행 앱 Use/자원 ACL을 우회하는 기본 기능으로 채택하지 않음. 감사형 소유권 복구는 관리자 자동 데이터 접근과 구분. G5·G6 |
| C03 Storage·업로드·다운로드 | `api:domains/files/storage_adapter.py:25`, `api:domains/files/router.py:666`, `api:domains/files/service.py:68`. 원본 한도는 업로드 250MiB, ZIP 500개·512MiB이며 임시 파일/spool·권한 확인·묶음 다운로드가 있음 | **없음.** v2의 4MiB 요청/응답 API와 별도 전송 계약 필요. 크기·진행률·취소·파일명 metadata·메모리 사용을 함께 정의하며 원본 한도를 자동 채택하지 않음. Files는 `api:api_registry.py:404`의 별도 앱. 폴더·corpus/RAG DB·MinIO 의존은 복사하지 않음. F1·F4 |
| C04 첨부 연결·읽기 핸들·임시 미디어 정리 | `api:domains/media/router.py:71`, `api:domains/media/router.py:189`, `api:domains/media/router.py:280`, `api:domains/media/router.py:317`, `api:domains/files/content_access.py:28`, `api:domains/files/content_access.py:158`. 임시 업로드→자원 연결→렌더→고아 정리. Files는 5분짜리 bearer URL 사용 시 발급자의 현재 ACL·파일 identity를 재검사 | **없음.** 공통 파일 수명과 앱의 현재 ACL을 연결. 발급자 재검사는 링크 수신자의 로그인/권한 확인과 다르므로 공유 링크·인증된 다운로드를 구분해 설계. 첨부 연결 실패를 성공 처리하지 않고 회수 재검사. 원본 URL 방식을 그대로 채택하지 않음. F1·F2 |
| C05 알림함·미읽음·읽음·이동 | `api:domains/notifications/router.py:21`, `api:domains/notifications/read_service.py:74`, `web:components/layout/NotificationPanel.tsx:30`, `ai-do:packages/contracts/src/notifications.ts:3` | **없음.** 셸 알림함과 제한된 발행/조회 계약은 공통. 업무 이벤트·수신 대상은 앱 소유. PMS 경로 해석·DM window event는 앱 목적지 descriptor로 교체. 목록·건수·이동의 원본 ACL 재검사는 v2 목표 계약이며 원본 수신자 검사만으로 충족되지 않음. 별도 모바일 push 중계의 채택은 C11과 함께 검토. F3 |
| C06 실시간 갱신·공동 편집 | `api:domains/realtime/router.py:49`, `api:domains/realtime/router.py:202`, `api:domains/realtime/router.py:333`, `ui:src/lib/editor/collaborative-block-editor.tsx:150` | **없음(업무용).** 전송·재접속·구독 클라이언트와 편집 UI는 재사용 후보, 구독 허가·문서 저장은 앱 소유. 원본은 notifications/DM와 docs.pages 중심이며 범용 이벤트 버스가 아님. 동일 출처 WS·query token은 새 프레임 인증에 맞춰 교체. F5·F7 |
| C07 검색·필터·스니펫·결과 이동 | `api:domains/search/router.py:16`, `api:domains/search/entity_registry.py:17`, `api:domains/search/service.py:408`, `web:platform/app-runtime/SearchSpaceBoundary.tsx:167` | **없음.** 검색 화면/provider 계약을 공통화. 앱 API/이벤트로 인덱싱하며 공유 DB JOIN·앱 Python import는 이식하지 않음. 권한 없는 제목·스니펫·건수도 노출하지 않음. F8 |
| C08 작업센터·진행·실패·취소 | `core-web:background-work.ts:13`, `core-web:background-work.ts:25`, `web:platform/background-work/background-work-provider.tsx:34`, `ai-do:apps/worker/src/ai_do_worker/celery_app.py:280` | **없음(업무 작업센터).** 공통 표시+앱별 list/cancel adapter. 실제 task·재시도·취소는 앱. 포털이 각 앱의 광범위 bearer를 보관해 polling하지 않도록 제한된 집계 계약 필요. 코어 배포 큐와 합치지 않음. PLAN 4의 업무 이벤트 연계 |
| C09 감사·활동 이력 | `api:domains/auth/access.py:1342`, `api:domains/admin/router.py:8970`. 주체·행동·대상·일자별 조회 | **부분.** 기존 관리 감사 재사용, 앱 업무 이력은 별도 권한. 공통 이벤트 형식·목록 UI 후보이며 업무 본문·비밀을 중앙 로그로 수집하지 않음. A1 후속 |
| C10 즐겨찾기·최근 열람·보관/복구 | `api:domains/docs/router.py:2208`, `api:domains/docs/router.py:2339`, `api:domains/docs/router.py:1537`, `api:domains/auth/app_spaces_service.py:298` | **없음(공통 업무 패턴).** 공용 상태/메뉴·앱 adapter 후보. 앱 레일 즐겨찾기는 플랫폼 설정, 문서 즐겨찾기·휴지통·복원은 앱 소유. 전역 휴지통/파일 버전 서비스가 원본에 완성된 것으로 보지 않음 |
| C11 외부 연결·서비스 키·앱 간 연계 | `api:domains/auth/platform_api_keys.py:32`, `api:domains/admin/api_keys_router.py:190`, `api:domains/groupware/sessions.py:118` | **부분.** v2 설치 승인·원래 사용자 보존 connections는 재사용. 서비스 주체·API Invoke는 후속 A2. 원본 키 scope는 HR/근태 중심이므로 범용 권한 모델로 복사하지 않음. 그룹웨어 비밀번호 hook·공유 쿠키 전달도 제외 |
| C12 사용 통계·분석 | `web:platform/analytics/matomo.ts:5`·`:144`(`matomo.dwdcc.kr`에 사용자 ID·로그인 ID·이름·앱·경로를 사용자 정의 차원으로 전송), `api:domains/usage/router.py:47`(앱·이벤트·자원 제목·경로를 담은 사용 이벤트), `api:domains/usage/app_rollup.py:26`, `web:platform/admin/admin-usage-section.tsx:2038` | **없음, 채택 결정 필요.** 프레임 CSP(`script-src`·`connect-src 'self'`)로 앱 안에서는 외부 분석 스크립트를 실행할 수 없다. 포털 셸은 앱 열기·경로 단위만 기록할 수 있다. Core는 사용자·앱별 프레임 실행을 알지만 만료 1시간 뒤 정리하므로, 채택하면 별도 집계(일별 건수 등)가 필요하다. 로그인 ID·이름·자원 제목을 외부 분석 도구로 보내지 않는 것을 기본으로 하고 보존 기간·조회 권한(관리자)·거부를 함께 정한다. PLAN에 아직 없음 |
| C13 통합 일정 | `api:domains/calendar/router.py:35`, `api:domains/calendar/source_catalog.py:32`, `api:domains/calendar/query_policy.py:14`, `web:platform/calendar/calendar-events-session.ts:40`. meeting·PMS·Planner·Google feed의 **본인 일정**을 합성. 타인 일정 공유는 없음, 종료 시각 제외·최대 366일 조회 | **없음.** 앱별 ACL 적용 일정 provider로 연결. Google은 OAuth가 아니라 사용자별 비밀 iCal URL의 암호화 저장·단방향 읽기(`api:domains/planner/google_calendar.py:264`). 개인 피드 연결은 C11, 서비스 주체 A2와 구분하며 OAuth는 신규 선택사항. U08은 화면만, PLAN 4 연계 |
| C14 조직·인사 마스터와 부서 정책 | `web:platform/admin/admin-hr-master-section.tsx:635`, `web:platform/admin/admin-department-policy-section.tsx:58`, `web:platform/department-policy/work-org-approval-api.ts:56`, `api:domains/hr/department_policy_service.py:1`. 인사 마스터, 부서 기준, 업무 조직 승인 흐름 | **부분.** v2는 코어 DB가 그룹·주 소속·수동 멤버의 정본이며 상위 부서 자동 상속을 하지 않는다(PLAN 결정 2). 사용자·그룹 관리 화면 표현은 C01·§4.5로 옮기고, 인사 동기화는 출처·회수 계약을 정한 커넥터로 추가한다. 부서 계층 상속·업무 조직 승인은 인사 앱·커넥터 범위이며 코어 기본값으로 들이지 않는다. G4 |
| C15 공통 요청·오류·취소·스트리밍 | `web:platform/api/client.ts:3`, `web:platform/api/client.ts:30`, `web:platform/api/sse-parser.ts:227`, `web:platform/workspaces/workspaces-api.ts:300`. 상태/payload를 보존한 오류, 204·FormData·언어 처리, UTF-8 분할 SSE·중단, 계정 변경 시 이전 응답 폐기 | **부분.** v2 API 타입/SDK 인증을 재사용하고 UI용 오류·수명 모델을 맞춤. 업무 401을 전체 로그아웃이나 쓰기 재전송으로 처리하지 않음. 현재 응답 버퍼링 경로는 SSE 전송 기능이 아니며 F7/AI 단계에서 별도 계약 필요. 캐시·진행 요청은 사용자·설치·환경·권한 수명에 결합. U3·U6·F7 |

### 4.4 후속 AI 공통 UI

| ID·대상 | 실제 재사용 근거 | 이식 범위 |
| --- | --- | --- |
| A01 대화 입력·대화열·도구 결과·승인 표시 | `web:app-modules/chatbot/public-api.ts:16`, `web:app-modules/chatbot/views/chat/ChatComposer.tsx:16`, `web:app-modules/chatbot/views/chat/ChatThread.tsx:21`은 web-search·claim-analysis·qa-assistant에서도 사용. 도구/승인 표면은 `web:app-modules/chatbot/views/chat/ToolCallCard.tsx:17`, `web:app-modules/chatbot/views/chat/ApprovalModal.tsx:20`에서 추출할 후보 | 입력·전송/중지·스트리밍·스크롤·재시도·도구 카드·산출물 표면은 공통화 후보. **v2 내장 AI 미구현.** 모델 연결·대화 저장·도구 권한·쓰기 승인 서버는 miy AI 계약으로 구현. AI-DO Chatbot 전체나 inference gateway를 공용 UI의 선행 조건으로 넣지 않음. PLAN 4-1·5 |

### 4.5 관리 콘솔 대응

AI-DO 관리 콘솔의 등록 ID(`web:platform/admin/admin-sections.ts:3`)를 v2의 담당 위치로 나눈다. 컴포넌트 파일명과 메뉴 ID를 구분한다. v2 콘솔은 관리 데이터만 표시하고 업무 앱 코드를 실행하지 않는다.

| AI-DO 섹션 | v2 위치 | 비고 |
| --- | --- | --- |
| general | v2 관리 요약 | `web:platform/admin/admin-console.tsx:158`의 사용자·관리자·감사 요약은 기존 v2 지표로 구성할 후보. 전역 workspace 집계는 옮기지 않음 |
| people | v2 콘솔 사용자 관리 | `web:platform/admin/admin-people-section.tsx:522`의 비밀번호 초기화는 IdP 책임, `web:platform/admin/admin-people-section.tsx:540`의 가장 로그인과 임시 비밀번호 토스트는 기본 이식 제외. 관리자에게 사용자 업무 세션을 부여하는 기능은 별도 중요 결정이다. 로그인 차단·필터 결과 CSV·검색 표는 기존 사용자 관리에 맞출 후보 |
| groups·apps(앱 접근·표시 그룹) | v2 그룹·접근(`v2:web/platform/src/pages/AdminAccess.tsx:79`)·관리 앱 | **기반 있음.** 그룹·앱 목록/편집 표현을 재사용. everyone 기본값·조직 하위 자동 포함은 제외(C01) |
| audit | v2 감사(`v2:web/platform/src/pages/AdminAudit.tsx`) | C09 |
| api-keys | 후속 서비스 주체 | C11·A2. 원본 API 키 scope를 그대로 옮기지 않음 |
| usage | 채택 결정 뒤 | C12 |
| hr·department-policies | 그룹 정본과 인사 커넥터 | C14 |
| workspaces | 각 업무 앱의 공간 관리 | S07·C02. 콘솔의 전역 공간 관리로 옮기지 않음 |
| batches·document-processing·community | 해당 업무 앱 | 콘솔 공통 기능이 아님 |
| llm·model-monitoring·ai-security | 내장 AI 서비스 | routing/providers/models/images는 llm 내부 탭(`web:platform/admin/admin-llm-management-section.tsx:13`). PLAN 4-1·5에서 서비스와 함께 적용 |

## 5. 그대로 옮기면 안 되는 의존성과 원본의 공백

### 코드 이동 방식

1. **UI·순수 모델 추출:** `packages/ui`, 날짜/선택/피드백 모델과 관련 테스트를 우선 활용한다. 소스 alias만으로 연결된 `@ai-do/ui`를 독립 배포 패키지가 완성된 것으로 보지 않는다. React·Radix·Tailwind·BlockNote/Mantine·Recharts·Yjs 등 실제 의존성을 선언하고 경량 UI와 editor/calendar/AI 모듈을 선택 로딩하도록 나눈다.
2. **연결 교체:** 부모와 공유하는 React context·document.body·window event·localStorage, 전체 사용자 token, 직접 `/api/v1/workspaces/...` 호출은 해당 앱의 UI·SDK·API 연결로 교체한다. 앱 프레임 자신의 `document.body`에 모달을 렌더링하는 방식은 유지할 수 있다. 공용 코드를 함께 사용하는 것과 DOM·세션을 함께 사용하는 것은 다른 일이다.
3. **서버 계약 재구현:** Python/SQLAlchemy·Celery·공유 DB에 결합된 ACL·알림·검색·작업은 Rust 코어와 앱 서버 adapter로 나눈다. 원본 API 이름을 유지하려고 호환 게이트웨이·전역 공간을 먼저 만들지 않는다.
4. **게시 자산 분리:** 현재 SDK·UI·번들의 게시 바이트는 바꾸지 않는다. 구현할 때 새 버전을 추가하고 CLI 템플릿·앱 지침·프레임 런타임을 필요한 범위에서 함께 검증한다.
5. **프레임 CSP에 맞춘 자산:** 업무 앱은 `script-src 'self'`(eval·`new Function` 불가), `style-src 'self' 'unsafe-inline'`, `font-src 'self'`, `img-src 'self' data: blob:`, `connect-src 'self'`, `media-src 'self' blob:`, `worker-src 'none'`, `frame-src 'none'`을 사용한다(`v2:crates/miy/src/app_frames.rs:1113`, [APP-FRAMES](APP-FRAMES.md) §3.5). 글꼴은 재배포 조건과 빌드 경로를 확인해 자기 출처 자산으로 공급한다. KaTeX·highlight.js·BlockNote/Mantine 등 실제 사용 라이브러리를 이 CSP에서 인수하며, 외부 분석·캘린더 호출은 서버 연결로 바꾼다. Mermaid는 package 의존성에 있으나 조사한 실행 경로에 전용 렌더러는 확인되지 않았다(`web:app-modules/chatbot/views/chat/ArtifactPanel.tsx:257`은 미래 타입 예시). 채택 시 신규 기능으로 검증한다.
6. **요청·캐시의 수명:** 계정/앱/환경 전환·권한 회수 때 진행 요청과 늦은 응답을 정리한다. 원본 `web:platform/media/media-url-resolution-session.ts:65`의 캐시는 URL만 key로 사용하고 `web:platform/media/use-media-upload.ts:16`의 session은 token 변경 때 재생성되지 않는다. 캐시 구조를 그대로 복사하지 않고 사용자·설치·환경·파일 권한/만료에 묶는다. SSE parser를 가져오는 것과 코어에 스트리밍 전송 경로를 구현하는 것도 구분한다.

### 기존 기능과 프레임 제약의 대응

| 원본 기능 | 현재 경계와 이식 조건 |
| --- | --- |
| HTML 미리보기·draw.io 임베드 | `web:app-modules/chatbot/views/chat/artifacts/HtmlArtifact.tsx:42`, `web:app-modules/docs/views/docs-html-frame.tsx:13`, `web:app-modules/diagrams/views/drawio-embed-protocol.ts:101`은 중첩 프레임을 사용한다. 현 지원 계약은 `frame-src 'none'`이며 UI만 복사해 지원 완료로 보지 않는다. 안전한 표시/export 대안 또는 좁은 별도 계약을 정한 뒤 인수. draw.io 서비스·도형 DB는 업무 앱 소유 |
| 인쇄·PDF 저장 | v2의 sandbox는 window.print를 위한 allow-modals를 포함하지만 원본의 숨은 인쇄 iframe·스타일 복제까지 검증한 것은 아니다. 앱 자체의 인쇄 CSS/화면과 원본 helper를 구분하고 페이지 분할·그림/폰트·출력 취소·정리를 확인. PDF 저장은 브라우저 인쇄 UX이며 공통 PDF 생성 서버 구현을 뜻하지 않음 |
| 클립보드·마이크/카메라 | 현 `v2:web/platform/src/app/AppFrame.tsx:206`에는 `allow` 권한 위임 속성이 없다. 텍스트·HTML·PNG 복사와 업무 앱의 장치 사용은 사용자 동작/거부·브라우저별 지원 인수가 필요하다. allow-modals나 HTTPS만으로 권한 위임 완료라고 보지 않음 |
| 업무 SSE·파일 전송 | 현 `v2:crates/miy/src/app_response.rs:59`는 응답을 끝까지 버퍼링하고 `v2:web/frame-runtime/src/main.tsx:198`은 전체 Blob을 반환한다. 일반 API에 SSE/대형 파일을 실어 스트리밍·진행률이 자동 제공된다고 가정하지 않음. 업로드와 SSE의 중단·메모리·권한 재검사는 해당 기능에서 별도 인수 |

이 표는 현 경계를 넓히는 승인이 아니다. 원본 UX를 유지하기 위해 구현할 범위를 식별한 것이며 이번에는 브라우저 호환성 시험을 실행하지 않았다.

### 구현된 것으로 오인하기 쉬운 항목

| 항목 | 조사 결과와 처리 |
| --- | --- |
| 자가가입 | `web:platform/auth/login-screen-model.ts:107`은 isSignupMode=false. 로그인 UI가 있다는 이유로 회원가입 제품을 이식 목록에 완료 기능으로 넣지 않음 |
| 알림 환경설정과 모바일 push | `web:platform/auth/NotificationsSettingsSection.tsx:35`의 개인 설정 저장은 미연결이다. 별도 push 중계는 `api:domains/notifications/mobile_push_outbox.py:86`, `ai-do:apps/worker/src/ai_do_worker/tasks/mobile_push.py:1`에 구현돼 있으나 근태 서버에 결합돼 있다. 설정 UI 연결과 v2의 채널 채택·이식을 구분 |
| 아바타 업로드 | 확인한 프로필 UI는 이니셜 표현. 사진 업로드가 이미 제공되는 것으로 분류하지 않음 |
| 브레드크럼브·단축키 | 공용 Topbar의 breadcrumb 슬롯은 있음. 자동 계층 연동이나 전역 Ctrl+K 명령 팔레트는 조사한 셸/플랫폼 범위에서 찾지 못했으므로 신규 후보와 구분 |
| 범용 고급 표·트리 | 기본 DataTable·FilterBar와 앱별 페이지 처리·선택·트리는 별개. 필요한 추가 공통화 범위를 U04·U05에서 정함 |
| 멘션·자원 참조 | 편집기 스키마/렌더러·타입은 있으나 추천·클릭·권한·알림 전체 공통 계약이 완성된 것은 아님 |
| 파일 미리보기·복구 | 원본 Files preview는 이미지 허용(`api:domains/files/router.py:853`). PDF/Office, 범용 파일 버전·전역 휴지통을 구현 완료로 간주하지 않음 |
| 홈 메일 위젯 | 원본 `web:app-modules/home/views/WorkspaceHomeView/WorkspaceHomeView.tsx:647`의 ComingSoonMailWidget은 실제 메일 집계가 아님 |
| 범용 초안·반복 작업·반응 | `api:domains/drafts/router.py:31`·`api:domains/drafts/router.py:47`은 고정 template/draft 예시 목록이며 공통 임시저장/복구 API가 아니다. PMS recurrence_rule 필드나 Google iCal 반복 일정 해석을 범용 업무 스케줄러로 간주하지 않는다. 조사한 API 등록 범위에서 공통 reaction 서비스도 확인하지 못함 |

### 업무 앱으로 남길 것

문서·PMS·Files·플래너·회의·녹음·DM·근태·결재·특허·학습·AI 업무 도구는 각자의 데이터와 업무 규칙을 가진 앱이다. 그 안의 표·선택기·첨부·에디터·대화 UI만 공통화 후보로 추출한다. PLM의 BOM·ECO·도면 버전, MES의 생산 상태도 같은 원칙을 따른다. 공통 기능 조사만으로 이 앱 전체의 채택이나 데이터 이전을 결정하지 않는다.

AI-DO Desktop과 inference gateway의 별도 저장소는 이번 공통 웹/API 조사 대상에 포함하지 않았다. 플랫폼의 설치 안내·연결 상태 표시는 재사용 후보지만 별도 제품 런타임을 코어에 포함할 근거는 아니다. 원본 DB·업로드·사용자 설정의 실제 데이터 이전도 후속 작업이며, 수행한다면 ID 대응·파일 연결·ACL·변환·백업/복원 인수를 별도로 정한다.

## 6. 기존 계획에 연결하는 구현 묶음

아래는 이 목록을 기존 PLAN에 연결한 표다. 새 병렬 로드맵을 만들지 않는다. S/U/C/A는 이 문서 대상 번호, `V1-*`는 기존 작업 번호다. 신규 후보는 해당 묶음에서 범위를 확정하며 모두를 코어의 필수 기능으로 자동 편입하지 않는다.

| PLAN 위치 | 이 목록에서 진행할 범위 | 묶음을 닫는 기준 |
| --- | --- | --- |
| 2의 다음 작업 G4 | C01·U06, 선택기 상태에 필요한 U01~03, C14의 그룹 정본 범위와 §4.5의 사용자·그룹 화면. 그룹/현재 멤버십/최소 공개 DTO·검색/페이지 처리 | 대표 앱에서 담당자·공유 대상 선택→서버 저장→계정/그룹 회수 후 거부까지. 관리용 사용자 API를 앱 directory로 대신하지 않음 |
| 2의 G5·G6 | C02·S07 및 앱 내부 멤버/공유 UI, 필요한 U17 댓글 권한 | 대표 프로젝트·업무 앱이 자기 DB에 공간·ACL을 소유. 다른 두 사용자의 목록/상세/파일/대기 작업 권한과 회수 일치. 공간 없는 앱도 정상 동작 |
| 3의 U1~U6, 독립 UI 병행 | S01~06·S08~10·S12, U01~09·U14, U15~17의 독립 UI·C15 요청/오류 모델. S11·S13 후보는 필요 범위 확정 | 기존 셸 위에 AI-DO 표현·조작 적용. 다른 앱 이동·테마/언어는 SDK/프레임과 함께 인수. 사용하지 않는 무거운 모듈은 기본 앱에서 제외 |
| 4의 Storage·F1/F2/F4 | C03·C04·U10·U11, U15 내보내기 파일 | 업로드→첨부→미리보기/다운로드→회수→고아 정리. 크기·메모리·전송 취소/실패·파일명을 검증. 대용량 CAD는 별도 전송 계약으로 검증 |
| 4의 F3·F5·F7·F8 및 업무 이벤트 | C05~10·C13·U12·U13, C15 SSE·U17 알림/활동 연계 | 알림·검색·실시간·작업 각각 실제 provider를 연결해 ACL·재접속·실패 확인. 편집 UI/Markdown 변환과 협업 서버, parser와 스트리밍 전송 완료를 구분 |
| 2의 A2/Invoke 및 4의 연결 | C11의 서비스 주체는 A2, C13의 개인 비밀 피드 연결은 4 | 기존 connections·원래 사용자 보존 유지. 개인 피드 비밀과 서비스 계정은 별도 종류로 관리하며 허용된 사용자/앱/환경/동작 밖에서 거부 |
| 4-1·5의 AI | A01, AI에 필요한 U13 | UI를 내장 AI 서비스와 연결하고 사용자별 대화 격리·중지·도구/쓰기 승인까지 인수. 화면 복사로 AI 실행 완료를 선언하지 않음 |
| PLAN에 없음(결정 필요) | C12 | 사용 통계를 채택할지, 외부 도구로 보낼 식별자·보존 기간·조회 권한을 사용자가 정한 뒤 PLAN에 넣는다 |

**첫 실제 이식은 G4의 사용자·그룹 선택기를 대표 앱에 연결하는 범위가 적절하다.** 이미 있는 사용자 목록 API를 보완하면서 공용 UI 패키징·프레임 연결·현재 권한 확인을 함께 검증할 수 있다. 로그인 기반이나 앱 런처를 처음부터 다시 만들 필요는 없다. 날짜·토큰·기본 폼처럼 독립된 UI 추출은 이 작업과 병행할 수 있다.

## 7. UX와 권한 인수 기준

이식 시작 시 AI-DO 기준 화면을 동일한 가상 데이터·viewport·언어·테마로 캡처하고, 각 묶음의 v2 화면과 비교한다. 필요한 fixture는 개인정보·실제 업무 파일을 포함하지 않는다. 화면이 비슷하다는 검사와 서버 권한 검사를 함께 수행하되 서로의 통과로 대체하지 않는다.

| 흐름 | 확인할 조건 |
| --- | --- |
| 로그인·설정 | 원래 주소 복귀, 인증 실패와 서버 장애 구분, 로그아웃·재로그인, 프로필 저장 실패 피드백, 테마·언어·시간대가 새 프레임에도 반영 |
| 셸·앱 메뉴 | 앱 레일·즐겨찾기·서브메뉴의 위치/밀도, 모바일 메뉴 구분, 직접 링크·query/hash·뒤로/앞으로, 다른 출처 새 탭. 다른 앱 이동은 사용자 확인·현재 접근 재검사, 공간 ID 자동 전달 금지. dirty/unknown 입력은 화면 갱신·로그아웃에서도 보호 |
| 공용 UI | 버튼·표·다이얼로그·drawer·날짜 입력의 키보드/포커스·Escape·오류·빈 상태, 프레임 안에서 잘리지 않는 팝오버, 라이트/다크 대비, 좁은 화면 표의 카드 전환 |
| 선택기·공간·공유 | 검색·페이지 추가·중복 선택·늦은 응답 처리, 후보와 실제 저장 권한 일치, 마지막 소유자 보호, 앱/환경 밖 ID 거부, 그룹 회수 뒤 캐시/직접 URL/대기 작업 거부 |
| 파일·편집 | 업로드 진행·취소·실패·연결 실패·재시도, 파일명·큰 파일의 메모리/크기 한도, 권한 회수 후 읽기 거부, 임시 파일 정리. 편집/보기 표현·Markdown 변환 손실·미저장 상태·협업 연결 종료 처리 |
| 출력·복사·댓글 | 현재 필터가 반영된 XLSX/CSV, 셀 안전 처리·HTML 파일명/그림, 인쇄 CSS/페이지 분할·취소 후 정리. 텍스트/HTML/PNG 복사 성공·권한 거부·fallback. 댓글/답글의 작성·수정·삭제 권한과 실패 시 입력 보존 |
| 개인 도구·일정 | 도킹/접힘/전체화면 복귀·메모 자동저장 실패·재시도, 할 일→업무 생성 실패/중복·생성 후 원본 유지 선택. 일정의 본인 범위·시간대/종료 제외·피드 해제/갱신·비밀 URL 미노출 |
| 요청 수명·스트리밍 | 204·구조화 오류·FormData·언어, 계정/설치 전환 후 늦은 결과·옛 캐시 폐기, 업무 401의 중복 쓰기 없음. SSE 분할 UTF-8·중단·재접속/권한 회수와 서버 버퍼·메모리 검사 |
| 알림·검색·작업 | 읽음/미읽음 동기화, 권한 없는 제목/스니펫/건수 제거, 잘못된 목적지 거부, 앱 이동 후 진행 상태 확인, 재접속·취소 가능 여부·오류 표시 |
| 플랫폼 경계 | 업무 UI는 설치별 프레임에서 실행. API는 자기 앱/승인 연결, 모르는 메시지·타 앱 경로 거부. 콘솔에 앱 코드 주입이나 모든 앱을 읽는 사용자 bearer 공유가 없어야 함 |
| 개발 자산 | 새 UI/SDK 버전으로 만든 앱을 플랫폼 재빌드 없이 설치. 선택 모듈을 쓰지 않는 앱의 의존성·초기 로딩, CLI 템플릿·예제·계약 검사가 일치 |

원본 테스트에서 이식할 사례는 다음과 같다. **테스트 소스를 읽었다는 사실은 실행 PASS가 아니다.**

| 대상 | 원본 사례 |
| --- | --- |
| 앱 진입·공간·권한 | `web:platform/app-runtime/AppEntry.spec.tsx:156`, `web:platform/app-runtime/AppSpaceManagementDialog.spec.tsx:372`, `ai-do:apps/api/tests/test_app_access_integration.py:139`, `ai-do:apps/api/tests/test_app_spaces_service.py:113` |
| 모달·피드백·표 | `ui:src/lib/overlay/dialog-surface-model.spec.ts:45`, `ui:src/lib/feedback/feedback-state.spec.ts:32`, `ui:src/lib/data-display/data-table-model.spec.ts:10` |
| 사용자·자원 선택·날짜 | `web:platform/users/user-option-picker-model.spec.ts:23`, `web:platform/pickers/resource-picker-session.spec.ts:109`, `web:components/date/DateInput.spec.tsx:19` |
| 파일·협업·렌더러 | `web:app-modules/files/file-upload-session.spec.ts:109`, `ui:src/lib/editor/collaborative-session.spec.ts:31`, `web:components/artifacts/MarkdownContent.spec.tsx:58` |
| 언어·검색 ACL | `ai-do:apps/web/e2e/i18n-locale-smoke.spec.ts:39`, `ai-do:apps/api/tests/test_keyword_search.py:261` |
| 출력·변환·댓글 | `web:app-modules/claim-analysis/model/claim-print.spec.ts:56`, `web:app-modules/perf-test/views/CompareReportDownload.spec.tsx:46`, `ui:src/lib/editor/markdown.spec.ts:30`, `ui:src/lib/editor/clipboard.spec.ts:17`, `web:app-modules/pms/views/TaskDetailActivityPanel.spec.tsx` |
| 요청 수명·도킹 | `web:app-modules/chatbot/api/sse-parser.spec.ts:48`, `web:platform/personal-widgets/PersonalWidgetHost.spec.tsx:116`, `web:platform/deployment/stale-asset-reload.spec.ts` |

v2에서는 기존 `v2:tests/platform-shell.spec.ts`·`v2:tests/app-frames.spec.ts`·`v2:tests/surface-boundaries.spec.ts`의 경계/탐색 사례를 유지한다. 추가 검사는 구현 영역에 맞춘 최소 범위로 실행하며 검증 빌드로 운영 `web/platform/dist`를 덮어쓰지 않는다.

## 8. 이번 조사에서 확인한 범위

- 실행: 각 저장소 `git rev-parse HEAD`·`git status --short`, dev/prod 관련 경로의 `git diff`, `rg --files`·`rg -n`·소스/테스트 읽기로 버전·등록·호출자·구현 범위를 대조했다.
- 최초 36개 대상판의 문서 검증 이력: `python3 scripts/check-doc-links.py`로 기존 추적 문서의 상대 링크 926개 PASS. 당시 새 문서는 별도 검사로 상대 링크/앵커 1개·소스 참조 159곳의 경로/행 범위·대상 ID 36개 중복 없음 PASS. `git diff --check` PASS. 아래 보완·재검토판의 통과로 이월하지 않으며 소스 참조 검사는 기능 실행 검증을 뜻하지 않는다.
- **NOT_RUN:** AI-DO/v2 기능 테스트·빌드, 브라우저 화면/스크린샷 비교, 서비스 API·실시간·성능 인수, Windows 실기, 실제 데이터 이전·배포. 이번 요청은 대상 식별·문서화이며 원본 테스트나 과거 이식 PASS를 새 기능의 증거로 옮기지 않았다.
- AI-DO 소스·기존 미커밋 계획, 양쪽 운영 상태·DB·파일·게시 자산은 변경하지 않았다. 새 기능 구현 착수와 UX 실물 비교는 이 조사 이후의 작업이다.

### 보완 조사(2026-11-14)

- 실행: AI-DO dev(`822c562702d`)의 `apps/web/src/platform/*`·`components/*`·`app/shell`, `packages/ui/src/lib`·`core-web`·`contracts`, `apps/api/src/ai_do_api/domains/*`, 관리 콘솔 섹션 목록을 문서 인용과 대조했다. `git log`·`git diff --stat 3c7b5d0..822c562`로 기준 이후 변경을 확인했다.
- 결과: 대상 S12·S13·U14·C12·C13·C14(총 42개)와 §4.5를 추가했다. 글꼴 파일 미포함과 프레임 CSP 제약을 반영했다. S11·U03에 원본 인용을 더했다.
- 공통 기능이 아닌 것으로 분류해 추가하지 않은 도메인: drafts(문서 작성 보조), community·attendance-codes·hr의 근태·경조금 등 업무 앱, rag·retrieval·conversations·document_processing 등 AI 기능(A01·PLAN 4-1·5).
- **NOT_RUN:** 원본·v2 기능 테스트, 브라우저 비교, Matomo 수집 실태 확인, 실제 데이터 이전.

### 재검토 범위(2026-11-14)

AI-DO dev `822c562702d2f0f7165b52d80e8911cdb5025823`과 v2 `9c7b4ba`의 실제 호출자·등록 ID·전송/CSP 구현을 다시 대조했다. 추가 대상은 U15~U17·C15이며 S06·S12·S13, U12·U13, C02~04·C13과 관리 콘솔 대응·인수표를 보완했다. Google 연결을 OAuth로 기술한 점과 새로고침 대기 설명은 정정했다. 최초판/보완판의 기록은 당시 범위로 보존했다.

최신판 검증은 별도로 수행했다. `python3 scripts/check-doc-links.py`의 기존 링크 926개와 별도 검사한 이 문서의 링크/앵커 2개·소스 참조 235곳의 경로/행 범위·대상 ID 46개·표 구조/공백은 PASS다. 인용한 AI-DO 파일 179개의 Git blob이 최초 기준 `3c7b5d0`와 재검토 기준 `822c562` 사이에 같음을 확인해 행 번호 기준도 대조했다. `git diff --check` PASS. 두 독립 재검토에서도 수정한 셸/서비스 범위의 추가 중요 오류는 발견하지 못했다.

테스트 소스는 인수 사례의 근거로 읽었으며 실제 기능 테스트·브라우저 호환성·운영 동작은 이번에도 `NOT_RUN`이다. 이 목록은 조사한 등록·호출 경로의 공통화 후보이며 AI-DO 모든 업무 앱의 제품 기능 전체를 이식한다는 의미가 아니다.
