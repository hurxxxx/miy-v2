# 게시 CLI Windows 11 인수 (현재 후보)

- 작성일: 2026-11-03
- 대상: [후보 파일](releases/windows-candidate.json)에 고정한 현재 게시 후보. 지금은 CLI **0.7.0** / SDK 번들 **0.8.0**, source `01e4d4924f8c8fc4e11720c1af81adf17ddb20bf`
- 상태: **절차·검사기 준비 완료. 사용자 Windows 11 실행은 `NOT_RUN`.** 사용자가 실행한 뒤 결과를 §결과에 추가한다.

이전 후보의 절차와 기록은 [0.6.3 인수](WINDOWS-0.6.3-ACCEPTANCE.md)에 보존한다. Windows Server CI 통과를 Windows 11 통과로 옮기지 않는다. 설치 파일의 동작 설명은 [설치기 설명](WINDOWS-INSTALLER.md)을 따른다.

이 절차는 **현재 게시된 0.7.0**을 대상으로 한다. 후속 [내부 업무 앱 최소안](APP-BOUNDARIES.md#확정-결정)은 iframe 없는 앱 통합·게시 CLI/SDK 계약을 유지하고 단일 IP의 포털·신뢰 콘솔 두 HTTPS origin을 사용한다. Keycloak과 CLI 승인은 콘솔에 두며 도메인·내부 DNS·추가 IP는 필수가 아니다. Windows 최소 인수는 준비한 인증서의 IP SAN/체인 확인, 브라우저·CLI의 CA 신뢰 설정, 기존 CLI의 상대 승인 경로 이동·로그인·갱신·철회다. 자동 CA 배포·자동 갱신은 후속이며 [전환·복구](APP-BOUNDARIES.md#전환과-복구)와 사용자 수행 인수는 `NOT_RUN`이다. 기존 HTTP IP 결과를 새 HTTPS 보안 계약의 통과로 쓰지 않고 게시 설치기도 변경하지 않는다.

## 두 HTTPS 출처의 Windows 확인 항목

새 경계 서버가 별도로 준비됐을 때 수행한다. 아래 주소와 CA 경로는 운영 담당자가 확인한 값으로 바꾸며 현재 게시 서버가 전환됐다고 가정하지 않는다. 회사 CA의 공개 인증서만 사용하고 서버 개인 키를 PC로 복사하지 않는다.

```powershell
# 조직에서 확인한 CA 인증서를 현재 사용자의 신뢰 저장소에 등록
Import-Certificate -FilePath C:\miy-certs\company-ca.cer -CertStoreLocation Cert:\CurrentUser\Root
miy context add internal --url https://192.168.10.20:8443 --ca-file C:\miy-certs\company-ca.pem
miy --context internal auth login
```

- 브라우저에서 포털 :8443과 콘솔 :9443 모두 인증서 경고 없이 열린다. DNS 이름 대신 실제 IP가 SAN에 있어야 한다. 브라우저를 재시작해 CA 신뢰를 적용하며 경고 무시·`--insecure`를 사용하지 않는다.
- 기존 CLI가 안내한 포털의 `/cli/device/{id}`에서 설정된 콘솔로 이동한다. 0.5 이전 CLI의 `/cli/authorize/{id}`는 콘솔의 업그레이드 안내만 표시한다. Keycloak 로그인과 승인은 콘솔에서 처리되고 포털/콘솔은 각각 세션을 만든다.
- 명시적 use가 없는 prod 배포자는 콘솔에서 관리할 수 있지만 포털에서 그 앱에 입장하지 못한다. dev develop 미리보기와 기존 SDK 앱의 직접 링크/reload는 동작한다.
- 콘솔 개인 CLI 화면에서 device와 legacy 연결을 확인하고 자기 연결만 회수한다. 브라우저 logout으로 독립 CLI가 자동 회수됐다고 판단하지 않는다.
- 새 CA·인증서로 수동 교체한 뒤 브라우저와 `miy --context internal auth status`를 다시 확인한다. 만료·다른 IP 인증서는 거부돼야 한다. 원래 CA/인증서로 복구한 결과도 기록한다.

이 항목은 사용자 Windows 실행 `NOT_RUN`이다. Linux의 격리 TLS·브라우저 결과를 Windows 통과로 옮기지 않는다.

## 후보와 출처

[검사기](../scripts/check-windows-published-installer.ps1)는 후보 파일의 값을 기대값으로 쓴다. 공개 API·`.sha256`·실제 파일이 서로 맞는지에 더해, 게시 파일이 **CI가 만든 파일과 같은지**를 서버와 무관하게 대조한다.

| 항목 | 후보 파일 값 | 출처 |
| --- | --- | --- |
| 설치 파일 | `MiySetup-0.7.0-windows-x64.exe`, 6,209,536 bytes, SHA-256 `8e01d1f65f4dbf1e3a69f9a6ae5ffefa9c82b9b8f2966274d3fc4a6e79a9612f` | Windows 설치 파일 CI run `41614954095` |
| CLI ZIP | `miy-0.7.0-x86_64-pc-windows-msvc.zip`, SHA-256 `cd082c473d70704b2f7bc89af82b8d555b64a137c6c59baf50a1ebe35a4403f3` | CLI 후보 CI run `41613023015` |
| ZIP 안 `miy.exe` | SHA-256 `148683289767eb578200de0f79664e48cca9b387956bd99c4774bd3093f261a8` | 같은 run |
| 기록 | [0.7.0 배포 기록](releases/core-0.7.0-deployment.json) | 2026-11-03 운영 반영 |

2026-11-03 Linux에서 공개 API를 읽기만 해서 위 값과 같음을 확인했다. 검사기는 실행할 때마다 API를 다시 읽고 다음을 모두 확인한다. API가 다른 후보로 바뀌었거나 후보 파일과 다르면 중단한다.
- 버전과 source commit
- EXE와 ZIP의 관계, 각 `.sha256`
- 실제 파일 해시와 CI 고정 해시
- ZIP 안 `release.json`과 내장 CLI 해시
- EXE 안의 provenance, 동일 ZIP 포함 여부

해시와 provenance는 Authenticode 서명이나 독립 서명 attestation을 대신하지 않는다. 현재 후보는 서명되지 않아 게시자 경고가 날 수 있으며, 보안 정책을 일괄 해제하지 않는다.

**새 릴리스로 바꿀 때:**
1. 배포 기록의 CI 해시로 `docs/releases/windows-candidate.json`을 갱신한다.
2. Linux의 `pwsh`로 문법 검사를 하고, 공개 API와 대조한다.
3. 이 문서의 후보 표를 갱신한다.

## 준비와 범위

- **세션:** Windows 11 x64의 **관리자가 아닌 사용자 세션**에서 실행한다. 관리자 PowerShell, Windows Server, ARM64는 검사기가 거부한다.
- **TLS:** 회사 TLS 신뢰가 정상인 환경에서 HTTPS로 접근하고, 인증서 검사를 끄지 않는다.
- **PowerShell:** Windows PowerShell 5.1(`powershell.exe`)과 PowerShell 7 x64(`pwsh.exe`)를 각각 준비한다. 검사기는 PowerShell 설치·PATH·실행 정책·프로필·Git 전역 설정을 바꾸지 않는다. 실행 정책에 막히면 조직 정책에 맞는 별도 프로세스 실행을 쓰고, 전역 `Set-ExecutionPolicy`로 우회하지 않는다.
- **checkout 위치:** 저장소는 로컬 디스크에 checkout한다. 한글·공백 경로는 괜찮다. 심볼릭 링크·junction·UNC 경로는 쓰지 않는다. 검사기는 checkout 안의 후보 파일을 읽으므로 이 문서와 같은 commit을 쓴다.
- **설치 환경:**
  - 새 설치는 기존 MIY 설치가 없는 별도 일반 사용자 프로필이나 시험 PC에서 한다.
  - 갱신은 이전 버전이 이미 설치된 환경에서 한다. 같은 버전 재실행은 중복 설치 방지 확인일 뿐 갱신 통과가 아니다.
  - 이전 버전 환경이 없으면 갱신은 `NOT_RUN`으로 둔다. 검사하려고 현재 설치를 자동 downgrade하지 않는다.

검사기가 하는 일은 다음뿐이다.
- 공개 EXE/ZIP을 내려받아 출처를 검증한다. 내려받은 코드·GUI·CLI는 실행하지 않는다.
- OS·CPU·PowerShell 환경, 단계, 종료코드, 파일 해시, 보존 비교 결과를 기록한다.
- 기존 config의 context 항목, 암호화 credential 파일과 marker, 선택한 프로젝트의 전후 지문을 비교한다. 기존 context는 보존돼야 하고, 설치 때 새 context·활성 선택이 바뀌는 것은 허용한다. 제거 때는 config 바이트도 같아야 한다.

**보존 비교의 한계:**
- Windows Credential Manager의 값은 읽지 않는다. credential 파일 지문이 같다고 vault 인증정보를 쓸 수 있다는 뜻은 아니므로, 기존 로그인 사용성은 사용자가 따로 확인한다.
- context·credential 파일이 없거나 프로젝트를 지정하지 않았으면 해당 보존 항목은 `NOT_RUN`이다.
- 프로젝트의 `.git`, `node_modules`, `target`, `dist`, `.local`은 비교에서 뺀다. 나머지 파일이 10,000개를 넘거나 파일 하나가 512MiB를 넘으면 중단한다.
- 이 비교를 디스크 전체·앱 DB·원격 데이터의 보존 증거로 넓히지 않는다.

## 공개 파일 준비: 두 PowerShell에서 각각 실행

저장소 루트에서 실행한다. PowerShell 5.1에서는 네이티브 프로그램 실패가 예외가 되지 않으므로 **호출 직후마다** `$LASTEXITCODE`를 확인한다.

```powershell
$checker = '.\scripts\check-windows-published-installer.ps1'
$shell = (Get-Process -Id $PID).Path
$PSVersionTable.PSVersion.ToString()
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase Prepare
if ($LASTEXITCODE -ne 0) { throw 'Prepare failed; do not run the downloaded installer.' }
```

성공 출력의 `run=run-...` 값을 `$run`에 넣는다. 실패한 Prepare 폴더는 재사용하지 않고 다시 Prepare한다.

```powershell
$run = 'run-YYYYMMDDTHHMMSSZ-32자리GUID' # 실제 성공 출력으로 교체
$evidenceRoot = Join-Path '.local\windows-published-acceptance' $run
$candidate = [IO.File]::ReadAllText((Join-Path $evidenceRoot 'candidate.json')) | ConvertFrom-Json
$installer = Join-Path $evidenceRoot ('downloads\' + $candidate.installerFile)
```

**증거 폴더:**
- 위치는 `.gitignore`에 포함된 `.local/windows-published-acceptance/<run>/`이다.
- 새 실행 폴더에는 데이터를 쓰기 전에 현재 사용자만 허용하는 보호 ACL을 설정한다.
- 각 단계는 `step-...` 하위 폴더의 `result.json` 또는 `failure.json`에 기록한다.
- 환경·ACL 선행 검사에서 멈추면 고정된 실패 단계와 종료코드만 출력하므로, 이를 수동 결과표에 적는다.

**`candidate.json`:**
- 이번 다운로드의 출처이며, 후보 파일과 CI run 번호를 함께 기록한다.
- 도구는 내장 번들을 실행하지 않으므로, `expectedBundleVersion`에는 `bundleVersionVerification=NOT_RUN`이 함께 기록된다.
- `public-api.json`에 있는 설치 명령 문자열은 실행하지 않는다.

**기록하지 않는 것:**
- context·토큰·쿠키·SID·사용자 이름·프로필 경로·프로젝트 경로의 원문은 저장하지 않는다. 보존 비교는 실행마다 임의 키로 만든 HMAC 지문을 쓴다.
- 공유할 때는 검토한 `candidate.json`, `result.json`, 수동 결과표만 복사한다. 전체 폴더·PowerShell history·transcript·인증 응답은 올리지 않는다.

## GUI 새 설치

새 설치가 가능한 환경에서만 한다.
- **보존할 프로젝트:** 보존할 작은 프로젝트가 있으면 선택하고, 없으면 `-ProjectPath`를 빼서 그 항목을 `NOT_RUN`으로 둔다.
- **검사 사이:** 프로젝트 편집, CLI 로그인·refresh, MCP 작업을 하지 않는다.
- **`MIY_CONFIG_DIR`:** 이 변수를 쓰는 환경이면 GUI와 검사기가 같은 설정 루트를 쓰는지 먼저 확인한다. 이를 맞추려고 전역 환경변수나 기존 설정 위치를 바꾸지 않는다.

```powershell
$project = 'D:\한글 작업\보존할 프로젝트' # 로컬에서만 사용; 결과표에 원문을 적지 않는다
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeInstall -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeInstall failed.' }
$beforeInstall = 'step-실제성공출력'
```

사용자가 탐색기에서 `$installer`에 해당하는 **검증된 파일을 직접 열고** 다음을 확인한다. 검사기는 GUI를 열거나 버튼을 누르지 않는다.

1. 설치 전 서버 주소 입력란, 연결 확인, 설치, CLI 제거 상태를 관찰한다.
2. `https://v2.dwdcc.kr`를 입력하고 연결 확인 후 **설치**를 누른다. 관리자 승격 없이 `%LOCALAPPDATA%\Programs\miy\bin`에 설치되는지 확인한다.
3. 완료 화면을 확인한다. 같은 버전의 설치 버튼이 비활성화되고 CLI 제거가 활성화되는지 보고, 창을 다시 열어도 같은지 기록한다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterInstall -RunId $run -BaselineStep $beforeInstall -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterInstall failed; preserve the evidence and inspect before retrying.' }
```

새 PowerShell 창에서 `miy --version`이 후보 버전(`miy 0.7.0`)인지, 새 사용자 PATH로 실행되는지 확인하고 종료코드만 기록한다. 원본 PATH는 저장하지 않는다.

## GUI 갱신과 기존 연결·인증·프로젝트 보존

이전 버전이 설치된 환경에서 Prepare부터 별도 run으로 수행한다.
- `AfterUpdate`는 이전 바이너리 해시와 후보 해시가 달라야 통과한다.
- 기존 MIY 창·MCP·CLI 작업은 정상 종료하고, 강제 종료하지 않는다.
- 갱신 전에 사용자가 `miy auth status`를 실행하고 성공 여부·종료코드만 기록한다. 로그인이나 refresh가 필요했다면 그 뒤에 BeforeUpdate를 찍는다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeUpdate -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeUpdate failed.' }
$beforeUpdate = 'step-실제성공출력'
# 사용자가 검증된 EXE를 열고 기존 서버 표시를 확인한 뒤 GUI에서 갱신한다.
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterUpdate -RunId $run -BaselineStep $beforeUpdate -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterUpdate failed; do not erase the old state or journal.' }
```

갱신 후 확인:
- 새 셸에서 `miy --version`, `miy auth status`를 실행하고 각각 `$LASTEXITCODE`를 확인한다.
- 기존 context가 남았는지, 기존 인증을 재로그인 없이 쓸 수 있는지 따로 기록한다. 파일 보존 성공과 인증 사용성 실패는 합치지 않는다.
- 기존 앱은 `miy app upgrade`로 번들 0.8.0으로 전환할 수 있는지 별도로 확인한다.

파일 잠금, 중단·복구, 변조 거부 같은 파괴적 실패를 실제 사용자 설치에 주입하지 않는다.

## GUI 제거

해당 시험 설치를 제거하기로 한 경우에만 진행한다. 업무용 기존 설치를 자동으로 제거하거나 덮어쓰지 않는다.

```powershell
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase BeforeRemove -RunId $run -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'BeforeRemove failed.' }
$beforeRemove = 'step-실제성공출력'
# 사용자가 같은 GUI의 CLI 제거를 직접 누르고 완료를 확인한다.
& $shell -NoLogo -NoProfile -NonInteractive -File $checker -Phase AfterRemove -RunId $run -BaselineStep $beforeRemove -ProjectPath $project
if ($LASTEXITCODE -ne 0) { throw 'AfterRemove failed; retain evidence.' }
```

새 셸에서 GUI가 관리하던 PATH 항목이 정리됐는지 확인하되, 원본 PATH는 기록하지 않는다. 검사기는 GUI 기본 경로의 `miy.exe` 부재와 기존 config·credential 파일·선택 프로젝트의 보존을 검사한다.

## 결과

두 셸의 실행은 별도 run으로 기록한다. GUI 조작은 눈으로 본 결과와 도구 결과를 구분하고, 같은 GUI 사건을 두 셸에서 중복 집계하지 않는다.

| 항목 | 현재 상태 | 근거 |
|---|---|---|
| 공개 API·CI 고정 해시 대조 | PASS (2026-11-03, Linux에서 읽기만, PowerShell 7.6.9로 검사기 조건식 평가, 변조 해시 거부 확인) | 이 문서의 후보 표 |
| 검사기 PowerShell 7 문법 검사 | PASS (Linux portable PowerShell 7.6.9 Parser) | 네이티브 Windows 실행과 별도 |
| Windows 11 PS5.1 다운로드·출처 검사 | NOT_RUN | run/step, PS·OS build, exit code |
| Windows 11 PS7 다운로드·출처 검사 | NOT_RUN | 별도 run/step |
| 새 GUI 설치·같은 버전 재실행 | NOT_RUN | Before/AfterInstall, 버튼 상태, 새 셸 버전 |
| 이전 버전(0.6.3)에서 GUI 갱신 | NOT_RUN | Before/AfterUpdate |
| 기존 context·인증 사용성·프로젝트 보존 | NOT_RUN | 지문 비교와 사용자 auth 결과, vault 미검사 구분 |
| GUI 제거·PATH 정리·기존 자료 보존 | NOT_RUN | Before/AfterRemove와 사용자 관찰 |
| 원격 앱 DB 보존·사내 HTTP IP 접속 | NOT_RUN | 해당 환경에서 별도 확인 |

각 실행을 아래 형식으로 기존 기록 뒤에 추가한다. 실패나 미실행 기록을 나중 성공으로 덮어쓰지 않는다.

```text
실행 UTC / 로컬 시간:
절차·검사기 checkout commit (git rev-parse HEAD):
후보 파일 version / source / installer SHA-256:
OS edition/build / CPU / PowerShell / 일반 사용자 여부:
run ID / step ID / 단계 / baseline step:
종료코드 / 결과: PASS | FAIL | NOT_RUN
GUI 사건: 독립 실행 | same GUI event | 실행 안 함
버튼 상태 / 버전 결과 / PATH 확인 (원문 경로 없이):
context 보존 / credential 파일 보존 / vault 사용성 / project 보존:
증거: 검토한 candidate.json/result.json 상대 파일명
실패 코드·재현 단계 또는 NOT_RUN 이유:
```
