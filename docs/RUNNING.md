# V0~V1 로컬 실행과 검증

이 단계는 UI 모듈 통합·OAuth·앱 DB 준비를 검증한다. 앱 컨테이너 설치와 업무 앱 개발 평가는 후속 단계이며, 검증용 UI의 `runtimeStatus`는 `not_deployed`로 표시한다.

## 준비

현재 지원·검증 환경은 Apple Silicon macOS와 Docker Desktop의 Linux 엔진이다. Rust 1.99.1, pnpm 10.34.5, Python 3, Docker Compose, Codex CLI 0.163.0을 사용한다. 프런트 의존성은 `pnpm-lock.yaml`, Rust 의존성은 `Cargo.lock`, 기반 이미지는 Compose의 digest로 고정한다.

Rust는 rustup으로 설치하고 `~/.cargo/bin`을 사용한다. 최초 실행에는 패키지·이미지를 내려받을 네트워크가 필요하다. `18443`은 HTTPS 진입점, `15432`는 로컬 PostgreSQL, `18080`은 Rust 코어, `19222`는 검증용 브라우저 연결에 사용한다.

```sh
bash scripts/up.sh
```

PostgreSQL·Keycloak·Caddy와 CA를 준비하고, Rust 빌드·코어 migration·검증 계정·정적 UI 빌드·DB 실행기·검증 브라우저를 기동한다. 최초 앱 등록은 인수 검사에서 수행한다. 코어·DB 실행기는 이 단계에서 호스트의 Rust 프로세스이며 Compose 기반 서비스와 연결한다.

자격증명은 최초 실행 시 생성한 `.local/credentials.json`, 서버 환경은 `.local/runtime.env`에 보관한다. `.local`은 Git에서 제외한다. `admin`, `developer`, `alice`, `bob`은 합성 검증 계정이다. 초기화 SQL과 realm에는 이 로컬 설정의 자격증명만 사용한다.

기동 도구는 코어 프로세스의 환경에서 DB 생성 자격증명을 제거하고, DB 실행기 환경에서 OAuth 클라이언트 비밀값을 제거한다. 로컬의 동일 OS 계정에서 실행하는 프로세스 분리를 제품의 OS 계정·컨테이너 격리 검증으로 해석하지 않는다.

접속 주소는 `https://localhost:18443`이다. 일반 브라우저를 사용하려면 `.local/caddy/caddy/pki/authorities/local/root.crt`를 신뢰해야 한다. 자동 검증은 별도 브라우저 컨테이너의 신뢰 저장소에 CA를 등록한다. 호스트의 시스템 신뢰 설정을 자동 변경하거나 TLS 검증을 끄지 않는다.

## 전체 검사

기존 Codex의 모델 공급자 인증이 연결되어 있어야 한다. 플랫폼의 MCP OAuth는 별도로 검증 계정으로 로그인한다. 모델 공급자 계정의 로그인·로그아웃 설정은 바꾸지 않는다.

```sh
bash scripts/check.sh
```

검사는 Rust 단위 검사·프런트 타입 검사, Playwright 통합 인수, 실제 DB 격리·migration·실행기 강제 종료, 실제 Codex MCP 호출과 갱신, 독립 OAuth·MCP 클라이언트 검사 순서다. DB 장애 검사 중에는 일반 실행기를 중지하고 검사 전용 실행기를 사용하며 종료 후 일반 실행기를 복구한다.

Codex 검사는 명령의 프로세스별 설정으로 `miy_validation` 서버만 구성하고 읽기 전용 `get_contract`만 허용한다. 로그인·호출은 실제 Codex 클라이언트를 사용하며 별도 개발 에이전트를 제작하지 않는다. 로컬 CA는 기존 CA에 추가한 검사 전용 묶음으로 제공한다. 개발 계정의 명시적인 `offline_access` grant와 scope로 Codex 갱신을 검사한다.

실행 증거는 `.local/evidence/`에 저장한다. `runtime.json`, `database.json`, `codex.json`, `mcp.json`을 모아 `summary.json`을 생성하며, 빠진 결과는 성공으로 처리하지 않는다. 실행 로그와 Playwright trace는 로컬에만 두고 외부에 게시하지 않는다.

## 중지와 재실행

```sh
bash scripts/down.sh
bash scripts/up.sh
```

중지는 DB 볼륨·개발 자산·자격증명을 보존한다. `up.sh`는 저장된 자격증명을 재사용하며 사용자 DB를 초기화하지 않는다. UI 검증은 매 실행마다 새 앱 ID와 설치를 사용해 플랫폼의 사전 하드코딩에 의존하지 않는다.

직접 Rust 검사를 실행하려면 `.local/runtime.env`를 현재 프로세스에 읽어야 한다. DB 장애 검사에는 실제 기반 서비스가 필요하며 일반 DB 실행기가 중지된 상태에서만 실행한다. 평상시 `cargo test`에는 해당 검사가 명시적으로 `ignored`로 표시되고, 전체 검사 스크립트가 이를 따로 실행한다.

## 현재 계약의 범위

- 릴리스는 해시로 검증한 정적 UI·manifest·SQL fixture다. OCI 이미지 배포물은 V4에서 연결한다.
- 앱 backend API 프록시는 아직 연결하지 않았다. 미배포 앱에서 SDK의 `api` 호출은 `app_runtime_not_deployed`로 거부한다.
- migration은 트랜잭션 가능한 SQL만 지원한다. 적용한 파일의 변경·알 수 없는 이력·비트랜잭션 표시는 거부한다.
- 통합 UI는 신뢰된 코드다. 브라우저 보안 격리를 제공하는 것으로 해석하지 않는다.
- Keycloak의 `resource-indicators`는 실험 기능이다. 이번 호환성 결과를 기업 SSO·다른 클라이언트 전체의 지원으로 확대하지 않는다.
- 기동·검사 스크립트의 Python과 브라우저 검증 도구는 개발 보조 도구다. 플랫폼 코어와 앱 백엔드의 실행 프로필은 Rust다.
