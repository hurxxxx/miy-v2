# 구현과 검증 결과

최신 보충은 아래 [개발·등록·배포 과정 재검증](#개발등록배포-과정-재검증-bundle-030)에 기록한다. 최초 V0~V1 결과와 후속 로컬 실증을 구분한다.

검증일: 2026-10-21

판정: **V0~V1 범위 통과.** `scripts/up.sh`와 `scripts/check.sh`의 연속 실행이 종료 코드 0으로 완료됐다. Rust 단위 검사 2개, 실제 DB 장애·격리 통합 검사, Playwright 인수 시나리오, 실제 Codex OAuth·MCP 호출·갱신, 독립 OAuth/MCP 검사 15개를 수행했다. Rust 포맷·Clippy의 경고 금지 검사와 프런트 타입 검사도 통과했다.

## 구현 범위

Rust 코어·인증·API/MCP·앱 등록 정보, 별도 DB 실행기, React 플랫폼과 런타임 UI 로더를 구현했다. 대상은 로컬 Docker 기반의 V0~V1이며, 제품 전체와 앱 OCI 배포를 완료한 결과는 아니다.

구현 전 문서·아키텍처·검증 계획은 원격 `main`의 체크포인트 `3cecbe1`에 보존했다. 실행 방법은 [RUNNING](RUNNING.md), 요구 범위는 [IMPLEMENTATION](IMPLEMENTATION.md)을 따른다.

## 확인한 동작

| 영역 | 실제 검증 결과 |
| --- | --- |
| UI 런타임 | 서로 다른 진입점·Module Federation 이름으로 독립 빌드한 기준정보·진단 모듈을 호스트 빌드 이후 새 앱 ID로 등록. iframe·호스트 재빌드 없이 실행 |
| 라우팅 | 플랫폼 메뉴·헤더 유지, 하위 경로·검색 파라미터·직접 링크·새로고침·뒤로 가기·앱 전환 |
| 실패 처리 | React 오류를 앱 영역에 표시하고 포털 메뉴 유지. 지원하지 않는 UI 계약과 미배포 backend 호출은 명시적으로 거부 |
| 웹 인증 | Keycloak OIDC + PKCE, 서버 측 암호화 토큰, Secure·HttpOnly 쿠키, CSRF·Origin 검사, 만료 후 갱신, 로그아웃 복귀 |
| MCP 인증 | 실제 Codex OAuth 로그인, 계약 도구 조회, access token 만료 후 갱신·재조회 |
| 권한 | 개발자의 지정 앱·개발 환경만 허용. 운영 환경·사용 권한만 가진 사용자의 DB 변경 거부. A/B 접근 분리와 권한 회수 |
| API 계약 | 구현한 공개 JSON API 목록과 OpenAPI operation 일치. HTTP·MCP의 공통 도메인 서비스와 manifest 검사 |
| DB 격리 | 코어·인증·앱·환경별 DB, 객체 소유·migration·실행 역할 분리. 실행 역할의 타 DB·DDL·임시 테이블·migration 이력 변경 거부 |
| DB 변경 | 기존 레코드 보존, 새 컬럼·테이블 적용, 신규 객체의 실행 역할 권한, 실패 migration의 트랜잭션 롤백 |
| 작업 복구 | 중복 요청의 같은 작업 ID, 다른 입력의 멱등성 충돌. DB 생성 직후·migration 직후 실행기 강제 종료와 재개 |
| Migration 이력 | 적용 파일 checksum 변경과 알 수 없는 적용 이력을 거부 |
| 독립 OAuth 검사 | metadata·PKCE·resource binding·Resources 조회, 잘못된 서명·audience·resource·만료 토큰 거부. 세션 철회·refresh 거부와 offline grant 철회 |

기본 Rust 검사에서는 계약·경로·산출물 변조 검사를 수행한다. 실제 서비스가 필요한 DB 검사는 일반 검사에 `ignored`로 표시하고 전체 검사 스크립트가 별도로 실행한다. 실제 Codex 검사와 독립 OAuth 클라이언트 검사를 구분하여 기록한다.

## 환경과 증거

- Apple Silicon macOS, Docker Desktop Linux 엔진, Rust 1.99.1, pnpm 10.34.5, Codex CLI 0.163.0.
- Axum 0.8.9, SQLx 0.8.6, rmcp 3.6.0, React 19.3.1, Vite 8.3.5, Module Federation Vite 1.23.5·runtime 2.9.2.
- PostgreSQL 18.3, Keycloak 26.8.1, Caddy 2.11.2. 기반 이미지 digest와 전체 패키지 잠금을 저장했다.
- 검증 브라우저는 전용 컨테이너의 신뢰 저장소에 로컬 CA를 등록했다. 인증서 검증을 끄지 않았으며 호스트의 시스템 신뢰 설정을 변경하지 않는다.
- `.local/evidence/`에 브라우저·DB·Codex·MCP 결과, 플랫폼 빌드 해시, UI 산출물·작업·설치 식별자와 화면을 저장한다. `.local`은 Git에서 제외한다.
- `scripts/check.sh`가 전체 실행 순서를 담당하고 `scripts/report.py`가 결과를 취합한다. 누락된 검사 결과를 성공으로 생성하지 않는다.

## 구현 중 확인하여 반영한 사항

검증용 Keycloak realm의 scope·subject mapper를 명시했다. 웹 토큰과 MCP 토큰의 audience를 나누고, MCP 리소스 클라이언트 ID를 canonical resource URI와 일치시켜 introspection의 audience 검사도 유지했다.

Codex 갱신에 필요한 `offline_access`를 명시적인 client scope·개발 계정 역할로 구성했다. 일반 세션 로그아웃과 offline client grant 철회는 각각 검사한다. 이 연결은 외부 개발자의 플랫폼 MCP 인증이며 내장 Hermes의 모델 연결 검증을 대신하지 않는다.

## 개발 화면의 간편 로그인

로컬 개발에서는 `http://localhost:18000`의 계정 버튼으로 비밀번호·인증서 설정 없이 접속한다. 관리자·개발자·사용자 A/B를 제공하고, 서버에 개발 세션을 저장해 기존 권한 검사를 적용한다. 명시적인 개발 설정과 loopback 서비스 주소가 함께 있을 때만 활성화한다. 실제 OAuth 로그인은 별도로 유지한다.

추가 변경 후 Rust 단위 검사 3개, Clippy·포맷·타입 검사, Playwright 시나리오 3개를 통과했다. 계정 버튼·로그아웃·계정 전환·잘못된 Origin/계정 거부와 기존 실제 OAuth 세션 갱신·권한·DB 준비를 확인했다. HTTP에서 동적 앱 모듈과 하위 라우팅도 확인했으며 HTTPS 요청이나 iframe 없이 실행됐다. 화면과 별도 실행 기록은 `.local/evidence/development-*`에 저장했다. 간편 로그인은 OAuth·MCP 인증 통과 증거를 대체하지 않는다.

## 다음 단계와 적용 한계

2026-10-22 추가 실증에서는 CLI로 만든 별도 앱을 실제 Codex가 개발하고 로컬 Docker에 배포했다. [휴가 관리 대장 실증 결과](LEAVE-LEDGER-RESULTS.md)에 38개 독립 인수 검사, 실제 이미지·소스 연결과 보완 기록을 정리했다. 아래 항목은 최초 V0~V1 검증 당시의 다음 단계이며, 추가 실증을 정식 V2~V7 전체 통과로 확대하지 않는다.

최초 V0~V1 이후에는 독립 앱용 CLI·SDK·개발 지침, Codex 앱 개발, 로컬 단일 이미지 배포와 실제 backend 연결을 추가했다. 아래 bundle 0.3.0 실증에서는 스키마 변경 업데이트와 실패 migration의 전진 복구까지 확인했다. 별도 Linux 서버·원격 이미지 전송·정식 운영 승격·백업 복원·전체 흐름의 독립 3회 반복은 아직 수행하지 않았다.

최초 V0~V1 릴리스는 UI·SQL fixture이며 `not_deployed`로 유지한다. 후속 실증의 Docker 앱은 실제 실행 상태를 별도로 제공한다. 실제 고객 데이터·기업 SSO 전체·신뢰하지 않는 UI 코드·다중 서버·전체 자원 관리·백업 복원·Hermes는 통과 범위에 포함하지 않는다.

코어의 프로세스 환경에서는 DB 생성 자격증명을 제거했다. 동일 OS 계정의 로컬 프로세스 분리를 제품의 OS 계정·컨테이너 격리 완료로 해석하지 않는다. Keycloak `resource-indicators`는 실험 기능이며 다른 인증 제품·개발 클라이언트의 호환성은 별도 검증해야 한다.

## 개발·등록·배포 과정 재검증 (bundle 0.3.0)

검증일: 2026-10-22 KST. 목적은 휴가 업무 기능이 아니라 **독립 개발자가 지침·SDK·MCP로 앱을 만들고 CLI로 등록·배포·갱신할 수 있는지** 확인하는 것이다. 판정은 **이번에 정한 로컬 개발 경로 통과**이며, 원격 서버를 포함하는 정식 V2~V7 완료는 아니다.

### 바꾼 사항

- CLI·MCP 개발 지침을 같은 파일에서 제공하고 bundle 0.3.0으로 고정했다. SDK·Dockerfile·도구체인·지침·초기 migration의 해시를 CLI 내장 번들과 비교한다. 기존 bundle은 자동 업그레이드하지 않는다.
- `app check`에 Rust 포맷, 보호 파일 무결성, migration 이름·번호·트랜잭션 규칙, frozen 의존성 설치, UI 빌드와 Federation 진입점·CSS/JS 자산 검사를 포함했다. `app build`도 같은 검사를 실행한다. SDK를 수정하지 말라는 지침과 포맷 검사의 충돌을 수정했다.
- 등록 신청 → 관리자 승인/거절 → dev 권한 부여 → 사용자 권한 부여를 CLI 명령으로 연결했다. 미승인 신청은 권한을 주지 않는다. 승인·거절 상태와 앱 생성·개발 권한 부여는 DB 트랜잭션으로 처리한다.
- 제출 전 멱등성 키 저장, `--no-wait`, `app resume`, 명시적 `--retry`, 오래된 빌드 배포 거부를 추가했다. 결과 JSON과 하위 빌드 로그를 stdout/stderr로 구분한다.
- 서버도 동일한 migration·UI 산출물 검사를 사용한다. DB migration이 실패하면 앱 상태와 함께 DB 상태도 `failed`로 표시한다.

### 실제 수행 결과

| 검사 | 결과 |
| --- | --- |
| 새 템플릿 3개 | 각각 생성 직후 포맷·Rust 테스트·TypeScript·UI 빌드 통과. 컴파일 캐시는 공유했으며 소스 폴더는 별개 |
| 잘못된 개발 자산 | SDK/Dockerfile 변경, 잘못된 bundle/UI 진입점, 중복 migration 번호, 명시적 트랜잭션·no-transaction 거부 |
| CLI 등록과 권한 | 일반 사용자의 신청 거부, 개발자 자기 승인 거부, 미승인 배포 거부, 관리자 승인·반복 승인, CLI 사용자 권한 부여, 개발자의 prod 제어 거부 |
| 계약 일치 | 실행 중인 계약 API와 CLI가 생성한 지침·보호 파일 해시 일치. MCP도 같은 도메인 응답 사용 |
| 실제 Codex v1 | 별도 앱 폴더에서 MCP 조회 후 Rust·React·SQL·테스트·README 작성. 추가 수정 요청 없이 check 통과. 입력 단위 테스트 4개 |
| 실제 Codex v2 | 기존 0001/0002 migration을 보존하고 새 tag 컬럼·API·UI 개발. 추가 수정 요청 없이 check 통과. 단위 테스트 7개 |
| CLI 배포와 DB | Git 커밋 후 CLI build/deploy로 이미지·DB·컨테이너 준비. 앱 등록·승인·권한은 수동 API 스크립트 없이 CLI로 수행 |
| 응답 유실 복구 | worker를 잠시 멈추고 제출 응답의 로컬 job ID만 지우는 장애를 주입. 같은 명령 재실행이 같은 서버 작업 ID를 반환. worker 재개 후 resume 성공 |
| UI 통합 | 기존 플랫폼 빌드 그대로 실제 브라우저 생성·상세 링크·직접 링크 새로고침. iframe 없이 동작, 브라우저 JS 오류 없음 |
| 앱 제어 | CLI 중지 시 API 503, 시작 후 동일 레코드 보존 |
| 스키마 변경 업데이트 | 0.1.0 → 0.1.1에서 새 컬럼 추가, 기존 ID·소유자·제목·내용 보존, 새 컬럼의 조회·변경과 v2 UI 동작 |
| 실패 migration | 0.1.2에서 컬럼 추가 직후 의도적 SQL 오류. DDL과 migration 이력 롤백, 기존 레코드 보존, 앱 정지·실패 상태 표시 |
| 전진 복구 | 적용되지 않은 migration을 수정한 새 이미지 0.1.3을 CLI로 배포. DB ready·앱 running, 기존 데이터와 새 컬럼 확인 |

별도 스크립트의 assertion은 기본/등록 20개, v1 10개, v2 6개, 실패·복구 5개다. 단계별 번들·호스트 해시 반복 확인도 이 수에 포함되므로 독립 요구사항 41개를 의미하지 않는다. 플랫폼 Rust 단위 검사 7개, Clippy 경고 금지, 포맷·타입 검사, 기존 Playwright 3개 시나리오와 실제 DB 격리·이력·실행기 장애 복구 회귀 검사도 통과했다. Playwright 직접 실행 시 로컬 CA 환경 변수를 누락한 실패 1회가 있었으며, 신뢰할 CA를 지정한 재실행은 통과했다. TLS 검증을 끄지 않았다.

Codex v1은 실행 명령 실패가 없었다. v2는 README 편집 명령의 인코딩 오류 1회를 스스로 수정하고 보고했다. 평가자가 앱 업무 코드를 대신 수정하거나 지침 피드백으로 고치도록 요청하지 않았다. Git 커밋·이미지 빌드·배포는 지침에 따라 평가자가 CLI로 실행했다. 0.1.2/0.1.3의 실패 주입·수정 migration은 평가 스크립트가 만든 시험 데이터이며 Codex가 구현한 기능으로 집계하지 않는다. v1/v2 개발·배포 중 제공 번들 및 플랫폼 UI의 해시는 동일했다.

### 소스·실행 증거

- 앱: `~/projects/miy-apps/workflow-notes-mvib6wzm` (별도 Git 저장소).
- 접속: `http://localhost:18000/apps/workflow-notes-mvib6wzm`. 사용자 A로 검증 레코드를 확인한다.
- v1 커밋: `038d0c93b323c3d4f509a170c2ce4686c1e1d86d`.
- v2 커밋: `f3d519a1d4da04b2cf92e90d778efa03aeb3c4f5`.
- 최종 복구 커밋: `dab1648cc54df3bb4b7e2e5af16ca1e7fc316f3a`, 릴리스 `0.1.3`.
- 최종 실행 이미지 ID: `sha256:e0c3cbfcd66785b5f232559455c34aca1acbdff23b32aee8b7d42302d1c57ea3`.
- 검사 코드: `scripts/check-developer-workflow.mjs`, `scripts/check-workflow-deployment.mjs`.
- 로컬 증거: `.local/evidence/workflow-mvib6wzm/{baseline,v1,v2,failure}.json`, 명령 로그와 화면. 실제 Codex 기록은 `.local/evidence/workflow-{v1,v2}-codex.jsonl`.

### 남은 범위

CLI 로그인은 localhost 개발 계정이며 정식 CLI OAuth는 미구현이다. 실제 Codex MCP OAuth와 기존 웹 OAuth 회귀 검사는 별도로 수행했다. 이미지 전송은 같은 Docker Engine의 로컬 저장소에 한정하며, 원격 레지스트리·별도 Linux 서버·운영 승격은 미검증이다.

실패 후 **새 릴리스로 전진 복구**를 확인했으며 자동 구버전 롤백·백업 복원·임의의 비호환 스키마 변경 복구를 검증한 것은 아니다. CLI의 정적 SQL 검사는 실제 DB migration 리허설을 대신하지 않는다. 새 템플릿 생성은 3회, 실제 Codex 전체 개발 경로는 한 앱의 v1/v2 1세트다. 다음 검증은 정식 CLI 인증과 원격 이미지 전송을 연결한 뒤 전체 개발·배포 경로를 독립적으로 3회 반복하는 것이다.

## 수동 개발 경로에서 발견한 환경·배포 문제

2026-10-22, 사용자의 Codex 대화 `01a160b8-0cad-72f2-9a38-f3e3736a9308`과 `my-local-test` 계산기 배포를 조사했다. 자동 평가가 제공한 환경을 일반 사용자의 Codex 세션도 자동으로 받는 것으로 볼 수 없었다.

| 증상 | 확인한 원인 | 조치 |
| --- | --- | --- |
| Codex에 miy_app MCP가 없음 | 평가 스크립트만 프로세스별 MCP 설정을 전달. app init/login은 일반 Codex 세션을 설정하지 않음 | 프로젝트 전용 `app setup-codex`, MCP·CA·PATH를 전달하는 `app codex` 실행/대화 재개 명령 추가 |
| cargo가 없다고 보고 | Rust는 설치돼 있었으나 Codex PATH에 `~/.cargo/bin`이 없었음 | CLI check/build가 rustup 경로를 탐색. doctor 진단과 Codex 실행 환경에 Rust PATH 제공 |
| 계산기 대신 기본 앱 화면 | 기본 앱 커밋 d7b855c와 계산기 수정 커밋 76a46a4를 모두 0.1.0으로 빌드. 새 이미지는 immutable_release_conflict로 거부되고 기존 앱 계속 실행 | 새 버전 0.1.1, 커밋 7a89f1a로 재빌드·배포. status에 활성 버전·로컬 이미지 일치 여부·마지막 작업과 해결 안내 추가 |

계산기 개발 세션은 MCP 미연결에 대해 사용자에게 확인받고 로컬 계약으로 진행했으며, Rust 검사 실패도 보고했다. 이 동작을 Codex의 무단 계약 우회로 판단하지 않는다. 빠진 부분은 수동 개발 환경 준비와 배포 실패 설명이었다. TypeScript 오류 한 건은 해당 세션이 스스로 수정해 재검사했다.

후속 확인: Rust PATH를 제외한 환경에서 전체 app check 통과, 계산기 테스트 12개 통과, 새 실행기로 실제 Codex `get_contract`·`search_api` 호출 및 cargo 버전 조회 성공. 코어 단위 검사 9개와 Clippy 경고 금지 검사도 통과했다. 계산기 0.1.1의 실제 Docker 이미지 ID는 `sha256:bd69c85419b09bb963e708da45a3da1e20b6d0e8103a2e998395eaec93e67c4e`이며 기존 설치 ID를 유지했다.

접속은 `http://localhost:18000/apps/my-local-test`, 개발자 또는 관리자 계정을 사용한다. 등록된 메뉴 이름은 `내 로컬 테스트`, 앱 내부 제목은 `계산기`다. 일반 사용자에게는 별도 사용 권한이 필요하며 이 수정에서 권한을 확대하지 않았다. `scripts/check-calculator-recovery.mjs`로 배포 이미지 일치·버튼/키보드 연산·오류 복구·직접 URL 새로고침을 검사하고 `.local/evidence/calculator-*`에 결과를 기록한다.
