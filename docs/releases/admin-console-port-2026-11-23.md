# 플랫폼 관리자 콘솔 포팅 검증

- 원본 AI-DO `0002af062`, miy-v2 시작점 `10bc486`
- 관리자 화면 범위와 책임 대응은 [AI-DO 공통 기능 이식](../AI-DO-COMMON-MIGRATION.md#45-관리-콘솔-대응), 진행 상태는 [PLAN](../PLAN.md#완료한-작업)에 둔다.

## 인수 범위

- 고정 콘솔 출처에서 개요·사용자·조직도·권한 그룹·앱·런처·감사·지원을 각각 직접 연다. 콘솔 주소에서 로그인하면 같은 화면과 필터로 돌아간다. 일반 사용자 및 포털 브라우저의 개요·계정 목록 API 요청은 403이다.
- 조직도에서 주 소속을 배정하면 조직 사용자 목록과 감사에 나타난다. 하위 조직 포함은 조회에만 반영한다. 이전 소속을 보던 관리자의 해제 요청은 최신 주 소속을 지우지 않는다.
- 카탈로그는 페이지 단위로 표시하고 그 페이지에서 이름과 앱 ID로 좁히며, 사용자 CSV는 현재 조건에 맞는 서버 페이지 전체를 내보낸다.
- 설치·릴리스에서 현재 릴리스의 플랫폼 권한을 승인한다. 앱 상세에서 개인·그룹의 접근 정책과 환경별 런타임·저장소를 확인한다. 등록 전 개인 권한 설정은 등록 신청 아래에서 연다.
- 이 검증 실행에서는 서버 반영과 실제 Keycloak·설치 인수가 `NOT_RUN`이었다. 브라우저 검증은 일회용 PostgreSQL과 별도 포트의 Core 및 개발 로그인, 또는 모의 API에서 실행했다. 콘솔 개편 자체에는 DB migration이 없다. 이후 miy.works 반영, OIDC 직접 링크와 일부 관리자 동작 점검은 [0.17.0 후속 반영 기록](miy-works-0.17.0-2026-11-22-deployment.json)에 있다.

## 실행 기록

| 명령 | 결과 | 확인 범위 |
| --- | --- | --- |
| `bash scripts/check-static.sh rust` | PASS | 포맷·쓰기 판정·Clippy, 서버 259건과 클라이언트 145건 단위 검사. 서버의 무시된 격리 검사는 별도 실행 |
| `bash scripts/check-static.sh scripts` | PASS | 문서 링크, 번역·토큰, API 호환성, 설정·프록시 경계 및 스크립트 |
| `bash scripts/check-static.sh web` | PASS | 최종 합본에서 공용 UI 78건, 플랫폼 123건, CLI 15건, 출처 20건, HTTP 1건과 격리 Vite 빌드 |
| `python3 scripts/runtime-regression/run.py` | 기능 관찰 163건 PASS, 실행기 FAIL | 최종 실행 `miy-runtime-regression-bx456cyx`. 권한 29건에서 관리자 디렉터리, 주 소속 경합 보호, 개요 집계와 비관리자 거부를 확인했다. 기능 종료 코드 0, 호스트 기준 비교 종료 코드 1 |
| `pnpm exec playwright test --config scripts/runtime-regression/server-ui.config.ts` | PASS | 실제 Core·일회용 PostgreSQL 10건. 콘솔 직접 로그인·주소 복귀, 조직 배정→필터→감사, 등록 요청→승인→앱 상세, 관리자 권한·출처 경계, CSRF, 지원 접수 |
| `pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts` | PASS | 통합 작업본 123건. 최종 라우팅 변경 뒤 관리자·런처·알림·런타임 관련 5개 파일 68건 재실행 PASS |
| `pnpm exec playwright test --config packages/ui/tests/playwright.config.ts --retries=2` | PASS | 공용 UI 78건. 아래의 두 정적 웹 실패 후 독립 재실행 |
| CLI·출처·HTTP 브라우저 검사 | PASS | `tests/cli-experience.config.ts` 15건, `tests/surface-boundaries.config.ts` 20건, `scripts/runtime-regression/http-ui.config.ts` 1건 |
| 격리 Vite 빌드 | PASS | 플랫폼, 프레임 검증용 reference·diagnosis를 `.local/evidence/admin-console-bundles`에 생성 |

정적 웹 합본의 첫 두 실행은 공용 공유 UI에서 각각 소유자 역할 필드와 저장 결과 조회 상태가 정해진 시간에 나타나지 않아 `FAIL`이었다. 공용 UI 78건의 독립 재실행과 전체 합본 재실행은 `PASS`였다.

격리 Core 첫 실행은 기존 회귀가 폐기한 관리자 세션을 새 디렉터리 검사가 재사용해 401로 `FAIL`이었다(`miy-runtime-regression-hskcbat3`). 새 두 검사는 독립 계정을 발급하도록 고쳤다. 중간 두 실행은 격리 스택 간의 기준 컨테이너가 겹치는 상태를 확인해 중단했다(`66u8qdrs`, `o8a6thr5`). 두 실행의 고유 라벨 컨테이너·네트워크·이미지 잔여 수는 각각 0이다.

최종 실행 종료 시 시작 기준에 있던 별도 GitLab CI 작업 컨테이너 4개를 조회할 수 없어 호스트 기준 비교가 실패했다. 비교에 남은 기준 컨테이너의 ID·이미지·시작 시각은 일치했다. 검사 실행의 작업 디렉터리 정리는 `PASS`였으며, 실행 소유 라벨 `miy.audit=miy-audit-1024-84b90b`의 컨테이너·네트워크·이미지 잔여 수는 각각 0으로 재확인했다.
