# CLI 배포 후보와 Windows 설치기

CLI·Windows 설치기의 후보 빌드, 서버 다운로드 게시, 정식 GitHub Release 조건을 관리한다. 사용자 설치·개발은 [RUNNING](RUNNING.md), Windows GUI·수동 ZIP 설치와 실제 사용자 인수는 [WINDOWS-ACCEPTANCE](WINDOWS-ACCEPTANCE.md)를 따른다. 검사 종류는 [CHECKS](CHECKS.md), 버전별 과거 결과는 [CLI 검증 이력](archive/results/CLI-VALIDATION-MATRIX.md)에 모은다.

서버의 현재 [실행·인증 계약](APP-BOUNDARIES.md#확정-결정)은 prod 포털·dev 포털·관리/로그인 콘솔의 세 출처다. device CLI 승인은 콘솔에서만 처리하고 legacy 시작·승인·교환은 410 `cli_upgrade_required`로 종료했다. 게시 CLI 0.9.0의 실제 연결 인수·현재 배포는 [경계 인수](APP-BOUNDARIES.md#경계-인수)와 [운영 안내](SERVER-OPERATIONS.md)를 따른다. 개발 단계의 구 형식 호환은 강제하지 않지만 게시 파일·해시는 불변이다. 후보 생성·서버 다운로드 게시·정식 공개는 서로 다른 단계다.

## 현재 0.9.0 게시

`173c6d5cfd35f9d1313fe643701d4af3f374377f`의 [CLI CI 43394975993](https://github.com/hurxxxx/miy-v2/actions/runs/43394975993)(5개 target과 Linux DB 연결 helper 2개, 공개 Release 없음)와 같은 CLI 출처·해시로 만든 [설치 파일 CI 43397724560](https://github.com/hurxxxx/miy-v2/actions/runs/43397724560)을 검증했다. 2026-11-13 서버 0.9.0과 함께 [설치 안내](https://v2.dwdcc.kr/cli/install)에 게시했다([0.9.0 배포 기록](releases/cli-0.9.0-2026-11-13-deployment.json)). SDK 번들도 0.9.0이며 앱 프레임 SDK(`setNavigationState`·`apiBlob`)와 구 번들 업그레이드를 포함한다. 기존 0.8.0 앱은 `miy app upgrade`로 옮긴다. Windows 11 사용자 인수는 [현재 후보 절차](WINDOWS-ACCEPTANCE.md)를 쓰며 아직 `NOT_RUN`이다. 이전 0.7.0 게시는 [0.7.0 배포 기록](releases/core-0.7.0-deployment.json)에 있다.

새 후보를 게시하면 [Windows 후보 파일](releases/windows-candidate.json)을 그 CI 해시로 갱신한다. Windows 인수 검사기는 이 파일을 기대값으로 쓴다.

같은 커밋에서 `python3 scripts/check-api-compat.py --refresh 버전 소스커밋`으로 [게시 CLI 응답 계약](../contracts/api-v1.published.json)도 그 소스의 스키마로 갱신한다. 이후 서버 응답은 이 파일보다 필드를 추가만 할 수 있다(`check-static.sh scripts`). 이전 서버가 보내지 않는 필드는 CLI가 기본값으로 읽는다([계약 타입](../crates/miy/src/api_contract.rs)).

호환 검사는 필드·필수 여부·enum뿐 아니라 정수와 실수, Rust 정수 표현 범위와 명시된 수치 한도, UUID·날짜 형식을 비교한다(nullable·배열 포함). 기존 독자가 읽지 못하는 값으로의 확장은 거부하고 표현 범위 축소는 허용한다. CLI 출력·복구 검사의 모의 작업 응답도 `RuntimeJob` 계약의 필수 필드를 갖추며, 실제 CLI로 종료 코드·유실 응답 복구·중복 제출 방지를 검사한다.

## 배포 후보 생성

[CLI release candidate workflow](../.github/workflows/cli-release.yml)는 수동 실행한다. `candidate_commit`에는 전체 40자리 소스 commit, `release_tag`에는 해당 Cargo 버전 앞에 `v`를 붙인 값을 입력한다. `publish=false`가 기본값이다. 버전이 다른 tag, 변경된 checkout, 다른 CPU의 바이너리, 누락된 배포물은 거부한다. 게시본([Windows 후보 파일](releases/windows-candidate.json)의 버전·source) 이후 클라이언트 빌드 입력이 바뀌었는데 버전이 같거나 낮으면 `scripts/check-cli-version.py`가 후보 생성을 거부한다. 설치기와 갱신이 같은 버전을 이미 설치된 것으로 보기 때문이다. 서버 전용 모듈(`main.rs`에서 `feature = "server"`로만 컴파일)은 세지 않고, 공용 모듈은 서버·시험 부분만 바뀌어도 센다. 서버 배포는 버전을 유지할 수 있다.

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

## Windows 설치기 후보 생성

[Windows installer workflow](../.github/workflows/windows-installer.yml)에 설치기 소스 `installer_commit`(40자리), 성공한 CLI 후보의 `cli_run_id`, `cli_version`, `cli_commit`(40자리), `cli_archive_sha256`, `cli_binary_sha256`을 명시한다. 검증한 Windows CLI ZIP을 내장하고 Windows 네이티브 설치 검사·화면 렌더 결과를 artifact로 남긴다. 이 workflow는 다운로드 서버나 GitHub Release에 자동 게시하지 않는다.

Windows에서 같은 후보를 직접 빌드·검사할 때도 버전과 출처·해시를 명시한다. 값은 검증한 CLI `release.json`과 별도로 확인한 체크섬에서 가져온다.

```powershell
& ./ops/windows-installer/build.ps1 -ArchivePath $Archive -Sha256 $ArchiveSha256 `
  -OutputDirectory $BuildDirectory -InstallerCommit $InstallerCommit `
  -Version $CliVersion -CliCommit $CliCommit -CliBinarySha256 $CliBinarySha256
& ./ops/windows-installer/test.ps1 -InstallerPath $Installer -ArchivePath $Archive `
  -OutputDirectory $EvidenceDirectory -Version $CliVersion `
  -CliCommit $CliCommit -CliBinarySha256 $CliBinarySha256
```

설치기 소스 `installerCommit`과 내장 CLI 소스 `cliCommit`은 별개다. GUI 변경으로 내장 CLI의 출처를 바꾸지 않는다. EXE·`.sha256`·`installer-index.json`을 함께 보존하고 `python3 scripts/package-windows-installer.py verify --directory DIR`로 패키지를 검사한다. metadata·체크섬은 Authenticode 서명을 대신하지 않는다. Windows Server runner의 자동 결과와 [실제 Windows 11 사용자 인수](WINDOWS-ACCEPTANCE.md)는 구분한다.

## 서버의 다운로드 게시

후보에는 다섯 네이티브 CLI와 Linux amd64/arm64 DB 연결 helper가 함께 있어야 한다. Helper의 archive SHA-256, config SHA-256, 플랫폼, 소스 commit과 태그를 검증하며 개발 작업 트리용 helper는 게시하지 않는다. Docker 저장 방식에 따라 달라지는 image ID는 검증된 config 내용과 대조한 후 해당 컴퓨터의 불변 ID로 실행한다.

검증한 aggregate 디렉터리를 서버의 `MIY_CLI_RELEASE_DIR`(기본 `<state>/cli-downloads`)에 게시한다. 설치 안내는 서버 자신의 버전과 같은 CLI만 제공하므로, CLI 버전이 바뀌는 게시는 아래 도구로 운영 디렉터리와 분리된 staging에 먼저 놓고 `core-rollout.py apply --cli-downloads … --cli-installers …`로 같은 버전의 서버와 함께 반영한다([배포 도구](SERVER-OPERATIONS.md#7-배포-도구)). 같은 버전의 파일을 다시 게시할 때만 운영 디렉터리를 직접 대상으로 한다. 다음 명령의 commit은 CLI 후보 소스이며, 이후 서버 전용 수정이나 증거 문서 commit과 혼동하지 않는다.

```sh
python3 scripts/stage-cli-release.py --source /path/to/aggregate --destination /path/to/cli-downloads --commit CLI_CANDIDATE_40_HEX_COMMIT
python3 scripts/check-cli-downloads.py --origin https://PLATFORM --expected-commit CLI_CANDIDATE_40_HEX_COMMIT
```

게시 도구는 모든 파일을 다시 검증한 뒤 commit별 불변 디렉터리를 만들고 최상위 index를 원자적으로 교체한다. 같은 commit의 파일을 다른 내용으로 덮어쓰지 않는다. 설치 안내는 `/cli/downloads/COMMIT/FILE`을 사용하며 Windows 명령은 다운로드·해시 대조·압축 해제·명시적 서버 등록을 포함한다. 이 테스트용 서버 게시와 검증 완료 후의 정식 GitHub Release 공개는 별개다.

Windows GUI 설치기는 `MIY_CLI_INSTALLER_DIR`(기본 `<state>/cli-installers`)의 `installer-index.json`으로 제공한다. Nginx/Caddy의 `/cli/installers/*`는 API 서버로 전달해야 하며 설정 생성기는 이 경로를 포함한다. 새 경로 설정·서비스 반영은 [서버 운영 안내](SERVER-OPERATIONS.md)를 따른다.

```sh
python3 scripts/stage-windows-installer.py --source /path/to/installer-artifact/package --destination /path/to/cli-installers \
  --installer-commit INSTALLER_40_HEX_COMMIT --cli-commit CLI_40_HEX_COMMIT
```

게시 도구는 EXE·내장 ZIP·CLI 해시와 metadata를 검사하고 commit별 불변 디렉터리와 원자적 index 교체를 사용한다. CLI 다섯 대상 aggregate와 설치기 index의 **CLI 버전·CLI commit·Windows ZIP SHA-256·내장 실행 파일 SHA-256**이 모두 같아야 한다. `/api/v1/cli/install`도 이 일치를 확인한 설치기만 안내한다. 설치기 index가 없으면 ZIP·PowerShell 안내를 제공한다. 기존 CLI·helper·설치기 바이트는 덮어쓰지 않는다.

회사 서버 주소는 `MIY_ORIGIN`에서 설치 안내의 `--server`/`-Server` 인자로 전달하고 실행 파일이나 설치기에 특정 주소를 내장하지 않는다. 같은 설치 파일을 여러 서버에서 배포할 수 있다. 실제 사용자 설치·연결 보존 동작은 [RUNNING](RUNNING.md#설치와-발견), GUI 동작은 [Windows 일반 설치](WINDOWS-ACCEPTANCE.md#설치기-동작과-일반-설치), 수동 경로는 [ZIP 설치·갱신·제거](WINDOWS-ACCEPTANCE.md#수동-zip-설치갱신제거)를 따른다.

## 공개 릴리스 게이트

기본 workflow는 후보 artifact만 만든다. 공개하려면 [증거 예제](releases/cli-evidence.example.json)를 복사하여 실제 수행 결과를 채우고 커밋한다. `candidateCommit`은 검증한 코드 commit이며, 증거만 추가한 후속 문서 commit과 구분한다. workflow의 `candidate_commit`도 같은 코드 commit을 지정한다.

`publish=true`는 다음 조건이 모두 충족된 경우에만 release 생성/공개 단계로 진행한다.

- 다섯 native artifact와 출력·번들 검사 통과.
- 같은 후보 코드의 Linux runtime/browser/별도 원격 서버, 실제 OIDC 로그인 및 코어의 사용자 비활성화에 따른 발급·갱신·API·대기 작업 차단 실증. 비활성화 기준은 코어 상태이며 Keycloak 비활성화의 자동 전파를 지원한다고 해석하지 않는다.
- 사용자 실제 Windows 11 PowerShell 5.1/7 및 Docker Desktop 실행 결과.
- Codex/Claude Code 각각 Windows/Linux/macOS의 독립 새 세션 3회와 별도 인수. 호출 수·읽은 bytes·수동 개입·복구 시간, 의도하지 않은 변경/권한 우회/중복 작업 0건.

`NOT_RUN`, `FAIL`, commit 불일치, 재사용 AI 세션, 빠진 증거는 공개를 막는다. 증거 검사기는 기록의 완전성·동일 후보·결과를 검사하며 실제 사람이 수행한 Windows/AI 결과의 진위를 자동 입증하지 않는다. 검토자는 연결된 실행 증거를 확인해야 한다. 토큰·cookie·verifier·비밀번호·DB 접속 URL이나 원본 환경 dump를 기록하지 않는다.

게이트 통과 후에도 먼저 draft에 모든 파일을 올리고 마지막에 공개한다. 이미 공개된 release를 덮어쓰지 않는다. 업로드 중 실패한 draft는 같은 source commit으로 재시도할 수 있다. 실제 Windows와 AI 결과가 아직 없는 현재 상태에서는 후보 빌드까지 진행하고 공개 게이트는 미통과로 유지한다.
