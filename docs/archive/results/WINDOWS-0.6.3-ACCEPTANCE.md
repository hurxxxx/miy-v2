> 보관 이력 — 2026-11-13 문서 정리 전 본문을 보존했다. 이 문서의 버전·현재 상태·PASS/FAIL은 당시 범위다. 새 설치·사용자 PC 인수는 [Windows 설치와 사용자 인수](../../WINDOWS-ACCEPTANCE.md), 후보 생성·게시는 [CLI 릴리스](../../CLI-RELEASE.md)를 따른다.

# 게시된 CLI 0.6.3 Windows 11 인수 준비

> 이 문서는 CLI **0.6.3 / source `8d7d042`** 후보의 당시 절차와 기록이다. 2026-11-03부터 [검사기](../../../scripts/check-windows-published-installer.ps1)는 [후보 파일](../../releases/windows-candidate.json)(현재 0.7.0)을 기대값으로 쓰므로, 아래 명령은 현재 후보를 검사한다. 현재 후보 절차는 [Windows 인수](../../WINDOWS-ACCEPTANCE.md)를 따른다. 0.6.3 당시 검사기는 commit `4a5980b` 이전 이력에 있다. 원래 대상과 해시는 당시 기록으로 보존한다.

이 문서는 **구현·실행 절차 준비 완료, 이번 사용자 Windows 실행은 `NOT_RUN`** 상태다. 사용자가 나중에 Windows에서 실행한 뒤 결과를 추가한다. Windows Server CI 통과를 이 표의 Windows 11 통과로 옮기지 않는다. 기존 [Windows 검증](WINDOWS-VALIDATION.md), [이전 0.6.3 Windows 게시 후 확인](../../releases/cli-0.6.3-windows-postdeploy-validation.json), [설치기 설명](../cli/WINDOWS-INSTALLER.md)은 별도 이력이며 덮어쓰지 않는다.

대상은 CLI **0.6.3**, SDK bundle **0.7.1**, [공개 설치 안내](https://v2.dwdcc.kr/cli/install)다. 2026-11-02에 읽은 [공개 API](https://v2.dwdcc.kr/api/v1/cli/install)는 아래 출처를 반환했다. 저장소를 main에 통합한 commit과 이미 게시한 바이너리의 source commit은 서로 다를 수 있다. 설치 파일의 출처를 main HEAD로 다시 표시하지 않는다.

| 항목 | 2026-11-02에 확인한 공개 값 |
|---|---|
| CLI source / installer source | 둘 다 `8d7d04271f9467d8bbdd16308e73715b33e4dc0c` |
| GUI EXE | `MiySetup-0.6.3-windows-x64.exe`, 6,105,600 bytes |
| EXE SHA-256 | `662f8a59c098ac4b2cbde4041799a99f0c6e21f193bbe8b31d2098f6095233af` |
| Windows CLI ZIP | `miy-0.6.3-x86_64-pc-windows-msvc.zip` |
| ZIP SHA-256 | `1403083b604b77036785d07c42f13c6aad754ea7ae6ff7de94bb15e0a1eb72e3` |
| ZIP 안 `miy.exe` SHA-256 | `055bc8655c706b586f1d8517cb719ac8cd1335107e2815aef10747fa3b7565e3` |

실행 때는 문서 값만 믿고 실행하지 않는다. 아래 도구가 API를 다시 읽고, 예상 버전·source commit·EXE/ZIP 관계·각 `.sha256`·실제 파일 해시·ZIP `release.json`·내장 CLI 해시·EXE 내부 provenance와 동일 ZIP을 모두 확인한다. API가 다른 후보로 바뀌면 중단한다. 새 후보 인수는 그 후보의 출처를 검토한 뒤 별도 계획으로 수행한다. 해시와 provenance는 Authenticode 서명이나 독립된 서명 attestation을 대신하지 않는다. 현재 후보는 서명되지 않았으므로 게시자 경고가 날 수 있으며, 보안 정책을 일괄 해제하지 않는다.

## 준비와 범위

Windows 11 x64의 **관리자 권한이 아닌 사용자 세션**에서 실행한다. 관리자 PowerShell·Windows Server·ARM64는 이 인수 도구가 거부한다. 회사 TLS 신뢰가 정상인 환경에서 HTTPS로 접근한다. 인증서 검사를 끄지 않는다.

Windows PowerShell 5.1(`powershell.exe`)과 PowerShell 7 x64(`pwsh.exe`)를 각각 준비한다. PowerShell 7은 5.1과 함께 설치할 수 있다. 설치 또는 사용자 폴더의 ZIP 방식은 [Microsoft 공식 설치 안내](https://learn.microsoft.com/en-us/powershell/scripting/install/install-powershell-on-windows)를 따른다. 이 검사 도구는 PowerShell 설치·PATH·실행 정책·프로필·Git 전역 설정을 변경하지 않는다. 기존 정책으로 실행이 차단되면 조직 정책에 맞는 별도 프로세스 실행 방식을 사용하고, 전역 `Set-ExecutionPolicy`로 우회하지 않는다.

저장소를 로컬 디스크의 한글·공백 경로에 checkout하면 경로 처리를 함께 확인할 수 있다. 심볼릭 링크·junction·UNC 경로는 사용하지 않는다. 이미 설치한 CLI와 프로젝트를 지우며 새 설치 환경을 만들지 않는다. 새 설치는 기존 MIY 설치가 없는 별도 일반 사용자 프로필/시험 PC에서, 갱신은 이전 버전이 이미 있는 환경에서 진행한다. 같은 0.6.3 재실행은 중복 설치 방지 확인이며 버전 갱신 통과가 아니다. 이전 버전 환경이 없으면 갱신은 `NOT_RUN`으로 둔다. 검사를 위해 현재 설치를 자동 downgrade하지 않는다.

[사전·후검사 스크립트](../../../scripts/check-windows-published-installer.ps1)는 다음만 수행한다.

- 공개 EXE/ZIP 다운로드와 출처 검증. 다운로드된 코드·GUI·CLI는 실행하지 않는다.
- OS/CPU/PowerShell 환경, 단계, 종료코드, 파일 해시와 보존 비교 결과 기록.
- 기존 config의 context 항목, 암호화 credential 파일/marker, 사용자가 선택한 프로젝트의 전후 지문 비교. 기존 context는 보존하고 설치 시 새 context/활성 선택 변경은 허용한다. 제거 시 config 바이트도 같아야 한다.

Windows Credential Manager의 값은 읽지 않는다. credential 파일 지문 통과는 vault 인증정보의 사용 가능성을 입증하지 않는다. 기존 로그인 사용성은 사용자가 별도로 확인한다. 기존 context/credential 파일이 없거나 프로젝트를 지정하지 않았다면 해당 보존 항목은 `NOT_RUN`이다. 프로젝트의 `.git`, `node_modules`, `target`, `dist`, `.local` 디렉터리는 제외하며, 나머지 파일이 10,000개 초과 또는 파일 하나가 512MiB 초과이면 중단한다. 이 범위를 전체 디스크·앱 DB·원격 데이터 보존 증거로 확대하지 않는다.

## 공개 파일 준비: 두 PowerShell에서 각각 실행

먼저 저장소 루트로 이동한다. 실행할 PowerShell 창에서 아래를 사용한다. PowerShell 5.1에서는 네이티브 프로그램의 실패가 `$ErrorActionPreference = 'Stop'`만으로 예외가 되지 않으므로 **각 호출 직후** `$LASTEXITCODE`를 확인한다.

```powershell
$checker = '.\scripts\check-windows-published-installer.ps1'
$shell = (Get-Process -Id $PID).Path
$PSVersionTable.PSVersion.ToString()
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase Prepare
if ($LASTEXITCODE -ne 0) { throw 'Prepare failed; do not run the downloaded installer.' }
```

성공 출력의 `run=run-...` 값을 아래 `$run`에 복사한다. 매번 GUID가 포함된 새 실행 폴더를 만들므로 재실행·PS5.1/7 결과가 서로 덮어쓰이지 않는다. 실패한 Prepare 폴더를 재사용하지 않고 다시 Prepare한다.

```powershell
$run = 'run-YYYYMMDDTHHMMSSZ-32자리GUID' # 실제 성공 출력으로 교체
$evidenceRoot = Join-Path '.local\windows-published-acceptance' $run
$installer = Join-Path $evidenceRoot 'downloads\MiySetup-0.6.3-windows-x64.exe'
```

증거는 `.gitignore`에 포함된 `.local/windows-published-acceptance/<run>/`에 저장한다. 새 실행 폴더는 데이터를 쓰기 전에 현재 사용자만 허용하는 보호 ACL을 설정하며, 다시 사용할 때도 검사한다. 각 단계는 새 `step-...` 하위 폴더의 `result.json` 또는 `failure.json`에 기록한다. 환경/ACL 선행 검사에서 중단되어 비공개 증거 폴더를 만들 수 없으면 고정된 실패 단계와 종료코드만 출력하므로 수동 결과표에 남긴다. 성공한 Prepare의 `candidate.json`이 이번 다운로드의 출처다. `expectedBundleVersion=0.7.1`은 인수 대상이며, 도구는 내장 번들을 실행해서 검사하지 않으므로 `bundleVersionVerification=NOT_RUN`을 함께 기록한다. `public-api.json`은 공개 API 응답이며, 거기에 있는 설치 명령 문자열을 실행하지 않는다.

원본 context·토큰·쿠키·SID·사용자 이름·프로필 경로·프로젝트 경로는 저장하지 않는다. 보존 비교는 실행별 임의 키로 만든 HMAC 지문을 사용한다. `comparison.key`와 `snapshot.json`을 포함한 실행 디렉터리는 로컬 비공개 자료로 유지한다. 공유할 때는 검토한 `candidate.json`, `result.json`, 아래 수동 결과표만 복사한다. 전체 폴더·PowerShell history·transcript·인증 응답·브라우저 쿠키를 업로드하지 않는다. PS5.1의 `>`/`Out-File` 기본 인코딩으로 JSON을 다시 저장하지 않는다. 스크립트는 UTF-8 BOM 없이 직접 기록한다.

## GUI 새 설치

새 설치를 할 수 있는 환경에서만 실행한다. 보존할 실제 작은 프로젝트가 있으면 선택하고, 없으면 `-ProjectPath`를 생략해 그 항목을 `NOT_RUN`으로 둔다. 같은 전후 검사에는 같은 경로를 전달한다. 검사 사이에 프로젝트 편집·CLI 로그인/refresh·MCP 작업을 하지 않는다. `MIY_CONFIG_DIR`를 사용하는 환경이라면 GUI와 사전·후검사가 같은 설정 루트를 사용하는지 먼저 확인한다. PowerShell 창에서만 바꾼 환경변수는 탐색기에서 실행한 GUI에 전달되지 않을 수 있으므로, 그 경우 사용자가 같은 셸에서 검증된 EXE를 직접 열어 동일한 환경을 상속한다. 이를 맞추기 위해 전역 환경변수나 기존 설정 위치를 변경하지 않는다.

```powershell
$project = 'D:\한글 작업\보존할 프로젝트' # 로컬에서만 사용; 증거표에 원문을 적지 않는다
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeInstall -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeInstall failed.' }
$beforeInstall = 'step-실제성공출력' # 실제 step 값
```

사용자가 탐색기에서 `$installer`에 해당하는 **검증된 파일을 직접 열고** 다음을 확인한다. 스크립트는 GUI를 열거나 버튼을 누르지 않는다.

1. 설치 전 서버 주소 입력란·연결 확인·설치·CLI 제거 상태를 관찰한다. 새 환경에서는 제거할 설치가 없고, 서버 주소를 입력할 수 있어야 한다.
2. `https://v2.dwdcc.kr`를 입력하고 연결 확인 후 **설치**를 누른다. 관리자 승격 없이 `%LOCALAPPDATA%\Programs\miy\bin`에 설치되는지 확인한다.
3. 완료 화면, 같은 버전의 설치 버튼 비활성화, CLI 제거 활성화를 확인한다. 창을 닫고 다시 열어도 같은 상태인지 기록한다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterInstall -RunId $run -BaselineStep $beforeInstall -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterInstall failed; preserve the evidence and inspect before retrying.' }
```

새 PowerShell 창에서 `miy --version`이 `miy 0.6.3`인지, 새 GUI가 추가한 사용자 PATH로 실행되는지 직접 확인하고 종료코드만 기록한다. 원본 PATH를 저장하지 않는다. GUI가 직접 하는 PATH 변경과 사전·후검사 도구의 무변경을 구분한다.

```powershell
miy --version
if ($LASTEXITCODE -ne 0) { throw 'Installed CLI version check failed.' }
```

## GUI 갱신과 기존 연결·인증·프로젝트 보존

이전 버전이 설치된 환경에서 Prepare부터 별도 run으로 수행한다. 이미 0.6.3이면 갱신 검사를 억지로 통과시키지 않는다. `AfterUpdate`는 이전 바이너리 해시와 후보 해시가 달라야 통과한다. 기존 MIY 창/MCP/CLI 작업은 정상 종료하며 강제 종료하지 않는다.

사용자가 갱신 전에 기존 `miy auth status`를 확인하고, 성공 여부·종료코드만 수동 기록한다. 필요한 로그인이나 refresh가 있었다면 그 후에 BeforeUpdate를 찍는다. 검사 스크립트는 인증 명령을 호출하지 않는다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeUpdate -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeUpdate failed.' }
$beforeUpdate = 'step-실제성공출력'
# 사용자가 검증된 EXE를 열고 기존 서버 표시를 확인한 뒤 GUI에서 갱신한다.
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterUpdate -RunId $run -BaselineStep $beforeUpdate -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterUpdate failed; do not erase the old state or journal.' }
```

AfterUpdate 후 새 셸에서 `miy --version`, `miy auth status`를 직접 실행하고 각각 `$LASTEXITCODE`를 확인한다. 원본 auth 출력은 수집하지 않는다. 기존 context가 남았는지 화면으로 확인하고, 기존 인증이 재로그인 없이 사용 가능한지 별도 결과로 기록한다. 만료·서버 철회 등으로 인증이 실패하면 파일 보존 성공과 인증 사용성 실패를 따로 적는다. 원인 확인 없이 보존 성공으로 합치지 않는다.

실행 중인 파일 잠금, 중단/복구, 변조 거부를 실제 사용자 설치에서 강제로 주입하지 않는다. 그런 파괴적 실패 주입은 기존 격리 fixture/CI 범위이며 이번 수동 기본 인수와 분리한다. 자연 발생한 실패는 당시 단계·고정 오류 코드·종료코드만 기록하고 기존 journal/backup을 삭제하지 않는다.

## GUI 제거

해당 시험 설치를 사용자가 제거하기로 한 경우에만 진행한다. 업무용 기존 전역 설치를 준비 과정에서 자동 제거하거나 덮어쓰지 않는다. 사용 중인 CLI/MCP를 정상 종료하고 전후 스냅샷 사이에는 설정·프로젝트를 변경하지 않는다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeRemove -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeRemove failed.' }
$beforeRemove = 'step-실제성공출력'
# 사용자가 동일 GUI의 CLI 제거를 직접 누르고 완료를 확인한다.
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterRemove -RunId $run -BaselineStep $beforeRemove -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterRemove failed; retain evidence.' }
```

사용자는 새 셸에서 GUI가 관리하던 PATH 항목이 정리됐는지 확인하되 원본 PATH를 기록하지 않는다. 다른 경로에 설치된 CLI가 있다면 `miy` 명령이 계속 발견될 수 있으므로 전체 명령 검색 실패를 제거의 필수 조건으로 삼지 않는다. 도구는 GUI 기본 경로의 `miy.exe` 부재와 기존 config·credential 파일·선택 프로젝트 보존을 검사한다. 기본 경로 밖의 파일, 인증 vault, 원격 앱/DB가 삭제되지 않았다는 범위는 별도 확인 결과로 남긴다.

## 수행 결과: 실행할 때 추가

두 셸의 Prepare·사전·후검사를 별도 run으로 기록한다. GUI 자체는 PowerShell 명령이 아니므로 눈으로 확인한 결과와 도구 결과를 구분한다. 한 셸에서 한 번 수행한 GUI 작업을 다른 셸의 독립 실행으로 중복 집계하지 않는다. 같은 상태를 두 셸로 확인했다면 `same GUI event`로 표시한다.

| 항목 | 현재 상태 | 실제 실행 후 필요한 근거 |
|---|---|---|
| 공개 API 확인·절차 작성 | PASS (Linux에서 읽기만 수행) | 2026-11-02, 공개 source/해시 위 표 |
| PowerShell 7 문법 검사 | PASS (Linux portable PS 7.6.9 Parser) | 네이티브 실행과 별도 |
| Windows 11 PS5.1 다운로드/출처 검사 | NOT_RUN | run/step, PS/OS build, exit code |
| Windows 11 PS7 다운로드/출처 검사 | NOT_RUN | 별도 run/step, PS/OS build, exit code |
| 새 GUI 설치·같은 버전 재실행 | NOT_RUN | Before/AfterInstall, 버튼 상태·새 셸 버전 |
| 이전 버전에서 GUI 갱신 | NOT_RUN | 다른 이전 바이너리, Before/AfterUpdate |
| 기존 context·인증 사용성·프로젝트 보존 | NOT_RUN | 지문 비교 + 사용자의 auth 결과; vault 미검사 구분 |
| GUI 제거·PATH 정리·기존 자료 보존 | NOT_RUN | Before/AfterRemove + 사용자 관찰 |
| 새 Codex/Claude 세션 업무 앱 실증 | NOT_RUN | 별도 새 세션에서 실제 작업/검증/결과 기록 |
| Windows 콘솔 Ctrl+C / Docker Desktop E2E | NOT_RUN | 별도 실제 환경 검증 |

각 수행을 아래 형식으로 기존 기록 뒤에 추가한다. 실패나 미실행을 나중 성공 기록으로 덮어쓰지 않는다. `Prepare PASS`나 사전·후검사 `PASS`만으로 GUI 조작·새 AI 세션 인수를 통과 처리하지 않는다.

```text
실행 UTC / 로컬 시간:
절차·스크립트 checkout commit (git rev-parse HEAD; 원격 게시 source와 구분):
공개 CLI source / installer source / CLI version / bundle:
OS edition/build / CPU / PowerShell / 일반 사용자 여부:
run ID / step ID / 단계 / baseline step:
명령: checker Phase=... (프로젝트·사용자 경로는 <LOCAL_PATH>로 가림)
종료코드:
결과: PASS | FAIL | NOT_RUN
GUI 사건: 독립 실행 | same GUI event | 실행 안 함
버튼 상태 / 버전 결과 / PATH 확인 (원문 경로 없이):
context 보존 / credential 파일 보존 / vault 사용성 / project 보존:
증거: 검토한 candidate.json/result.json 상대 파일명, 필요 시 가린 화면
실패 코드·재현 단계 또는 NOT_RUN 이유:
```

이번 작업에서는 Windows GUI·네이티브 사전/후검사·새 Windows AI 세션을 실행하지 않았다. 실제 사용자 검증 후 이 문서의 상태를 별도 실행 기록으로 갱신한다.
