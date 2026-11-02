# CLI 0.6.3 Linux 서버 후속 검증·반영

이 인수 절차의 Linux 실행 결과는 [검증·반영 보고서](CLI-0.6.3-LINUX-VALIDATION.md)와 [JSON 증거](releases/cli-0.6.3-linux-validation.json)에 있다. 0.6.3 서버·UI·다운로드를 반영했고 실제 IdP 정상 로그인·소비된 실제 callback 재사용은 NOT_RUN으로 구분했다. 아래는 원래 전달한 절차다.

Windows 수정은 `codex/cli-experience-improvements`에 있다. 기능 소스는 `0db21c38d89061c4fb6d774585ffba46ee10407c`, CI 환경 격리 보완은 `8d7d04271f9467d8bbdd16308e73715b33e4dc0c`다. 이 브랜치에는 기존 서버 빌드와 Windows 검증 수정도 포함된다. 상세 결과는 [Windows 기록](WINDOWS-VALIDATION.md)과 [0.6.3 결과 JSON](releases/cli-0.6.3-experience-validation.json)에 보존한다.

아래 프롬프트를 Linux 서버의 Codex에 전달한다.

```text
MIY CLI 0.6.3 개선 소스의 Linux 검증과 서버 반영을 진행해 주세요.

저장소: https://github.com/hurxxxx/miy-v2.git
브랜치: codex/cli-experience-improvements
후보 소스: 8d7d04271f9467d8bbdd16308e73715b33e4dc0c
서버: https://v2.dwdcc.kr

기존 작업과 서버 설정을 보존하고 fetch 후 별도 깨끗한 checkout에서 이
브랜치를 확인하세요. 강제 reset이나 다른 앱/DB/Keycloak 초기화는 하지 마세요.
AGENTS.md 및 docs/CLI-EXPERIENCE-SERVER-HANDOFF.md와 관련 문서를 먼저 읽으세요.

변경 내용:
- CLI 0.6.3 / SDK 번들 0.7.1 유지.
- Docker 없는 로컬 빌드 실패에 app build --remote 명령과 recovery 제공.
- CLI 로그인 유효 시각/남은 시간/원래 context의 재로그인 명령 제공.
- 브라우저 전용 GET /api/v1/cli/devices/{id}가 비밀 없는 요청 상태/유효시간 제공.
- 승인 페이지의 남은 시간, 만료/처리 완료 안내, 코드 수정/연결 복구 개선.
- OAuth 실패 콜백은 로그인 쿠키를 지우고 고정된 복구 화면으로 redirect.
- 모바일 사이드바를 접어 앱 영역 확보.
- Windows 설치기에서 0.6.0 고정을 없애고 검증한 CLI 후보의 출처/해시를 입력받음.

1. MIY_CONFIG_DIR를 격리한 뒤 fmt, default-feature 서버 check/Clippy -D warnings/
   test, --no-default-features 클라이언트 검사와 pnpm frozen install/typecheck/
   플랫폼 build를 수행하세요. Windows에서 106개 단위, 13그룹 출력, 원격 빌드
   10그룹, 복구 6그룹, 브라우저 fixture 14개, 실제 설치기 24개가 통과했습니다.
   Windows 결과를 새 Linux 통합 결과로 이월하지 마세요.

2. 이 서버의 기존 Docker Engine에서 소유권이 격리된 회귀를 실행하세요.
   python3 scripts/runtime-regression/run.py
   python3 scripts/check-server-builds.py
   새 auth_sessions_regression의 device_status와 OAuth callback 검사도 실제로
   실행됐는지 확인하세요. GET 상태는 브라우저 로그인 필수, bearer CLI는 거부,
   승인된 다른 사용자는 404, 비밀/인증코드 미노출, no-store를 확인해야 합니다.
   새 SQL 마이그레이션은 없고, 기존 적용 SQL은 바꾸지 않았습니다.

3. pnpm exec playwright test --config tests/cli-experience.config.ts 로 격리한
   화면 회귀를 실행하세요. Linux에서는 Playwright Chromium을 사용합니다.
   실제 로컬 플랫폼/IdP 환경에서는 tests/development-login.spec.ts와
   로그인 요청 만료, 이미 사용한 OAuth 콜백, 정상 로그인 회귀도 확인하세요.
   실제 IdP 로그인·승인이 필요하면 사용자가 직접 처리하도록 안내하세요.
   인증 검사나 CSRF/Origin/state/nonce 검사를 우회하지 마세요.

4. CI 후보 생성은 publish=false로 실행했습니다:
   https://github.com/hurxxxx/miy-v2/actions/runs/40826960538
   성공 여부를 확인한 뒤 아래 artifact를 내려받으세요:
   miy-0.6.3-8d7d04271f9467d8bbdd16308e73715b33e4dc0c
   package-cli-release.py verify로 다섯 CLI와 두 Linux helper의 출처/해시를
   재확인하세요. 실패했으면 원인을 해결해 새 후보를 만들고 출처를 다시 기록하세요.

5. 해당 실행의 release-index.json에서 Windows 행의 version, commit,
   sha256, binarySha256을 읽고 windows-installer.yml에 전달하세요.
   workflow ref: codex/cli-experience-improvements
   installer_commit: 8d7d04271f9467d8bbdd16308e73715b33e4dc0c
   cli_run_id: 40826960538
   cli_version: 0.6.3
   cli_commit: 8d7d04271f9467d8bbdd16308e73715b33e4dc0c
   cli_archive_sha256 / cli_binary_sha256: 해당 CI index의 Windows 값
   성공한 installer artifact를 내려받아 package-windows-installer.py verify로
   검증하세요. Windows 로컬에서 만든 0db21c3 후보의 해시를 CI 후보에 섞지 마세요.

6. 서비스 반영 전에 배포/되돌림 대상을 준비하세요. 0.6.3 서버 API는 게시된
   release-index와 설치기 index가 같은 버전/CLI commit/Windows 해시일 때만
   설치 안내를 제공합니다. 구버전 index를 새 서버에 연결한 채 완료 처리하지 마세요.
   docs/CLI-RELEASE.md와 docs/WINDOWS-INSTALLER.md의 stage 도구로 불변
   디렉터리를 만들고, 검증한 새 index와 서버/UI를 계획대로 반영하세요.
   공개 GitHub Release/tag는 별도 승인·증거가 필요한 작업이므로 생성하지 마세요.

7. 기존 서비스 관리 방식으로 API·worker와 플랫폼 UI를 반영하고 버전/health,
   /api/v1/cli/install과 다운로드 실제 SHA-256을 확인하세요. 인증 실패 복구와
   390px/데스크톱 앱 화면도 실제 브라우저에서 확인하세요. 기존 Todo 앱
   todo-win-20261029의 release와 DB/레코드는 보존하세요. 이번 작업 때문에
   앱 버전을 올리거나 운영 승격할 필요는 없습니다.

8. 수행한 소스/바이너리 hash, CLI/installer CI, 검사 수, 실제 배포한 버전,
   브라우저 증거, 실패/수정/NOT_RUN과 cleanup을 문서에 남기고 커밋·푸시하세요.
   토큰/쿠키/DB 접속 문자열/개인 인증 저장소는 로그나 문서에 복사하지 마세요.
```

Windows에서 생성한 로컬 0.6.3 설치 파일은 공개 게시물이 아니다. 전체 OS/CPU 후보와 서버 회귀·실제 IdP 검증, 서버 재배포, 다운로드 index 게시를 이 Windows 실행의 통과로 표시하지 않는다.
