# CLI 0.6 핵심 흐름 사용자 테스트

대상은 CLI **0.6.3** / 개발 번들 **0.7.1**이다. 현재 운영 서버와 게시 CLI의 artifact source는 `8d7d042`이며 main 통합 commit과 별도로 기록한다. 기본 네이티브 실행과 서버 빌드 경로에는 Docker Desktop·WSL·SSH가 필요하지 않다. 서버는 `app.builds.v1`을 제공해야 한다. 이번 인수는 Codex·Claude 각각의 새 업무 앱 개발·서버 원격 빌드·dev 배포·데이터 보존까지이며, 화면·사용성 개선은 [보류 중인 사용성 개선](PLAN.md#보류-중인-사용성-개선)에 기록한다.

2026-11-02 Linux에서 Codex·Claude 각각의 독립 개발과 v1→v2 인수를 완료했다. 각 도구는 새 폴더·앱 ID·대화로 비품대장 v1을 개발하고 자신의 같은 대화를 재개하여 v2의 보관 위치 기능을 추가했다. 실제 Rust/앱 검사, 서버 원격 빌드와 dev 배포를 수행했으며 최종 v2 평가에서 API CRUD·입력 검증·소유권, 브라우저 생성·수정·삭제·새로고침, 동일 DB와 v1 레코드 보존을 확인했다. v1과 v2에서 각각 실행한 검사의 차이와 source/build/job/release/image는 [독립 AI 인수 결과](INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md)를 따른다.

Windows 설치기 구현과 새 CI 배포물 검증 절차는 [준비 완료](WINDOWS-0.6.3-ACCEPTANCE.md)다. 실제 Windows 11 설치·갱신·제거 검사는 사용자 지시에 따라 사용자가 추후 수행하며 `NOT_RUN`으로 남긴다.

독립 AI 평가환경의 컴파일러 별칭·프록시 요청 본문 읽기·Codex 프로젝트 신뢰 설정을 보완한 뒤 개발을 이어갔다. 평가자가 앱 업무 코드나 SDK를 작성·수정하지 않았다. 이번 인수에서는 로컬 `app dev`를 실행하지 않았으므로 로컬→dev 데이터 보존은 `NOT_RUN`이다. prod 승격·전체 V7·여러 차례의 새 환경 반복 검증도 완료 범위에 포함하지 않는다.

2026-10-29 Windows 11 / PowerShell 7.6.5에서 소스 `2fabe68`의 CLI로 네이티브 Todo CRUD → 서버 원격 빌드 → dev 배포 → 동일 DB/레코드 → 브라우저 확인을 통과했다. 사용자 선택 admin 계정의 결과이며 개발자 최소 권한·PowerShell 5.1·운영 승격까지 확대하지 않는다. [실행 기록](WINDOWS-VALIDATION.md)과 [검증 행렬](CLI-VALIDATION-MATRIX.md)에 미실행 범위를 구분한다.

10-30 게시 0.6.3 배포물의 다운로드·해시 확인과 11-02 CLI/MCP 연결 검사도 별도 증거다. 이 기록들은 새 CI 설치기의 실제 실행이나 Codex·Claude의 새 세션 개발 인수를 대신하지 않는다.

## 설치와 로그인

현재 게시된 [CLI 설치 안내](https://v2.dwdcc.kr/cli/install)에서 0.6.3 설치기와 해당 OS의 CLI를 받는다. 실제 내려받은 파일의 버전·source·SHA-256을 [게시 기록](CLI-VALIDATION-MATRIX.md)과 대조하고, 설치 후 실행 경로와 실행 파일 SHA-256도 기록한다. 게시 배포물과 소스에서 직접 빌드한 실행 파일을 구분한다. 아래 `miy`는 설치된 CLI이며, 생성 프로젝트의 `.miy-tools/miy` 또는 `.miy-tools/miy.exe`를 사용할 때에도 실제 실행 경로와 버전을 확인한다.

Windows 사용자는 [0.6.3 실제 인수 절차](WINDOWS-0.6.3-ACCEPTANCE.md)와 [GUI 설치 안내](WINDOWS-INSTALLER.md)에 따라 다음 실제 인수를 추후 수행한다. 일반 사용자 여부·Windows 버전·PowerShell 5.1/7을 기록하고 최초 설치, 기존 설치의 갱신, 제거와 재설치를 각각 확인한다. 갱신 전후 실행 경로·버전·파일 해시와 기존 context·프로젝트 보존을 비교하며 원격 앱 DB 보존은 별도로 확인한다. 제거 시에는 실행 파일·설치 등록·PATH 정리와 사용자 데이터 보존을 구분하여 기록한다. 제거 후 이어서 앱을 개발하려면 먼저 재설치한다. 아직 실행하지 않은 단계는 `NOT_RUN`으로 둔다.

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

각 도구의 설치·인증을 확인한 뒤 해당 폴더에서 새 v1 세션을 열고 다음과 같이 요청한다. 인증이 준비되지 않은 도구는 `NOT_RUN`과 준비되지 않은 이유를 기록한다. v1은 각 도구의 독립 새 대화로 시작하고, 아래 v2는 해당 도구 자신의 v1 대화를 재개한다. 다른 도구의 결과를 복사하거나 하나의 대화를 두 도구의 독립 실행으로 집계하지 않는다.

> 이 프로젝트의 지침과 MCP·CLI를 읽고 비품대장 앱을 만들어줘. 비품명·수량을 등록·조회·수정·삭제하고, 로그인 사용자가 자기 비품만 다루도록 SDK Identity와 데이터베이스를 사용해줘. 잘못된 입력과 다른 사용자의 접근을 거부하고 의미 있는 업무 검사를 추가해줘. 앱 검사와 Rust 검사를 실행한 뒤 소스를 커밋하고 서버 원격 빌드로 dev 영역에 배포해줘. 실제 실행한 명령·도구·source/build/job/release/image와 수동 개입을 기록해줘. UI/UX 개선은 TODO로 남기고 운영 배포는 실행하지 마.

v1 배포를 별도로 평가하고 보존할 레코드를 만든 뒤 같은 대화를 재개하여 요청한다.

> 비품대장에 선택적인 보관 위치를 추가해줘. 적용된 SQL을 수정하지 않고 새 migration을 추가하고 API·화면·검사를 함께 갱신해줘. 이전 레코드의 UUID·비품명·수량과 같은 DB를 보존해야 해. 앱 버전을 올리고 소스를 커밋한 뒤 서버 원격 빌드와 두 번째 dev 배포를 완료해줘. 평가자가 만든 보존 레코드는 수정·삭제하지 말고 실제 실행 결과와 작업 식별자를 기록해줘.

생성 프로젝트에는 `AGENTS.md`, `CLAUDE.md`, 개발/CLI 스킬, CLI 참조, 프로젝트 실행 파일과 두 도구의 MCP 설정이 들어 있다. 각 AI 도구의 프로젝트 신뢰·MCP 승인 화면은 정상적으로 처리한다. `doctor`의 MCP 검사는 로컬 프로세스 시작과 도구 목록 검사이며, 실제 AI가 도구를 사용했다는 증거와 구분한다.

도구별로 AI 이름·버전·새 세션 식별자, OS·CLI 실행 파일의 버전과 해시, 앱 ID·소스 commit, 실제 MCP/CLI 호출과 수동 개입을 별도 기록한다. 비밀정보를 제거한 실행 기록으로 AI가 프로젝트 지침을 읽고 개발·검증·배포를 수행했는지 확인한다.

## 원격 빌드·dev 배포와 v1→v2 확인

1. v1 업무 코드와 의미 있는 검사를 작성한 뒤 `miy app fmt`, `miy app check`와 해당 업무 검사를 실행한다. 이 검사를 로컬 개발 서버 실행 증거로 대신하지 않는다.
2. 소스를 커밋하고 `miy app build --remote`, `miy app deploy --environment dev`를 실행한다. 기본 `app build`는 로컬 Docker 빌드이므로 Docker 없는 PC에서는 `--remote`를 명시한다. 작업 ID로 watch·get·logs를 확인하고 필요한 경우 resume으로 상태를 복구한다.
3. v1 배포의 API CRUD·잘못된 입력 거부·사용자 소유권과 브라우저 표시를 별도로 검사한다. 보존할 레코드를 만들고 ID·비품명·수량, DB 식별자, installation·release·image와 원격 빌드·배포 작업을 기록한다.
4. 같은 도구의 v1 대화를 재개하여 v2 기능과 새 migration을 추가하고 manifest version을 올린다. 검사·커밋·서버 원격 빌드·dev 배포를 반복한다. v2의 source commit·build·deploy job·image가 실제 installation과 일치하는지 확인한다.
5. v2에서 같은 DB·installation을 유지한 채 release가 바뀌었는지, 보존한 UUID·비품명·수량이 같은지 확인한다. 새 보관 위치의 생성·수정, 다른 사용자의 조회·수정·삭제 거부, 브라우저 폼의 생성·수정·삭제·새로고침과 DB 반영을 검사한다. 행 수만으로 데이터 보존을 판정하지 않는다.
6. 실제 확인한 결과만 [인수 기록](../templates/rust-react/docs/ACCEPTANCE.md) 형식으로 남긴다. `miy app accept` 제출 여부는 별도로 기록하고, 실행하지 않은 명령을 완료 목록에 넣지 않는다.

이번 인수는 dev까지다. prod 승격과 dev/prod DB 분리 검사는 별도 작업으로 수행하며, 이후 명시된 범위와 운영 권한을 확인한 뒤 검증한 동일 이미지를 승격한다.

## 별도로 남은 로컬 실행 검증

이번 독립 AI 인수에서 `miy app dev --no-open`을 통한 로컬 네이티브 실행은 `NOT_RUN`이다. 이 경로를 추가 인수할 때에는 서버 개발 DB에 연결한 로컬 앱에서 CRUD와 인증·권한을 확인하고 보존할 레코드를 만든다. 개발 세션을 `miy app dev-stop`으로 종료한 뒤 커밋 소스를 서버에서 빌드·dev 배포하여 같은 DB와 레코드가 이어지는지 검사한다. 연결 실패를 별도 로컬 DB로 자동 전환하여 통과로 처리하지 않는다.

스키마 변경은 이전 개발 앱을 중지할 수 있다. 적용된 SQL 파일을 수정·삭제하지 않고 새 마이그레이션을 추가한다. 세션 종료·만료·권한 회수는 임시 연결을 닫지만 데이터베이스를 삭제하지 않는다. 로컬 전용 Docker DB가 필요한 경우에만 `miy app dev --runtime docker --database local`을 명시한다.

실패 시 명령, 오류 코드, job/session ID, 기대 결과와 실제 결과를 기록한다. 비밀번호·토큰·쿠키·DB 접속 문자열은 테스트 기록에 복사하지 않는다. 성공하지 않은 단계와 실행하지 않은 단계는 각각 FAIL과 NOT_RUN으로 구분한다.
