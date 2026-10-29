# Docker 없는 로컬 개발

CLI 0.6.1 / 번들 0.7.1은 기본 `miy app dev`에서 로컬 Rust 실행 파일을 구동하고 빌드한 React 화면을 CLI의 loopback 서버에서 제공한다. Docker Desktop, WSL, 로컬 PostgreSQL은 필요하지 않다. 이 변경은 개발 중인 소스에 해당하며, 기존 홈페이지에 게시된 CLI 0.6.0 설치 후보와 구분한다.

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

`app check`와 네이티브 로컬 개발은 Linux 배포 이미지 생성과 별개다. 현재 `app build`는 Linux Docker 엔진을 사용하므로 Docker 없는 Windows PC에서 배포까지 하려면 별도 Linux 빌드 환경이 필요하다. 네이티브 Windows 실행 파일을 Linux 배포 이미지로 표시하지 않는다. 개발 배포는 로컬 세션을 종료한 뒤 같은 개발 DB를 재사용한다.

실행 결과는 [Windows 검증](WINDOWS-VALIDATION.md)에 기록한다. 소스 검사·실제 DB CRUD·브라우저 통합·배포 결과를 각각 구분한다.
