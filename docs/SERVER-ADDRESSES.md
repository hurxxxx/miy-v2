# 서버 주소 설정과 변경

도메인은 설치 조건이 아니다. **단일 IP의 포트 분리와 도메인 분리를 모두 지원**한다. 주소는 설치 설정이며 특정 조직의 도메인을 Core·CLI에 넣지 않는다. 새 설치는 [SETUP](SETUP.md), 실제 호스트의 주소·서비스명은 [SERVER-OPERATIONS](SERVER-OPERATIONS.md)가 관리한다.

## 세 화면과 두 개발 환경

| 화면 | 설정 | 사용자와 역할 |
| --- | --- | --- |
| 업무 포털 (`Portal`) | `MIY_ORIGIN` | 일반 사용자의 prod 앱 실행. CLI/API·HTTP MCP의 대표 주소 |
| 앱 개발 포털 (`DevPortal`) | `MIY_DEV_PORTAL_ORIGIN` | dev 앱 실행·시험. 활성 사용자에게 해당 앱의 `use@dev`가 있으면 이용 가능하며 관리자·`develop@dev`의 미리보기 편의도 적용. 앱 비활성화·위임 등 세부 조건은 권한 정책을 따름 |
| 관리 콘솔 (`Console`) | `MIY_CONSOLE_ORIGIN` | 시스템 관리자와 권한 있는 앱 개발자의 관리, CLI device 승인, Keycloak 로그인. 슈퍼 관리자 전용 화면이 아님 |

개발 포털은 IDE나 CLI 접속 서버가 아니다. 브라우저 로그인과 해당 환경의 앱 실행용이며 bearer 요청을 거부한다. 관리 API는 권한 있는 CLI bearer로도 호출할 수 있어 개발자가 콘솔 화면을 열지 않고 빌드·배포할 수 있다. 화면 접근과 실제 명령 권한은 별도로 검사한다.

**플랫폼 코어의 dev/prod 분리**는 플랫폼 인스턴스·상태·DB·서비스를 둘로 나누는 일이다. 각 인스턴스 안에 위 세 화면과 업무 앱의 dev/prod가 다시 있다. `devPortalOrigin`만 추가해서 코어 개발 플랫폼이 분리되지는 않는다.

## 주소 파일 하나로 설정

[IP·포트 예제](../ops/addresses.ports.example.json) 또는 [도메인 예제](../ops/addresses.domains.example.json)를 복사해 여섯 필드를 지정한다. 예제의 예약 IP·도메인은 실제 주소로 바꾼다.

```json
{
  "origin": "https://192.0.2.10:8443",
  "consoleOrigin": "https://192.0.2.10:9443",
  "devPortalOrigin": "https://192.0.2.10:9444",
  "appFrameOrigin": "https://192.0.2.10:{port}",
  "appFramePorts": "20000-20199",
  "allowHttp": false
}
```

도메인 방식에서는 `portal.example.com`, `console.example.com`, `dev.example.com`, `app-{key}.example.com`처럼 역할별 이름을 쓰고 `appFramePorts`는 `null`로 둔다. `{key}`는 설치 UUID의 32자리 16진수이며 첫 호스트 라벨에만 둔다. `sandbox.example.com` 같은 이름도 가능하지만 문서에서는 계속 **앱 개발 포털**로 부른다. 특정 이름·TLD는 제품 필수값이 아니다. 향후 별도 코어 개발 인스턴스는 `dev-portal`·`dev-console`·`dev-apps`·`dev-app-{key}` 같은 충돌 없는 이름이나 별도 IP/포트로 구성한다.

포트 방식은 IPv4와 대괄호로 감싼 IPv6를 받는다. 세 브라우저 출처의 scheme을 같게 하고 서로 다른 출처를 지정한다. 앱 프레임 포트 풀은 세 출처와 겹치면 안 된다. 포트는 설치별로 영속 배정하고 삭제된 설치의 출처·포트도 재사용하지 않는다. HTTPS에는 클라이언트가 신뢰하는 IP SAN 인증서 또는 해당 호스트명 인증서가 필요하다. 사설 CA의 앱 SDK 전달 한계는 [SETUP의 현재 범위](SETUP.md)를 따른다. 제한된 HTTP 설치는 `http://` 주소들과 `allowHttp: true`를 명시한다.

```bash
# 파일 검증만 수행: 상태 생성·비밀 읽기·DNS 변경·서비스 재시작 없음
python3 scripts/bootstrap.py --profile server --state-dir /srv/miy \
  --addresses-file /path/to/addresses.json --check

# 빈 서버 최초 설정 생성. 이후의 TLS·DB·서비스 설치는 SETUP 절차를 따른다.
python3 scripts/bootstrap.py --profile server --state-dir /srv/miy \
  --addresses-file /path/to/addresses.json --no-shared-development-login
```

주소 입력 우선순위는 **명령행의 개별 옵션 → 주소 파일 → 환경변수/저장된 설정/기본값**이다. 파일은 오타·중복 키·누락 필드를 거부한다. 기존 `--origin`, `--console-origin`, `--dev-portal-origin`, `--app-frame-origin`, `--app-frame-ports`도 유지한다. 파일을 수정한 것만으로 실행 중인 서비스가 바뀌지는 않는다. `--check`는 주소 문법·경계 검사이며 실제 DNS·인증서·포트 사용 여부를 확인하는 기능은 아니다.

`dwdcc.kr` 사내 호스트의 실제 입력 파일은 `.local/addresses.json`이며 **`portal.dwdcc.kr`·`console.dwdcc.kr`·`dev2.dwdcc.kr`·`app-{key}.dwdcc.kr`**를 사용한다. 2026-11-17에 설정 파일뿐 아니라 Nginx·IdP·기존 프레임 매핑까지 옮겼다([전환 기록](releases/dwdcc-role-addresses-2026-11-17-deployment.json)). `dev.dwdcc.kr`은 기존 AI-DO가 사용하므로 앱 개발 포털은 `dev2`로 정했다. 이 호스트의 실제 구성·복구는 [dwdcc.kr 운영 기준](SERVER-OPERATIONS.md#dwdcckr-주소-설정-정본)을 따른다. `miy.works`는 별도 설치이며 그 호스트의 `dev.miy.works`와 구분한다.

| 생성되는 값 | 입력과 관계 |
| --- | --- |
| `server.json`·`runtime.env`·`compose.env` | 세 포털, 프레임 템플릿/풀, 공개 Host·포트 |
| `MIY_ISSUER`·Keycloak `frontendUrl` | 콘솔 주소 + `/identity/realms/miy`, 콘솔 주소 + `/identity` |
| Keycloak `miy-web` | 세 출처의 정확한 callback/logout URI. CORS webOrigins는 업무 포털·콘솔만 |
| OAuth MCP resource/client/audience | 업무 포털 주소 + `/mcp` |
| CLI 설치 안내 | 업무 포털 주소 + `/cli/install`; 안내 데이터는 `/api/v1/cli/install` |
| Nginx | 고정 Host/X-Forwarded-Host·proto/port, CLI 승인 redirect, CSP, 프레임 경로를 생성 설정에서 렌더링 |

## CLI와 MCP에 배포할 고정 주소

CLI 설치기의 `--server` 또는 `miy context add NAME --url URL`로 입력한 업무 포털 주소를 개발 PC의 context에 저장한다. 서버 주소가 바뀌어도 이미 배포된 PC 설정은 자동으로 바뀌지 않는다. CLI 설치 안내 URL과 CLI가 접속할 서버 URL을 구분한다. `cli.example.com`을 설치 안내로 redirect할 수 있지만 API용 context의 주소는 업무 포털이다.

프로젝트 `.mcp.json`·`.codex/config.toml`은 로컬 `miy mcp`를 실행하는 **stdio MCP**다. 원격 MCP URL이 아니며 앱 개발 안내를 제공한다. 앱 개발·빌드·배포는 CLI 인증/context와 명령을 사용한다. 직접 HTTP MCP는 `<origin>/mcp`이며 허용 Host/Origin, OAuth resource/audience와 metadata가 이 주소를 기준으로 한다. 임의 `mcp.example.com` alias를 Nginx에 추가하는 것만으로 독립 MCP 접속 주소가 되지 않는다. 이 구조는 도메인과 IP·포트 방식에서 동일하다.

## 기존 설치의 주소 변경

설정값을 바꾸면 새 설정을 만들 수 있지만 기존 IdP·프레임 DB·개발 PC까지 자동 이전되는 것은 아니다. 개발자에게 CLI를 배포하기 전에 대표 주소를 확정하는 편이 좋다. 아래는 **동일한 플랫폼 상태를 유지한 주소 변경**이며 초기화·seed 절차가 아니다.

### 변경 후보 준비

서비스 계정으로 현재 상태를 입력하고 새 비공개 디렉터리에 후보를 만든다. 아래 변수는 설치한 호스트의 값으로 지정한다. 후보에는 기존 자격증명이 있으므로 Git이나 공개 디렉터리에 두지 않는다.

```bash
export MIY_STATE_DIR=/srv/miy
export MIY_ADDRESS_CANDIDATE=/srv/miy-address-change
python3 scripts/bootstrap.py --state-dir "$MIY_STATE_DIR" \
  --addresses-file /path/to/addresses.json --check
python3 scripts/bootstrap.py --state-dir "$MIY_STATE_DIR" \
  --addresses-file /path/to/addresses.json \
  --no-shared-development-login --output-dir "$MIY_ADDRESS_CANDIDATE"
```

`--output-dir`은 새 디렉터리만 받으며 현재 상태 파일을 수정하지 않는다. 후보의 런타임 경로는 원래 `--state-dir`을 가리킨다. 기존 비밀을 재사용하고 `MIY_DATABASE_CONTAINER` 같은 별도 환경 설정도 보존한다. 생성기가 관리하는 항목은 선택값으로 다시 생성되므로 현재 설치의 DB/네트워크·서비스 drop-in·Compose override와 대조한다. 사설 CA를 계속 사용할 때는 `--ca-file /실제/CA.pem`도 명시한다. 기존 상태를 공개 CA로 바꾸면서 CA 지정이 남아 있지 않은지도 확인한다.

후보 Nginx와 systemd 파일을 생성한다. HTTP 프로필은 인증서·키 옵션을 생략한다. 인증서가 여러 SAN/와일드카드 또는 IP SAN으로 **모든 출처**를 포함해야 한다.

```bash
python3 scripts/render-server-config.py --state-dir "$MIY_STATE_DIR" \
  --settings-dir "$MIY_ADDRESS_CANDIDATE" \
  --binary "$MIY_STATE_DIR/bin/miy-server" --user "$(id -un)" \
  --service-prefix miy-v2 --certificate /path/to/live/fullchain.pem --key /path/to/live/privkey.pem
```

렌더러는 인증서의 실제 내용은 검사하되 Nginx에는 전달받은 symlink 경로를 유지한다. 갱신된 인증서로 symlink가 이동하면 reload 후 새 인증서가 사용된다. 이 명령은 파일 생성만 하며 설치·reload는 하지 않는다. Nginx의 `server_name`만 수동으로 바꾸지 않는다.

### 기존 프레임 출처 계획

`app_frame_origins`에 저장된 주소는 템플릿만 바꿔도 자동 변경되지 않는다. 명시적인 DB 컨테이너에서 공개 주소 매핑만 내보낸 뒤 후보 설정에 맞는 SQL을 생성한다. 여기서 `MIY_DATABASE_CONTAINER`는 예제를 복사하지 말고 실제 설치 값을 지정한다.

```bash
test -n "$MIY_DATABASE_CONTAINER"
docker exec "$MIY_DATABASE_CONTAINER" psql -X -U postgres -d miy_core -At \
  -v ON_ERROR_STOP=1 -c "SELECT COALESCE(json_agg(t),'[]'::json) FROM (SELECT origin,installation_id,port FROM app_frame_origins ORDER BY origin) t" \
  > "$MIY_ADDRESS_CANDIDATE/frame-origins.json"
python3 scripts/plan-frame-origin-move.py \
  --settings "$MIY_ADDRESS_CANDIDATE/server.json" \
  --snapshot "$MIY_ADDRESS_CANDIDATE/frame-origins.json" \
  --output "$MIY_ADDRESS_CANDIDATE/frame-origins.sql"
```

도구는 DB에 접속하거나 변경하지 않는다. 생성 SQL은 한 트랜잭션에서 전체 매핑의 스냅샷 일치를 확인하고, 변경 설치의 실행/갱신 코드를 지우고 기존 프레임 세션을 회수한 뒤 새 출처를 배정한다. 옛 행은 설치 연결만 해제하고 출처·포트 예약을 남긴다. 앱 DB·릴리스·설치 UUID는 보존한다. 포트 풀이 소진되거나 목표 주소가 과거에 예약됐다면 거부하며, 풀을 늘리거나 새 이름을 선택해야 한다. 포트 프로필의 IP를 옮길 때도 기존 포트 예약을 재사용하지 않는다.

### 반영과 복구 체크리스트

1. DNS가 있는 설치는 새 이름의 A/AAAA와 관리 주체를 확인한다. IP 설치는 새 포트와 방화벽을 확인한다. 서버·개발 PC·앱 컨테이너가 새 주소에 도달하고 인증서를 신뢰해야 한다.
2. 해당 설치의 [백업·별도 복원 확인](SERVER-OPERATIONS.md)을 완료한다. Core DB·Keycloak DB와 기존 설정/Nginx/서비스·TLS를 함께 보관한다. 다른 운영자의 설정 변경과 앱 작업도 멈춘다.
3. Core·worker를 중지한 뒤 프레임 SQL을 검토·적용한다. 미리 준비한 스냅샷이 달라졌으면 SQL이 전체를 롤백하므로 중지 상태에서 다시 내보내고 **새 파일**로 계획한다.

   ```bash
   docker exec -i "$MIY_DATABASE_CONTAINER" psql -X -U postgres -d miy_core \
     -v ON_ERROR_STOP=1 < "$MIY_ADDRESS_CANDIDATE/frame-origins.sql"
   ```

4. 후보의 `server.json`·`runtime.env`·`compose.env`·`realm.json`을 원래 상태 경로에 소유권과 기존 비공개 권한을 유지하며 설치한다. `credentials.json`·`init.sql`을 덮어쓰거나 DB 초기화·seed를 실행하지 않는다. 기존 Compose 프로젝트/볼륨/override로 Keycloak의 `KC_HOSTNAME`을 새 콘솔에 맞춰 재생성하고 준비 상태를 확인한다. 다른 설치의 Compose 기본값을 쓰지 않는다.
5. 기존 realm을 동기화한다. `MIY_STATE_DIR`은 실제 상태 경로다. 업무 포털도 옮겼다면 `python3 scripts/configure-identity.py --previous-origin '<이전 업무 포털 URL>'`을 실행한다. 이 옵션은 이전 MCP resource client의 내부 ID를 유지하면서 새 resource URL로 바꾼다. 콘솔만 옮겼다면 옵션 없이 실행한다. `frontendUrl`·정확한 callback/logout·audience를 갱신하며 사용자 subject·비밀번호는 재생성하지 않는다.
6. 후보 Nginx를 호스트의 실제 설정 위치에 설치하고 `nginx -t` 후 reload한다. 사용자 정의 default/redirect 사이트, 인증서 갱신 hook과 systemd drop-in도 대조한다. Core·worker를 시작하고 세 health, OIDC discovery의 issuer, MCP metadata의 resource/authorization_servers를 확인한다. 이미 생성된 앱 컨테이너의 `MIY_CORE_ORIGIN`은 옛 값이 남는다. `app start`·`app restart`는 기존 컨테이너를 재사용하므로 환경변수가 바뀌지 않는다. 이전에 실행 중이던 설치는 승인된 동일 서버 빌드 이미지/릴리스로 재배포해 컨테이너를 재생성하고 DB·스키마·설치 식별자가 유지되는지 확인한다. 중지된 설치도 다시 사용하기 전에 재생성한다. 이어 로그인·CLI 승인·HTTP MCP OAuth·기존 dev/prod 앱 프레임과 앱 백엔드의 새 포털 연결을 확인한다.
7. 실패 시 Core·worker를 중지하고 **DB/IdP와 설정·Nginx를 같은 백업 시점**으로 복구한다. 설정만 되돌리면 이미 이전한 프레임 매핑과 어긋난다. 복구 후 같은 연결 검사를 실행한다.

기존 `console-origin-move.py`와 최초 도입용 `app-frame-settings.py`는 옛 호스트의 제한된 전환용이다. 임의 설치의 전체 주소 이전 도구로 실행하지 않는다. 위 절차는 주소 후보/SQL 생성과 적용 순서를 제공하며 전체 서비스를 한 번에 이전하는 자동 배포 명령은 아니다.

## 세션과 개발 PC의 영향

| 종류 | 영향과 처리 |
| --- | --- |
| 브라우저 세션 | 새 호스트의 쿠키·issuer/callback에 맞춰 다시 로그인. 열려 있던 앱은 저장 후 닫고 새 포털에서 실행 |
| MIY CLI device 세션 | `miy_access_`·`miy_refresh_`는 Core DB의 opaque 토큰이다. issuer만 바뀐다고 전부 무효화되지는 않음. 업무 포털 주소가 바뀌면 context와 주소별 로컬 자격증명이 자동 이전되지 않으므로 새 context를 등록·선택하고 로그인 |
| HTTP MCP OAuth 토큰 | issuer와 `<origin>/mcp` resource/audience 변경 영향을 받으므로 MCP 클라이언트의 URL·OAuth 연결을 갱신하고 재인증 |
| 기존 서버 빌드 영수증 | 로컬 `dist/image.json`의 영수증은 origin·context·사용자에 결합된다. 새 context로 전환한 개발 PC에서는 다시 `app build` 후 배포한다. 플랫폼 관리자의 기존 서버 이미지 재배포와 개발 PC의 영수증 검증을 구분하며 영수증을 임의 편집하지 않음 |
| 프로젝트 stdio MCP | 로컬 `miy mcp` 실행 구조 유지. CLI context를 새 주소로 로그인하고 프로젝트의 연결을 확인. 실행 파일 경로/번들까지 옮겼다면 `miy app setup`으로 프로젝트 설정 갱신 |

개발 PC에서 새 연결을 명시하는 예:

```bash
miy context add moved --url https://portal.example.com
miy --context moved auth login
miy --context moved doctor --for operate
```

사설 CA는 `context add`에 `--ca-file`을 추가한다. 프로젝트 binding이 옛 context를 가리키면 활성 context보다 우선하므로 새 context를 명시해 프로젝트 설정을 확인한다([CLI context 우선순위](RUNNING.md#서버-context와-인증)).

## 같은 사이트의 범위

HTTPS의 같은 등록 도메인 아래 서비스는 IP가 달라도 브라우저 기준 같은 사이트다. 이는 같은 출처라는 뜻이나 모든 host-only 쿠키를 읽을 수 있다는 뜻은 아니다. `__Host-` 쿠키와 Origin/Fetch Metadata 검사를 유지하되 SameSite만으로 형제 호스트의 요청을 막는다고 설명하지 않는다. 실제 운영 전 루트와 다른 하위 도메인을 누가 통제하는지 확인한다. 도메인 없는 포트 분리에서는 쿠키가 포트별로 분리되지 않는다는 제약도 유지된다. 자세한 지원 범위는 [앱 경계](APP-BOUNDARIES.md)와 [프레임 배치 프로필](APP-FRAMES.md#4-배치-프로필과-격리-수준)을 따른다.
