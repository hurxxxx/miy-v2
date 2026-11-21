# 공용 UI 유지관리 소스

`src/`가 새 공용 UI 버전의 유지관리 원본이다. 게시된 `registry/components/ui/`의 버전(1.0.0·1.0.1·1.1.0)은 수정하지 않는다. `scripts/build-ui-component.mjs`가 새 버전을 만들며, 호환 SDK 번들은 그 스크립트의 `version`·`bundleVersion`(생성되는 `component.json`의 `bundleVersions`)과 CLI의 `crates/miy/src/bundle.rs`에서 관리한다. 새 프로젝트의 기본 버전은 `templates/rust-react/miy.components.json`이다.

현재 소스는 2.0.0이다(소스 구현, `registry/components/ui/2.0.0` 후보 생성, 미게시). AI-DO의 UI 패키지(`packages/ui`, main `0002af062`)를 새로 설계하지 않고 그대로 옮겼고, miy 조합 컴포넌트를 함께 낸다. 전체 마이그레이션 상태와 순서는 루트 `docs/PLAN.md`가 관리한다.

## 구성

- `src/lib/`: AI-DO `packages/ui/src/lib`의 사본. 편집기(`lib/editor/`)와 `*.spec.*`만 뺐고 코드는 고치지 않았다. 고칠 일이 생기면 AI-DO 원본과 달라진 곳을 이 문서에 적는다.
- `src/index.tsx`: AI-DO `index.ts`가 내보내는 것(편집기 제외)과 miy 조합 컴포넌트를 내보낸다. `styles.css`를 불러온다.
- `src/directory.tsx`·`sharing.tsx`·`files.tsx`·`utilities.ts`: miy 조합 컴포넌트와 유틸리티.
- `src/internal/`: 조합 컴포넌트가 쓰는 버튼·배지·상태 표시·네이티브 `<dialog>`. 공개하지 않는다.
- `src/styles.css`: AI-DO `packages/ui/styles.css`의 토큰 뒤 규칙(편집기 규칙 제외)과 조합 컴포넌트의 `.miy-*` 규칙. 토큰 값은 선언하지 않는다.
- `src/theme.css`: AI-DO `apps/web/src/index.css`의 `@theme` 블록과 `@custom-variant dark`. 앱의 Tailwind 진입 CSS가 import한다.

## 의존성과 스타일

AI-DO와 같은 라이브러리를 쓰고 버전은 AI-DO lockfile에 맞춰 `package.json`에 고정한다: Tailwind CSS 4(`@tailwindcss/vite`), Radix UI 8종, lucide-react, clsx, tailwind-merge, class-variance-authority, @tanstack/react-table, recharts(차트를 그릴 때 동적으로 불러온다)와 그 peer인 react-is. 컴포넌트 패키지는 소스만 담으므로 앱이 이 패키지들을 설치한다. 생성 스크립트가 `package.json`에서 목록을 읽어 `component.json`의 `dependencies`에 넣고(React 제외, Tailwind 빌드 도구 포함), CLI는 앱 `package.json`에 없는 이름을 `component_dependencies_missing`으로 알린다.

컴포넌트는 Tailwind 유틸리티 클래스로 그려진다. 앱은 Vite에 `@tailwindcss/vite`를 넣고 Tailwind 진입 CSS 하나를 둔다.

```css
@import "tailwindcss";
@import "../../platform-components/ui/theme.css";
@source ".";
@source "../../platform-components";
```

`@source`가 없으면 Tailwind가 컴포넌트의 클래스를 찾지 못해 스타일 없이 그려진다. `index.tsx`를 import하면 `styles.css`가 함께 로드되므로 앱이 따로 import하지 않는다.

토큰(`--ui-*`) 값과 Pretendard는 플랫폼이 제공한다(`packages/tokens/tokens.css`, 프레임 런타임이 모든 앱 프레임에 전달). 이 패키지는 토큰 값을 선언하지 않는다. 다크 모드는 문서 루트(`<html>`)의 `dark` 클래스와 `data-miy-theme="dark"`를 따른다. `theme.css`의 색 이름은 문서 루트에서 토큰 값으로 풀리므로, 문서 안쪽 요소에만 다크를 적용하면 `bg-ui-*` 같은 유틸리티에는 반영되지 않는다.

## 제공 범위

- AI-DO 기본 요소: `Button`·`IconButton`, `Input`, `SearchField`, `Badge`·`StatusBadge`, `Panel`, `Toolbar`, `FilterBar`, `Tabs`, `Select`, `RangeSlider`, `Tooltip`, `Dialog`, `DropdownMenu`, `ScrollArea`, `cn`
- AI-DO 레이아웃: `AppShell`, `SidebarNav`, `Topbar`, `SplitPane`, `DetailDrawer`
- AI-DO 데이터 표시: `DataTable`·`DataTableToolbar`, `EmptyState`, `ContentState`, `Skeleton`, `MetricInline`, `ChartFrame`와 선·막대·복합·도넛 차트 카드
- AI-DO 피드백: `FeedbackProvider`·`useFeedback`(토스트), `StatusSlot`, `FormMessage`, `ContextNote`, `ConfirmDialog`·`useConfirm`, `PromptDialog`·`usePrompt`
- miy 조합: 검색·페이지·키보드·사용자/그룹 복수 선택 `DirectoryPicker`, 역할 편집·공유 해제·충돌/결과 조회 `ShareEditor`, 작은 파일 순차 전송·첨부·취소·다운로드 `FileTransferPanel`, raster 전용 `FileImagePreview`
- 유틸리티: 표시용 `formatByteSize`, 불변 배열 순서 변경 `moveItem`, 요청 ID용 `requestId`

`requestId`는 `crypto.getRandomValues`로 UUID v4를 만든다. 사내 HTTP 주소의 앱 프레임은 보안 컨텍스트가 아니어서 `crypto.randomUUID`가 없으므로, 컴포넌트와 앱은 요청·클라이언트 ID에 이 함수를 쓴다.

1.x의 `Field`·`Loading`·`classNames`는 없다(`Input`과 앱의 `<label>`, `ContentState kind="loading"`, `cn`을 쓴다). 이름이 같은 `Button`·`IconButton`·`Badge`·`Panel`·`ContentState`·`Dialog`·`useConfirm`·`usePrompt`는 AI-DO 구현이며 props가 1.x와 다르다. 블록 편집기와 `apps/web/src/components`의 AI-DO 공통 화면은 포함되지 않는다. 추가 시점은 [범용 기능 계획 §5.5](../../docs/COMMON-FEATURES-PLAN.md#55-공용-ui-구성과-추가-시점)를 따른다.

## AI-DO 기본 요소와 조합 컴포넌트를 함께 쓸 때

- 조합 컴포넌트는 1.x의 기본 요소를 `internal/`에 두고 그대로 쓴다. `busy` 버튼, 닫힌 뒤 연 요소로 돌아가는 포커스, 소유 컴포넌트가 사라질 때 false로 끝나는 확인 요청처럼 조합 컴포넌트의 복구·접근성 동작이 여기에 의존하고, AI-DO 기본 요소에는 같은 동작이 없다.
- AI-DO의 `Dialog`·`useConfirm`·`usePrompt`는 닫힌 뒤 포커스를 연 요소로 돌려주지 않는다(`DetailDrawer`는 돌려준다). `useConfirm`·`usePrompt`는 문구(`confirmLabel` 등)가 필수이고, 소유 컴포넌트가 unmount되면 대기 중인 요청을 끝내지 않는다.
- 조합 컴포넌트를 AI-DO `Dialog`·`DetailDrawer` 안에 둘 수 있다. Escape는 열린 선택 목록, 조합 컴포넌트의 확인 대화상자, 바깥 서랍 순으로 하나씩만 닫는다.
- `styles.css`는 Tailwind의 레이어 순서(`properties, theme, base, components, utilities`)를 먼저 선언한다. 이 파일이 앱의 Tailwind CSS보다 먼저 로드돼도 순서가 바뀌지 않게 하기 위해서다.

## 앱 어댑터 계약

컴포넌트는 관리자 DTO, 인증 토큰, 플랫폼의 Core Storage를 직접 사용하지 않는다. Directory는 SDK의 공개 디렉터리 DTO를 `DirectoryProvider`로 변환한다. 공유·파일은 현재 앱의 서버 API를 호출하는 `ShareProvider`·`FileService`로 연결한다. 서버가 매 요청의 현재 권한·대상·소유자 보호·업무 자원 연결을 다시 검사한다. `locked`·`canManage`·선택 결과는 화면 표현이며 권한 증거가 아니다.

Provider는 `useMemo` 등으로 안정된 참조를 유지한다. 사용자/설치/환경/업무 자원이 바뀌면 provider를 새로 만들거나 컴포넌트를 새 key로 remount해 요청과 초안을 분리한다. 이전 사용자·업무 자원의 복구 상태를 재사용하지 않는다. AbortSignal을 전달하되 UI도 세대 번호로 늦은 응답을 무시한다.

### 디렉터리

`DirectoryProvider.load({kind,query,cursor,limit,signal})`는 표시용 `DirectoryOption`과 `nextCursor`만 반환한다. 서버 페이지 크기는 1~100으로 제한한다. `currentUserId`는 받은 후보 중 본인을 먼저 표시하는 용도다. 불러오지 않은 사용자를 새로 추정하지 않는다. 검색어 입력은 250ms 동안 멈춘 뒤 한 번 조회한다. 서버가 조회마다 권한을 확인하고 감사 기록을 남기므로 글자마다 조회하지 않는다. 목록을 처음 열 때는 바로 조회한다. `lockedKeys`는 `user:<id>` 또는 `group:<id>` 값이다.

### 공유

`ShareProvider.load`는 역할 목록·revision·명시 공유 대상을 반환한다. 새로 추가한 대상에는 역할을 자동으로 주지 않는다. 역할 목록의 순서는 의미가 없으며, snapshot의 `defaultRole`이 역할 목록에 있는 값일 때만 그 역할을 미리 선택한다. 그렇지 않으면 사용자가 역할을 고를 때까지 저장할 수 없다. `replace`는 전체 초안, `expectedRevision`, 새로운 `requestId`를 한 번 전달한다. 네트워크 실패처럼 결과를 모르면 자동으로 다시 쓰지 않고 `lookupResult`만 실행한다. 조회 결과가 `null`이면 미확정 상태를 유지한다. 충돌·권한 거부 때 초안을 남기고 다시 불러오기 전에 폐기를 확인한다. 에러 원문을 표시하지 않으며 어댑터는 `UiOperationError`의 안전한 코드나 지원하는 ACL 에러 코드를 전달한다.

화면을 닫아도 복구해야 하는 앱은 `onPendingRequestChange`로 요청 ID를 저장하고 같은 사용자·자원에서만 `initialPendingRequestId`로 복원한다. 복원한 ID는 결과를 확인하기 전까지 새 편집을 막는다. `onDirtyChange`는 앱의 미저장 변경 경고와 연결할 수 있다.

### 파일

파일당 최대 10MiB, 한 패널에서 최대 32개를 선택하며 실제 전송은 순차로 진행한다. `maxFileSize`로 더 작게 제한할 수 있다. `FileService.upload`의 진행률은 어댑터가 측정한 `loaded/total` 값이다. 서버가 스트리밍 전송을 제공하지 않는 어댑터는 정확한 중간 진행률을 주장하지 않는다.

수명은 `create → upload → attach`다. 바이트가 준비된 `ready`와 업무 항목에 연결된 `attached`를 별도로 표시한다. 응답 유실 시 `byRequest` 또는 `get`으로 조회하며, 연결 결과를 모른다는 이유로 파일을 삭제하지 않는다. 취소는 별도의 서버 작업이다. 서버가 이미 연결된 파일 취소를 거부하면 UI가 이를 임의로 지우지 않는다. 재전송·재연결은 사용자가 명시적으로 선택한다.

`onStateChange`의 상태는 File·토큰·다운로드 URL을 포함하지 않는다. 앱이 복구에 필요한 상태를 저장했다면 `initialTransfers`로 전달한다. 복원된 전송 중 요청은 결과 조회부터 수행하며 아직 바이트가 없는 파일은 사용자가 원래 파일을 다시 선택한다. 이름·크기 비교는 잘못된 선택을 줄이는 UX이며 파일 identity나 권한 증명이 아니다. 페이지가 종료되어도 서버의 결과가 확인됐다고 간주하지 않는다.

다운로드는 현재 앱 문서의 Blob URL을 잠시 만들고 회수한다. 미리보기는 MIME과 파일 앞부분을 검사한 PNG/JPEG/GIF/WebP만 이미지로 표시하며 HTML/SVG를 실행하지 않는다. PDF/Office/CAD, 대형 파일·분할 전송·중단 위치부터 이어 올리기는 이 계약에 없다.

## 격리 검사

`pnpm exec playwright test --config packages/ui/tests/playwright.config.ts`는 loopback fixture 서버와 가상 데이터로 실행한다. fixture 서버는 앱과 같은 방식으로 빌드한다: `@tailwindcss/vite`, 진입 CSS `tests/app.css`(Tailwind, `theme.css`, `@source`), `packages/tokens/tokens.css`. 운영 `web/platform/dist`나 `.local`의 운영 상태를 덮어쓰지 않는다. 결과는 `.local/evidence/common-ui/results`에 기록한다.

- `common-ui`·`sharing`·`files`: 조합 컴포넌트의 생명주기·접근성·복구 동작과 공개 `useConfirm`·`usePrompt`
- `common-work`: 대표 예제 앱(`examples/common-work`)의 화면 흐름
- `primitives`: AI-DO 기본 요소가 Tailwind 스타일·토큰 값으로 그려지는지, 다크 전환, 대화상자·서랍·탭·선택·메뉴·툴팁·표·토스트·차트의 동작, 라이트·다크 화면 기록
- `models/`: AI-DO의 순수 모델 테스트(`*.spec.ts` 6개)를 Playwright 러너로 옮긴 것. AI-DO의 React 렌더링 테스트(`*.spec.tsx`)는 옮기지 않았고 `primitives`가 브라우저에서 같은 컴포넌트를 확인한다

이 검사는 UI 생명주기·접근성·복구 동작을 확인한다. 서버 ACL·Storage·실제 브리지·운영 배포의 검증을 대신하지 않는다. AI-DO 원본과 동일 데이터의 실물 화면 비교는 별도 인수다.
