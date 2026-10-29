# CLI 0.6 핵심 흐름 사용자 테스트

대상은 CLI **0.6.2** / 개발 번들 **0.7.1**이다. Windows 11의 로컬 개발에는 Rust/MSVC·Windows SDK, Node, 지정된 pnpm과 Git을 준비한다. 기본 네이티브 실행과 서버 빌드 경로에는 Docker Desktop·WSL·SSH가 필요하지 않다. 서버는 `app.builds.v1`을 제공해야 한다. 이번 테스트는 핵심 기능·권한·데이터 보존을 검증하며, 화면·사용성 개선은 [TODO](TODO.md)에 기록한다.

2026-10-29 Windows 11 / PowerShell 7.6.5에서 소스 `2fabe68`의 CLI로 네이티브 Todo CRUD → 서버 원격 빌드 → dev 배포 → 동일 DB/레코드 → 브라우저 확인을 통과했다. 사용자 선택 admin 계정의 결과이며 개발자 최소 권한·PowerShell 5.1·운영 승격까지 확대하지 않는다. [실행 기록](WINDOWS-VALIDATION.md)과 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 미실행 범위를 구분한다.

## 설치와 로그인

현재 게시된 [CLI 설치 안내](https://v2.dwdcc.kr/cli/install)의 GUI 후보는 0.6.0이며 새 네이티브 개발·서버 빌드를 포함하지 않는다. 0.6.2가 게시되기 전에는 [Docker 없는 개발 안내](NATIVE-DEVELOPMENT.md)의 소스 빌드와 `app setup` 절차로 프로젝트 CLI를 준비한다. 아래 `miy`는 Windows에서 해당 프로젝트의 `.\.miy-tools\miy.exe`로 실행한다. 실제 버전·소스 commit·실행 파일 SHA-256을 기록한다.

GUI 설치 후보 자체는 [GUI 설치 안내](WINDOWS-INSTALLER.md)에서 별도로 검증한다. CLI에는 기본 서버 주소가 없다. 기존 context가 있으면 보존하고 새 서버는 명시적으로 등록한다.

```powershell
miy --version
miy context show
miy auth login
```

브라우저에서 개발자 계정으로 로그인하고 CLI에 표시된 인증 코드를 확인·승인한다. 서버 주소 없이 설치했다면 먼저 `miy context init`을 실행해 `http://IP:포트` 또는 `https://도메인`을 직접 입력한다. 기존 연결이 있으면 `miy context list`에서 대상을 확인한다. 다른 주소를 전달한 설치 명령은 기존 연결을 자동으로 덮어쓰지 않는다.

## 새 앱과 AI 개발

```powershell
miy app init ./core-test --id core-test --name '핵심 흐름 테스트'
Set-Location core-test
miy component sync
miy doctor --for develop
miy app register
```

별도 관리자 계정으로 플랫폼의 등록 요청을 승인한다. 개발자에게 dev/develop 권한이 부여되며 운영 배포 권한은 별도로 승인해야 한다. 관리자와 개발자는 브라우저 프로필을 분리해 계정 혼동을 피한다.

이 폴더에서 Codex 또는 Claude의 새 세션을 열고 다음과 같이 요청한다.

> 이 프로젝트의 지침과 MCP·CLI를 읽고 할 일 앱을 만들어줘. 추가·조회·완료·삭제 기능과 데이터베이스를 구현하고, 로컬에서 검증한 뒤 서버 개발 영역에 배포해줘. 서버 개발 DB를 사용하고 개발 배포 후 같은 데이터가 남는지 확인해줘. UI/UX 개선은 TODO로 기록하고 운영 배포는 아직 실행하지 마.

생성 프로젝트에는 `AGENTS.md`, `CLAUDE.md`, 개발/CLI 스킬, CLI 참조, 프로젝트 실행 파일과 두 도구의 MCP 설정이 들어 있다. 각 AI 도구의 프로젝트 신뢰·MCP 승인 화면은 정상적으로 처리한다. `doctor`의 MCP 검사는 로컬 프로세스 시작과 도구 목록 검사이며, 실제 AI가 도구를 사용했다는 증거와 구분한다.

## 확인할 데이터와 배포

1. `miy app dev --no-open`으로 로컬 주소를 열고 레코드를 만든다. 기본값은 서버 개발 DB다. 연결 실패 시 별도 로컬 DB로 자동 전환하지 않는다.
2. 로컬에서 CRUD와 인증·권한을 확인한다. 개발 세션은 앱당 하나이며, 다른 세션이나 개발 배포가 충돌을 명확히 알려야 한다.
3. `miy app dev-stop`으로 세션을 종료한다. 재배포는 manifest의 version을 사용하지 않은 다음 버전으로 올리고 소스를 커밋한 뒤 `miy app build --remote`, `miy app deploy --environment dev`를 실행한다. 기본 `app build`는 로컬 Docker 빌드이므로 Docker 없는 PC에서는 `--remote`를 명시한다. 작업 ID로 watch·logs·resume을 확인한다.
4. 플랫폼의 개발 앱을 열어 로컬에서 만든 레코드가 그대로 남는지 확인한다. CLI가 출력한 DB 식별자와 실제 배포 release/image도 기록한다.
5. 실제 확인한 결과만 [인수 기록](../templates/rust-react/docs/ACCEPTANCE.md) 형식으로 남기고 `miy app accept`로 해당 릴리스를 인수한다.
6. 별도 관리자에게 운영 배포 권한을 받은 후 AI에 운영 승격을 요청한다. 운영은 인수한 동일 이미지를 사용하지만 별도 DB이므로 개발 테스트 레코드가 없어야 한다. 운영에 만든 레코드도 개발 DB에 나타나면 안 된다.

스키마 변경은 이전 개발 앱을 중지할 수 있다. 적용된 SQL 파일을 수정·삭제하지 않고 새 마이그레이션을 추가한다. 세션 종료·만료·권한 회수는 임시 연결을 닫지만 데이터베이스를 삭제하지 않는다. 로컬 전용 Docker DB가 필요한 경우에만 `miy app dev --runtime docker --database local`을 명시한다.

실패 시 명령, 오류 코드, job/session ID, 기대 결과와 실제 결과를 기록한다. 비밀번호·토큰·쿠키·DB 접속 문자열은 테스트 기록에 복사하지 않는다. 성공하지 않은 단계와 실행하지 않은 단계는 각각 FAIL과 NOT_RUN으로 구분한다.
