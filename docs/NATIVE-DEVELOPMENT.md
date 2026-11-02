# Docker 없는 로컬 개발

CLI 0.6.1부터 / 번들 0.7.1은 기본 `miy app dev`에서 로컬 Rust 실행 파일을 구동하고 빌드한 React 화면을 CLI의 loopback 서버에서 제공한다. CLI 0.6.2는 `app build --remote`로 서버 Docker 빌드도 지원한다. Docker Desktop, WSL, SSH, 로컬 PostgreSQL은 필요하지 않다. 이 변경은 개발 중인 소스에 해당하며, 기존 홈페이지에 게시된 CLI 0.6.0 설치 후보와 구분한다.

Rust/MSVC·Windows SDK, Node, 프로젝트에 지정된 pnpm, Git과 개발서버 로그인을 준비한다. 새 앱을 생성하거나 이전 앱의 소스를 커밋한 후 새 CLI로 `app upgrade`를 실행한다. 새 번들은 Rust SDK에 `MIY_BIND_ADDR`를 추가해 네이티브 백엔드가 임의의 loopback 포트에서 실행되게 한다. 업무 코드·적용 SQL·의존성 lock·컴포넌트는 보존한다. 관리 파일을 직접 수정하지 않는다.

```powershell
miy component sync
miy doctor --for develop
miy app register
# 등록 승인 후
miy app dev --no-open
miy app dev-stop
```

기본 DB는 선택한 서버의 해당 앱 개발 DB다. CLI가 기존 개발 세션 API로 마이그레이션을 요청하고 임시 DB 권한을 받은 뒤 메모리 안의 인증된 WS/WSS 터널로 연결한다. DB 포트와 Rust 백엔드는 loopback에만 바인딩한다. 브라우저는 CLI 게이트웨이를 사용하며 사용자 ID를 임의로 지정하거나 인증을 우회하지 않는다. 중지 시 앱 프로세스·게이트웨이·터널·서버 세션을 종료하며 개발 DB의 레코드는 보존한다.

컨테이너 실행을 명시하려면 `miy app dev --runtime docker`와 `miy doctor --for develop --runtime docker`를 사용한다. 별도 로컬 DB는 `miy app dev --runtime docker --database local`로 선택한다. 서버 연결 실패를 로컬 DB로 자동 전환하지 않는다.

`app check`와 네이티브 로컬 개발은 Linux 배포 이미지 생성과 별개다. 기본 `app build`는 기존 로컬 Linux Docker 빌드를 유지한다. Docker 없는 Windows에서는 호환 CLI 0.6.2와 `app.builds.v1` 기능을 제공하는 서버를 사용해 다음과 같이 실행한다. 기존 context와 로그인을 재사용하며, CLI는 로컬 Docker·Rust·Node 검사를 하지 않는다. 컴파일과 검사는 서버 Docker에서 실행한다.

이 브랜치의 Windows CLI를 직접 준비하려면 **기존 앱 폴더에서** 다음 PowerShell 명령을 실행한다. 기존 플랫폼 저장소는 변경하지 않고 임시 폴더에 별도로 받는다. 기존 설치 페이지의 구버전 CLI는 새 기능을 포함하지 않는다.

```powershell
$cliSource = Join-Path $env:TEMP ("miy-cli-" + [guid]::NewGuid().ToString("N"))
git clone --branch codex/server-docker-build --single-branch https://github.com/hurxxxx/miy-v2.git $cliSource
if ($LASTEXITCODE -ne 0) { throw "git clone failed" }
Push-Location $cliSource
try {
    cargo build --locked --release --no-default-features --bin miy
    if ($LASTEXITCODE -ne 0) { throw "CLI build failed" }
} finally { Pop-Location }
& (Join-Path $cliSource "target\release\miy.exe") app setup
if ($LASTEXITCODE -ne 0) { throw "app setup failed" }
.\.miy-tools\miy.exe auth status
```

이후 아래 `miy` 명령은 Windows에서 `.\.miy-tools\miy.exe`로 실행한다. 현재 선택한 context와 프로젝트 연결은 `app setup`이 보존한다. 아래 `todo-dev`는 이 검증에서 사용한 context 이름이며, 기존 Windows context 이름이 다르면 해당 이름으로 바꾸거나 프로젝트에 이미 연결된 context를 사용하도록 `--context todo-dev`를 생략한다.

```powershell
# 앱 폴더에서 새 CLI로 프로젝트 도구를 갱신한다. SDK 0.7.1과 업무 SQL은 유지된다.
miy app setup
miy component sync
miy app dev-stop
# 재배포할 때 manifest.json의 version을 사용하지 않은 다음 버전으로 올린다.
git add .
git commit -m "Prepare committed app for server build"

miy --context todo-dev --app todo-win-20261029 app build --remote --no-wait
# 위 결과의 id를 사용한다.
miy --context todo-dev --app todo-win-20261029 job watch BUILD_JOB_ID --timeout 1800
miy --context todo-dev --app todo-win-20261029 job logs BUILD_JOB_ID --after 0
miy --context todo-dev --app todo-win-20261029 app deploy --environment dev --no-wait
miy --context todo-dev --app todo-win-20261029 job watch DEPLOY_JOB_ID --timeout 600
miy --context todo-dev --app todo-win-20261029 app status
```

연결이 끊기면 출력된 요청 ID 또는 빌드 작업 ID로 `miy --context todo-dev --app todo-win-20261029 job resume ID --environment dev`를 실행한다. `job watch ID --output jsonl`은 순서 번호가 있는 빌드 로그를 출력한다. `job logs ID --after N`으로 마지막 번호 이후 로그를 조회할 수 있다. CLI 관찰 종료는 서버 작업을 취소하지 않는다. 동일한 소스·커밋·플랫폼의 진행 중이거나 성공한 작업은 중복 생성하지 않는다. 실패한 작업의 resume은 원래 실패를 보여 주며, 원인을 해결하고 `app build --remote`를 다시 실행하면 새 시도를 요청한다.

업로드는 깨끗한 Git HEAD의 허용된 업무·SDK·마이그레이션·잠금 파일만 포함한다. 인증 설정, `.git`, `.miy`, `.miy-tools`, `node_modules`, `target`, `dist`, `.env` 및 개인 키 파일은 제외하고 심볼릭 링크와 하위 저장소를 거부한다. 임의 파일명으로 하드코딩한 비밀을 모두 식별하는 기능은 아니므로 소스에 인증정보를 커밋하지 않는다. 서버는 전송 크기·SHA-256, 앱 개발 권한, SDK 원본, 공개된 컴포넌트 해시, 적용 SQL 이력을 검증한다. 빌드는 기존 개발 DB를 변경하지 않는다. 커밋 ID는 CLI가 Git HEAD에서 읽어 제공하는 출처 정보이며, 서버가 Git 저장소에 접속해 검증하는 서명 증명은 아니다. 서버가 검증하는 실제 소스 바이트의 식별자는 전송 SHA-256이다.

빌드 영수증에는 서버·사용자·앱·커밋·소스 해시·불변 이미지 ID가 결합된다. `app deploy --environment dev`는 이 영수증을 다시 조회하고 서버 이미지를 직접 사용한다. Windows로 이미지를 내려받거나 다시 업로드하지 않는다. 개발 배포는 로컬 세션을 종료한 뒤 같은 개발 DB를 재사용한다.

실행 결과는 [Windows 검증](WINDOWS-VALIDATION.md)에 기록한다. 소스 검사·실제 DB CRUD·브라우저 통합·배포 결과를 각각 구분한다.
