# CLI 고장 신고

CLI 0.12.0과 core schema 24의 기능이며 2026-11-16 miy.works에 게시·반영했다([반영 기록](releases/support-reports-2026-11-16-deployment.json)). 게시·서버 반영 여부는 [CLI 릴리스](CLI-RELEASE.md)와 [서버 운영 안내](SERVER-OPERATIONS.md)를 따른다. SDK 번들은 0.11.0 바이트를 유지한다.

## 사용자

로그인이나 MCP 없이 CLI로 직접 신고한다. 현재 선택한 context의 서버로 보내며 자격증명 저장소를 열지 않는다.

```sh
miy report send --message "브라우저 승인 후 auth status가 실패합니다"
```

응답의 `id`가 신고 번호다. `received`는 서버 DB의 접수 응답을 받았다는 뜻이며 문제 해결을 뜻하지 않는다. 토큰·비밀번호·개인정보·원문 로그를 설명에 넣지 않는다. 미리 확인하거나 오프라인에서 저장할 수 있다.

```sh
miy report send --message "문제 설명" --dry-run
miy report send --message "문제 설명" --save-only
miy report list
miy report show <신고-UUID>
miy report retry <신고-UUID>
```

`--dry-run`은 가려진 전송 내용을 보여주고 저장·전송하지 않는다. `--save-only`는 저장만 한다. 나머지는 전송 전에 기본 설정 폴더의 `reports/<UUID>.json`에 기록한다. 네트워크·서버 장애나 접수 제한으로 응답을 못 받으면 `report_queued` 오류와 파일 위치, 재전송 인자를 반환한다. 응답 유실도 같은 UUID·내용으로 재전송하므로 중복 신고가 생기지 않는다. 재전송 목적지는 원래 서버로 고정된다. 접수된 파일도 로컬에서 확인할 수 있도록 보존한다.

설정 파일 자체가 고장 난 경우 공개 서버 origin과 별도 비공개 보관 폴더를 직접 지정한다. 도메인, IP·포트, 사설 CA 환경에서 동일하게 동작한다.

```sh
miy report --outbox "$HOME/miy-reports" send \
  --server https://portal.example.org --message "설정 파일 오류"
miy report --outbox "$HOME/miy-reports" retry <신고-UUID>
# 도메인 없는 설치 예: --server https://192.168.10.4:8443 --ca-file company-ca.pem
```

자동 전송은 없다. CLI/MCP 실패 시 로컬 `reports/last-error.json` 한 파일에 가장 최근의 안전한 오류 코드와 발생 시각을 덮어쓴다. 다음 수동 신고에 CLI/SDK 버전, OS·CPU, 설정 경로의 선택 방식(값 제외), 전역/프로젝트 실행 파일 구분과 함께 포함한다. 가장 최근 오류는 다른 명령·context에서 발생했을 수 있다. 원문 오류 메시지·인자·환경 변수 값·경로·사용자 ID·자격증명·소스·로그는 자동 수집하지 않는다. 기존 0.11.0에서 발생한 오류는 새 진단 파일에 소급 수집되지 않는다.

설명과 관리자 메모에서 알려진 비밀 표시가 있는 줄을 가리지만, 임의의 비밀을 모두 탐지한다고 보장하지 않는다. 원문 로그·파일을 자동 첨부하지 않는 이유다. Unix 보관 폴더/파일은 0700/0600이며, Windows 파일은 기존 private-file ACL 함수를 사용한다.

## 서버와 관리자

- 업무 포털의 `POST /api/v1/support/reports`만 비로그인 접수를 허용한다. bearer·cookie로 사용자를 식별하지 않으며 인증 실패 상태의 클라이언트도 접수할 수 있다. dev·console listener와 브라우저 Origin/Fetch Metadata 요청은 이 접수 경로를 사용할 수 없다.
- 입력은 버전이 있는 구조화 JSON이며 미정의 필드를 거부한다. 요청은 16KiB, 설명은 4,000자까지다. UUID v4는 클라이언트가 생성하고 같은 ID의 다른 내용은 409로 거부한다. 응답은 ID와 접수 상태만 반환하며 기존 신고의 내용·처리 상태를 공개하지 않는다.
- `support_reports`가 정본이다. 여러 서버 프로세스가 PostgreSQL advisory lock으로 접수를 직렬화하며 동시 경합·시간당 신규 120건·총 20,000건 한도는 429로 거부한다. 한도는 전체 설치에 적용하며 IP별 제한이 아니다. 임의의 forwarded 헤더를 신뢰하거나 IP를 저장하지 않는다. 접수 제한 시 CLI 로컬 파일에서 나중에 재전송한다.
- 콘솔의 **플랫폼 관리 → 고장 신고 → 신고 조회**에서 목록·진단·설명을 확인하고 접수/조사 중/해결/종료와 메모를 저장한다. 목록은 50건 단위로 이어서 읽고 상태로 필터링한다.
- `GET /api/v1/support/reports`와 `PATCH /api/v1/support/reports/{id}`는 현재 활성 관리자만 허용한다. 콘솔 쿠키는 CSRF 검사를 거치며 관리 CLI bearer는 기존 업무 포털 경계를 사용한다. 일반 사용자·dev/업무 포털 쿠키·위임 토큰은 신고 내용을 읽거나 수정할 수 없다. 변경 시 트랜잭션 안에서 관리자 권한을 재확인하며 revision 충돌은 409다. 감사에는 신고 ID와 상태만 기록한다.
- 익명 신고는 제출자가 제공한 미확인 정보다. 계정 신원·진단의 진실성·에이전트 실행 지시로 신뢰하지 않는다. 화면은 텍스트로 렌더링하며 신고의 코드·명령·링크를 자동 실행하지 않는다.

자동 삭제는 하지 않는다. 전체 20,000건 한도에 접근하면 관리자가 기존 DB 백업 절차로 보관한 뒤 처리 완료 신고의 보존 기간과 정리를 결정한다. 백업 없이 기존 신고를 삭제하거나 앱 데이터와 함께 정리하지 않는다.

## macOS 인증 오류 조사

[클라이언트 관찰 기록](../CLI-CREDENTIAL-STORAGE-ISSUE-2026-11-16.md)에 CLI 0.11.0과 Codex Background 세션의 실패가 있다. 서버 대조에서는 해당 device 요청이 `consumed`이고 세션·refresh 자격증명 발급이 완료됐다. 교환 후 쓰기 실패는 다른 오류 코드로 반환하는 0.11.0 코드와 대조하면 기존 자격증명을 읽는 경로로 좁힐 수 있다. 실제 backend와 OS 거부 원인은 아직 미확인이다.

0.12.0은 저장소 오류에서 `operation`(read/write/remove), 허용된 `reason`, Keychain 실패의 `backend=macos_keychain`과 숫자 `osStatus`를 보존한다. 교환 후 기존 자격증명 읽기가 실패하면 새 서버 세션의 회수를 시도하고 결과도 반환한다. 저장소가 고장 나면 MCP가 반복 로그인을 권하지 않는다. Keychain 항목을 삭제하거나 backend를 몰래 바꾸지 않는다.

[CLI 업데이트와 MCP 갱신 절차](RUNNING.md#cli-업데이트와-mcp-갱신)로 전역·프로젝트 CLI를 갱신하고 에이전트를 재시작한다. 그 뒤 같은 OS 사용자·context에서 **같은 바이너리**의 일반 GUI 터미널과 Codex 실행을 비교해야 한다. 전역 CLI 갱신만으로 이미 설치한 프로젝트 `.miy-tools/miy`와 실행 중인 MCP가 갱신되는 것은 아니다. 프로젝트 폴더에서 새 전역 `miy app setup`을 실행하고 에이전트를 다시 시작한 뒤 다음을 실행한다.

```sh
./.miy-tools/miy auth status --output json
./.miy-tools/miy report send --message "GUI 터미널/Codex 중 오류가 발생한 실행 환경과 증상"
```

실제 Mac의 원인 확정과 복구는 이 비교 결과로 검증한다. 서버에서 빌드·검사한 사실을 Mac에서 고장이 해결됐다는 증거로 쓰지 않는다.
