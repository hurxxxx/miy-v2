# CLI 배포 후보와 Windows 설치기

이 문서는 배포 도구의 사용법과 현재 후보를 기록한다. 후보 게시와 정식 GitHub Release 발행은 구분한다. 실제 수행 범위는 [검증 행렬](CLI-VALIDATION-MATRIX.md), 아직 남은 사용자 PC 실행은 [Windows 검증](WINDOWS-VALIDATION.md)을 따른다.

## 현재 0.6 검증 후보

CLI 소스 `b67798bfb84f299257ba4326beaf034b125cde7a`의 [기반 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40602323702)와 [후보 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949)가 모두 통과했다. 다섯 네이티브 CLI와 두 Linux helper를 검증해 [현재 서버 설치 화면](https://v2.dwdcc.kr/cli/install)에 게시했다. [전체 후보 artifact](https://github.com/hurxxxx/miy-v2/actions/runs/40602334949/artifacts/12779751510)와 [배포물별 SHA-256·소스 index](releases/cli-0.6.0-candidate.json)를 보존한다. 실제 공개 경로의 일곱 파일 해시와 홈페이지 명령·다운로드 링크도 확인했다.

Windows Server 2022에서 네이티브 단위 95개, 출력 11그룹, 서버 등록 9그룹, PowerShell 5.1.20348.5756과 7.6.8의 설치 검사 각각 9개가 통과했다. 사용자 Windows 11·Docker Desktop·독립 AI 세션 인수는 별도다. `publish=false`로 실행했으므로 정식 GitHub Release나 공개 tag를 생성하지 않았다. 시작 절차는 [핵심 흐름 테스트](CORE-TESTING.md)를 따른다.

핵심 흐름을 검증한 서버 소스는 이미지 식별자 호환성과 PostgreSQL 네트워크 연결 보존을 수정한 `5ca4047`이며 [서버 CI](https://github.com/hurxxxx/miy-v2/actions/runs/40606859082)가 통과했다. 게시된 CLI `b67798b`와 함께 실제 Rust/React 앱의 로컬 개발 DB CRUD·dev 데이터 유지·별도 prod DB 승격과 CRUD를 검증했다. [두 소스와 실제 수행 범위](releases/cli-0.6.0-core-validation.json)를 별도로 기록하며 CLI 후보 파일을 서버 소스 commit으로 다시 표시하지 않는다.

Windows GUI 설치 파일과 설치 안내 API는 이후 `92fc608` / 서버 `2f7d453`으로 추가했다. [네이티브 설치 검사와 다운로드 증거](WINDOWS-INSTALLER.md#현재-설치-후보)를 별도로 기록한다. 기존 CLI·DB·앱 런타임의 검증 범위를 새 설치기 검증으로 대체하지 않는다.

## 이전 0.5 검증 후보

2026-10-27 소스 `0fe94a4b9d851e7cf71dc9c8e91fb469423134e2`의 [CI 40460582868](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868)가 5개 native build와 aggregate 검사를 모두 통과했다. [전체 후보 artifact](https://github.com/hurxxxx/miy-v2/actions/runs/40460582868/artifacts/12715067551)를 내려받아 Actions의 바깥 ZIP을 풀면 OS/CPU별 아카이브·`.sha256`·`SHA256SUMS`·provenance를 얻는다. [배포물별 SHA-256과 소스 index](releases/cli-0.5.0-candidate.json)도 저장소에 보존한다. 이 index는 공개 승인 증거 JSON이 아니다.

Windows 사용자 검증에는 [Windows 전용 후보 및 실행 안내](WINDOWS-VALIDATION.md)를 사용한다. `publish=false`로 실행했으므로 GitHub Release와 공개 tag를 만들지 않았다. Actions artifact는 보존 기간이 있는 검증용 파일이다.

## 배포 후보 생성

[CLI release candidate workflow](../.github/workflows/cli-release.yml)는 수동 실행한다. `candidate_commit`에는 전체 40자리 소스 commit, `release_tag`에는 해당 Cargo 버전 앞에 `v`를 붙인 값을 입력한다. `publish=false`가 기본값이다. 버전이 다른 tag, 변경된 checkout, 다른 CPU의 바이너리, 누락된 배포물은 거부한다.

| 배포물 | 네이티브 runner |
| --- | --- |
| Linux x64 / ARM64 | `ubuntu-24.04` / `ubuntu-24.04-arm` |
| macOS x64 / ARM64 | `macos-15-intel` / `macos-15` |
| Windows x64 MSVC | `windows-2022` |

runner 명칭과 CPU 구분은 [GitHub 공식 목록](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)을 따른다. Windows Server runner의 검사와 실제 Windows 11/Docker Desktop 인수를 구분한다.

각 runner는 Rust 1.99.1로 fmt/check/Clippy/unit/build, 실제 실행 파일의 출력 계약·번들/Git 왕복을 검사한다. Windows runner는 일반 설치·업데이트, 체크섬 거부, 사용 중 파일 거부, 변조된 backup 거부, 중단 후 복구, 제거 시 사용자 파일 보존을 PowerShell 5.1/7에서 검사한다. 다섯 번들의 hash와 source commit을 비교한 뒤 후보 artifact를 만든다.

아카이브 이름은 `miy-VERSION-TARGET.tar.gz` 또는 `miy-VERSION-x86_64-pc-windows-msvc.zip`이다. 각각의 `.sha256`, 전체 `SHA256SUMS`, `release-index.json`과 target별 provenance를 함께 보존한다. 아카이브 내부 `release.json`에는 CLI 버전·target·source commit·바이너리 SHA-256이 있다. 이 metadata는 코드 서명이나 서명된 attestation과 동일하지 않다.

로컬의 깨끗한 checkout에서도 이미 빌드한 **해당 OS/CPU의 실행 파일**을 포장할 수 있다.

```sh
python3 scripts/package-cli-release.py package --binary target/x86_64-unknown-linux-gnu/release/miy --target x86_64-unknown-linux-gnu --output .local/release-assets
```

다섯 target의 결과를 같은 디렉터리에 모은 뒤 `python3 scripts/package-cli-release.py verify --directory DIR`로 검증한다. 하나의 OS에서 다른 target 바이너리를 실제 실행한 것으로 기록하지 않는다.

## 서버의 다운로드 게시

CLI 0.6 후보에는 다섯 네이티브 CLI와 Linux amd64/arm64 DB 연결 helper가 함께 있어야 한다. Helper의 archive SHA-256, config SHA-256, 플랫폼, 소스 commit과 태그를 검증하며 개발 작업 트리용 helper는 게시하지 않는다. Docker 저장 방식에 따라 달라지는 image ID는 검증된 config 내용과 대조한 후 해당 컴퓨터의 불변 ID로 실행한다.

검증한 aggregate 디렉터리를 서버의 `MIY_CLI_RELEASE_DIR`에 게시한다. 다음 명령의 commit은 CLI 후보 소스이며, 이후 서버 전용 수정이나 증거 문서 commit과 혼동하지 않는다.

```sh
python3 scripts/stage-cli-release.py --source /path/to/aggregate --destination /path/to/cli-downloads --commit CLI_CANDIDATE_40_HEX_COMMIT
python3 scripts/check-cli-downloads.py --origin https://PLATFORM --expected-commit CLI_CANDIDATE_40_HEX_COMMIT
```

게시 도구는 모든 파일을 다시 검증한 뒤 commit별 불변 디렉터리를 만들고 최상위 index를 원자적으로 교체한다. 같은 commit의 파일을 다른 내용으로 덮어쓰지 않는다. 설치 안내는 `/cli/downloads/COMMIT/FILE`을 사용하며 Windows 명령은 다운로드·해시 대조·압축 해제·명시적 서버 등록을 포함한다. 이 테스트용 서버 게시와 검증 완료 후의 정식 GitHub Release 공개는 별개다.

## Unix 설치

자신의 OS/CPU tar.gz와 `.sha256`을 받고 SHA-256을 비교한 뒤 새 디렉터리에 푼다. 그 디렉터리에서 `bash install-cli.sh --prefix "$HOME/.local/bin"`을 실행한다. 동봉된 `release.json`과 `miy`가 있으면 Rust 빌드 없이 바이너리를 설치한다. 명시적인 다른 파일은 `--from /absolute/path/to/miy`로 지정한다. 소스 checkout에서 실행하는 경우에만 `--no-default-features` 클라이언트를 빌드한다. shell profile/PATH는 자동 변경하지 않는다.

## 서버 주소가 포함된 설치 안내

이번 변경을 포함한 새 배포물은 플랫폼의 `/cli/install` 안내에서 서버 주소가 채워진 명령을 복사할 수 있다. Unix에서는 검증한 배포물을 푼 디렉터리에서 다음 형태로 실행한다.

```sh
bash ./install-cli.sh --server https://PLATFORM
miy auth login
```

서버는 `MIY_ORIGIN`으로 설치 안내의 주소를 결정한다. 실행 파일이나 설치기에 특정 도메인을 내장하지 않으며, 브라우저 Host로 주소를 추측하지 않는다. 서버 인수는 설치된 CLI의 `context init --url`로 전달된다. 사용자 설정이 없으면 `default` 연결을 만들고 선택하며, 기존 설정이 있으면 바이트를 그대로 보존한다. 초기화 실패는 비정상 종료와 함께 실행 파일은 이미 설치되었음을 알린다. 서버 인수를 생략한 대화형 설치는 URL을 입력받고, 비대화형 설치는 실행 파일만 설치한 뒤 `miy context init`을 안내한다. 위에 기록된 `0fe94a4` 후보에는 새 옵션이 포함되지 않으므로 새 설치기와 새 CLI를 함께 배포해야 한다.

Windows의 `install-cli.ps1`과 `update-cli.ps1`에는 `-Server https://PLATFORM`을 추가할 수 있다. 아래처럼 `$Archive`와 `$ExpectedSha256`을 준비한 다음, 플랫폼 안내에 나온 주소가 포함된 명령을 실행한다. 기존 context·활성 연결·사용자·인증서·자격증명은 보존한다.

## Windows 설치·갱신·제거

일반 사용자는 [Windows GUI 설치 파일](WINDOWS-INSTALLER.md)을 사용한다. 회사 홈페이지에서 내려받아 서버 주소 입력·연결 확인·설치를 진행한 뒤 새 터미널에서 `miy auth login`을 실행한다. 다음 ZIP·PowerShell 방식은 수동 설치와 진단을 위한 보조 경로다.

ZIP을 신뢰한 배포 위치에서 받고 게시된 SHA-256과 대조한 다음 별도 디렉터리에 풀어 설치 스크립트를 실행한다. ZIP 내부에는 `miy.exe`, `release.json`, 설치·갱신·제거 스크립트와 공통 라이브러리가 있다. 설치 스크립트 자체도 검증한 ZIP 또는 확인한 소스 checkout에서 사용한다.

일반 사용자 계정의 Windows x64 PowerShell 5.1/7에서 다음과 같이 실행한다. 기본 설치 위치는 `%LOCALAPPDATA%\Programs\miy\bin`이다. 예제의 SHA-256은 별도로 확인한 게시 값으로 대체한다.

```powershell
$ExpectedSha256 = 'REPLACE_WITH_PUBLISHED_SHA256'
$Archive = (Resolve-Path '.\miy-0.6.0-x86_64-pc-windows-msvc.zip').Path
if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'ZIP checksum mismatch' }
Expand-Archive -LiteralPath $Archive -DestinationPath '.\miy-release-0.6.0'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File '.\miy-release-0.6.0\install-cli.ps1' -ArchivePath $Archive -Sha256 $ExpectedSha256
if ($LASTEXITCODE -ne 0) { throw "Install failed: $LASTEXITCODE" }
& "$env:LOCALAPPDATA\Programs\miy\bin\miy.exe" --version
if ($LASTEXITCODE -ne 0) { throw "CLI failed: $LASTEXITCODE" }
```

`-ExecutionPolicy Bypass`는 위 자식 PowerShell 프로세스에만 적용된다. 설치기는 시스템/사용자 실행 정책이나 profile을 변경하지 않는다. PowerShell 7에서는 `powershell.exe` 대신 `pwsh`를 사용할 수 있다. 조직의 적용 정책으로 스크립트 실행이 제한된 환경은 그 정책에 맞는 실행 방식을 사용한다.

설치기에 `-AddToPath`를 명시한 경우에만 사용자 PATH에 설치 경로를 추가한다. 현재 터미널의 PATH는 자동으로 바꾸지 않는다. 다른 드라이브나 한글/공백이 있는 경로는 `-InstallDir 'D:\도구 모음\miy'`로 지정한다. 기존 사용자 파일이 있는 미관리 디렉터리, junction/symlink를 통과하는 경로, 다른 사용자가 수정할 수 있는 설치 디렉터리는 거부한다.

업데이트는 검증한 새 배포물의 외부 스크립트로 실행한다.

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File '.\miy-release-0.6.0\update-cli.ps1' -ArchivePath $Archive -Sha256 $ExpectedSha256
if ($LASTEXITCODE -ne 0) { throw "Update failed: $LASTEXITCODE" }
pwsh -NoProfile -ExecutionPolicy Bypass -File '.\miy-release-0.6.0\uninstall-cli.ps1'
if ($LASTEXITCODE -ne 0) { throw "Uninstall failed: $LASTEXITCODE" }
```

사용자 PATH 항목도 제거하려면 제거 명령에 `-RemoveFromPath`를 명시한다. 사용자 지정 설치 경로는 갱신·제거에서도 같은 `-InstallDir`를 전달한다. `-Version 0.5.0`을 사용하면 신뢰한 기존 설치기가 명시 버전의 ZIP과 체크섬을 GitHub Releases에서 받아 검증한다. `latest`를 자동 선택하지 않는다. 공개 릴리스가 아직 없으면 이 원격 경로는 사용할 수 없다.

설치기는 SHA-256, ZIP 경로, PE x64 header, 내부 binary hash, 실제 `--version`을 먼저 검사한다. 실행 중인 CLI/MCP 때문에 파일을 교체할 수 없으면 종료 코드 6으로 거부한다. 프로세스를 강제로 종료하거나 실행 중인 CLI가 자기 파일을 덮어쓰게 하지 않는다. 실행 파일 교체와 제거는 설치 디렉터리의 소유한 `miy.exe`와 metadata를 대상으로 한다. `-Server`를 전달한 설치는 사용자 설정이 없을 때만 기본 연결을 추가하며, 기존 설정·인증·프로젝트·앱 데이터와 사용자 파일은 보존한다. 이전 버전 backup도 남긴다.

중단 기록은 `.miy-install-journal.json`, backup은 `.miy-backups/ID/`에 있다. 다음 외부 설치/갱신/제거가 기록을 확인해 회복한다. 다른 프로세스의 파일 잠금이나 사용자 수정·backup 변조로 복구가 불가능하면 journal과 backup을 유지하고 오류를 반환한다. 그 경우 설치 디렉터리를 지우지 말고 원인을 해결한 뒤 재실행한다. PowerShell 5.1은 native 명령의 실패만으로 자동 중단하지 않으므로 각 명령 직후 `$LASTEXITCODE`를 검사한다.

## 공개 릴리스 게이트

기본 workflow는 후보 artifact만 만든다. 공개하려면 [증거 예제](releases/cli-evidence.example.json)를 복사하여 실제 수행 결과를 채우고 커밋한다. `candidateCommit`은 검증한 코드 commit이며, 증거만 추가한 후속 문서 commit과 구분한다. workflow의 `candidate_commit`도 같은 코드 commit을 지정한다.

`publish=true`는 다음 조건이 모두 충족된 경우에만 release 생성/공개 단계로 진행한다.

- 다섯 native artifact와 출력·번들 검사 통과.
- 같은 후보 코드의 Linux runtime/browser/별도 원격 서버, 실제 OIDC 로그인 및 코어의 사용자 비활성화에 따른 발급·갱신·API·대기 작업 차단 실증. 비활성화 기준은 코어 상태이며 Keycloak 비활성화의 자동 전파를 지원한다고 해석하지 않는다.
- 사용자 실제 Windows 11 PowerShell 5.1/7 및 Docker Desktop 실행 결과.
- Codex/Claude Code 각각 Windows/Linux/macOS의 독립 새 세션 3회와 별도 인수. 호출 수·읽은 bytes·수동 개입·복구 시간, 의도하지 않은 변경/권한 우회/중복 작업 0건.

`NOT_RUN`, `FAIL`, commit 불일치, 재사용 AI 세션, 빠진 증거는 공개를 막는다. 증거 검사기는 기록의 완전성·동일 후보·결과를 검사하며 실제 사람이 수행한 Windows/AI 결과의 진위를 자동 입증하지 않는다. 검토자는 연결된 실행 증거를 확인해야 한다. 토큰·cookie·verifier·비밀번호·DB 접속 URL이나 원본 환경 dump를 기록하지 않는다.

게이트 통과 후에도 먼저 draft에 모든 파일을 올리고 마지막에 공개한다. 이미 공개된 release를 덮어쓰지 않는다. 업로드 중 실패한 draft는 같은 source commit으로 재시도할 수 있다. 실제 Windows와 AI 결과가 아직 없는 현재 상태에서는 후보 빌드까지 진행하고 공개 게이트는 미통과로 유지한다.
