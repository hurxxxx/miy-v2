# CLI 0.6 핵심 흐름 사용자 테스트

대상은 CLI 0.6.0 / 개발 번들 0.7.0이다. Windows 11의 개발 도구와 Docker Desktop Linux 엔진은 준비되어 있다고 가정한다. 이번 테스트는 핵심 기능·권한·데이터 보존을 검증하며, 화면·사용성 개선은 [TODO](TODO.md)에 기록한다. 실제 Windows와 독립 Codex·Claude 세션의 미실행 항목은 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 구분한다.

현재 설치 후보는 `b67798b`, 서버는 `5ca4047`이다. Linux에서 실제 로그인·MCP/API·로컬 CRUD·개발 DB 재사용·운영 DB 분리까지 검증했고, Windows Server CI의 PowerShell 5.1/7 설치 검사도 통과했다. 아래 절차는 아직 실행하지 않은 사용자 Windows 11·Docker Desktop·독립 AI 개발 흐름을 확인하기 위한 것이다.

## 설치와 로그인

현재 배포의 [CLI 설치 안내](https://v2.dwdcc.kr/cli/install)에 접속해 Windows 명령을 실행하고 새 PowerShell을 연다. 화면의 소스 commit과 다운로드 SHA-256을 실행 기록에 남긴다. 이 도메인은 현재 서버의 배포 설정이다. CLI에 기본 주소가 들어 있지 않으며, 다른 서버의 설치 화면은 그 서버의 주소를 명령에 전달한다.

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
3. `miy app dev-stop`으로 세션을 종료한 뒤 소스를 커밋하고 `miy app build`, `miy app deploy --environment dev`를 실행한다.
4. 플랫폼의 개발 앱을 열어 로컬에서 만든 레코드가 그대로 남는지 확인한다. CLI가 출력한 DB 식별자와 실제 배포 release/image도 기록한다.
5. 실제 확인한 결과만 [인수 기록](../templates/rust-react/docs/ACCEPTANCE.md) 형식으로 남기고 `miy app accept`로 해당 릴리스를 인수한다.
6. 별도 관리자에게 운영 배포 권한을 받은 후 AI에 운영 승격을 요청한다. 운영은 인수한 동일 이미지를 사용하지만 별도 DB이므로 개발 테스트 레코드가 없어야 한다. 운영에 만든 레코드도 개발 DB에 나타나면 안 된다.

스키마 변경은 이전 개발 앱을 중지할 수 있다. 적용된 SQL 파일을 수정·삭제하지 않고 새 마이그레이션을 추가한다. 세션 종료·만료·권한 회수는 임시 연결을 닫지만 데이터베이스를 삭제하지 않는다. 로컬 전용 DB가 필요한 경우에만 `miy app dev --database local`을 명시한다.

실패 시 명령, 오류 코드, job/session ID, 기대 결과와 실제 결과를 기록한다. 비밀번호·토큰·쿠키·DB 접속 문자열은 테스트 기록에 복사하지 않는다. 성공하지 않은 단계와 실행하지 않은 단계는 각각 FAIL과 NOT_RUN으로 구분한다.
