# V0~V1 구현과 검증 결과

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

## 다음 단계와 적용 한계

V2에서 이 계약을 독립 앱용 CLI·Rust SDK·배포 가능한 UI 패키지·개발 지침·검사 하네스로 제공한다. V3의 Codex 업무 앱 개발, V4의 단일 OCI 이미지·Linux 서버·Docker 설치, V5의 실제 backend 연결, V6의 앱 업데이트·삭제·복구, V7의 전체 흐름 3회 반복은 아직 수행하지 않았다.

현재 릴리스는 검토한 UI·SQL fixture다. DB 준비 후에도 앱 컨테이너 상태를 `not_deployed`로 유지한다. 실제 고객 데이터·기업 SSO 전체·신뢰하지 않는 UI 코드·다중 서버·자원 한도·백업 복원·Hermes는 이번 통과 범위에 포함하지 않는다.

코어의 프로세스 환경에서는 DB 생성 자격증명을 제거했다. 동일 OS 계정의 로컬 프로세스 분리를 제품의 OS 계정·컨테이너 격리 완료로 해석하지 않는다. Keycloak `resource-indicators`는 실험 기능이며 다른 인증 제품·개발 클라이언트의 호환성은 별도 검증해야 한다.
