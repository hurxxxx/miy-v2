# miy-v2 플랫폼 개발 규칙

이 저장소는 miy 플랫폼 자체(Rust 코어·CLI, 플랫폼 웹, SDK·공용 UI, 설치·릴리스 도구)를 개발한다. 생성되는 업무 앱의 지침은 `templates/`에 따로 있다.

## 먼저 읽을 것

- [문서 지도](docs/README.md): 주제별 담당 문서. 작업 순서는 [추진 계획](docs/PLAN.md)의 작업표 하나만 따른다.
- [변경별 검사 지도](docs/CHECKS.md): 바꾼 영역에 맞는 최소 검사.
- 제품 계약은 [PRD](docs/PRD.md), 권한 계약은 [코어 권한](docs/CORE-PERMISSIONS.md)이 기준이다. 구현된 동작은 코드와 테스트가 최종 기준이다.

- 앱 출처·세션·실행 계약을 바꿀 때 [사내 최소 보안 설계](docs/APP-BOUNDARIES.md)를 먼저 읽는다. iframe 없는 MF·통합 라우팅·단일 IP HTTPS를 유지한다. 업무 포털(dev/prod)과 관리·CLI 승인·Keycloak 콘솔 두 출처, portal/console 세션 용도는 서버·UI `a4ad370`으로 2026-11-07 운영에 반영했다([배포 기록](docs/releases/internal-minimum-security-2026-11-07-deployment.json)). 2026-11-09 후속 `027caab`(다른 출처 쿠키 요청 거부·legacy CLI 로그인 종료)도 운영에 반영했다. 운영 콘솔은 `https://v2.dwdcc.kr:9443`이며 공유 개발 로그인은 꺼져 있다. 구현·격리 검증·운영 전환 상태를 구분한다. 별도 IdP/dev 출처·외부 앱 격리·MFA 의무화·자동 CA를 기능 개발의 선행 조건으로 추가하지 않는다. 같은 페이지 앱 격리나 포트 쿠키 격리를 주장하지 않으며 공개 운영을 사내 전제로 자동 인정하지 않는다. 권한은 CORE-PERMISSIONS, 순서·상태는 PLAN을 따르고 게시 번들·현행·목표·역사를 구분한다.

## 이 호스트의 운영 경계

이 checkout의 `.local`은 v2.dwdcc.kr 운영 상태이고, Nginx는 `web/platform/dist`를 그대로 서빙한다.

- `pnpm build`나 기본 `vite build`로 `web/platform/dist`를 덮어쓰지 않는다. 검증 빌드는 `--outDir .local/evidence/...` 또는 `scripts/check-static.sh`를 쓴다. 운영 화면 교체는 [서버 운영 안내](docs/SERVER-OPERATIONS.md)의 백업·추가·`index.html` 교체 절차로만 한다.
- 운영 `.local`을 대상으로 다음을 실행하지 않는다: `scripts/check.sh`(origin 가드로 거부됨), `services.py`, `up.sh`, `down.sh`, `compose up`, `bootstrap.py`, `seed`, `configure-identity.py`.
- 일회용 검증은 격리 도구만 쓴다: `check-static.sh`, `runtime-regression/run.py`, `server-ui.config.ts`, `stack-ui.sh`. 격리 스택은 `MIY_STATE_DIR`과 `MIY_DATABASE_CONTAINER`를 반드시 따로 지정한다(기본값은 운영 DB 컨테이너).
- 운영 컨테이너(`miy-v2-validation-*`, 운영 `miy-app-*`)·네트워크·볼륨과 운영 DB를 변경하거나 지우지 않는다.
- 운영 반영, 서비스 재시작, DB migration 적용은 사용자의 명시적 요청이 있을 때만 하고, 변경 범위의 백업과 별도 영역 복원 확인을 먼저 한다.

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
