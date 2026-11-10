# miy-v2

전담 IT 운영팀이 없는 중소기업을 위한 AI 앱 개발·배포·운영 플랫폼이다. 기존 `../miy`의 후속 프로젝트이며, `miy-v2`는 내부 구분용 이름이고 대외 서비스명은 `miy`를 사용한다.

고객사는 사내 서버 또는 클라우드에 miy를 설치하고, 직접 운영하거나 개발사·유지관리 파트너에 맡길 수 있다. 앱 개발은 고객사 개발자뿐 아니라 개발사·파트너와 앱 구현이 가능한 도메인 전문가도 수행한다.

CLI로 앱의 기본 구조를 만들고, 자신의 개발 에이전트를 MCP로 연결하여 공통 계약 안에서 자유롭게 개발한다. 앱은 로컬 또는 miy의 개발 영역에서 실행·평가하고, 플랫폼 코드를 변경하지 않고 격리 배포할 수 있도록 한다. miy는 소스 관리, 인증·권한, DB·파일 저장소, 앱 운영과 Hermes 기반 업무 챗봇을 함께 제공하는 것을 목표로 한다.

## 현재 단계

현재 [사내 최소 보안 설계](docs/APP-BOUNDARIES.md)는 **iframe 없는 MF/AppRoot·통합 라우팅·독립 빌드와 단일 IP HTTPS**를 유지한다. 업무 포털(prod)·dev 포털(dev)·관리/CLI 승인/Keycloak 콘솔의 세 출처와 목적별 세션을 사용하며 2026-11-09 [dev 포털 배포](docs/releases/dev-portal-2026-11-09-deployment.json)와 [콘솔 이동](docs/releases/console-hostname-2026-11-09-deployment.json)을 반영했다. 다음 기능은 공통 디렉터리·ACL이며 별도 IdP·외부 앱 격리·자동 CA는 선행 조건이 아니다. 현재 서버는 개발 중이며 공개 서버를 사내 범위로 자동 인정하거나 같은 포털 안의 앱 격리를 보장하지 않는다.

현재 호스트의 prod 포털은 [v2.dwdcc.kr](https://v2.dwdcc.kr), dev 앱은 [dev 포털](https://v2-dev.dwdcc.kr), 관리·device CLI 승인·로그인은 [콘솔](https://v2-console.dwdcc.kr)에서 제공한다. [CLI 설치 안내](https://v2.dwdcc.kr/cli/install)는 현재 게시 CLI **0.7.0 / 번들 0.8.0**을 제공하며 CLI의 서버 주소는 prod 포털이다. 공유 개발 로그인은 꺼져 있다. 실행 서버·UI와 게시 배포물의 출처 commit은 [현재 서버 운영 안내](docs/SERVER-OPERATIONS.md)를 따른다.

[코어 권한 계약](docs/CORE-PERMISSIONS.md)은 설치/릴리스별 명시 승인, 제한된 사용자 위임, 앱 후속 호출을 설명한다. 코어 안정화 1·2차와 [기본 UI 이식](docs/UIUX-MIGRATION.md) UI-1~5를 마치고 v2.dwdcc.kr에 반영했다. 다음 작업 순서는 [추진 계획](docs/PLAN.md#다음-우선-작업)의 작업표 하나로 관리한다. 공식 v1(`../miy`)에서 가져올 규칙과 제외할 구조는 [v1 적용 계획](docs/V1-ADOPTION.md)에 정리했다.

현재는 **독립 앱 개발·로컬 미리보기·테스트 배포·운영 승격을 연결하고 구조 검증을 수행한 단계**다. 플랫폼과 사용자 앱의 Rust backend, React/Vite UI, Docker, 앱·환경별 DB를 분리한다. 일반 Codex·Claude Code용 프로젝트 MCP·스킬, 브라우저 승인 CLI 로그인, 인증된 이미지 업로드, 테스트한 릴리스의 운영 승격과 별도 배포 권한을 제공한다.

과거 실행 범위는 [구조 진단 결과](docs/IMPLEMENTATION-RESULTS.md#구조-심층-진단과-핵심-9개-요건-개선)에 보존한다. 개발·운영 라우터, 로컬 DB 격리, 다른 앱 API, 권한 회수, 기존 앱 보존을 실제 검사했다. 신뢰된 UI 코드의 통합이며 악성 JavaScript의 브라우저 격리는 제공하지 않는다. 5개 OS/CPU의 CLI 후보와 회사 홈페이지 배포를 제공하며, 제품 전체 완성과 독립 반복 인수는 별도 범위다.

공용 UI는 CLI 내장 코드에서 분리했다. 플랫폼 컴포넌트 저장소에 새 버전을 게시하면 같은 CLI와 MCP에서 즉시 조회하며, 앱은 명시적인 설치·업데이트 전까지 기존 버전을 유지한다. [설치·갱신 안내](docs/RUNNING.md#공용-컴포넌트-설치와-업데이트)를 따른다.

Windows CLI는 회사 홈페이지의 `/cli/install`에서 [GUI 설치 파일](docs/WINDOWS-INSTALLER.md)을 받아 서버 주소를 입력해 설치한다. Unix의 기존 설치 명령은 `bash scripts/install-cli.sh`이며, 새 앱 시작은 `miy app init ./my-app --id my-app --name '내 앱'`이다. 로그인·등록 승인·로컬 테스트·배포 순서는 [실행 안내](docs/RUNNING.md)를 따른다. 이전 [휴가 앱 실증](docs/LEAVE-LEDGER-RESULTS.md)과 bundle 0.3 검증은 당시의 보완 과정과 한계가 남아 있는 이력이다.

[CLI 0.5 개선 계획](docs/CLI-IMPLEMENTATION-PLAN.md)의 context·인증·작업·로그·실행 전 검증과 Windows 설치/갱신 도구를 구현했다. [개발 인계](docs/archive/CLI-HANDOFF.md)와 [세부 검증](docs/CLI-VALIDATION-MATRIX.md)에서 현재 증거와 남은 조건을 확인한다. 사용자 Windows의 기존 네이티브 개발·서버 빌드 실증은 기록에 보존한다. 새 CI 설치기의 Windows 11 실제 설치·갱신·제거는 사용자가 나중에 수행하며, Codex·Claude의 독립 새 세션 인수는 별도로 관리한다.

최초 V0~V1 계획은 [보관 원문](docs/archive/IMPLEMENTATION-V0-V1.md)이며 현재 구현 지침이 아니다. 유효한 미완료 검증은 [새 인수표](docs/APP-BOUNDARIES.md#기존-검증의-승계)로 승계했다.

별도의 loopback 개발 스택에서는 `bash scripts/up.sh`로 기동하고 `bash scripts/check.sh`로 전체 인수를 수행한다. 이 호스트의 운영 `.local`을 대상으로 실행하지 않으며, 여기서는 [격리 검사 도구](docs/CHECKS.md)를 사용한다. 브라우저에서 [http://localhost:18000](http://localhost:18000)을 열고 **관리자로 시작**을 누르면 비밀번호 없이 개발 화면을 사용할 수 있다. 개발자·사용자 계정 버튼도 제공한다. 실제 OAuth·MCP 검증과 준비 조건·결과 경로는 [실행 안내](docs/RUNNING.md)를 따른다. 구현 전 문서 체크포인트는 Git 커밋 `3cecbe1`이다.

당시 구현 범위와 확인한 동작은 [V0~V1 검증 결과](docs/IMPLEMENTATION-RESULTS.md)에 보존한다.

## 합의한 방향

- 기업별로 사내 또는 클라우드의 서버·VM 한 대에 독립 설치하는 방식으로 시작한다.
- 설치 위치와 별개로 고객사 직접 운영, 개발사·파트너 대행, 공동 운영을 선택할 수 있다.
- 고객사·개발사·파트너·도메인 전문가가 같은 앱 계약으로 개발하며, 고객사가 작업 범위와 접근 권한을 위임한다.
- 코어 업데이트와 서버 유지관리는 앱 개발·배포와 별도 역할로 관리한다. 개발사가 코어 업데이트를 제공하고 지정된 유지관리 담당자가 적용·검증한다.
- 웹앱, API 서비스, 백그라운드·예약 작업을 지원한다.
- CLI로 프로젝트·SDK·UI 자산·개발 지침을 준비하고, 공통 계약을 지키는 범위에서 앱을 자유롭게 구현한다.
- 로컬 실행과 플랫폼 개발 영역의 미리보기를 모두 제공하여 개발 중에도 평가할 수 있게 한다.
- 내장 Git 저장소와 외부 GitHub·GitLab 연결을 제공한다.
- 개인 개발·미리보기는 허용하고, 운영 배포는 관리자 또는 위임받은 담당자가 수행한다.
- 앱 업데이트는 대상 앱의 일시 중단을 허용하여 기존 버전을 중지한 뒤 새 버전을 실행한다. 무중단 배포는 초기 필수 조건이 아니다.
- miy 관리 화면에서 앱·환경별 시작·중지·재시작·업데이트·삭제와 상태·로그·자원 조회를 제공한다. 수동 중지는 유지하고 앱 삭제 시 DB·업무 파일은 기본 보존한다.
- 파일, PostgreSQL, 사내 HTTP API를 첫 데이터 연결 대상으로 삼는다.
- miy 웹사이트에 챗봇을 내장한다. 내부 Hermes 엔진이 모델·도구 호출을 반복하여 답변하고, 업무 변경은 사용자 확인 후 실행한다.
- 내부 AI는 관리자가 설정한 공통 Codex·Claude Code 연결을 사용하며, 직원은 miy 로그인만으로 챗봇을 이용한다. 사용자별·대화별 독립 세션과 업무 권한을 유지하고 API LLM도 선택 지원한다.
- 관리자가 데이터 소스별 외부 AI 이용 범위를 설정한다.
- 플랫폼 코어와 개별 앱의 백엔드는 Rust를 사용하며 Next.js는 제외한다.
- MinIO는 도입하지 않는다. 파일 저장은 miy Storage API를 통해 제공하고 저장 backend를 분리한다.

## 기획 문서

전체 문서의 담당 범위와 보관 문서는 [문서 지도](docs/README.md)에 있다. 먼저 읽을 문서는 다음과 같다.

- [제품 요구사항](docs/PRD.md): 제품 정의, 합의 사항, 기능 요구사항과 인수 조건
- [사내 최소 보안 설계](docs/APP-BOUNDARIES.md): 세 출처·세션 용도·개발 단계 계약, 현재 필수와 후속, 발견된 문제·한계·전환·인수
- [추진 계획](docs/PLAN.md): 다음 우선 작업표(유일한 순서 기준)와 제품 로드맵
- [v1 적용 계획](docs/V1-ADOPTION.md): 공식 v1 검토 결과, 가져올 항목과 제외 항목
- [UI/UX 이식 계획](docs/UIUX-MIGRATION.md): 기본 UI 이식 결과와 후속 UI 작업의 경계
- [코어 안정화 계획](docs/CORE-HARDENING.md): 자원·복구·새 서버 인수의 완료 기준
- [현재 플랫폼 서버 운영](docs/SERVER-OPERATIONS.md): 실제 접속 주소, CLI/MCP 연결, 배포 소스와 기록
- [현재 아키텍처](docs/architecture/README.md): 새 경계의 Mermaid 구성도와 과거 그림의 보관 위치

현재 플랫폼 웹은 TypeScript, React, Vite, TanStack Router를 사용한다. 확정된 제품 요구·검토 중인 보안 개선안·현행 구현의 차이는 [기술 구성](docs/STACK.md)과 PRD에서 구분한다.

현재 앱 기본안은 Rust 백엔드와 React 정적 화면을 같은 Docker/OCI 이미지로 배포하고 내부 runner가 수명주기를 관리하는 방식이다. 정적 화면의 Rust 바이너리 내장은 선택안으로 두며 Kubernetes는 초기 구성에 포함하지 않는다.

독립 앱 화면은 iframe 없는 동적 모듈과 통합 라우터로 실행한다. 사내 승인 앱은 하나의 UI 신뢰 영역이며 앱 없는 관리·로그인 출처를 분리한다. 공통 ACL·SDK·토큰만으로 악성 앱 간 격리를 보장하지 않는다. 기존 MF·SDK·앱 데이터를 유지하고 전 앱 재개발·일괄 전환 없이 필요한 인증 변경만 적용했다. 공통 ACL 등 미구현 범위는 추진 계획을 따른다.

CLI 초기화는 Codex·Claude Code용 `miy-cli` 스킬과 `docs/CLI-REFERENCE.md`도 설치한다. 명령·옵션은 실행 파일의 정의에서 자동 생성하며 CLI 버전별로 잠근다. 전역 CLI 갱신 후 호환 프로젝트에서 `miy app setup`을 실행하면 동봉 CLI·스킬·매뉴얼을 함께 갱신한다. SDK 변경은 `app upgrade`로 구분한다. [운영 방법](docs/RUNNING.md#호환-도구-갱신과-이전-이름)
