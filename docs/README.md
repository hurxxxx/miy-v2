# 문서 지도

- 정리일: 2026-11-14

먼저 PLAN의 [완료 요약](PLAN.md#완료한-작업)·[일부 구현](PLAN.md#일부-구현된-작업)·[남은 구현 순서](PLAN.md#다음-우선-작업)·[검증과 반영 대기](PLAN.md#검증과-반영-대기)를 확인한다. 제품 범위가 필요하면 [PRD](PRD.md)를 읽는다. 작업에 해당하는 아래 담당 문서만 추가로 읽는다. 플랫폼 개발 규칙은 [AGENTS.md](../AGENTS.md)를 따른다.

## 현재 기준

| 질문·작업 | 담당 문서 | 이 문서가 관리하는 것 |
| --- | --- | --- |
| 무엇을 만들며 어디까지 제공하나 | [PRD](PRD.md) | 제품 요구·확정 원칙·제외/후속 범위 |
| 무엇을 끝냈고 다음에 무엇을 하나 | [PLAN](PLAN.md) | 전체 상태·유일한 작업 순서·남은 결정 |
| 변경 후 무엇을 검사하나 | [CHECKS](CHECKS.md) | 영역별 최소 명령·실행 환경·판정 범위 |
| 포털·콘솔·로그인 경계 | [APP-BOUNDARIES](APP-BOUNDARIES.md) | 세 출처·세션·보안 범위·BV 인수 |
| 업무 앱 화면과 브리지 | [APP-FRAMES](APP-FRAMES.md) | 설치별 출처·프레임 인증·브리지·배치·FV 인수 |
| 앱 접근·위임·공통 업무 ACL | [CORE-PERMISSIONS](CORE-PERMISSIONS.md) | 권한 행렬·현재 집행·앱 소유 ACL의 후속 계약 |
| 앱·릴리스·데이터 개발 계약 | [DEVELOPMENT](DEVELOPMENT.md) | 앱 구현 계약과 명시적으로 구분한 장기 설계 |
| AI-DO의 UX·공통 기능을 무엇부터 옮기나 | [AI-DO-COMMON-MIGRATION](AI-DO-COMMON-MIGRATION.md) | 최신 dev 소스별 대상·현재 v2 공백·보존할 UX·플랫폼/공용 UI/앱 책임·인수 기준 |
| miy v1에서 참고할 계약과 기존 적용 ID | [V1-ADOPTION](V1-ADOPTION.md) | V1 ID·채택/제외·권한/운영 교훈·UI 책임·부분 구현. 현재 AI-DO 이식 대상과 과거 UI-1~5 이력을 구분 |
| 자원·복구·새 서버의 완료 조건 | [CORE-HARDENING](CORE-HARDENING.md), [CAPACITY](CAPACITY.md) | 코어 안정화/종합 인수와 자원 예산·개별 제한 |
| 내장 AI 설계 | [AI](AI.md) | 공통 연결·Hermes·사용자별 세션·도구 승인. 개발 에이전트 인수와 구분 |
| 기술 선택 | [STACK](STACK.md) | 현재 선택과 선택 근거, 아직 구현되지 않은 기술 구분 |
| CLI·로컬 개발·앱 배포 사용법 | [RUNNING](RUNNING.md) | 네이티브 개발·DB 보존·원격 빌드·승격·서버 설정 안내 |
| 실제 서버와 반영·복구 | [SERVER-OPERATIONS](SERVER-OPERATIONS.md) | 주소·배포 소스·schema·실제 설정·반영 도구 |
| CLI 후보·게시·패키징 | [CLI-RELEASE](CLI-RELEASE.md) | 소스와 게시 버전·후보 검사·배포물·릴리스 절차 |
| Windows 설치·갱신·제거 인수 | [WINDOWS-ACCEPTANCE](WINDOWS-ACCEPTANCE.md) | 현재 후보·설치기 계약·사용자 수행 절차·NOT_RUN 범위 |

공통 컴포넌트 전 [코어 준비 리뷰](reviews/CORE-PLATFORM-READINESS-2026-11-13.md)는 R01~R13의 검토 근거·인수 조건을 보존한다. 각 항목의 작업 묶음은 PLAN에 통합하며 리뷰를 두 번째 작업 순서표로 쓰지 않는다.

아키텍처 도식의 위치는 [architecture 안내](architecture/README.md), 외부 사례·선택 근거는 [조사 자료](reference/RESEARCH.md)에서 찾는다. 참고 자료의 제안을 구현 완료나 새로운 작업 순서로 해석하지 않는다.

## 과거 기록과 증거

- [archive 목록](archive/README.md): 완료된 계획, 이전 CLI·Windows·독립 AI 인수, 기본 UI 이식, 누적 결과와 이전 도식.
- [releases](releases/): 실행별 소스·환경·명령·PASS/FAIL/NOT_RUN, 배포·복원 기록과 당시 도구 사본.

이전 경로를 가리키는 JSON의 source/file/hash 값은 실행 당시 출처이므로 그대로 둔다. 문서의 새 위치는 archive 목록과 [정리 기록](releases/docs-cleanup-2026-11-13.json)에서 찾는다. 구 문서의 실패·미실행 결과는 새 문서 정리나 후속 PASS로 지우지 않는다.

2026-11-14의 완료/잔여 통합·현재 상태 정정·운영 이력 분리는 [문서 통합 기록](releases/docs-consolidation-2026-11-14.json)에 있다. 업무 개발 지침·게시 컴포넌트·예제는 해당 버전의 계약으로 보존했다.

## 문서 작성 규칙

- 한 주제의 규범은 위 담당 문서 한 곳에 둔다. 새 변경은 그 문서를 갱신하고 다른 곳에는 필요한 연결만 남긴다.
- 상태·순서는 PLAN, 실제 서버 상태는 SERVER-OPERATIONS, CLI 게시 상태는 CLI-RELEASE가 관리한다. 설계·조사 완료, 일부 기반 구현, 실제 구현 중, 환경별 인수 대기를 구분한다. 문서 머리말마다 배포 버전·주소·이력을 복제하지 않는다.
- 명령 사용법은 RUNNING, 검사 명령은 CHECKS, 실행 결과는 releases에 쓴다. 계획 문서를 날짜별 진행 로그로 늘리지 않는다.
- 구현·격리 검증·개발 서버 반영·CLI 게시·사용자 실기를 구분한다. 실행하지 않으면 NOT_RUN, 실행 후 실패는 FAIL이며 이전 PASS를 확대하지 않는다.
- 과거 문서는 archive로 옮기고 의미·증거는 보존한다. 관련 상대 링크·앵커를 함께 고치며, 리다이렉트 전용 문서를 새 정본처럼 늘리지 않는다.
- 게시 SDK·템플릿·컴포넌트 바이트와 적용된 migration은 문서 정리를 이유로 바꾸지 않는다. 비밀값은 어떤 문서·증거에도 넣지 않는다.
