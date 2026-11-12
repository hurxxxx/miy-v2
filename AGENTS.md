# miy-v2 플랫폼 개발 규칙

이 저장소는 miy 플랫폼 자체(Rust 코어·CLI, 플랫폼 웹, SDK·공용 UI, 설치·릴리스 도구)를 개발한다. 생성되는 업무 앱의 지침은 `templates/`에 따로 있다.

## 먼저 읽을 것

- [문서 지도](docs/README.md): 주제별 담당 문서. 작업 순서는 [추진 계획](docs/PLAN.md)의 작업표 하나만 따른다.
- [변경별 검사 지도](docs/CHECKS.md): 바꾼 영역에 맞는 최소 검사.
- 제품 계약은 [PRD](docs/PRD.md), 권한 계약은 [코어 권한](docs/CORE-PERMISSIONS.md)이 기준이다. 구현된 동작은 코드와 테스트가 최종 기준이다.

- 앱 출처·세션·실행 계약을 바꿀 때 [사내 최소 보안 설계](docs/APP-BOUNDARIES.md)를 먼저 읽는다. 업무 앱 UI는 설치별 출처 iframe에서 실행하고([앱 프레임 설계](docs/APP-FRAMES.md), 2026-11-10 채택), 플랫폼 화면·통합 라우팅·단일 IP HTTPS는 유지한다. 현재는 업무 포털(prod)·dev 포털(dev)·관리/CLI 승인/Keycloak 콘솔의 세 출처와 `portal | dev_portal | console` 세션 용도를 사용한다. 운영 주소는 각각 `https://v2.dwdcc.kr`, `https://v2-dev.dwdcc.kr`, `https://v2-console.dwdcc.kr`이며 공유 개발 로그인은 꺼져 있다. 현재 운영은 서버·UI·프레임 런타임 `7defdb0`·schema 22다([리뷰 보완 배포](docs/releases/app-frames-review-fixes-2026-11-12-deployment.json)). 그 전 서버 `f9d4084`·UI `3760ef0`·schema 21의 [dev 포털 배포](docs/releases/dev-portal-2026-11-09-deployment.json)와 [콘솔·issuer 이동](docs/releases/console-hostname-2026-11-09-deployment.json)을 구현·격리 검증과 구분한다. 다른 출처 쿠키 요청은 거부하고 legacy CLI 로그인은 종료했다. 별도 IdP 출처·외부 앱 격리·MFA 의무화·자동 CA를 기능 개발의 선행 조건으로 추가하지 않는다. 같은 사이트 앱 프레임(P2·P3)의 쿠키 격리나 프로세스 분리를 주장하지 않으며 공개 운영을 사내 전제로 자동 인정하지 않는다. 권한은 CORE-PERMISSIONS, 순서·상태는 PLAN을 따르고 게시 번들·현행·목표·역사를 구분한다.
- 업무 앱 프레임은 2026-11-12 운영에 처음 반영했다(서버·UI `bcb6356`, schema 21→22, 앱 출처 `https://v2-app-<키>.dwdcc.kr`, [최초 배포](docs/releases/app-frames-2026-11-12-deployment.json)). 최초 도입과 같은 schema의 후속 갱신·되돌리기는 [APP-FRAMES §9](docs/APP-FRAMES.md#9-운영-반영-계획)를 따르고 소스 구현·격리 검증·운영 반영을 구분해 기록한다. 게시 CLI `01e4d49`·번들 0.8.0은 유지하며, 번들 0.9.0과 구 번들 업그레이드 보완을 포함한 CLI는 아직 게시하지 않았다.

## 개발 단계 원칙 (실제 운영 전환 전까지)

v2.dwdcc.kr은 "운영" 주소로 부르지만 **아직 개발 중인 서버**다. 실제 서비스 운영으로 바뀌면 사용자가 이 절을 다시 정한다. 그 전까지는 운영을 고려한 의사결정을 하지 않는다.

- 서비스 재시작, 짧은 중단, 사용자 재로그인, 외부 클라이언트 재인증을 이유로 설계를 미루거나 바꾸지 않는다. 유지보수 창이나 무중단 전환을 계획하지 않는다.
- 리팩토링·아키텍처 변경에서 하위 호환을 유지하지 않아도 된다. 구 API·구 CLI·구 번들·구 설정 형식을 위한 호환 경로, 이전 형식 fallback, 단계적 전환 계층을 기본으로 넣지 않는다. 필요하면 새 버전으로 끊고 CLI·SDK·웹·문서를 함께 바꾼다.
- 게시된 산출물(CLI·SDK 번들·공용 UI)의 바이트는 다시 쓰지 않고 새 버전을 추가한다. 새 버전이 옛 버전과 호환될 필요는 없다.
- 그대로 지키는 것: 기본 거부 권한 모델, 비밀 취급, DB·앱 데이터 보존(데이터를 바꾸는 반영 전 변경 범위 백업, 적용된 migration 수정 금지), 다른 작업자 변경 보호, 검증 보고 규칙, 운영 반영은 사용자가 요청할 때만.
- 문서의 호환 보존 문구(예: APP-BOUNDARIES의 AB-06·07)는 이 단계에서는 필수 조건이 아니다.

## 이 호스트의 운영 경계

이 checkout의 `.local`은 v2.dwdcc.kr 운영 상태이고, Nginx는 `web/platform/dist`를 그대로 서빙한다.

- `pnpm build`나 기본 `vite build`로 `web/platform/dist`를 덮어쓰지 않는다. 검증 빌드는 `--outDir .local/evidence/...` 또는 `scripts/check-static.sh`를 쓴다. 운영 화면 교체는 [서버 운영 안내](docs/SERVER-OPERATIONS.md)의 백업·추가·`index.html` 교체 절차로만 한다.
- 운영 `.local`을 대상으로 다음을 실행하지 않는다: `scripts/check.sh`(origin 가드로 거부됨), `services.py`, `up.sh`, `down.sh`, `compose up`, `bootstrap.py`, `seed`, `configure-identity.py`.
- 일회용 검증은 격리 도구만 쓴다: `check-static.sh`, `runtime-regression/run.py`, `server-ui.config.ts`, `stack-ui.sh`. 격리 스택은 `MIY_STATE_DIR`과 `MIY_DATABASE_CONTAINER`를 반드시 따로 지정한다(기본값은 운영 DB 컨테이너).
- 운영 컨테이너(`miy-v2-validation-*`, 운영 `miy-app-*`)·네트워크·볼륨과 운영 DB를 변경하거나 지우지 않는다.
- 운영 반영(서비스 재시작 포함)과 DB migration 적용은 사용자가 요청할 때 한다. 요청된 반영에 따르는 재시작·짧은 중단·재로그인은 따로 확인받지 않는다. DB나 데이터를 바꾸는 반영만 변경 범위의 백업과 별도 영역 복원 확인을 먼저 한다.

## 비밀

`credentials.json`, `runtime.env`의 값, 토큰, 쿠키, DB 접속 문자열, 복구 키를 출력하거나 문서·채팅·Git·증거 파일에 넣지 않는다. 설정 키 이름과 공개 origin만 인용한다.

## Git

- `main`에 직접 커밋해도 된다. 작성자는 저장소 기본 신원을 쓴다.
- push, PR·릴리스 게시, 운영 반영, 파괴적 정리는 사용자가 요청할 때만 한다.
- 같은 작업본을 다른 작업자도 쓴다. 자기 파일만 stage하고, 다른 사람의 미커밋 변경을 되돌리거나 함께 커밋하지 않는다.

## 변경 원칙

- 기본 거부·명시 승인 권한 모델을 유지한다. 화면 표시나 선택값으로 권한을 판정하지 않고 서버가 현재 권한을 다시 확인한다.
- 게시된 SDK 번들·공용 UI 버전은 불변이다. 바꿀 때는 새 버전을 추가한다.
- 적용된 migration은 수정하지 않고 새 번호로 추가한다.
- 후속 설계의 v1 근거는 [v1 적용 계획](docs/V1-ADOPTION.md)의 ID(`V1-*`)로 연결하고, v1 코드·데이터 모델을 그대로 옮기지 않는다.

## 검증 보고

- 실행한 명령과 결과, 실행하지 않은 검사와 그 이유를 함께 보고한다. 실행하지 못한 단계는 `NOT_RUN`, 실행 후 실패는 `FAIL`이다.
- 이전 실행의 PASS를 새 소스나 더 넓은 범위의 통과로 옮기지 않는다. 실패 기록은 지우지 않고 재실행 결과를 추가한다.
- 담당 문서에는 현재 상태만 두고 실행 이력은 결과 문서나 `docs/releases/`에 남긴다.
