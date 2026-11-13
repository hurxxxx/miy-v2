# 보관 문서

이곳은 이전 계획·비교·실행 결과를 찾는 곳이다. 현재 상태와 작업 순서는 [PLAN](../PLAN.md), 현재 담당 문서는 [문서 지도](../README.md)를 따른다. 보관본의 “현재”는 그 문서 작성·실행 당시를 뜻한다.

## 실행 결과와 인수 이력

| 기록 | 적용 범위 |
| --- | --- |
| [누적 구현 결과](results/IMPLEMENTATION-RESULTS.md) | 초기 코어·앱 개발·MF·배포의 소스별 검증 |
| [CLI 상세 검증](results/CLI-VALIDATION-MATRIX.md) | CLI 0.5~0.6.3 결과와 과거 후보 준비 |
| [Windows 검증](results/WINDOWS-VALIDATION.md) | 버전별 로컬 빌드·설치기·네이티브 개발·원격 배포 |
| [Windows 0.6.3 인수](results/WINDOWS-0.6.3-ACCEPTANCE.md) | 당시 후보의 사용자 실행 절차와 NOT_RUN |
| [CLI 0.6.3 Linux 검증](results/CLI-0.6.3-LINUX-VALIDATION.md) | 해당 서버·게시 후보·앱 데이터 보존 |
| [독립 Codex·Claude 인수](results/INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md) | 별도 에이전트가 만든 앱의 당시 검사 |
| [0.6 핵심 흐름 사용자 테스트](results/CORE-TESTING-0.6.md) | 당시 앱 개발·배포 인수 절차 |
| [휴가 앱 실증](results/LEAVE-LEDGER-RESULTS.md) | 초기 독립 앱·권한·배포의 실증 범위 |

수치·실패·미실행 범위는 원문대로 보존한다. 새 실행은 [releases](../releases/)에 새 기록으로 추가한다. 현재 사용자는 [실행 안내](../RUNNING.md), [Windows 인수](../WINDOWS-ACCEPTANCE.md), [CLI 릴리스](../CLI-RELEASE.md)를 사용한다.

## 이전 설계와 도입 계획

| 문서 | 보관 이유·현재 담당 |
| --- | --- |
| [최초 제품 로드맵](PLAN-INITIAL-ROADMAP.md) | 현재 순서는 PLAN만 사용 |
| [V0~V1 구현](IMPLEMENTATION-V0-V1.md), [MF 검증](VALIDATION-MODULE-FEDERATION.md) | 당시 같은 페이지 실행의 계획. 현재 경계·프레임 계약과 구분 |
| [이전 권한 계약](CORE-PERMISSIONS-0.7.0.md) | `22c219d` 소스 고정 비교본 |
| [기본 UI-1~5 이식](UIUX-MIGRATION-UI1-5.md) | ai-do 기반 완료 이력 |
| [프론트엔드 비교](design/FRONTEND.md) | 기술 선택은 현재 STACK, 앱 실행은 APP-FRAMES |
| [플랫폼 UI 후속 책임](design/UIUX-MIGRATION.md) | 현재 책임·V1 ID는 V1-ADOPTION에 통합 |
| [CLI 0.5 구현 계획](cli/CLI-0.5-IMPLEMENTATION-PLAN.md) | 현재 사용법은 RUNNING, 검사는 CHECKS |
| [네이티브 개발 도입](cli/NATIVE-DEVELOPMENT-0.6.md) | 구 브랜치·명령 예시. 현재 로컬 개발은 RUNNING |
| [Windows 설치기 도입](cli/WINDOWS-INSTALLER.md) | 현재 설치·인수는 WINDOWS-ACCEPTANCE, 빌드·게시는 CLI-RELEASE |
| [과거 아키텍처 도식](architecture-2026-10/README.md) | 당시 SVG·PNG·PDF. 현재 도식으로 재사용하지 않음 |

## 완료된 인계

[CLI 개발 인계](CLI-HANDOFF.md), [CLI 개선 서버 인계](CLI-EXPERIENCE-SERVER-HANDOFF.md), [Windows→Linux 인계](LINUX-TODO-HANDOFF.md)는 원래 전달한 절차를 보존한다. 새 작업 지시로 사용하지 않는다.

## 경로와 증거 보존

2026-11-13 정리에서 문서 본문과 실행 사실은 보존하고 분류·상대 링크를 조정했다. 이전 경로와 새 위치의 전체 대응은 [정리 기록](../releases/docs-cleanup-2026-11-13.json)에 있다. 과거 JSON의 source/file/hash 값과 증거 파일은 당시 출처 그대로이며 이동한 Markdown의 현행 해시로 덮어쓰지 않는다. 게시 SDK·템플릿·공용 컴포넌트는 이번 정리 대상이 아니다.
