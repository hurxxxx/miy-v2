# CLI 0.6 핵심 흐름 사용자 테스트

대상은 CLI **0.6.3** / 개발 번들 **0.7.1**이다. 현재 운영 서버와 게시 CLI의 artifact source는 `8d7d042`이며 main 통합 commit과 별도로 기록한다. 기본 네이티브 실행과 서버 빌드 경로에는 Docker Desktop·WSL·SSH가 필요하지 않다. 서버는 `app.builds.v1`을 제공해야 한다. 이번 인수는 Codex·Claude 각각의 새 업무 앱 개발·서버 원격 빌드·dev 배포·데이터 보존까지이며, 화면·사용성 개선은 [TODO](TODO.md)에 기록한다.

2026-11-02 현재 Windows는 설치기 구현과 새 CI 배포물 검증 절차를 준비하는 범위다. 실제 Windows 11 설치·갱신·제거 검사는 사용자 지시에 따라 사용자가 추후 수행하며 `NOT_RUN`으로 남긴다. 독립 AI 개발 검사는 Linux에서도 진행할 수 있다. Claude 인증과 두 도구의 새 세션 개발 인수는 아직 완료되지 않았다.

2026-10-29 Windows 11 / PowerShell 7.6.5에서 소스 `2fabe68`의 CLI로 네이티브 Todo CRUD → 서버 원격 빌드 → dev 배포 → 동일 DB/레코드 → 브라우저 확인을 통과했다. 사용자 선택 admin 계정의 결과이며 개발자 최소 권한·PowerShell 5.1·운영 승격까지 확대하지 않는다. [실행 기록](WINDOWS-VALIDATION.md)과 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 미실행 범위를 구분한다.

10-30 게시 0.6.3 배포물의 다운로드·해시 확인과 11-02 CLI/MCP 연결 검사도 별도 증거다. 이 기록들은 새 CI 설치기의 실제 실행이나 Codex·Claude의 새 세션 개발 인수를 대신하지 않는다.

## 설치와 로그인

현재 게시된 [CLI 설치 안내](https://v2.dwdcc.kr/cli/install)에서 0.6.3 설치기와 해당 OS의 CLI를 받는다. 실제 내려받은 파일의 버전·source·SHA-256을 [게시 기록](CLI-VALIDATION-MATRIX.md)과 대조하고, 설치 후 실행 경로와 실행 파일 SHA-256도 기록한다. 게시 배포물과 소스에서 직접 빌드한 실행 파일을 구분한다. 아래 `miy`는 설치된 CLI이며, 생성 프로젝트의 `.miy-tools/miy` 또는 `.miy-tools/miy.exe`를 사용할 때에도 실제 실행 경로와 버전을 확인한다.

Windows 사용자는 [GUI 설치 안내](WINDOWS-INSTALLER.md)에 따라 다음 실제 인수를 추후 수행한다. 일반 사용자 여부·Windows 버전·PowerShell 5.1/7을 기록하고 최초 설치, 기존 설치의 갱신, 제거와 재설치를 각각 확인한다. 갱신 전후 실행 경로·버전·파일 해시와 기존 context·프로젝트·앱 데이터 보존을 비교한다. 제거 시에는 실행 파일·설치 등록·PATH 정리와 사용자 데이터 보존을 구분하여 기록한다. 제거 후 이어서 앱을 개발하려면 먼저 재설치한다. 아직 실행하지 않은 단계는 `NOT_RUN`으로 둔다.

네이티브 로컬 개발에는 Rust, Node, 지정된 pnpm과 Git을 준비하며 Windows에서는 MSVC·Windows SDK도 필요하다. Linux의 준비 조건은 [네이티브 개발 안내](NATIVE-DEVELOPMENT.md)를 따른다. CLI에는 기본 서버 주소가 없다. 기존 context가 있으면 보존하고 새 서버는 명시적으로 등록한다.

```powershell
miy --version
miy context show
```

서버 주소 없이 설치했다면 먼저 `miy context init`을 실행해 `http://IP:포트` 또는 `https://도메인`을 직접 입력한다. 기존 연결이 있으면 `miy context list`에서 대상을 확인한다. 다른 주소를 전달한 설치 명령은 기존 연결을 자동으로 덮어쓰지 않는다. `miy auth login`을 실행하고 브라우저에서 개발자 계정으로 로그인한 뒤 CLI에 표시된 인증 코드를 확인·승인한다. 명령이 pending 상태로 끝나면 출력된 `auth wait` 복구 명령으로 승인을 기다린다.

## 새 앱과 AI 개발

Codex와 Claude는 서로 다른 새 폴더·앱 ID·AI 세션을 사용한다. 아래는 Codex용 예이며 Claude용은 `core-claude-<고유값>`으로 별도 생성한다. 예시 ID가 이미 있으면 새 고유값을 사용하고 기존 앱·데이터를 재사용하거나 삭제하지 않는다. Linux에서는 `Set-Location` 대신 `cd`를 사용한다.

```powershell
miy app init ./core-codex-20261102 --id core-codex-20261102 --name 'Codex 업무 앱 인수'
Set-Location core-codex-20261102
miy component sync
miy doctor --for develop
miy app register
```

별도 관리자 계정으로 플랫폼의 등록 요청을 승인한다. 개발자에게 dev/develop 권한이 부여되며 운영 배포 권한은 별도로 승인해야 한다. 관리자와 개발자는 브라우저 프로필을 분리해 계정 혼동을 피한다.

각 도구의 설치·인증을 확인한 뒤 해당 폴더에서 새 세션을 열고 다음과 같이 요청한다. 인증이 준비되지 않은 도구는 `NOT_RUN`과 준비되지 않은 이유를 기록한다. 다른 도구의 결과를 복사하거나 기존 세션을 재개하여 두 도구의 독립 실행으로 집계하지 않는다.

> 이 프로젝트의 지침과 MCP·CLI를 읽고 업무 요청 관리 앱을 만들어줘. 요청 등록·목록·상세 조회·담당자 지정·상태 변경·삭제 기능과 데이터베이스를 구현해줘. 서버 개발 DB를 사용하는 네이티브 로컬 실행에서 검증 데이터를 만들고 CRUD를 확인한 뒤, 소스를 커밋하고 서버 원격 빌드로 dev 영역에 배포해줘. 배포 후 같은 DB와 레코드 내용이 남는지 확인하고 실제 실행한 명령·도구·작업 ID와 수동 개입을 기록해줘. UI/UX 개선은 TODO로 남기고 운영 배포는 실행하지 마.

생성 프로젝트에는 `AGENTS.md`, `CLAUDE.md`, 개발/CLI 스킬, CLI 참조, 프로젝트 실행 파일과 두 도구의 MCP 설정이 들어 있다. 각 AI 도구의 프로젝트 신뢰·MCP 승인 화면은 정상적으로 처리한다. `doctor`의 MCP 검사는 로컬 프로세스 시작과 도구 목록 검사이며, 실제 AI가 도구를 사용했다는 증거와 구분한다.

도구별로 AI 이름·버전·새 세션 식별자, OS·CLI 실행 파일의 버전과 해시, 앱 ID·소스 commit, 실제 MCP/CLI 호출과 수동 개입을 별도 기록한다. 비밀정보를 제거한 실행 기록으로 AI가 프로젝트 지침을 읽고 개발·검증·배포를 수행했는지 확인한다.

## 확인할 데이터와 배포

1. `miy app dev --no-open`으로 로컬 주소를 열고 레코드를 만든다. 기본값은 서버 개발 DB다. 연결 실패 시 별도 로컬 DB로 자동 전환하지 않는다.
2. 로컬에서 CRUD와 인증·권한을 확인한다. 개발 세션은 앱당 하나이며, 다른 세션이나 개발 배포가 충돌을 명확히 알려야 한다.
3. `miy app dev-stop`으로 세션을 종료한다. 재배포는 manifest의 version을 사용하지 않은 다음 버전으로 올리고 소스를 커밋한 뒤 `miy app build --remote`, `miy app deploy --environment dev`를 실행한다. 기본 `app build`는 로컬 Docker 빌드이므로 Docker 없는 PC에서는 `--remote`를 명시한다. 작업 ID로 watch·logs·resume을 확인한다.
4. 플랫폼의 개발 앱을 열어 로컬에서 만든 레코드의 ID·필드 값·상태가 그대로 남는지 확인한다. 행 수만 비교하지 않고 배포 전후 DB 식별자와 레코드 내용을 함께 기록한다. 원격 빌드·배포 job ID, 소스 commit, 실제 배포 release/image도 연결한다.
5. 실제 확인한 결과만 [인수 기록](../templates/rust-react/docs/ACCEPTANCE.md) 형식으로 남기고 `miy app accept`로 해당 릴리스를 인수한다.

이번 인수는 dev까지다. prod 승격과 dev/prod DB 분리 검사는 별도 작업으로 수행하며, 이후 명시된 범위와 운영 권한을 확인한 뒤 검증한 동일 이미지를 승격한다.

스키마 변경은 이전 개발 앱을 중지할 수 있다. 적용된 SQL 파일을 수정·삭제하지 않고 새 마이그레이션을 추가한다. 세션 종료·만료·권한 회수는 임시 연결을 닫지만 데이터베이스를 삭제하지 않는다. 로컬 전용 Docker DB가 필요한 경우에만 `miy app dev --runtime docker --database local`을 명시한다.

실패 시 명령, 오류 코드, job/session ID, 기대 결과와 실제 결과를 기록한다. 비밀번호·토큰·쿠키·DB 접속 문자열은 테스트 기록에 복사하지 않는다. 성공하지 않은 단계와 실행하지 않은 단계는 각각 FAIL과 NOT_RUN으로 구분한다.
