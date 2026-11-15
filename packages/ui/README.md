# 공용 UI 유지관리 소스

`src/`가 새 공용 UI 버전의 유지관리 원본이다. 게시된 `registry/components/ui/1.0.x`는 수정하지 않는다. 루트의 컴포넌트 생성 도구가 새 버전을 만들며, SDK/CLI 번들과의 조합은 루트 manifest에서 관리한다.

AI-DO의 중립 색상·입력 밀도·선택 칩·본인 우선·잠긴 대상 표현을 옮겼다. React 이외의 런타임 의존성이나 외부 CDN은 없다. 글꼴은 원본과 같이 시스템에 설치된 Pretendard 계열 이름을 사용한다. `index.tsx`를 import하면 스타일이 로드되며, 스타일만 필요하면 `styles.css`를 import한다. 앱 document에 `.dark` 또는 `data-miy-theme="dark"`를 적용하면 어두운 색상 토큰으로 바뀐다.

현재 제공 범위:

- `Button`, `IconButton`, `Field`, `Panel`, `Badge`, `Loading`, `ContentState`
- 현재 앱 문서 안의 `Dialog`, 비동기 `useConfirm`·`usePrompt`
- 검색·페이지·키보드·사용자/그룹 복수 선택 `DirectoryPicker`
- 역할 편집·공유 해제·충돌/결과 조회 `ShareEditor`
- 작은 파일 순차 전송·첨부·취소·다운로드 `FileTransferPanel`, raster 전용 `FileImagePreview`
- 표시용 `formatByteSize`, 불변 배열 순서 변경 `moveItem`, 요청 ID용 `requestId`

`requestId`는 `crypto.getRandomValues`로 UUID v4를 만든다. 사내 HTTP 주소의 앱 프레임은 보안 컨텍스트가 아니어서 `crypto.randomUUID`가 없으므로, 컴포넌트와 앱은 요청·클라이언트 ID에 이 함수를 쓴다.

Drawer, SplitPane, 표/트리, 공용 토스트, rich editor 및 다른 AI-DO 공통 UI는 이 버전에 포함되지 않는다. 전체 마이그레이션 상태와 순서는 루트 `docs/PLAN.md`가 관리한다.

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

`pnpm exec playwright test --config packages/ui/tests/playwright.config.ts`는 loopback fixture 서버와 가상 데이터로 실행한다. 운영 `web/platform/dist`나 `.local`의 운영 상태를 덮어쓰지 않는다. 결과는 `.local/evidence/common-ui/results`에 기록한다.

이 검사는 UI 생명주기·접근성·복구 동작을 확인한다. 서버 ACL·Storage·실제 브리지·운영 배포의 검증을 대신하지 않는다. AI-DO 원본과 동일 데이터의 실물 화면 비교는 별도 인수다.
