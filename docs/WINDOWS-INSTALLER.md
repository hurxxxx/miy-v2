# Windows GUI 설치 파일

Windows 11 x64 사용자는 회사 MIY 홈페이지의 `/cli/install`에서 **Windows 설치 파일 다운로드**를 선택한다. 설치 파일에는 CLI와 검증된 설치 스크립트가 들어 있다. 설치할 때 GitHub나 고정된 공개 배포 서버에 접속할 필요가 없다.

## 설치와 서버 등록

1. 홈페이지에 표시된 회사 서버 주소를 복사하고 `MiySetup-0.6.0-windows-x64.exe`를 실행한다.
2. 빈 서버 주소 입력란에 `https://회사도메인` 또는 `http://사내IP:포트`를 입력하고 연결을 확인한다. 기본 도메인이나 IP는 없다.
3. 사용자 영역의 설치 위치와 사용자 PATH 추가 여부를 확인하고 설치한다.
4. 설치 완료 후 브라우저 로그인을 시작해 해당 서버에서 CLI 접근을 승인한다.
5. 새 PowerShell을 열고 `miy auth status`로 연결을 확인한다. 이후 [핵심 흐름 테스트](CORE-TESTING.md)를 진행한다.

기존 설정이 있으면 현재 활성 서버를 표시하고 설정과 인증정보를 보존한다. 다른 서버를 입력한 경우 기존 연결을 바꾸기 전에 새 연결 이름 또는 기존 연결을 명시적으로 선택한다. 설치 파일을 다시 실행하면 같은 설치 경로의 CLI를 갱신할 수 있다. 갱신할 설치 파일도 회사 MIY 홈페이지나 사내 배포 경로에서 받는다.

기본 설치 위치는 `%LOCALAPPDATA%\Programs\miy\bin`이다. 관리자 권한 설치, 시스템 PATH 변경, 실행 중인 CLI 강제 종료를 하지 않는다. 실행 파일이 사용 중이면 해당 CLI/MCP를 정상 종료하고 재시도한다. 제거 기능은 설치한 CLI만 대상으로 하며 설정·인증정보·프로젝트·앱 데이터는 남긴다.

설치 파일은 Windows에 포함된 .NET Framework를 사용한다. Windows 11의 .NET Framework 포함 범위는 [Microsoft 문서](https://learn.microsoft.com/en-us/dotnet/framework/install/on-windows-and-server)를 참고한다. 이 후보에는 Authenticode 서명이 없으므로 Windows가 게시자를 확인할 수 없다는 경고를 표시할 수 있다. 회사 배포 위치와 게시된 SHA-256을 확인한다.

## 회사 서버에 게시

같은 설치 파일을 여러 회사가 각각 배포할 수 있다. 서버 주소는 설치 파일에 포함하지 않고 사용자 PC의 CLI 연결 설정에 저장한다. 설치 파일을 회사마다 다시 빌드할 필요가 없다.

서버는 `MIY_CLI_INSTALLER_DIR`의 `installer-index.json`을 읽는다. `scripts/bootstrap.py`의 기본 경로는 상태 디렉터리 아래 `cli-installers`다. 기존 서버는 이 환경변수를 설정하고 API 서비스를 재시작한다. Nginx 또는 Caddy에서 `/cli/installers/*`를 API 서버로 전달해야 한다. 저장소의 프록시 설정 생성기는 이 경로를 포함한다.

검증된 Actions artifact를 풀고 다음과 같이 게시한다.

```sh
python3 scripts/stage-windows-installer.py --source /path/to/installer-artifact/package --destination /path/to/cli-installers \
  --installer-commit INSTALLER_40_HEX_COMMIT --cli-commit CLI_40_HEX_COMMIT
```

게시 도구는 EXE·내장 ZIP·CLI 해시와 metadata를 검사한다. 소스 commit별 불변 디렉터리를 만든 뒤 최상위 index를 원자적으로 바꾸며 기존 배포물을 덮어쓰지 않는다. `/api/v1/cli/install`은 현재 게시된 CLI Windows ZIP과 설치 파일의 버전·CLI 소스 commit·ZIP 해시·실행 파일 해시가 일치할 때 설치 파일을 안내한다. 설치 파일 index가 아직 없으면 기존 ZIP·PowerShell 안내를 제공한다.

설치 파일의 소스 `installerCommit`과 내장 CLI의 소스 `cliCommit`은 별개다. GUI 설치기를 개선했다고 기존 CLI 바이너리의 출처를 새 commit으로 바꾸지 않는다. 설치 파일 `.sha256`과 `installer-index.json`을 함께 보존한다. 이 metadata는 코드 서명을 대신하지 않는다.

## 빌드와 검증

[Windows installer workflow](../.github/workflows/windows-installer.yml)는 40자리 설치기 소스 commit과 검증한 Windows CLI ZIP으로 Windows x64 설치 파일을 만든다. 기존 다섯 OS/CPU CLI와 Linux DB helper 배포 묶음은 그대로 유지한다. Windows 네이티브 설치 검사와 화면 렌더 결과를 artifact에 보존한다.

호스티드 Windows Server의 자동 검사는 사용자 Windows 11에서의 실제 클릭·브라우저 승인·Docker Desktop·앱 개발 인수를 대신하지 않는다. 해당 결과는 [Windows 검증](WINDOWS-VALIDATION.md)에 실제 실행 후 기록한다. Linux/macOS 설치 편의 개선은 이후 단계다.
