# miy-v2 문서 지도

- 정리일: 2026-11-07

주제마다 **담당 문서 하나**가 현재 기준을 관리한다. 다른 문서는 그 문서로 연결하고 같은 내용을 따로 유지하지 않는다. 구현된 동작은 코드와 테스트가 최종 기준이다. 작업 순서는 [추진 계획](PLAN.md#다음-우선-작업)의 작업표 하나만 따른다.

기본 UI 이식 UI-1~5는 ai-do 기반으로 완료한 이력이다. 향후 설계는 공식 miy v1을 기준으로 하며, 플랫폼의 사용자·그룹별 앱 사용 권한과 앱 내부의 선택적 데이터 공간을 적용한다. 원본 변경을 기존 UI 검증 결과의 변경으로 표시하지 않는다.

현재 기준은 세 문서다. [APP-BOUNDARIES](APP-BOUNDARIES.md)는 사내 업무용 최소 설계로 두 출처·세션 용도·CLI 승인 보호와 호환성, 필수/후속·한계·인수를 관리한다. [CORE-PERMISSIONS](CORE-PERMISSIONS.md)는 권한과 PLM 등 독립 앱의 공통 ACL, [PLAN](PLAN.md)은 작업 순서를 담당한다. iframe 없는 MF·단일 IP HTTPS를 유지하며 별도 IdP/dev 출처·외부 앱 격리·자동 CA는 후속이다. 격리 환경의 검증 뒤 서버·UI `a4ad370`으로 2026-11-07 운영에 반영했다([배포 기록](releases/internal-minimum-security-2026-11-07-deployment.json)). 게시 CLI `01e4d49`는 유지하며 운영 콘솔은 `https://v2.dwdcc.kr:9443`, 공유 개발 로그인은 꺼짐 상태다. 공개 서버를 사내 범위로 자동 인정하지 않고 현행 배포와 역사 증거는 구분한다.

## 현재 기준 문서

| 주제 | 담당 문서 | 관리하는 내용 |
| --- | --- | --- |
| 앱 실행·관리 경계 | [APP-BOUNDARIES.md](APP-BOUNDARIES.md) | 사내 두 출처 최소 설계·기존 SDK/CLI 호환, 필수/후속/한계·AB 결정·임시 기준·전환·BV 인수와 V0~V7 승계 |
| 제품 요구사항 | [PRD.md](PRD.md) | 제품 정의, 합의 사항, 기능 요구사항, 인수 조건, 후속 범위 |
| 작업 순서 | [PLAN.md](PLAN.md) | 다음 우선 작업표·진행 상태, 코어 묶음 요약, 보류 중인 사용성 개선. 최초 로드맵은 archive |
| 검사 | [CHECKS.md](CHECKS.md) | 변경 영역별 최소 검사, 정적·격리 Docker·로컬 스택·운영 검사의 구분. 플랫폼 개발 규칙은 루트 `AGENTS.md` |
| v1 적용 | [V1-ADOPTION.md](V1-ADOPTION.md) | 후속 설계 기준인 공식 v1(`/projects/miy`)의 검토·재검증 결과, 적용 항목(ID `V1-*`)과 제외 항목, 실패 교훈과 남은 결정 사항 |
| 기본 UI | [UIUX-MIGRATION.md](UIUX-MIGRATION.md) | 세 영역의 후속 UI 책임과 v1 공용 UI 경계. 완료된 UI-1~5는 archive |
| 코어 안정화 | [CORE-HARDENING.md](CORE-HARDENING.md) | worker·권한·자원·복구·새 서버 인수 1~5차의 완료 기준과 증거 |
| 코어 권한 | [CORE-PERMISSIONS.md](CORE-PERMISSIONS.md) | 권한 행렬, 설치/릴리스 승인·위임·앱 후속 호출, PLM 등 독립 앱의 공통 ACL·업무 연계 목표와 현행 소스의 차이. archive는 22c219d 소스 고정본이며 운영 상태와 구분 |
| 앱 개발 | [DEVELOPMENT.md](DEVELOPMENT.md), [NATIVE-DEVELOPMENT.md](NATIVE-DEVELOPMENT.md) | 앱 개발 계약, CLI 흐름, 로컬·네이티브 실행, 개발 영역 평가와 운영 승격 |
| CLI 사용·서버 구성 | [RUNNING.md](RUNNING.md) | CLI 설치·인증, 앱 등록·빌드·dev 배포·prod 승격, 공용 컴포넌트 갱신, 현행 서버 구성과 단일 IP HTTPS 지원 목표의 구분 |
| 운영 서버 | [SERVER-OPERATIONS.md](SERVER-OPERATIONS.md) | v2.dwdcc.kr 구성, 게시 CLI, 배포 소스와 기록, 되돌리기 위치 |
| CLI | [CLI-IMPLEMENTATION-PLAN.md](CLI-IMPLEMENTATION-PLAN.md), [CLI-RELEASE.md](CLI-RELEASE.md), [CLI-VALIDATION-MATRIX.md](CLI-VALIDATION-MATRIX.md) | 명령·인증·OS 계약, 배포 후보·게시 절차, 요구별 검증 범위 |
| Windows | [WINDOWS-ACCEPTANCE.md](WINDOWS-ACCEPTANCE.md), [WINDOWS-INSTALLER.md](WINDOWS-INSTALLER.md), [WINDOWS-VALIDATION.md](WINDOWS-VALIDATION.md) | 현재 게시 후보(0.7.0)의 Windows 11 인수 절차·출처 검사기, 설치 파일 동작, 사용자 수행 빌드·검증 절차와 실행 기록. [0.6.3 인수](WINDOWS-0.6.3-ACCEPTANCE.md)는 당시 후보의 이력 |
| 자원 | [CAPACITY.md](CAPACITY.md) | CPU·메모리·파일·DB 예산과 한도 초과 대응 |
| 내부 AI | [AI.md](AI.md) | 공통 AI 연결, 사용자별 독립 세션, Hermes 연동 계획 |
| 기술 선택 | [STACK.md](STACK.md), [FRONTEND.md](FRONTEND.md), [RESEARCH.md](RESEARCH.md) | 스택 추천안, 프론트엔드 비교, 유사 프로젝트 차용 판단 |
| 구조도 | [architecture/README.md](architecture/README.md) | 현재 Mermaid 정본 안내와 과거 SVG·PNG·PDF 보관 위치 |

## 실행 결과와 증거

| 문서 | 내용 |
| --- | --- |
| [IMPLEMENTATION-RESULTS.md](IMPLEMENTATION-RESULTS.md) | 날짜·소스·버전별 누적 실행 이력. 과거 MF 결과를 새 경계 PASS로 옮기지 않음 |
| [CORE-TESTING.md](CORE-TESTING.md) | 게시 설치기와 독립 AI 앱 개발 인수 절차 |
| [INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md](INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md), [CLI-0.6.3-LINUX-VALIDATION.md](CLI-0.6.3-LINUX-VALIDATION.md), [LEAVE-LEDGER-RESULTS.md](LEAVE-LEDGER-RESULTS.md) | 특정 시점의 인수 결과 |
| [releases/](releases/) | 버전별 후보·검증·배포 JSON 기록과 배포 스크립트 사본 |

결과 문서는 실행 당시의 기록이다. 새 실행은 새 기록으로 추가하고, 이전 PASS를 새 소스의 통과로 바꿔 적지 않는다. 장비나 인증이 없어 실행하지 못한 단계는 `NOT_RUN`, 실행 후 실패한 단계는 `FAIL`로 둔다.

[VALIDATION.md](VALIDATION.md)는 이전 링크를 위한 이동 안내만 남겼다. 인수 기준과 승계표는 APP-BOUNDARIES 한 곳에서 유지한다.

## 보관 문서

- [최초 MF 검증 계획](archive/VALIDATION-MODULE-FEDERATION.md), [V0~V1 구현 계획](archive/IMPLEMENTATION-V0-V1.md), [최초 제품 로드맵](archive/PLAN-INITIAL-ROADMAP.md)
- [UI-1~5 이식·리뷰](archive/UIUX-MIGRATION-UI1-5.md), [0.7.0 권한 계약](archive/CORE-PERMISSIONS-0.7.0.md)
- [2026-10 구조도와 SVG·PNG·PDF](archive/architecture-2026-10/README.md)


[archive/](archive/)에는 완료된 인계·전달 절차를 둔다. 당시 기록이며 현재 상태가 아니다.

| 문서 | 당시 용도 |
| --- | --- |
| [archive/CLI-HANDOFF.md](archive/CLI-HANDOFF.md) | CLI 0.5~0.6.3 시기의 환경 간 개발 인계 |
| [archive/CLI-EXPERIENCE-SERVER-HANDOFF.md](archive/CLI-EXPERIENCE-SERVER-HANDOFF.md) | CLI 0.6.3 Linux 서버 검증·반영 전달 절차 |
| [archive/LINUX-TODO-HANDOFF.md](archive/LINUX-TODO-HANDOFF.md) | Windows 네이티브 개발 이후 Linux 배포 검증 전달 프롬프트 |

`docs/TODO.md`의 열린 항목은 [추진 계획](PLAN.md#보류-중인-사용성-개선)으로 옮기고 파일은 삭제했다. 이전 내용은 Git 이력에 남아 있다.

## 문서 작성 규칙

- 새 계획은 담당 문서에 추가하고, 이 지도에 담당 범위가 없을 때만 새 문서를 만든다.
- 진행 로그를 계획 문서에 계속 쌓지 않는다. 현재 상태만 남기고 실행 이력은 결과 문서나 `releases/`로 보낸다. v1의 `platform-redesign/`이 진행 로그로 수천 줄까지 커진 전례를 피한다.
- 비밀값(`credentials.json`, `runtime.env`, 토큰, 쿠키, 접속 문자열)은 문서·증거에 넣지 않는다.

새 규범을 정할 때 기존 본문의 관련 목표를 함께 교체한다. 현재 규범에는 상충하는 선택지를 계속 덧붙이지 않는다. 게시 번들·템플릿의 문서는 그 버전의 보호된 산출물이므로 문서 정리만으로 변경하지 않고 새 구현·번들 버전에서 전환한다.
