# Windows 네이티브 기반 검증 안내

- 작성일: 2026-10-27
- 현재 코드 버전: CLI **0.4.2** / 개발 번들 **0.6.1**
- 대상: Windows 11 x64, Windows PowerShell 5.1과 PowerShell 7
- 상태: Windows에서 사용자가 직접 빌드·검증하고 결과를 추가할 안내. **사용자 Windows 11 PC의 실행 결과는 아직 없다.** 호스티드 Windows MSVC CI 결과는 아래에 별도로 기록한다.

이 문서는 현재 구현된 클라이언트 빌드 경계, OS 유틸리티, 앱 생성·설정과 번들 바이트 보존을 확인한다. [CLI 0.5 구현 계획](CLI-IMPLEMENTATION-PLAN.md)의 전체 기능이나 Windows 정식 지원 완료를 선언하지 않는다. 두 PowerShell 버전에서 아래 절차를 각각 실행하고, 같은 commit이라도 실행 기록은 별도로 남긴다. WSL 결과는 Windows 네이티브 결과로 기록하지 않는다.

## 1. 현재 증거와 이번 검증 범위

[기반 구현 결과](IMPLEMENTATION-RESULTS.md#cli-05-기반-구현-착수-2026-10-27)에는 Linux 서버/클라이언트 Rust 검사, Docker/PostgreSQL 격리 회귀, 브라우저 회귀, CLI 지침·번들 검사가 기록되어 있다. Linux의 별도 최소 crate에서 Windows OS 모듈을 `x86_64-pc-windows-msvc --all-targets`로 **타입검사**했지만, 전체 Windows CLI 링크·실행 결과는 아니다. Linux의 전체 Windows 교차 검사는 MSVC `lib.exe`가 없어 중단됐다.

[CLI 기반 CI](../.github/workflows/cli-foundation.yml)는 Windows MSVC 빌드·단위 검사와 OS 간 번들 비교를 정의한다. 워크플로 파일이 있다는 사실과 실제 실행 성공을 구분한다. 호스티드 Windows Server runner 결과만으로 Windows 11/PowerShell/Docker Desktop 검증을 대신하지 않는다.

`fbbd0870f45ec976d150988cc568c0765c9453f1`의 [CI 실행 40446385352](https://github.com/hurxxxx/miy-v2/actions/runs/40446385352)는 2026-10-27에 Linux 서버/클라이언트, macOS 클라이언트, Windows MSVC 클라이언트와 OS 간 번들 해시 비교를 모두 통과했다. Windows job은 `windows-2022`의 Bash 셸에서 debug 빌드·단위 검사·오프라인 앱 생성·Git 왕복을 수행했다. 아래 사용자 PowerShell release 빌드·ZIP·MCP 검증과 구분한다.

| 지금 확인할 수 있는 항목 | 이번 절차에서 확인하지 않는 항목 |
| --- | --- |
| `--no-default-features` Windows 클라이언트 빌드·단위 검사 | Windows 서버/worker 실행 |
| `.exe` 경로, Windows ACL·파일 교체 관련 단위 검사 | 실제 로그인 세션의 장기 갱신·철회, Credential Manager/DPAPI |
| 한글·공백 경로의 `app init`, `app setup`, `app describe` | Node/pnpm·Docker를 사용하는 전체 `app check/dev/build/deploy` |
| stdio MCP 초기화·도구 목록·오프라인 `get_cli_help` | Codex/Claude 실제 세션에서의 로그인·계약/API 조회·개발 작업 |
| 생성 앱의 Git checkout 바이트·보호 해시 보존 | 전체 setup/upgrade 실패 복구·프로세스 Job Object |
| 로컬 실행 파일·ZIP·SHA-256 생성 | Windows 설치·갱신·제거 스크립트와 공식 배포물 공개 |

현재 명령은 `miy app ...`다. 현재 출력 계약 구현은 `--output auto|text|json`과 `schemaVersion: 1` envelope를 추가한다. `fbbd087`은 이전 출력 형식이므로 검사할 commit을 반드시 기록한다. JSON 응답의 성공 값은 `data`, 실패는 `error`에서 읽고 종료 코드는 `$LASTEXITCODE`로 확인한다. `auto`는 터미널에서 text, 리디렉션에서 JSON이다. MCP와 Markdown 도움말은 원래 형식을 유지한다. `jsonl`은 아직 지원하지 않아 실행 전에 거부한다.

0.5의 루트 `context`, `auth`, `job`, `completion`, `--dry-run`, `doctor --for`는 아직 구현하지 않았으므로 이 절차에 사용하지 않는다. 현재 `app doctor`는 도구 버전 조회 중심이며 Docker daemon 준비 여부를 보장하지 않는다.

## 2. Windows 준비

일반 사용자 계정의 Windows 11 x64에서 PowerShell 5.1과 PowerShell 7을 준비한다. 시작 메뉴에서 Windows PowerShell과 PowerShell 7을 각각 열거나, 별도 터미널에서 `powershell.exe -NoProfile`, `pwsh.exe -NoProfile`로 시작한다. 관리자 권한 터미널을 기본 검증 환경으로 사용하지 않는다.

필요한 도구는 다음과 같다.

- Git for Windows와 Python 3.12: `git`, `python` 명령을 실행할 수 있어야 한다. Python이 `py -3.12`로만 제공되면 아래의 `python` 호출을 해당 명령으로 일관되게 바꾸고 실제 버전을 기록한다.
- Windows용 rustup과 Rust **1.99.1 MSVC** 도구 체인.
- Visual Studio Installer에서 **Desktop development with C++** 작업 또는 동등한 MSVC x64/x86 C++ Build Tools 및 Windows SDK. `ring` 등 C 코드를 포함하는 의존성을 위해 C/C++ 컴파일러와 `link.exe`, `lib.exe`도 필요하다. 설치 구성은 [Rust의 MSVC 준비 안내](https://rust-lang.github.io/rustup/installation/windows-msvc.html)와 [Microsoft의 C++ 설치 안내](https://learn.microsoft.com/en-us/cpp/build/vscpp-step-0-installation?view=msvc-170)를 따른다.

이미 필요한 구성요소가 있으면 재설치할 필요가 없다. `link.exe`/`lib.exe` 탐색에 실패하면 Visual Studio Installer의 C++/SDK 설치 상태를 확인한다. 필요할 때 설치된 Visual Studio의 Developer PowerShell에서 x64 도구 환경을 활성화하고, 실제 사용한 셸과 도구 버전을 기록한다. 시스템 PATH·Git 전역 설정·시스템 인코딩·전역 PowerShell 실행 정책을 이 검사 때문에 강제로 바꾸지 않는다.

이 안내의 Rust 빌드는 처음에 패키지 다운로드가 필요할 수 있다. 이후 앱 생성·설정·아래 MCP 도움말 검사는 서버 접속 없이 실행되며 Node/pnpm·Docker·플랫폼 로그인은 필요하지 않다. Docker Desktop을 포함하는 앱 개발 E2E는 별도 작업이다.

## 3. Checkout과 실행 기록 준비

기존 수정이 있는 작업본을 강제로 reset하지 않는다. 새 검증용 checkout을 만들거나 원하는 commit을 기존의 깨끗한 작업본에서 선택한다. 다음은 사용자 홈 아래 새 디렉터리의 예다. 이미 디렉터리가 있으면 다른 이름을 선택한다.

```powershell
$ErrorActionPreference = 'Stop'
$MiyCheckout = Join-Path $env:USERPROFILE 'miy-windows-validation'
git clone https://github.com/hurxxxx/miy-v2.git $MiyCheckout
if ($LASTEXITCODE -ne 0) { throw "git clone failed: $LASTEXITCODE" }
Set-Location -LiteralPath $MiyCheckout
git status --short --branch
if ($LASTEXITCODE -ne 0) { throw "git status failed: $LASTEXITCODE" }
git log -5 --oneline
if ($LASTEXITCODE -ne 0) { throw "git log failed: $LASTEXITCODE" }
```

Windows PowerShell 5.1에서는 네이티브 실행 파일의 비정상 종료가 `$ErrorActionPreference = 'Stop'`만으로 자동 예외가 되지 않는다. 아래에서는 각 네이티브 명령 직후 `$LASTEXITCODE`를 확인하고 실패하면 중단한다. PowerShell cmdlet은 `$ErrorActionPreference`를 따른다. 종료 코드의 의미는 [Microsoft의 자동 변수 문서](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-5.1)를 참고한다.

아래 준비 블록은 **각 PowerShell 세션에서** 실행한다. `$MiyEvidence`는 Git에서 제외된 실행별 디렉터리다. 다른 환경의 `.local` 증거가 이 checkout에 있다고 가정하지 않는다.

```powershell
$MiyRepo = (Get-Location).Path
$MiyCommit = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "git rev-parse failed: $LASTEXITCODE" }
$MiyDirty = @(git status --porcelain)
if ($LASTEXITCODE -ne 0) { throw "git status failed: $LASTEXITCODE" }
if ($MiyDirty.Count -ne 0) { throw 'Use a clean checkout or record the exact local patch separately.' }
$MiyRun = '{0}-ps{1}-{2}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $PSVersionTable.PSVersion.Major, ([guid]::NewGuid().ToString('N').Substring(0,8))
$MiyEvidence = Join-Path $MiyRepo ('.local\evidence\windows-validation\' + $MiyRun)
New-Item -ItemType Directory -Path $MiyEvidence | Out-Null
$MiyUtf8 = New-Object System.Text.UTF8Encoding($false)
$MiyResults = Join-Path $MiyEvidence 'commands.jsonl'

function Assert-MiyExit([string]$Step) {
    $MiyExit = $LASTEXITCODE
    $MiyRow = [ordered]@{
        time = (Get-Date).ToString('o')
        command = $Step
        exitCode = $MiyExit
        status = $(if ($MiyExit -eq 0) { 'success' } else { 'fail' })
    } | ConvertTo-Json -Compress
    [IO.File]::AppendAllText($MiyResults, $MiyRow + "`n", $MiyUtf8)
    if ($MiyExit -ne 0) { throw "$Step failed: exit $MiyExit; evidence: $MiyEvidence" }
}

$MiyOs = Get-CimInstance Win32_OperatingSystem
$MiyCpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$MiyEnvironment = [ordered]@{
    commit = $MiyCommit
    dirty = $false
    windows = $MiyOs.Caption
    osVersion = $MiyOs.Version
    osBuild = $MiyOs.BuildNumber
    osArchitecture = $MiyOs.OSArchitecture
    cpu = $MiyCpu.Name
    process64Bit = [Environment]::Is64BitProcess
    powershellVersion = $PSVersionTable.PSVersion.ToString()
    powershellEdition = $PSVersionTable.PSEdition
    repository = $MiyRepo
}
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'environment.json'), ($MiyEnvironment | ConvertTo-Json), $MiyUtf8)
Start-Transcript -Path (Join-Path $MiyEvidence 'console.txt')
git --version
Assert-MiyExit 'git --version'
python --version
Assert-MiyExit 'python --version'
rustup --version
Assert-MiyExit 'rustup --version'
```

이 transcript는 아래의 로그인 없는 검사에 사용한다. 토큰·쿠키·verifier·`.miy/session.json` 내용은 기록하지 않는다. 나중에 실제 로그인을 검사한다면 비밀 파일 내용을 출력하지 말고 인증 검사의 성공/실패와 필요한 오류 코드만 별도로 남긴다.

PowerShell 5.1의 `>`/`Out-File` 기본값은 UTF-16LE이므로 CLI JSON을 그대로 UTF-8 파일로 보존한다고 가정하면 안 된다. 위 예제는 기록 파일에 UTF-8을 명시한다. 아래 Python 검사는 UTF-8 JSON을 직접 쓰고 MCP 바이트를 직접 처리하므로 PowerShell의 텍스트 파이프를 거치지 않는다. 한글이 포함된 예제를 `.ps1`로 저장해 5.1에서 실행한다면 그 **로컬 스크립트**는 UTF-8 BOM으로 저장한다. 저장소 SDK·SQL 파일의 인코딩은 바꾸지 않는다. [PowerShell 인코딩 문서](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_character_encoding?view=powershell-5.1).

## 4. MSVC 클라이언트 검사와 release 빌드

명시적인 MSVC 도구 체인과 target을 사용한다. `--no-default-features`는 서버 모듈과 `serve/worker/migrate/seed` 명령을 제외하는 현재 클라이언트 빌드 계약이다. `cargo fmt`는 소스 형식 검사이므로 target/feature 플래그를 받지 않는다.

```powershell
rustup toolchain install 1.99.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy
Assert-MiyExit 'rustup toolchain install 1.99.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy'
rustc +1.99.1-x86_64-pc-windows-msvc -vV
Assert-MiyExit 'rustc +1.99.1-x86_64-pc-windows-msvc -vV'
cargo +1.99.1-x86_64-pc-windows-msvc --version
Assert-MiyExit 'cargo +1.99.1-x86_64-pc-windows-msvc --version'

$MiyTarget = 'x86_64-pc-windows-msvc'
$MiyTargetDir = Join-Path $MiyRepo 'target'
python scripts/check-bundle-portability.py source
Assert-MiyExit 'python scripts/check-bundle-portability.py source'
cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check
Assert-MiyExit 'cargo +1.99.1-x86_64-pc-windows-msvc fmt --all -- --check'
cargo +1.99.1-x86_64-pc-windows-msvc check --workspace --all-targets --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo check --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'
cargo +1.99.1-x86_64-pc-windows-msvc clippy --workspace --all-targets --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir -- -D warnings
Assert-MiyExit 'cargo clippy --workspace --all-targets --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target -- -D warnings'
cargo +1.99.1-x86_64-pc-windows-msvc test --workspace --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo test --workspace --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'
cargo +1.99.1-x86_64-pc-windows-msvc build --release --locked --no-default-features --target $MiyTarget --target-dir $MiyTargetDir
Assert-MiyExit 'cargo build --release --locked --no-default-features --target x86_64-pc-windows-msvc --target-dir <repo>/target'

$MiyExe = Join-Path $MiyTargetDir 'x86_64-pc-windows-msvc\release\miy.exe'
& $MiyExe --version
Assert-MiyExit 'miy.exe --version'
& $MiyExe --help
Assert-MiyExit 'miy.exe --help'
& $MiyExe app --help
Assert-MiyExit 'miy.exe app --help'
```

기대 결과는 `miy 0.4.2`이며 루트 도움말에 서버용 네 명령이 없어야 한다. 테스트 수·ignored 수와 이름은 실제 출력대로 기록한다. 빌드 성공 후의 실행 파일은 `target\x86_64-pc-windows-msvc\release\miy.exe`다. 실패한 검사는 로그와 종료 코드를 남기고 이후 의존하는 항목을 `not_run`으로 기록한다.

## 5. 한글·공백 경로, 번들·Git checkout 검사

```powershell
python scripts/check-bundle-portability.py check --cli $MiyExe --output (Join-Path $MiyEvidence 'bundle-windows.json')
Assert-MiyExit 'python scripts/check-bundle-portability.py check --cli <release miy.exe> --output <evidence>/bundle-windows.json'
```

공통 출력 계약이 포함된 후속 commit에서는 다음도 실행한다. 이 검사는 외부 플랫폼에 접속하지 않고 임시 loopback HTTP 서버와 fixture 자격증명으로 성공/실패 envelope, HTTP 분류, 오류의 비밀 비노출, 작업 실패를 확인한다. 실제 Windows 콘솔 Ctrl+C는 별도 검증 항목이며 Python의 POSIX signal 검사는 Windows에서 명시적으로 건너뛴다.

```powershell
python scripts/check-cli-output.py --cli $MiyExe
Assert-MiyExit 'python scripts/check-cli-output.py --cli <release miy.exe>'
```

이 스크립트는 임시 디렉터리의 `한글 경로/app with spaces`에 실제 CLI로 앱을 생성한다. 내장 템플릿·보호 파일·CLI 지침 해시를 확인하고, 생성 앱을 `core.autocrlf=true`로 commit/clone하여 관리 파일·SQL·CRLF 컴포넌트 fixture의 바이트 보존을 확인한다. fixture SQL은 실행하지 않는다. Git 설정은 명령/검증 checkout 범위이며 전역 `core.autocrlf`를 바꾸지 않는다. 임시 앱은 스크립트 종료 시 정리되고 JSON 해시 manifest가 증거 디렉터리에 남는다.

Windows manifest 하나의 성공은 OS 간 동일성 검사의 성공이 아니다. **같은 commit**에서 얻은 Linux/macOS manifest가 있으면 아래처럼 실제 파일 경로를 지정한다. 없으면 비교 항목은 `not_run`으로 기록한다.

```powershell
# 실제로 가져온 같은 commit의 두 manifest가 있을 때만 실행한다.
python scripts/check-bundle-portability.py compare (Join-Path $MiyEvidence 'bundle-windows.json') 'C:\evidence\same-commit\bundle-linux.json' 'C:\evidence\same-commit\bundle-macos.json'
Assert-MiyExit 'python scripts/check-bundle-portability.py compare <Windows> <Linux> <macOS> (same commit)'
```

## 6. 오프라인 앱 생성·setup·stdio MCP

아래는 공통 출력 계약 구현 이후 checkout 기준이다. 기반 commit `fbbd087`을 검사한다면 describe의 `--output json`을 생략한다. 다음 앱은 실행별 증거 폴더에 보존한다. `--url`은 설정만 저장하며 이 절차에서 해당 서버에 접속하지 않는다. 생성 시 플랫폼 등록·컴포넌트 다운로드·로그인은 수행하지 않는다.

```powershell
$MiyApp = Join-Path $MiyEvidence '한글 경로\app with spaces'
& $MiyExe app init $MiyApp --id windows-foundation-check --name 'Windows 한글 검사' --url http://localhost:18000
Assert-MiyExit 'miy.exe app init <Korean-and-space-path> --id windows-foundation-check --name <Korean-name> --url http://localhost:18000'
Push-Location -LiteralPath $MiyApp
try {
    & $MiyExe app setup
    Assert-MiyExit 'miy.exe app setup (offline)'
    & $MiyExe app setup
    Assert-MiyExit 'miy.exe app setup (repeat)'
    & '.\.miy-tools\miy.exe' --version
    Assert-MiyExit '.miy-tools/miy.exe --version'
    & '.\.miy-tools\miy.exe' app describe --format json --output json
    Assert-MiyExit '.miy-tools/miy.exe app describe --format json --output json'
} finally {
    Pop-Location
}
```

`app setup`은 전역 Codex/Claude 설정을 바꾸지 않고 프로젝트 MCP 설정·동봉 CLI·관리 지침을 맞춘다. 기존 사용자 `.gitattributes`와 충돌하면 덮어쓰기 대신 오류를 내는 것이 현재 계약이다. 위의 새 fixture에서는 반복 setup이 성공해야 한다. `app check`는 컴포넌트·개발 도구를 요구하는 전체 앱 검사이므로 여기의 오프라인 setup 성공과 구분한다.

MCP는 `miy.exe app mcp`를 단독 실행하면 stdin에서 요청을 기다린다. 다음 로컬 Python probe는 초기화, 도구 목록, 로그인 없는 `get_cli_help`를 실제 stdio로 호출하고 자식 프로세스를 종료한다. 플랫폼이 필요한 `get_contract`나 `search_api`는 호출하지 않는다.

```powershell
$MiyMcpProbe = Join-Path $MiyEvidence 'mcp-offline.py'
$MiyMcpSource = @'
import json
from pathlib import Path
import subprocess
import sys
import threading

project, evidence = map(Path, sys.argv[1:3])
with (evidence / "mcp-stderr.txt").open("wb") as errors:
    process = subprocess.Popen(
        [str(project / ".miy-tools" / "miy.exe"), "app", "mcp"],
        cwd=project, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=errors, text=True, encoding="utf-8",
    )
    timeout = threading.Timer(30, process.kill)
    timeout.start()
    responses = []
    def send(value):
        process.stdin.write(json.dumps(value, ensure_ascii=True) + "\n")
        process.stdin.flush()
    def request(identifier, method, params):
        send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
        while True:
            line = process.stdout.readline()
            if not line:
                raise RuntimeError("MCP exited or timed out before response")
            response = json.loads(line)
            if response.get("id") == identifier:
                if "error" in response:
                    raise RuntimeError(response["error"])
                responses.append(response)
                return response["result"]
    try:
        request(1, "initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "windows-foundation-check", "version": "1"},
        })
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listing = request(2, "tools/list", {})
        assert any(tool["name"] == "get_cli_help" for tool in listing["tools"])
        reply = request(3, "tools/call", {"name": "get_cli_help", "arguments": {}})
        assert not reply.get("isError", False), reply
        help_data = json.loads(next(block["text"] for block in reply["content"] if block["type"] == "text"))
        assert help_data["cliVersion"] == "0.4.2", help_data["cliVersion"]
        assert help_data["bundleVersion"] == "0.6.1", help_data["bundleVersion"]
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        (evidence / "mcp-offline.json").write_text(
            json.dumps({"status": "success", "responses": responses}, ensure_ascii=True, indent=2),
            encoding="utf-8",
        )
        print("PASS: stdio initialize, tools/list, get_cli_help; no platform login")
    finally:
        timeout.cancel()
        if process.poll() is None:
            process.kill()
            process.wait()
'@
[IO.File]::WriteAllText($MiyMcpProbe, $MiyMcpSource, $MiyUtf8)
python $MiyMcpProbe $MiyApp $MiyEvidence
Assert-MiyExit 'python <evidence>/mcp-offline.py <fixture-app> <evidence>'
```

이 성공은 실제 Codex/Claude 앱에서의 MCP 연결 승인·세션 유지 검증을 포함하지 않는다. 실제 클라이언트 검사는 추후 각 클라이언트 버전과 함께 별도로 기록한다.

## 7. 로컬 ZIP과 SHA-256 생성

아래 파일은 검증한 바이너리를 옮기기 위한 **로컬 검증 산출물**이다. 설치기나 공식 릴리스가 아니며 PATH 등록·자동 갱신·제거 기능을 제공하지 않는다.

```powershell
$MiyPackageName = 'miy-0.4.2-windows-x64-' + $MiyCommit.Substring(0,12)
$MiyPackageDir = Join-Path $MiyEvidence $MiyPackageName
New-Item -ItemType Directory -Path $MiyPackageDir | Out-Null
$MiyPackagedExe = Join-Path $MiyPackageDir 'miy.exe'
Copy-Item -LiteralPath $MiyExe -Destination $MiyPackagedExe
$MiyZip = Join-Path $MiyEvidence ($MiyPackageName + '.zip')
Compress-Archive -LiteralPath $MiyPackagedExe -DestinationPath $MiyZip
$MiyExeHash = (Get-FileHash -LiteralPath $MiyPackagedExe -Algorithm SHA256).Hash.ToLowerInvariant()
$MiyZipHash = (Get-FileHash -LiteralPath $MiyZip -Algorithm SHA256).Hash.ToLowerInvariant()
$MiyChecksums = "$MiyExeHash  $MiyPackageName/miy.exe`n$MiyZipHash  $MiyPackageName.zip`n"
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'SHA256SUMS'), $MiyChecksums, $MiyUtf8)
[IO.File]::WriteAllText((Join-Path $MiyEvidence 'source-commit.txt'), $MiyCommit + "`n", $MiyUtf8)
Get-Content -LiteralPath (Join-Path $MiyEvidence 'SHA256SUMS') -Encoding UTF8
Stop-Transcript
```

다른 PC에서 ZIP을 사용할 때에는 받은 ZIP의 `Get-FileHash -Algorithm SHA256` 결과와 기록값을 비교하고, 새 디렉터리에 풀어 `miy.exe --version`의 종료 코드도 확인한다. SDK/Node/Docker 없는 실제 PC에서의 원격 관리 검증은 별도 결과가 필요하다.

## 8. 남은 구현과 결과 반영 규칙

Windows 네이티브 빌드·단위 검사·오프라인 생성이 모두 성공해도 다음 항목은 완료로 바꾸지 않는다.

- Windows Job Object 등 소유권 기반 자식 프로세스 정리.
- setup/upgrade 실행 파일·지침·잠금·MCP 설정 전체의 staging 및 중간 실패 복구. 현재 파일 단위 교체와 충돌 검사는 전체 트랜잭션 완료를 뜻하지 않는다.
- PowerShell 설치·갱신·제거, 실제 사용 중인 프로젝트 MCP와의 갱신 흐름, 공식 ZIP 공개.
- 전체 Docker Desktop Linux 모드 개발·배포, 서버 CPU와 이미지 platform 대조, 원격 서버 UI/API와 데이터 보존 실증.
- CLI 0.5의 새 명령/context/device-code/갱신 인증/작업 이벤트/로그/dry-run. 기존 앱 명령의 공통 출력 계약만 먼저 구현했다.

실행 결과는 아래 형식으로 **새 기록을 추가**한다. 실패 뒤 수정하여 재실행했으면 이전 실패 기록을 삭제하거나 성공으로 덮어쓰지 않고 새 run ID·commit으로 추가한다. 검사하지 않은 행은 `not_run`과 이유를 남긴다. 후속 문서 업데이트 시 증거와 실제 실행 범위에 맞춰 [구현 결과](IMPLEMENTATION-RESULTS.md)와 [인계 체크리스트](CLI-HANDOFF.md)를 갱신한다.

```markdown
### Windows 실행 기록 — YYYY-MM-DD / RUN_ID

- 실행자:
- commit (`git rev-parse HEAD`):
- 작업 트리: clean / dirty (dirty면 patch 위치)
- Windows 버전·build / CPU / 64-bit 프로세스 여부:
- PowerShell 전체 버전·Edition:
- Rust / Cargo / MSVC Build Tools / Windows SDK / Git / Python 버전:
- 일반 사용자 / 관리자 여부:
- 실행 명령 원문·종료 코드: <evidence>/commands.jsonl
- 상세 로그: <evidence>/console.txt 및 실패 로그
- 실행 파일·ZIP SHA-256: <evidence>/SHA256SUMS
- 증거 보관 위치 (이 PC 경로와 공유한 위치를 구분):

| 항목 | 명령 또는 시나리오 | 종료 코드 | 결과: success / fail / not_run | 증거·실패 또는 미실행 사유 |
| --- | --- | --- | --- | --- |
| 소스/LF/SQL | check-bundle-portability.py source | | not_run | |
| 형식 | cargo fmt | | not_run | |
| Windows MSVC 타입 검사 | cargo check --no-default-features --target x86_64-pc-windows-msvc | | not_run | |
| Clippy | cargo clippy ... -- -D warnings | | not_run | |
| 단위 검사 | cargo test ...; passed/failed/ignored 수 | | not_run | |
| 네이티브 release 빌드 | cargo build --release ... | | not_run | |
| 실행/명령 경계 | miy.exe --version, --help, app --help | | not_run | |
| 번들·한글/공백·Git 왕복 | check-bundle-portability.py check | | not_run | |
| 공통 출력·오류·종료 코드 | check-cli-output.py --cli <miy.exe> | | not_run | 후속 출력 계약 commit; Windows 콘솔 Ctrl+C 제외 |
| OS 간 해시 비교 | compare; 각 OS의 동일 commit 명시 | | not_run | |
| 앱 생성/반복 setup | app init, app setup 2회 | | not_run | |
| 오프라인 stdio MCP | initialize, tools/list, get_cli_help | | not_run | |
| 로컬 ZIP/체크섬 | Compress-Archive, Get-FileHash (cmdlet: 예외 유무 기록) | 해당 없음 | not_run | |
| 실제 Codex/Claude MCP | 클라이언트·버전·범위 명시 | | not_run | 이번 오프라인 probe와 별도 |
| Docker Desktop 개발·배포 | 수행한 전체 단계 명시 | | not_run | 기반 검사와 별도 |
| 실제 사용자 인증·원격 운영 | 서버 commit·CPU·대상·수행 범위 | | not_run | 기반 검사와 별도 |

- 이번 실행으로 확인한 범위:
- 남은 문제와 다음 작업:
- 이전 실패를 해결한 경우 이전 RUN_ID 및 변경 commit:
```

## Windows 실행 기록

사용자 Windows 11 실행 기록은 아직 없음. 호스티드 CI 증거는 위 1절을 참고하고, Windows에서 직접 실행한 결과는 위 템플릿으로 이어서 추가한다.
