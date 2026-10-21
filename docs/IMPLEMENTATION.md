# V0~V1 구현 계획

확정일: 2026-10-21  
기준: [핵심 검증 계획](VALIDATION.md), PRD 0.5  
상태: 구현 착수 전 체크포인트

## 목적과 범위

플랫폼 재빌드 없는 앱 UI 실행, 실제 OAuth 인증, 앱별 DB 생성·권한·마이그레이션을 먼저 구현·검증한다. 검증한 계약을 V2의 CLI·템플릿·SDK·개발 하네스 기준으로 사용한다.

- 현재 Mac의 Docker 환경에서 시작한다. 별도 Linux 서버 배포는 V4에서 확인한다.
- Rust·Axum·SQLx, React·Vite·TanStack Router, PostgreSQL, Keycloak, Caddy를 사용한다.
- UI 통합은 Module Federation을 우선 검증한다. iframe 표시로 통과를 대신하지 않는다.
- 웹 OIDC와 Codex MCP OAuth 로그인·갱신·철회를 V0에 포함한다.
- CLI 전체 기능, Codex 업무 앱 개발 평가, OCI 설치·업데이트, Storage, Hermes는 후속 단계다.
- UI 자산 등록과 DB 준비는 각각 검증하며 이를 실제 컨테이너 배포 완료로 표시하지 않는다.

## 프로젝트와 UI 계약

Rust workspace에 코어, 공통 계약·도메인 서비스, DB 실행기를 분리한다. 프런트 workspace에는 플랫폼, 브라우저 SDK·공통 UI, 독립 빌드하는 검증용 앱 두 개를 둔다. 고객 앱 저장소는 이후 플랫폼과 분리한다.

Compose는 PostgreSQL·Keycloak·Caddy와 검증 서비스를 구성한다. 로컬 개발 서버는 사용할 수 있으나 UI 인수 검사는 독립적으로 빌드한 정적 산출물로 수행한다.

- 플랫폼은 `/apps/{appId}/{remainingPath}` 범용 라우트와 공통 레이아웃을 유지한다. 초기 화면 대상은 `dev`이며 모델·권한에는 환경을 명시한다.
- 앱은 `./AppRoot`를 공개한다. 등록 정보에서 릴리스별 모듈 주소를 조회하고 런타임에 로드한다.
- 실행 문맥에는 설치·릴리스 ID, 상대 경로·검색 파라미터, `navigate`, 앱 API 클라이언트, 테마를 제공한다. 인증 토큰은 전달하지 않는다.
- 브라우저 history는 플랫폼이 관리하고 앱은 자기 경로 안에서 SDK로 이동한다.
- React·React DOM을 공유하고 CSS Modules·공통 디자인 토큰을 사용한다. 앱 전역 CSS 초기화는 허용하지 않는다.
- 앱 캐시·구독을 분리하고 앱 전환·로그아웃 때 정리한다. 오류는 앱 영역에 표시한다.
- 불변 URL·해시로 정적 자산을 관리한다. 이미 열린 화면은 갱신을 안내하며 상태 보존 강제 교체는 구현하지 않는다.
- 통합 UI는 신뢰한 코드이며 브라우저 실행 공간을 공유한다. 컨테이너 격리와 혼동하지 않는다.

## 인증과 공개 인터페이스

Keycloak에 관리자·개발자·일반 사용자 A/B를 준비한다. 계정 인증은 Keycloak, 앱·환경 권한은 코어가 담당한다.

웹은 Authorization Code + PKCE와 서버 측 토큰 관리, HttpOnly·Secure 세션 쿠키, 변경 요청 CSRF 검사를 사용한다. MCP는 공식 Rust SDK의 Streamable HTTP를 `/mcp`에 제공한다.

Codex OAuth는 사전 등록한 공개 클라이언트와 정확한 callback으로 시작한다. CIMD·동적 등록은 후속 범위다. 수동 bearer token으로 OAuth 성공을 대체하지 않는다. 최초 대상은 설치된 Codex CLI 0.163.0이다.

Keycloak의 `resource-indicators` 기능으로 resource·audience 연결을 검사하며 실험 상태와 버전을 기록한다. 토큰의 서명·issuer·audience·만료·scope와 코어의 최신 권한을 검사한다. 초기 단계에는 인증 상태의 장기 캐시를 두지 않고 introspection과 권한 조회로 철회·회수를 확인한다. MCP 토큰을 앱에 그대로 전달하지 않는다.

로컬 HTTPS는 Caddy 내부 CA를 사용하며 브라우저·Codex·코어에서 CA를 신뢰하도록 구성한다. 공개 issuer를 일관되게 유지하고 인증서 검증을 끄지 않는다.

| 인터페이스 | 최소 기능 |
| --- | --- |
| 인증 | 로그인·callback·로그아웃, 현재 사용자·실효 권한 |
| 등록 | 앱·불변 릴리스·환경별 설치 생성·조회 |
| UI | 허용된 설치의 활성 릴리스·모듈 진입점·계약 버전 |
| 계약 | manifest 검사, 버전별 계약·API 설명·SDK 예제 |
| DB 작업 | 지정 설치·릴리스의 DB 준비, 작업 상태·실패 원인 |
| 권한 | 관리자의 앱·환경별 권한 부여·회수 |

HTTP API는 `/api/v1`과 OpenAPI를 사용한다. MCP는 `get_contract`, `search_api`, `get_api`, `validate_manifest`, `prepare_database`, `get_job`을 제공하고 문서를 Resources로도 제공한다. HTTP·MCP는 같은 도메인 서비스·권한·계약 검사기를 호출한다.

manifest는 `miy.app/v1alpha1`로 시작한다. 식별자·버전, Rust 프로필, UI 진입점, DB·migration, 요청 권한을 검사한다. 권한 요청은 부여가 아니다.

변경 작업은 작업 ID를 반환한다. 같은 멱등성 키·입력은 기존 작업을 반환하고 다른 입력은 거부한다. 오류에는 안정적인 코드·요청 ID·수정 안내를 포함한다.

## DB와 작업 실행

- 코어·Keycloak·앱·환경별 DB를 분리한다. 앱의 객체 소유·migration·실행 역할을 나누고 실행 역할은 필요한 CRUD만 허용한다.
- DB 생성 자격증명은 DB 실행기만 사용한다. 코어 웹 프로세스에는 제공하지 않는다.
- `PUBLIC` 기본 권한을 정리하고 코어·타 앱·타 환경 접속과 실행 역할 DDL 거부를 실제로 검사한다.
- 작업을 영속 저장하고 설치별 잠금으로 동시 변경을 막는다. 재시작 시 기존 자원과 단계를 대조해 재개한다.
- DB 생성의 부분 완료를 기록하고 재시도 때 기존 DB·역할을 재사용한다.
- SQLx migration 순서·checksum을 검사한다. 초기에는 트랜잭션 가능한 SQL만 지원한다.
- 적용된 SQL 변경·알 수 없는 이력을 거부하며 실패 시 DB 초기화·자동 down migration을 수행하지 않는다.
- 작업은 등록된 릴리스의 migration을 참조한다. 공개 API로 임의 경로·SQL을 실행하지 않는다.
- DB 준비 상태와 실제 앱 실행 상태를 분리한다.

## 구현 순서와 통과 조건

| 순서 | 작업 | 통과 조건 |
| --- | --- | --- |
| 1 | 도구체인·workspace·Compose·HTTPS | 빈 검증 환경에서 기동 재현 |
| 2 | 웹 OIDC·MCP OAuth·공통 권한 | 실제 로그인·Codex 조회 성공과 비인증 거부 |
| 3 | 플랫폼·두 모듈·런타임 등록 | 호스트 빌드 이후 새 앱을 재빌드·iframe 없이 실행 |
| 4 | 앱·릴리스·설치·권한·작업 API | 상태 영속과 HTTP·MCP 정책 일치 |
| 5 | DB·역할·migration·잠금·복구 | 격리·중복 방지·데이터 보존·실패 추적 |
| 6 | 브라우저·Codex·DB 통합 인수 | 필수 검사 통과와 증거 보고 |

필수 검사: 직접 링크·새로고침·뒤로 가기·앱 전환·로드 실패·로그아웃 정리, OAuth 갱신·철회·잘못된 토큰·권한 회수, 앱·환경별 거부, DB CRUD·타 DB/DDL 거부·신규 객체 권한, migration 재실행·checksum, 동시 요청·부분 생성·실행기 재시작이다.

Rust 통합 검사는 실제 PostgreSQL·Keycloak, UI는 Playwright, MCP는 프로토콜 검사와 실제 Codex 호출을 사용한다. 필수 서비스가 없으면 미실행으로 기록한다.

## 완료와 후속 작업

기동·초기화·검사 명령, 도구체인·lockfile·기반 이미지 digest, 검증한 공개 계약, 정제한 작업·인증·브라우저 증거와 결과 보고서를 남긴다. 인증값·DB 암호는 결과에서 제외한다.

모든 필수 검사를 인증 우회·플랫폼 재빌드·수동 DB 보정 없이 재현해야 완료다. 불가한 항목은 실패 또는 미실행으로 기록한다. 통과한 계약으로 V2를 구현하고, 전체 개발·배포 3회 반복은 V7에서 수행한다.
