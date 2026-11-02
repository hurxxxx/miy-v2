# 코어 권한 계약과 설치 전환

- 작성일: 2026-11-03
- 소스 버전: CLI/core **0.7.0**, 개발 번들 **0.8.0**, Rust SDK **0.3.0**
- 범위: [코어 안정화 2차](CORE-HARDENING.md). miy-v1, UI/UX, 서비스 계정 발급은 포함하지 않는다.
- 운영 반영: 별도. 현재 게시 서버·CLI는 여전히 0.6.3 / 번들 0.7.1이다.

## 설치가 요청한 권한과 관리자가 승인한 권한

manifest의 `requestedPermissions`는 요구 사항이다. 다른 앱 호출과 공통 directory 조회는 **현재 사용자 권한 ∩ 위임 자격증명의 범위 ∩ 현재 릴리스의 요청 ∩ 해당 설치/릴리스의 승인 ∩ 대상 앱의 사용자 정책**을 모두 만족해야 한다.

승인 키는 `(installationId, releaseId)`다. dev와 prod, 서로 다른 설치, 새 릴리스 사이에 승인을 복사하지 않는다. 승인하지 않은 기능은 403 `installation_permission_denied`로 차단한다. 앱의 설치·배포 자체는 부분 승인 또는 승인 없음으로도 가능하다. 자체 DB의 준비·migration·연결은 기존 앱/환경 개발·배포 권한과 격리 정책을 따른다. `database`를 이 외부 호출 승인 목록에 추가해 다른 앱의 DB에 접근할 수는 없다.

`0016_installation_permissions.sql`은 기존 설치를 자동 승인하지 않는다. 실제 서버에 migration을 적용하면 기존 외부 연결도 명시적으로 승인하기 전에는 거부된다. 기존 앱 DB와 레코드는 수정하지 않는다. 관리자는 적용 전에 실제 사용하는 연결을 검토하고, 적용 후 정확한 설치·릴리스에 필요한 부분집합만 승인해야 한다. 이전 릴리스로 돌아가면 그 릴리스의 현재 승인 기록이 적용된다.

권한 API는 다음과 같다. `releaseId` 조회를 생략하면 현재 설치 릴리스를 조회한다.

```text
GET /api/v1/installations/{id}/permissions?releaseId={releaseId}
PUT /api/v1/installations/{id}/permissions
```

조회는 해당 설치의 사용자 권한을 요구한다. 수정은 위임되지 않은 관리자와 `miy.dev`가 필요하다. 수정 본문은 정확한 릴리스·관찰한 revision·승인할 전체 목록이다. 빈 목록은 모두 철회한다.

```json
{
  "releaseId": "검토한 릴리스 UUID",
  "expectedRevision": 0,
  "permissions": ["platform:directory:read", "app:inventory:read"]
}
```

동시 수정은 한 명만 성공하며 나머지는 409 `permission_revision_changed`다. 현재 승인과 변경 이력은 같은 transaction으로 저장한다. 이력에는 주체·시각·revision·승인 목록을 기록하며 조회 응답은 최근 100개를 반환한다. 요청하지 않은 권한, 잘못된 대상 릴리스, 중복 권한은 거부한다.

```sh
miy --context platform permissions get --installation INSTALLATION_UUID
miy --context platform permissions set --installation INSTALLATION_UUID --input @approval.json
```

HTTP MCP의 `get_installation_permissions`, `set_installation_permissions`도 같은 정책과 승인 이력을 사용한다. 로컬 stdio MCP는 조회·안내 도구를 제공하며 변경은 명시적인 CLI 명령으로 수행한다.

## CLI·MCP의 사용자 위임

일반 로그인으로 얻은 사용자 자격증명과, 그 사용자가 특정 작업에 발급한 위임 자격증명을 구분한다. 위임은 하나의 앱·환경과 정확한 action 목록, 이름, 60~86,400초의 고정 유효기간을 가진다. 사용자당 활성 위임은 최대 32개다. refresh는 만료 시점이나 범위를 늘리지 않는다. 부모 CLI 세션이 더 일찍 만료되면 그 만료를 넘지 못한다.

지원 action:

```text
read app_read app_write directory_read build upload dev_session prepare_database
accept deploy promote start stop restart remove logs
```

`read`는 상태·설치·작업 조회의 별도 권한이다. 일반 작업 흐름에 관찰이 필요하면 함께 지정한다. `start`가 `stop`이나 `remove`를 허용하지 않는다. 관리자가 발급해도 지정한 앱·환경·action을 벗어날 수 없다. 위임 자격증명으로 다시 위임하거나 설치 권한을 승인하고, 계정·전역 관리·등록 API를 호출할 수 없다. 단일 앱 위임으로 다른 앱의 API 접근 범위까지 얻는 것은 지원하지 않는다.

```sh
miy --context platform auth delegations create --app inventory --environment dev \
  --action read --action build --action deploy --name inventory-agent \
  --expires-in 3600 --credential-file ./private/inventory-agent.json
miy --context platform auth delegations list
miy --context platform auth delegations revoke DELEGATION_UUID
```

비밀은 표준 출력에 표시하지 않고 지정한 비공개 파일에 저장한다. 대상 작업은 별도 `MIY_CONFIG_DIR`에 같은 서버 context를 만든 뒤 `auth delegations import --credential-file ...`로 가져온다. 기존 사용자 credential을 덮어쓰지 않는다. 가져오기 후 원본 비밀 파일은 자동 삭제하지 않으므로 사용자가 필요 기간만 보관한다. 파일이나 토큰을 Git·문서·채팅·MCP 도구 응답에 복사하지 않는다.

로컬 stdio MCP는 프로젝트 CLI의 동일한 credential을 사용한다. 직접 HTTP MCP도 Core `miy_access_` credential을 인식하며 기존 OIDC 인증 경로도 유지한다. 서버는 요청마다 위임 만료·철회, 현재 사용자 상태·grant를 다시 확인한다. 목록과 cursor도 위임된 앱·환경으로 제한한다.

runtime·DB 준비·서버 빌드·이미지 업로드·로그 작업에 원래 `delegation_id`를 저장한다. 다른 위임으로 같은 요청 식별자를 재전송해 권한을 바꾸지 못한다. worker는 실행·복구 시 현재 위임을 확인하고 빌드는 결과 게시 직전에도 확인한다. 개발 DB 세션의 heartbeat·연결도 부모 세션과 위임을 재검사한다. 위임 철회, 부모/발급된 CLI 세션 철회, refresh 재사용, 사용자 비활성화·grant 회수·만료가 제한에 반영된다.

이미 완료한 외부 부작용을 되돌리거나 진행 중인 모든 작업을 즉시 강제 취소하는 기능은 아니다. 일반 사용자 credential로 접수한 기존 작업은 종전의 사용자/grant 재검사 정책을 유지한다. 서비스 계정과 사용자 위임의 재위임은 별도 설계 범위다.

## 앱 간 호출의 원래 사용자와 호출 경로

앱에 전달하는 서명된 identity에는 원래 사용자, 수신 설치·릴리스, 환경, 호출 앱들의 설치·릴리스 순서와 유효한 외부 권한을 포함한다. 브라우저가 보낸 identity·사용자·호출자 헤더를 전달하지 않는다.

앱이 가진 `MIY_GATEWAY_SECRET`은 자기 앱에서 Core의 요청을 확인하는 용도다. 앱이 이 키로 만든 JWT를 Core 인증이나 다른 사용자 위임으로 사용할 수 없다. 후속 호출에는 Core가 별도로 발급하고 hash만 저장한 `miy_call_` 증표가 필요하다. 이 증표는 원래 만료를 연장하지 않고 최대 30초 동안 유효하다.

후속 호출은 증표를 받은 설치의 `connections/.../api/...`와 읽기 directory 경로에만 허용한다. 직접 대상 API·권한 승인·작업·관리 API에는 사용할 수 없다. 각 단계는 원래 사용자 권한, 원본 CLI/browser/legacy 세션, 모든 이전 설치의 현재 릴리스·승인 및 이전 호출에 사용한 권한을 다시 검사한다. 원본 로그아웃이나 중간 앱 권한 철회도 기존 증표의 다음 사용을 거부한다. IdP 변경을 Core 사용자 상태에 자동 동기화하는 기능은 아니다.

다음 단계의 유효 권한은 이전 유효 권한과 수신 앱의 승인 권한의 교집합이다. 예를 들어 A→B→C 호출에는 A와 B 모두 C 호출 권한을 요청·승인받아야 한다. B만 가진 더 강한 권한으로 A를 대신할 수 없다. 순환 호출과 8 hop을 넘는 확장은 거부한다. 한 사용자당 살아 있는 증표는 1,000개로 제한하고 만료된 증표는 발급 중 정리한다.

요청 본문은 최대 4MiB·15초로 수집한 뒤 실제 전달 직전에 권한과 호출 경로를 다시 검사한다. 이미 앱으로 전달한 요청의 롤백까지 보장하지는 않는다.

Rust SDK의 `Identity.context`로 검증된 호출자 정보를 읽는다. `Identity::app_api`와 `platform_api("directory")`는 비공개 증표와 현재 release를 붙여 `MIY_CORE_ORIGIN`으로 요청한다. 런타임은 서버의 명시적인 origin을 전달한다. 기본 HTTPS이며 서버가 HTTP를 명시적으로 허용한 경우에만 `MIY_CORE_ALLOW_HTTP=1`을 전달한다. redirect는 따라가지 않고 오류·Debug 출력에 증표나 응답 body를 포함하지 않는다. 실제 설치 환경에서는 앱 컨테이너에서 이 origin에 접근할 수 있어야 한다.

로컬 preview의 외부 API도 승인된 dev 설치를 요구한다. 로컬 manifest만으로 권한을 늘리지 않는다. 아직 dev 설치가 없으면 외부 연결을 사용할 수 없다. preview 코드가 배포된 릴리스와 같다고 주장하는 후속 증표는 발급하지 않는다.

## SDK 전환

새 계약은 번들 0.8.0 / Rust SDK 0.3.0이다. 0.7.1 snapshot을 보존하며 기존 앱은 명시적인 `miy app upgrade`로 전환한다. 앱의 사업 코드와 적용된 SQL 바이트를 보존한다. staged SDK 의존성 갱신 후 기존 SDK 외 패키지의 버전·출처·checksum이 모두 유지될 때에만 protected SDK와 Cargo.lock을 같은 파일 transaction으로 적용한다.

외부 path/workspace/patch 또는 custom Cargo 설정처럼 이 제한된 전환에서 안전하게 해석하지 못하는 구성이 있으면 기존 파일·실행 파일을 유지하고 거부한다. 이전 앱 SDK도 추가 identity 필드를 무시하여 기존 사용자 identity를 받을 수 있으나, 새 backend 호출 기능은 새 SDK가 필요하다. 새 CLI/SDK가 운영에 게시되었다고 해석하지 않는다.

## 검증 기록

실행 결과와 검증 소스는 최종 [코어 권한 검증 기록](releases/core-permissions-2026-11-03-validation.json)에 기록한다. 실제 Windows 실행과 새 호스트·운영 배포는 이번 검증에 포함하지 않는다. 다단계 Core 정책은 격리 PostgreSQL/HTTP 및 서명 검증 backend fixture로, SDK HTTP 전송은 실제 loopback TCP 검사로 구분한다. 두 검사를 운영 Docker 네트워크의 전체 연결 인수로 합산하지 않는다.
