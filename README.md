# miy-v2

전담 IT 운영팀이 없는 중소기업을 위한 AI 앱 개발·배포·운영 플랫폼이다. 기업별 사내 서버 또는 클라우드에 설치하고, 고객사·개발사·파트너가 같은 앱 계약으로 개발·운영한다. `miy-v2`는 내부 프로젝트명이며 서비스명은 `miy`다.

CLI와 MCP로 독립 Rust/React 앱을 만들고 로컬 개발 → dev 배포 → 검증한 동일 산출물의 prod 승격으로 연결한다. 플랫폼 화면은 통합 라우터를 사용하고, 업무 앱은 설치별 출처 iframe에서 실행한다. 공통 인증·앱 접근 권한은 플랫폼이, 업무 데이터·선택적 공간·내부 ACL은 각 앱이 소유한다.

## 현재 단계

앱 개발·실행·배포 기반과 기본 관리 화면은 개발 서버에 반영했다. 공통 업무 ACL, 파일·알림·업무 이벤트, 내부 Hermes AI와 베타 전 종합 인수는 후속이다. **기능 구현, 서버 반영, CLI 게시, 사용자 실기 인수는 별개다.** 완료·마무리 대기·다음 순서는 [PLAN](docs/PLAN.md#전체-작업-현황) 한 곳에서 확인한다.

## 먼저 읽기

| 목적 | 문서 |
| --- | --- |
| 전체 상태와 다음 작업 | [추진 계획](docs/PLAN.md) |
| 제품 범위와 원칙 | [PRD](docs/PRD.md) |
| 앱 개발·CLI 사용 | [실행 안내](docs/RUNNING.md) |
| 이 서버의 주소·배포 상태·반영 절차 | [서버 운영 안내](docs/SERVER-OPERATIONS.md) |
| 플랫폼 수정·검사 | [AGENTS.md](AGENTS.md), [검사 지도](docs/CHECKS.md) |
| 주제별 설계와 과거 기록 찾기 | [문서 지도](docs/README.md) |

## 개발할 때

이 저장소는 플랫폼 코어·CLI·웹·SDK·설치 도구를 개발한다. 생성한 업무 앱의 지침은 `templates/`에서 별도로 관리한다. 기본 거부 권한, 적용된 SQL·업무 데이터 보존, 게시 번들·컴포넌트 불변 원칙을 지킨다.

이 호스트의 `.local`과 `web/platform/dist`는 실제 개발 서버가 사용한다. 검증은 [CHECKS](docs/CHECKS.md)의 격리 도구를 쓰고, 서버 반영은 [운영 절차](docs/SERVER-OPERATIONS.md)를 따른다. 새 서버 설치 경로의 프레임 런타임 준비와 사용자 Windows 인수는 아직 남아 있다.

현재 기술 선택은 [STACK](docs/STACK.md), 공통 기능·UX 이식 원본과 대상은 [AI-DO 이식 목록](docs/AI-DO-COMMON-MIGRATION.md), v1의 운영·권한 교훈과 기존 작업 ID는 [V1 적용 계획](docs/V1-ADOPTION.md)에 있다. 과거 시험과 최초 설계는 [archive](docs/archive/README.md), 실행별 증거는 [releases](docs/releases/)에서 보존한다. 과거 PASS를 현재 소스나 새로운 환경의 결과로 이월하지 않는다.
