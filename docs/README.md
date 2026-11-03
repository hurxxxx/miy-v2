# miy-v2 문서 지도

- 정리일: 2026-11-03

주제마다 **담당 문서 하나**가 현재 기준을 관리한다. 다른 문서는 그 문서로 연결하고 같은 내용을 따로 유지하지 않는다. 구현된 동작은 코드와 테스트가 최종 기준이다. 작업 순서는 [추진 계획](PLAN.md#다음-우선-작업)의 작업표 하나만 따른다.

## 현재 기준 문서

| 주제 | 담당 문서 | 관리하는 내용 |
| --- | --- | --- |
| 제품 요구사항 | [PRD.md](PRD.md) | 제품 정의, 합의 사항, 기능 요구사항, 인수 조건, 후속 범위 |
| 작업 순서 | [PLAN.md](PLAN.md) | 다음 우선 작업표, 코어 묶음 요약, 보류 중인 사용성 개선, 최초 설계 로드맵 |
| v1 적용 | [V1-ADOPTION.md](V1-ADOPTION.md) | 공식 v1(`/projects/miy`) 검토 결과, 가져올 항목(ID `V1-*`)과 제외 항목, v1의 실패 교훈 |
| 기본 UI | [UIUX-MIGRATION.md](UIUX-MIGRATION.md) | ai-do UI 이식의 범위·계약·검증 결과, 후속 UI 작업의 경계 |
| 코어 안정화 | [CORE-HARDENING.md](CORE-HARDENING.md) | worker·권한·자원·복구·새 서버 인수 1~5차의 완료 기준과 증거 |
| 코어 권한 | [CORE-PERMISSIONS.md](CORE-PERMISSIONS.md) | 설치/릴리스 승인, 사용자 위임, 앱 후속 호출 계약과 기존 설치 전환 |
| 앱 개발 | [DEVELOPMENT.md](DEVELOPMENT.md), [NATIVE-DEVELOPMENT.md](NATIVE-DEVELOPMENT.md) | 앱 개발 계약, CLI 흐름, 로컬·네이티브 실행, 개발 영역 평가와 운영 승격 |
| 실행·운영 명령 | [RUNNING.md](RUNNING.md) | 로컬 기동, 공용 컴포넌트 설치·갱신, 배포 상태 확인 |
| 운영 서버 | [SERVER-OPERATIONS.md](SERVER-OPERATIONS.md) | v2.dwdcc.kr 구성, 게시 CLI, 배포 소스와 기록, 되돌리기 위치 |
| CLI | [CLI-IMPLEMENTATION-PLAN.md](CLI-IMPLEMENTATION-PLAN.md), [CLI-RELEASE.md](CLI-RELEASE.md), [CLI-VALIDATION-MATRIX.md](CLI-VALIDATION-MATRIX.md) | 명령·인증·OS 계약, 배포 후보·게시 절차, 요구별 검증 범위 |
| Windows | [WINDOWS-INSTALLER.md](WINDOWS-INSTALLER.md), [WINDOWS-VALIDATION.md](WINDOWS-VALIDATION.md), [WINDOWS-0.6.3-ACCEPTANCE.md](WINDOWS-0.6.3-ACCEPTANCE.md) | 설치 파일, 사용자 수행 빌드·검증 절차와 실행 기록 |
| 자원 | [CAPACITY.md](CAPACITY.md) | CPU·메모리·파일·DB 예산과 한도 초과 대응 |
| 내부 AI | [AI.md](AI.md) | 공통 AI 연결, 사용자별 독립 세션, Hermes 연동 계획 |
| 기술 선택 | [STACK.md](STACK.md), [FRONTEND.md](FRONTEND.md), [RESEARCH.md](RESEARCH.md) | 스택 추천안, 프론트엔드 비교, 유사 프로젝트 차용 판단 |
| 구조도 | [architecture/README.md](architecture/README.md) | 전체 구조 한 장, 서비스·논리·개발 흐름·권한 구성도 |

## 실행 결과와 증거

| 문서 | 내용 |
| --- | --- |
| [VALIDATION.md](VALIDATION.md) | 최초 핵심 흐름 검증(V0~V7)의 계획과 통과 조건 |
| [IMPLEMENTATION.md](IMPLEMENTATION.md), [IMPLEMENTATION-RESULTS.md](IMPLEMENTATION-RESULTS.md) | V0~V1 구현 계획과 누적 구현·검증 결과 |
| [CORE-TESTING.md](CORE-TESTING.md) | 게시 설치기와 독립 AI 앱 개발 인수 절차 |
| [INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md](INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md), [CLI-0.6.3-LINUX-VALIDATION.md](CLI-0.6.3-LINUX-VALIDATION.md), [LEAVE-LEDGER-RESULTS.md](LEAVE-LEDGER-RESULTS.md) | 특정 시점의 인수 결과 |
| [releases/](releases/) | 버전별 후보·검증·배포 JSON 기록과 배포 스크립트 사본 |

결과 문서는 실행 당시의 기록이다. 새 실행은 새 기록으로 추가하고, 이전 PASS를 새 소스의 통과로 바꿔 적지 않는다. 장비나 인증이 없어 실행하지 못한 단계는 `NOT_RUN`, 실행 후 실패한 단계는 `FAIL`로 둔다.

## 보관 문서

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
