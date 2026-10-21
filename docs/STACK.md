# miy 기술 스택 추천안

검토일: 2026-10-20  
수정일: 2026-10-21  
기준: PRD 0.5  
상태: 채택 전 추천안 · 구현과 성능 검증은 아직 수행하지 않음

**Rust 코어·앱 백엔드, TypeScript·React 화면, PostgreSQL, Docker 기반 실행을 중심으로 구성하는 것을 추천한다.** 업무 앱은 Rust 서버와 정적 화면을 같은 OCI 이미지로 배포하고 일시 중단을 허용해 교체한다. 코어의 API·작업 처리·호스트 실행 제어는 필요한 권한과 실행 수명에 따라 별도 프로세스로 운영한다.

기업별 단일 서버·VM, 사내·클라우드 설치, 개발사·파트너의 유지관리, 외부 앱 제작자의 개발 편의를 기준으로 선택했다. 플랫폼 코어와 개별 앱의 Rust 백엔드·Next.js와 MinIO 제외·내장 Hermes·공통 AI 연결과 사용자별 독립 세션은 합의한 방향이다. 구체적인 연동 방식과 도구는 아래의 추천이며 제품 요구사항의 확정 결정과 구분한다.

제품 전체 구성에 앞서 [집중 검증 계획](VALIDATION.md)의 최소 플랫폼·CLI·Codex·DB·Docker·UI 라우팅 경로를 구현·평가한다. 아래 구성 요소 전체를 선행 설치해야 한다는 뜻은 아니며, 검증 단계의 범위와 선택 버전을 먼저 고정한다.

## 추천 구성

| 영역 | 추천 기술 | miy에서의 역할과 선택 이유 |
| --- | --- | --- |
| 서버 OS | Ubuntu LTS 계열 Linux | 설치·서비스 관리·백업 절차를 하나의 지원 OS로 먼저 검증. 정확한 배포판 버전은 Docker·자원 제한 호환성 확인 후 고정 |
| 코어 백엔드 | Rust + Axum + Tokio + Tower | 인증된 API, 앱·환경·릴리스 관리, 정책 집행과 작업 제어를 공통 모듈로 구현 |
| DB 접근 | SQLx + 버전이 있는 SQL migration | 배포 상태 전이·작업 큐·권한 조회의 SQL과 트랜잭션을 명시적으로 관리 |
| 관리 웹 | TypeScript strict + React + Vite + TanStack Router·Query | Rust API를 사용하는 SPA. 라우팅·서버 상태 관리와 타입을 일관되게 구성 |
| 앱 UI 통합 | 공통 앱 라우트 + 런타임 UI 로더, Module Federation 우선 검증 | 독립 빌드한 신뢰된 React 앱 모듈을 플랫폼 재빌드 없이 표시. 패키지·호환 버전은 검증 후 고정 |
| 공통 UI | Tailwind CSS + shadcn/ui 기반 `@miy/ui` | 디자인 토큰·필수 연동 컴포넌트는 버전 패키지로 제공하고 업무별 화면은 확장 |
| API 계약·SDK | OpenAPI + utoipa + openapi-typescript·openapi-fetch | Rust API 스키마에서 TypeScript 타입·클라이언트 연결. manifest·공개 기능은 버전이 있는 JSON Schema로 검증 |
| MCP | 공식 Rust SDK `rmcp` + Streamable HTTP | 외부 개발 에이전트에 개발 맥락과 도구 제공. HTTP API와 같은 권한 서비스를 호출 |
| CLI | Rust + clap + reqwest | 독립 실행 파일로 배포하며 초기화·로그인·검사·미리보기·릴리스·로그 명령 제공 |
| 첫 앱 개발 프로필 | Rust + Axum + SQLx, React + Vite 정적 화면 | Rust 서버·정적 화면·작업·migration을 한 OCI 이미지로 배포. 정적 파일은 이미지에 포함하며 바이너리 내장은 선택안 |
| 인증 | Keycloak + OIDC, MCP OAuth 연동은 조건부 | 로그인·SSO를 담당. 앱·환경·리소스 권한과 대행 범위는 miy 코어가 판단 |
| DB·벡터 검색 | PostgreSQL + pgvector | 플랫폼 상태, 앱 DB, 작업 상태와 문서 임베딩 저장. 앱·환경별 DB·역할 분리 |
| 작업 큐 | PostgreSQL 영속 작업 테이블 + Rust worker | 예약·재시도·빌드·배포 작업을 기록. 실행할 앱 코드는 별도 격리 환경에서 구동 |
| 소스·이미지 저장소 | Gitea + 내장 OCI Container Registry | 고객사 내부 Git과 배포 이미지를 함께 관리. 외부 GitHub·GitLab도 연결 |
| 개발 자산 배포 | 버전 개발 번들 + Rust SDK crate + npm 호환 브라우저 SDK·UI 패키지 | CLI가 지침·SDK·UI까지 준비. 초기 Rust SDK는 번들에 포함하고 npm 패키지는 Gitea Registry 활용 검토 |
| 빌드·실행 | Docker Engine + BuildKit + Compose | Compose로 고정 기반 서비스를 설치하고 별도 runner가 앱 컨테이너 수명주기를 관리 |
| 자원·용량 제어 | Linux cgroup v2 + runner·코어 용량 관리 + 파일 예약·DB 측정 | 실행 전 서버·앱 예산 확인, CPU·메모리 제한, 저장량 경보·초과 대응. 공유 DB 용량은 관리 한도로 구분 |
| 진입점·TLS | Caddy | 관리 웹·API·앱별 출처의 라우팅과 인증서 운영 |
| 파일 저장 | miy Storage API + 로컬 영속 볼륨, S3 backend 선택 연결 | 초기 단일 서버에는 별도 객체 저장 서버 없이 시작. 저장 구현을 바꿔도 앱의 파일 API 계약 유지 |
| 내장 챗봇 | Hermes의 Python 런타임 + miy 전용 어댑터 | Rust가 대화·실행·권한을 관리하고 Hermes가 모델·도구 루프를 수행. 브라우저에는 SSE로 이벤트 전달 |
| 내부 AI 연결 | 공통 연결 어댑터 + 사용자별 공급자 세션 + 선택형 API·사내 모델 | 공급자 인증은 관리자 관리. miy 사용자·대화에 Codex thread를 매핑하고 실행·이벤트·도구 권한을 분리 |
| 관측·복구 | Rust tracing, 구조화 로그·메트릭, restic + PostgreSQL 백업 도구 | 기본 운영 화면에 상태를 표시하고 OpenTelemetry 수집기는 필요에 따라 연결 |
| 개발·검증 | Cargo workspace + pnpm workspace, Rust tests + Vitest + Playwright | 코어·CLI·SDK·UI의 호환성을 검증. 고객 앱 저장소는 플랫폼과 분리 |

Axum은 Tokio·Tower와 결합하는 HTTP 라이브러리이며 SQLx는 비동기 SQL과 정적 쿼리 검증을 제공한다. 이 구성을 miy의 코어에 적용하는 것은 명시적 상태·트랜잭션 관리와 공통 API 정책을 위한 설계 판단이다. [Axum](https://docs.rs/axum/latest/axum/), [SQLx](https://github.com/transact-rs/sqlx)

React 공식 안내는 Vite, TanStack Router·Query를 SPA 구성 선택지로 소개한다. shadcn/ui의 편집 가능한 컴포넌트를 제품의 UI 계약에 맞게 관리하되, 공통 부분은 각 앱이 별도 복사·수정하기보다 `@miy/ui`로 제공하는 것을 추천한다. [React 구성 안내](https://react.dev/learn/build-a-react-app-from-scratch), [shadcn/ui](https://ui.shadcn.com/docs)

utoipa는 Rust 기반 OpenAPI 생성을, openapi-typescript는 OpenAPI의 TypeScript 타입 생성을 지원한다. 생성 타입은 런타임 입력 검증을 대신하지 않으므로 서버와 앱의 계약 검사를 별도로 둔다. [utoipa](https://github.com/juhaku/utoipa), [openapi-typescript](https://openapi-ts.dev/introduction)

## 코어와 앱 개발 언어

플랫폼의 인증·정책·배포 상태·자원 제어와 개별 앱 백엔드는 합의에 따라 Rust로 개발한다. 업무 앱 화면은 TypeScript·React·Vite를 기본안으로 두며 Rust API 계약에서 화면용 타입·클라이언트를 생성한다. 프런트엔드에서 JavaScript ES Module(`.mjs`)을 사용하는 선택은 백엔드 언어와 별개다. 앱 개발용 SDK·템플릿을 제공해 언어가 다른 화면과 서버를 한 프로젝트로 개발하게 한다.

첫 실행 프로필은 Rust 실행 파일이 HTTP API와 정적 화면을 제공하는 컨테이너로 좁힌다. Axum은 요청 처리, SQLx는 앱 전용 PostgreSQL 접근·마이그레이션에 사용한다. 기본 추천은 실행 파일과 `frontend/dist`를 같은 이미지에 포함하는 방식이며 `rust-embed`를 통한 바이너리 내장은 선택안이다. 어느 경우든 서버는 같은 앱 컨테이너 하나를 관리한다. 앱은 SDK로 플랫폼 공개 기능에 접근하며 플랫폼 내부 DB에 직접 연결하지 않는다. [Axum](https://docs.rs/axum/latest/axum/), [SQLx](https://github.com/transact-rs/sqlx), [rust-embed](https://docs.rs/rust-embed/latest/rust_embed/)

로컬에서는 Vite·Rust 개발 서버와 테스트 데이터를 함께 실행하고 배포 빌드에서는 화면·서버를 한 이미지로 묶는다. Rust 도구체인·Cargo.lock·프런트 패키지 잠금 파일을 고정한다. 앱 제작자는 서버 운영 설정을 매번 작성하지 않고 CLI가 빌드·포트·상태 확인·작업 실행 설정을 준비한다.

CLI 기본 개발 번들에는 지침·SDK·UI·타입·예제를 포함한다. MCP는 설치 버전에 맞는 코어 공개 API 전체의 명세와 사용법을 검색·조회하고 허용된 작업을 호출하는 경로로 제공한다. 기본 개발은 CLI만으로도 가능하게 하며, 패키지 배포·버전 고정·맥락 동기화는 [개발 자산 제공 계획](DEVELOPMENT.md#개발-자산-제공과-mcp의-역할)을 따른다.

앱은 manifest·UI 모듈·Rust 서버·작업·마이그레이션을 묶은 독립 패키지로 설치한다. 코어와 같은 언어여도 앱 백엔드 코드를 코어 서버 프로세스에 직접 로드하지 않고 등록 정보와 HTTP 계약으로 연결한다. 메뉴·통합형 UI·공개 API·작업과 수명주기는 [Rust 앱 개발 계획](DEVELOPMENT.md#rust-풀스택-앱의-플러그인-구조)에 정리한다.

동기 API와 예약·백그라운드 작업은 같은 릴리스의 서로 다른 진입점으로 구성한다. 작업을 실행하는 프로세스에 다른 앱의 코드·파일·자격증명을 함께 넣지 않는다. 함수 실행 자원의 공유와 유휴 실행 중지는 실제 자원 사용을 측정한 뒤 최적화한다.

## 인증과 MCP: 먼저 검증할 조합

Keycloak은 로그인·계정 인증·OIDC 연결을 담당하고 miy는 앱·환경·리소스·대행 범위의 실효 권한을 담당하도록 추천한다. 고객사가 인증 제품의 관리자 화면을 일상적으로 다루지 않도록 설치와 기본 계정 관리를 miy 운영 경험에 통합한다. Keycloak의 운영 프로세스·DB·업데이트도 유지관리 범위에 포함해야 한다. [Keycloak OIDC](https://www.keycloak.org/securing-apps/oidc-layers)

**Keycloak의 최신 MCP 인증 연동은 확정 전 검증이 필요하다.** 확인한 공식 문서는 Resource Indicators와 Client ID Metadata Documents를 실험 기능으로 분류하며 MCP 2025-06-18·2025-11-25·2026-07-28 적합성도 실험 단계로 표시한다. 일반 OIDC 지원을 개발 에이전트 전체의 MCP 인증 호환으로 해석하지 않는다. [Keycloak MCP 연동](https://www.keycloak.org/securing-apps/mcp-authz-server)

`rmcp`는 공식 Rust MCP SDK이며 Streamable HTTP를 Tower 서비스로 제공한다. 이를 Axum에 연결하되, SDK가 제공하는 전송 기능과 인증 서버·리소스 권한 집행을 구분한다. 브라우저의 챗봇 SSE와 MCP 전송도 서로 다른 인터페이스다. [공식 Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)

확정 조건은 대상 개발 에이전트별 로그인, 클라이언트 등록, PKCE, 대상 서버에 묶인 토큰 검증, 갱신·철회와 권한 회수다. 실험 기능 수용 여부와 유지관리 비용을 평가한 뒤 인증 구성을 확정한다. 이를 만족하지 못하면 인증 서버 후보를 재검토한다.

## 실행·배포·코어 업데이트

Compose는 PostgreSQL·Keycloak·Gitea·Hermes·miy 등 설치 기반 서비스를 묶는 데 사용한다. 고객 앱의 설치·삭제·승격은 별도 Rust runner가 Docker Engine을 제어하도록 한다. 빌드는 BuildKit으로 수행하고 커밋·빌드 결과·OCI 이미지 digest를 릴리스에 연결한다. [Compose 운영 배포](https://docs.docker.com/compose/how-tos/production/), [BuildKit](https://docs.docker.com/build/buildkit/)

관리 화면·CLI·API에서 앱·환경별 시작·중지·재시작·업데이트·삭제와 상태·로그·자원 조회를 제공한다. 코어는 목표 상태·작업을 저장하고 runner는 실제 컨테이너 상태를 대조해 실행한다. 중지 상태는 요청·예약·호스트 재시작으로 자동 해제하지 않는다. 앱 제거와 데이터 영구 삭제는 구분하며 DB·파일은 기본 보존한다.

업데이트는 이미지 확보·검사 후 대상 앱을 점검 상태로 바꾸고 **기존 실행 종료 → migration → 새 버전 실행 → 정상 확인 → 접근 재개**로 처리한다. 일시 중단을 허용하며 두 버전의 동시 실행이나 Kubernetes를 기본 요구로 두지 않는다. 실패 시 DB와 호환되는 이전 버전을 재실행하거나 복구 필요 상태를 표시한다. 중지 앱을 업데이트해도 일반 서비스는 자동 시작하지 않는다.

앱·미리보기·빌드의 파일·네트워크·자격증명을 분리하고 CPU·메모리·작업 동시 실행 한도를 적용한다. Docker 소켓은 앱과 공개 API 컨테이너에 전달하지 않는다. Engine 제어는 좁은 명령 계약을 가진 내부 runner가 담당한다. Rootless 실행을 우선 검증하되 대상 OS의 자원 제한·네트워크·볼륨 기능까지 확인한다. Rootless 자체를 앱 간 완전한 격리 보장으로 간주하지 않는다. [Docker Rootless](https://docs.docker.com/engine/security/rootless/)

runner의 초기 실행 어댑터는 Docker Engine API 하나로 좁힌다. 패키지 계약·관리형 자원 연결은 Cloudron, 수명주기와 앱 관리 경험은 Synology·Dokku·Coolify에서 참고한다. Podman Quadlet·호스트 systemd·Wasm 플러그인·Nomad는 적용 범위가 다른 비교 후보로 남긴다. 제품 전체 도입과 개념 차용을 구분한 근거는 [런타임 설치·업데이트 기술 비교](RESEARCH.md#런타임-앱-설치와-업데이트-기술-비교)에 정리한다.

서버 전체에서 기반 서비스·복구 여유를 먼저 제외하고 앱·작업·빌드의 자원 예약을 관리한다. Rootless 환경의 cgroup 지원·controller 위임과 실제 한도 집행을 설치 진단에 포함한다. 저장량·연결 수·DB 부하·캐시 정리와 단계별 효율화는 [용량 관리 계획](CAPACITY.md)을 따른다.

Caddy는 공통 진입점으로 사용하고 앱 API와 릴리스별 UI 자산을 라우팅한다. 통합형 UI는 플랫폼 라우터에 로드하고, 분리형 UI는 별도 출처를 사용한다. 공개 도메인은 인증서 자동 발급 경로를, 사내 도메인은 고객사 인증서나 내부 CA의 신뢰 배포 절차를 설치 프로필에 포함한다. 사내 설치에서 공개 인증서 발급이 자동으로 가능하다고 가정하지 않는다. [Caddy HTTPS](https://caddyserver.com/docs/automatic-https)

코어 업데이트는 앱 배포 큐와 별도 권한으로 수행한다. 호스트의 작은 업데이트 도구를 systemd 서비스 등으로 운영하여, 코어가 중지되어도 백업·버전 적용·정상 동작 확인·복구가 가능하게 설계한다. 업데이트 묶음에는 검증한 구성 요소의 버전·이미지 digest·앱 계약 호환 범위·migration 절차를 고정한다.

## 데이터·작업·검색

초기에는 하나의 PostgreSQL 인스턴스를 운영하되 플랫폼·인증·Git 서비스와 앱·환경의 DB 및 역할을 분리한다. 앱마다 독립 PostgreSQL 서버 프로세스를 만드는 방식은 자원 사용과 운영 요구가 생기면 추가한다.

논리 DB 분리는 DB 처리 자원의 물리적 격리가 아니다. 앱 컨테이너의 CPU·메모리 한도와 별도로 DB 연결·쿼리·임시 파일·저장량을 관리한다. DB별 용량은 초기 공유 구성에서 측정과 접근 차단을 이용한 관리 한도로 제공하며, 엄격한 자원 경계가 필요하면 별도 인스턴스·저장 공간 프로필을 검토한다. 연결 수가 병목이면 PgBouncer의 도입과 ORM·마이그레이션 호환성을 검증한다.

앱의 업무 스키마는 버전이 있는 변경 SQL과 SQLx migration으로 제공한다. 앱 실행을 중지한 뒤 해당 앱·환경 DB의 미적용 변경을 실행하고 새 버전의 정상 상태를 확인한다. DB 준비·마이그레이션·일상 실행 권한을 구분하며 같은 SQLx를 사용해도 코어 DB와 앱 DB의 변경 이력은 독립적으로 관리한다. 구체적인 계약은 [앱 전용 DB 계획](DEVELOPMENT.md#앱-전용-db와-스키마-적용)에 정리한다.

작업 큐는 PostgreSQL의 작업 테이블과 `FOR UPDATE SKIP LOCKED`를 사용하는 방향을 추천한다. PostgreSQL은 이 기능의 큐 형태 사용을 문서화하지만, 큐의 실행 보장까지 제공하는 것은 아니다. 작업 임대·heartbeat·재시도·취소·실패 보관·중복 방지와 재시작 후 복구는 miy의 계약으로 구현해야 한다. 실행 보장은 재전달 가능성을 전제로 하고 업무 변경을 멱등하게 처리한다. [PostgreSQL SELECT](https://www.postgresql.org/docs/current/sql-select.html)

빌드·배포·앱 작업과 Hermes 실행 요청은 작업 유형·동시 실행 한도·실행기를 구분한다. Hermes가 모델·도구 호출을 반복하는 로직은 Hermes 런타임 안에 유지한다.

문서 검색의 초기 벡터 저장소는 pgvector를 추천한다. 문서·청크에 원본 권한과 출처를 연결하고 검색 결과가 모델로 전달되기 전에 현재 접근 권한을 확인한다. 한국어 문서의 키워드·벡터 검색 품질과 권한 필터 적용 후 검색 누락은 파일럿에서 측정한다. 별도 검색 서버 도입은 측정 결과에 따라 결정한다. [pgvector](https://github.com/pgvector/pgvector)

파일은 miy Storage API가 접근 정책을 집행하고 초기 저장 backend는 영속 볼륨을 사용한다. 클라우드 또는 고객사 객체 저장소가 있으면 S3 backend를 연결한다. 파일 메타데이터·권한과 실제 바이트 저장을 분리하고, S3 backend에서만 가능한 서명 URL·수명주기 기능을 공통 계약의 필수 기능으로 가정하지 않는다.

MinIO는 도입 대상에서 제외한다. 자체 구현 범위는 파일 API·권한·메타데이터와 저장 어댑터이며, 분산 객체 저장 엔진을 새로 만드는 것을 뜻하지 않는다. 앱은 저장 backend의 경로나 관리자 자격증명을 받지 않고 SDK·API로 파일을 이용한다.

## 소스·Hermes·복구

Gitea는 자체 Git과 OCI Container Registry를 함께 제공하므로 별도 Git 플랫폼과 이미지 레지스트리 운영을 줄일 수 있다. 다만 Gitea 저장소·패키지 권한과 miy의 앱·릴리스 권한은 연동이 필요하다. 배포 시 변경 가능한 이미지 태그 대신 검증한 digest를 사용한다. [Gitea 소개](https://docs.gitea.com/), [Container Registry](https://docs.gitea.com/usage/packages/container/)

Hermes는 별도 Python 런타임으로 고정 버전을 운영하고 miy 전용 어댑터를 둔다. miy의 사용자·실행 식별자를 유지하며 허용된 도구만 연결한다. 기존 miy 통합에서 확인한 책임 구분을 차용하되 상위 Hermes에 miy 전용 HTTP API가 기본 제공된다고 가정하지 않는다. [Hermes](https://github.com/NousResearch/hermes-agent), [Hermes MCP](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp), [기존 통합 조사](RESEARCH.md#기존-miy에서-확인한-내장-챗봇-경로)

모델 연결은 관리자가 설정하는 공통 Codex·Claude Code 연결을 사용하고 직원별 공급자 로그인은 요구하지 않는다. Codex App Server의 공식 계정·thread·turn 인터페이스를 우선 검증하고, 각 miy 대화에 독립 thread를 배정한다. 코딩 에이전트 실행과 Hermes의 모델 API를 동일시하지 않으며 Hermes 연동과 공급자별 공통 연결의 지원 조건을 확인한다. API LLM과 사내 모델은 명시적으로 선택하는 별도 연결이다. 근거·지원 조건·검증 항목은 [내부 AI 계획](AI.md#공급자별-적용-후보와-확인할-조건)에 모은다.

Rust 코어는 공통 연결·연결 세대와 별도로 사용자·대화·Hermes 세션·공급자 thread·turn의 소유 관계를 보관한다. 내부 어댑터만 공통 인증과 원시 RPC에 접근하고, 이벤트·도구 요청은 저장된 실행 소유자로 라우팅한다. thread 분리를 파일·도구·메모리의 보안 격리로 간주하지 않으며 필요한 실행 격리를 별도로 제공한다. 영속 세션을 보존하면서 유휴 프로세스를 종료할 수 있어야 한다.

초기 DB 백업은 `pg_dump`와 역할·설정 정보를 포함하고, 파일·Git·이미지 저장·필요한 복구 키는 정합성 있는 백업 묶음으로 restic 등에 보관한다. 실행 중 PostgreSQL 데이터 디렉터리를 일반 파일 백업으로 복사하는 것으로 DB 백업을 대신하지 않는다. 파일 변경과 DB 메타데이터의 일관성도 확인하고 새 서버의 실제 복원을 검증한다. 시점 복구 요구가 생기면 물리 백업·WAL 보관 구성을 추가한다. [PostgreSQL SQL Dump](https://www.postgresql.org/docs/current/backup-dump.html), [restic 백업 예제](https://restic.readthedocs.io/en/stable/080_examples.html)

관측은 구조화 로그·작업 이력·상태 확인·메트릭부터 제공한다. OpenTelemetry 연동은 선택 모듈로 두고 언어·신호별 SDK 성숙도와 호환 버전을 따로 고정한다. 외부 수행자가 여러 고객사를 관리하더라도 로그·대화·데이터의 중앙 전송은 고객사 정책을 따른다. [OpenTelemetry 상태](https://opentelemetry.io/status/)

## 초기에 추가하지 않을 구성

| 구성 | 도입을 검토할 조건 |
| --- | --- |
| Kubernetes | 다중 서버·고가용성·자동 스케일 요구가 구체화될 때 |
| 무중단·블루·그린·롤링 배포 | 대상 앱의 일시 중단을 허용할 수 없는 운영 요구가 생길 때 |
| Redis·별도 메시지 브로커 | PostgreSQL 큐·캐시·이벤트의 병목을 측정했을 때 |
| Temporal 같은 별도 워크플로 엔진 | 장시간 분산 실행과 복잡한 보상·재개 요구가 단순 상태 머신 범위를 넘을 때 |
| 별도 벡터·검색 서버 | pgvector와 현재 검색 구성의 품질·규모가 요구를 충족하지 못할 때 |
| 자체 S3 서버의 필수 번들 | 고객사 내 S3 API 제공 자체가 필요해지고 운영·복구 기준을 검증했을 때 |
| 전체 모니터링 서버 묶음 | 기본 운영 화면 이상의 분석·보존·중앙 관제가 필요할 때 |
| Windmill 등 전체 플랫폼 내장 | 현재 추천에는 포함하지 않음. CLI·작업·개발 경험의 개념을 차용 |

## 확정과 버전 고정 기준

실제 버전은 지원되는 안정 릴리스를 대상으로 SDK·MCP·DB·런타임의 호환 조합을 고정한다. Rust 도구체인·대상 CPU·Cargo 의존성을 고정하고 화면 빌드용 Node는 지원 중인 LTS, Python은 선택한 Hermes 릴리스의 지원 버전을 사용한다. 새 릴리스가 있다는 이유만으로 고객사 설치를 자동 갱신하지 않는다.

확정 전에 다음 네 가지를 우선 검증한다.

1. **외부 개발 에이전트 인증:** Keycloak·rmcp·클라이언트별 실제 로그인, 대상 토큰 검사와 권한 회수.
2. **앱 실행 프로필:** CLI 생성 → 완전한 로컬 실행 → 개발 영역 평가 → 커밋 릴리스 승격, 격리·자원 한도와 실패 복구.
3. **설치·코어 업데이트·복원:** 사내·클라우드에서 설치하고 코어를 갱신한 뒤 기존 앱과 데이터를 유지하며 새 환경으로 복원.
4. **공통 AI 연결과 독립 세션:** 공급자별 지원 조건과 Hermes 호환성, 같은 연결을 사용하는 여러 사용자의 thread·이벤트·도구 격리, 개별 취소와 공통 인증 만료·한도 초과 처리. 외부 개발 에이전트의 MCP 로그인과 별도로 검증.

라이선스 확인에서도 구성 요소와 배포 형태를 구분한다. Gitea 커뮤니티 프로젝트와 Hermes는 MIT, Keycloak 본체는 Apache-2.0으로 안내된다. 채택 버전과 포함 의존성·이미지의 고지 파일을 배포 목록에 기록하고, 별도 상용 제품·지원 계약의 조건과 혼동하지 않는다. [Gitea 프로젝트](https://docs.gitea.com/), [Hermes 라이선스](https://github.com/NousResearch/hermes-agent/blob/main/LICENSE), [Keycloak 라이선스](https://github.com/keycloak/keycloak/blob/main/LICENSE.txt)

채택이 결정되면 PRD의 기술 기본안과 개발 계획을 갱신한다. 이 문서는 기술 선택을 제안하는 단계이며 구현 완료나 제품 전체의 호환성 검증을 뜻하지 않는다.
