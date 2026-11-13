# 코어 권한 계약과 설치 전환

> 이력 전용 — `22c219d`의 docs/CORE-PERMISSIONS.md에서 보관한 당시 계약·계획이다. 새 구현의 지침이 아니다. [현재 경계 설계](../APP-BOUNDARIES.md)와 [현재 인수 기준](../APP-BOUNDARIES.md#경계-인수)을 따른다. 수치·결과는 당시 범위로만 해석한다.

- 작성일: 2026-11-03
- 소스 버전: CLI/core **0.7.0**, 개발 번들 **0.8.0**, Rust SDK **0.3.0**
- 범위: [코어 안정화 2차](../CORE-HARDENING.md)와 사용자·그룹 앱 권한([추진 계획](../PLAN.md#다음-우선-작업) 순서 2). 서비스 계정 발급은 포함하지 않는다.
- 운영 반영: 2026-11-03 v2.dwdcc.kr 서버·게시 CLI 0.7.0 / 번들 0.8.0 ([배포 기록](../releases/core-0.7.0-deployment.json)). 기존 설치는 모두 `database`만 요청해 추가 승인 대상이 없었다.

## 설치가 요청한 권한과 관리자가 승인한 권한

manifest의 `requestedPermissions`는 요구 사항이다. 다른 앱 호출과 공통 directory 조회는 **현재 사용자 권한 ∩ 위임 자격증명의 범위 ∩ 현재 릴리스의 요청 ∩ 해당 설치/릴리스의 승인 ∩ 대상 앱의 사용자 정책**을 모두 만족해야 한다.

승인 키는 `(installationId, releaseId)`다. dev와 prod, 서로 다른 설치, 새 릴리스 사이에 승인을 복사하지 않는다. 승인하지 않은 기능은 403 `installation_permission_denied`로 차단한다. 앱의 설치·배포 자체는 부분 승인 또는 승인 없음으로도 가능하다. 자체 DB의 준비·migration·연결은 기존 앱/환경 개발·배포 권한과 격리 정책을 따른다. `database`를 이 외부 호출 승인 목록에 추가해 다른 앱의 DB에 접근할 수는 없다.

`0016_installation_permissions.sql`은 기존 설치를 자동 승인하지 않는다. 실제 서버에 migration을 적용하면 기존 외부 연결도 명시적으로 승인하기 전에는 거부된다. 기존 앱 DB와 레코드는 수정하지 않는다. 관리자는 적용 전에 실제 사용하는 연결을 검토하고, 적용 후 정확한 설치·릴리스에 필요한 부분집합만 승인해야 한다. 이전 릴리스로 돌아가면 그 릴리스의 현재 승인 기록이 적용된다.

권한 API는 다음과 같다. `releaseId` 조회를 생략하면 현재 설치 릴리스를 조회한다.

```text
GET /api/v1/installations/{id}/permissions?releaseId={releaseId}
PUT /api/v1/installations/{id}/permissions
```

조회는 그 설치의 운영 기록 조회 판정(`Observe`: 사용 권한 또는 실행 관리 읽기)을 요구한다. 그래서 회사 사용 중지된 prod 앱도 관리자·배포 담당자는 승인 기록을 읽는다. 수정은 위임되지 않은 관리자와 `miy.dev`가 필요하다. 수정 응답은 커밋 전에 같은 트랜잭션에서 읽은 기록이며, 커밋 뒤에 다시 판정하지 않는다. 수정 본문은 정확한 릴리스·관찰한 revision·승인할 전체 목록이다. 빈 목록은 모두 철회한다.

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

## 사용자·그룹의 앱 권한과 회사 사용 중지

2026-11-03 소스(migration `0018_groups_and_app_access.sql`)는 [PRD](../PRD.md#앱-사용-권한과-앱-내부-데이터-권한)의 앱 사용 권한을 사용자와 그룹 두 주체로 넓힌다. 2026-11-04 운영 서버에 반영했다([배포 기록](../releases/core-review-2026-11-04-deployment.json)). 아래의 사용 중지 중 실행 관리와 관리자 재확인 보완은 2026-11-05 반영했다([배포 기록](../releases/core-runtime-management-2026-11-05-deployment.json)). 이후 `main` 변경의 운영 반영 여부는 [운영 안내](../SERVER-OPERATIONS.md#3-현재-배포-소스와-이-작업본의-차이)를 따른다.

- **정본:** 그룹과 주 소속의 정본은 코어 DB다. Keycloak은 인증만 맡는다.
- **그룹 종류:**
  - 조직(인사, `hr`) 그룹: 주 소속(`users.primary_group_id`)이 정확히 그 그룹인 사용자만 포함한다.
  - 수동(`local`) 그룹: 관리자가 직접 추가한 사용자만 포함한다.
  - 계층은 조직 그룹에만 있다. 상위·하위 조직이나 책임자에게 권한이 이어지지 않는다. 계층 변경은 잠금 후 최신 관계로 순환을 검사해 `409 group_hierarchy_cycle`로 거부한다.
- **그룹 권한:** `use`와 `develop`만 줄 수 있다. 운영 배포(`deploy`)는 사람별로만 부여하며, 그룹에 요청하면 `400 group_permission_not_allowed`다.
- **판정:** 권한 판정은 `effective_grants` 뷰(직접 권한 ∪ 현재 그룹 멤버십 ∪ 전체 허용)를 매번 계산한다. 그룹 권한을 사용자 권한 행으로 복사하지 않는다. 멤버십·주 소속·그룹 활성 상태가 바뀌면 다음 요청부터 반영되고, 대기 작업도 실행 시점에 같은 뷰로 다시 검사한다. 판정에서는 판정 모듈(`policy.rs`, 아래 "판정 구조")만 이 뷰를 읽는다. 대기 작업·개발 세션·DB 작업의 재확인도 이 모듈을 거친다. `/me`의 권한 목록은 같은 뷰를 표시만 한다.
- **입장 대상:** 앱·환경마다 `selected`(기본, 권한을 받은 사용자·그룹만)와 `all`(사용 중인 모든 사용자) 중 하나다. `all`은 명시적으로 고를 때만 적용된다.
- **회사 사용 중지:** 앱 사용 경로(화면 자산, 앱 API 호출, 앱 간 호출 대상)는 `403 app_disabled`로 차단한다. 일반·페이지 설치 목록과 앱 상세의 설치 목록에는 같은 정책을 적용해 사용 불가능한 설치를 제외한다. 관리용 앱 이름·ID 목록은 유지한다. 관리자도 운영(prod) 사용 중지를 우회하지 못한다. 다만 그 앱의 개발 권한이 있는 사용자는 테스트(dev) 환경에서 앱을 고칠 수 있다.
- **사용 중지 중 실행 관리:** 사용 중지는 사용을 막고 실행 관리는 막지 않는다. 시작·중지·재시작·삭제·배포와 그 작업 상태 조회는 기존 실행 권한(dev는 `develop`, prod는 `deploy` 또는 관리자)으로 계속할 수 있다. 사용 권한만 있는 사용자는 작업 상태도 `app_disabled`로 거부된다. 관리자 실행 관리 목록(`GET /api/v1/runtime/installations`, 위임되지 않은 관리자 전용)은 사용 중지된 앱의 설치도 `appDisabled`와 함께 돌려주고, 지금 열 수 있는지는 서버 판정 `canOpen`으로 알린다. 관리 화면 "앱 실행 상태"는 이 목록으로 배지를 달고, 열 수 없는 설치에는 "앱 열기"를 표시하지 않는다.
- **운영 기록 조회의 일치:** 작업 목록·상세·events와 설치의 외부 호출 승인 기록은 현재 실행 관리 권한 또는 앱 사용 권한으로 판정한다(`Observe`). 실행 관리 경로에는 `miy.dev` scope가 필요하고, 위임 자격증명에는 대상 앱·환경의 `read`가 필요하다. 회사 사용 중지된 prod 앱은 이 조건을 갖춘 관리자·배포 담당자만 작업과 승인 기록을 읽고, 사용 권한만 있는 사람의 목록에서는 제외한다. dev는 기존 관리자·개발자의 수정·조회 예외를 유지한다. 목록 cursor를 이어 읽어도 현재 권한과 회사 중지를 다시 검사한다. 이 보완은 소스에 적용했으며 운영 반영은 [운영 안내 §3](../SERVER-OPERATIONS.md#3-현재-배포-소스와-이-작업본의-차이)에서 구분한다.
- **판정 구조:** 앱 단위 판정은 `crates/miy/src/policy.rs` 한 곳이 한다.
  - 요청마다 사용자 활성·관리자, `effective_grants`, 회사 사용 중지, 위임 범위와 유효 세션을 **하나의 SELECT와 동일 DB 스냅샷**에서 읽고 자격증명의 `miy.dev`를 적용한다. HTTP 조회·위임 검사·대기 작업 재확인이 같은 읽기를 사용한다. 과거의 활성 상태와 비활성화 뒤 새로 부여된 권한을 합쳐 승인하지 않는다.
  - 동작별 순수 판정으로 결정한다. 동작은 사용(`Use`), 개발 쓰기(`Develop`), 실행·운영 관리(`Manage(동작)`), 운영 기록 조회(`Observe`: 실행 작업·설치 승인 기록), 기록이 남는 요청 접수(`Request(동작)`: 실행·DB 작업, 시험 결과 기록)다. 호출부는 동작을 지정할 뿐 판정 함수를 고르지 않는다.
  - 상세 판정, 목록(앱·설치·작업)의 노출 범위, 설치 목록의 표시값이 모두 이 판정에서 나온다. 목록 SQL에는 권한 조건이 없고, 판정이 허용한 앱·환경 목록으로만 거른다.
  - 대기 작업(실행·DB·빌드·이미지 적재·로그 요청·개발 세션)은 시작 전과 긴 대기(잠금, 컴파일, 이미지 적재) 뒤에 `recheck_queued` 하나로 다시 판정한다. 저장된 행위자의 활성 상태와 환경 관리 권한(관리자, dev `develop`, prod `deploy`), 저장된 위임의 범위를 본다. 접수 때의 `Manage(동작)`에서 자격증명의 `miy.dev`만 뺀 판정이다.
  - 확정 거부는 `user_disabled` 또는 `permission_revoked`로 기록한다. 권한을 읽지 못하면 `database_unavailable`(로그 요청은 `diagnostic_unavailable`)이고 회수로 분류하지 않는다. 거부된 DB 작업은 실행한 것이 없으므로 설치의 DB 상태를 바꾸지 않는다.
  - 쓰기는 `policy::Write`로만 커밋한다. 쓰기와 잠금을 먼저 하고 같은 트랜잭션 안에서 판정하며, 판정을 통과한 트랜잭션만 커밋할 수 있다. 앱 단위 쓰기(실행·DB 작업과 시험 결과 접수, 릴리스·설치 생성, 이미지 적재·로그 요청·개발 세션·빌드 접수, 앱 호출 토큰, 위임 발급)는 위 동작으로, 관리자 변경(아래 "관리")과 앱 등록 요청은 플랫폼 동작으로 판정한다.
  - 판정 전에 행위자의 사용자 행→위임 행 순으로 공유 잠금을 얻고, 잠금 대기가 끝난 뒤 새 SELECT로 사실 전체를 읽는다. 잠금은 커밋까지 유지하므로 사용자 비활성화·관리자 회수·위임 철회는 진행 중인 쓰기가 끝날 때까지 기다린다. 위임 발급은 사용자→부모 세션 순으로 먼저 잠그고 마지막 판정도 유지해, 사용자 비활성화의 세션 회수와 교착되지 않게 한다. 권한·그룹 변경은 기다리지 않는다. 판정 직후 회수되면 그 쓰기는 커밋되고, 대기 작업은 실행 전에 다시 확인한다.
  - 인증 흐름, 본인 자격증명·임대 회수, 읽기 전용 스냅샷, worker 기록은 `scripts/check-write-decisions.py`의 목록에 이유와 함께 둔다. 이 검사(`check-static.sh rust`)는 목록 밖에서 트랜잭션을 직접 열거나, 주체를 받는 함수가 `Write` 없이 쓰면 실패한다.
- **표시값:** 서버가 계산하는 안내 값이며, 실제 요청은 서버가 다시 판정한다.
  - `canOpen`은 사용 판정이다. `canManage`는 관리 읽기(`Manage(read)`), `canPromote`는 prod 승격 요청 접수(`Request(promote)`) 판정이다. `acceptance`는 `canManage`일 때만 담는다.
  - 위임 자격증명도 같은 판정을 따른다. 범위에 `read`가 있고 해당 환경의 관리 권한이 있으면 `canManage`가 참이다. `canPromote`는 위임 범위의 `promote`·`read`에 더해 실제 `deploy` 권한과 `miy.dev`까지 본다. 이전에는 위임이면 `canManage`가 항상 거짓이었고, 페이지 목록의 `canPromote`는 위임 범위만 봤다.
- **관리:** 위임되지 않은 관리자만 바꿀 수 있다. 그룹·앱 정책·개인 권한 변경, 사용자 활성 상태 변경, 앱 등록 승인·거절, 앱 생성·표시 이름 변경, 공용 컴포넌트 게시는 커밋 전에 현재 `admin AND enabled`를 다시 확인하고 사용자 행 잠금을 커밋까지 유지한다. 대상 잠금을 기다리는 동안 관리자 권한이 회수됐다면 변경과 감사 기록을 함께 취소한다. 주 소속·사용자 활성 상태 변경은 행위자·대상 사용자 행을 같은 ID 순서로 잠근다. 사용자 활성 상태는 변경 전에 확인하므로 관리자가 자신의 계정을 비활성화할 수 있다(마지막 활성 관리자는 제외).
- **감사:** 개인 권한(`grant.enabled`·`grant.disabled`. 앱 등록 승인으로 요청자에게 생기는 dev `develop` 포함), 그룹·그룹 권한(`group.*`·`group_grant.*`), 사용자 활성 상태(`user.enabled`·`user.disabled`)와 주 소속(`user.primary_group_set`), 앱 정책(`app.admission_set`·`app.disabled`·`app.enabled`) 변경을 같은 트랜잭션에 기록한다. 개인 권한에는 사용자·앱·환경·권한, 사용자 활성 상태·주 소속에는 변경 전후 값을 남긴다. 플랫폼 관리 화면의 "사용자·그룹 권한"에서 다룬다.

```text
GET|POST /api/v1/groups                  PATCH /api/v1/groups/{id}
PUT /api/v1/groups/{id}/parent            GET|PUT /api/v1/groups/{id}/members
PUT /api/v1/users/{id}/primary-group      GET|PUT /api/v1/group-grants
GET /api/v1/apps/{app}/access             PUT /api/v1/apps/{app}/admission
PUT /api/v1/apps/{app}/control            GET /api/v1/runtime/installations
```

감사 기록은 `GET /api/v1/audit`(관리자 전용)로 기간·행위자·동작 접두어(예: `grant.`, `group.`, `app.`)를 걸러 최신순으로 조회한다. `nextCursor`로 이전 기록을 이어 읽는다. 요청 미들웨어 기록은 `METHOD:status`와 경로, 도메인 변경은 위 동작 이름이다. 플랫폼 관리 화면의 "감사 기록"은 검색 조건과 해당 결과의 cursor를 함께 관리하며 새 검색이 실패하면 이전 페이지를 이어 붙이지 않는다. migration `0019_audit_query.sql`은 행위자·시각 인덱스만 추가한다. 별도 대상 종류·결과·요약 필드와 보존 정책은 V1-A1의 후속 범위다. 보존 기간은 PRD 미결정 사항이라 아직 삭제하지 않는다.

격리 회귀는 실제 PostgreSQL·HTTP로 아래 규칙을 확인한다. 보완 소스의 실행 결과는 [리뷰 후 검증 기록](../releases/core-review-2026-11-04-validation.json), 이후 [관리자 재확인 보완 기록](../releases/core-admin-recheck-2026-11-04-validation.json)·[실행 관리 목록 기록](../releases/core-runtime-management-2026-11-04-validation.json)·[위임/작업 조회 후속 보완](../releases/core-review-followup-2026-11-05-validation.json)·[판정 불일치 수정](../releases/core-policy-consistency-2026-11-05-validation.json)에 남긴다.
- 멤버 해제 즉시 회수, 복사된 권한 없음
- 조직 정확 일치와 상속 없음, 순환 거부
- `deploy` 거부와 develop의 dev 한정
- `all` 입장 대상의 명시 선택과 해제
- 관리자도 막는 회사 사용 중지와 개발자의 dev 수정
- 사용 중지 중 관리자 실행 관리 목록(`appDisabled`·`canOpen`, 비관리자 거부)과 격리 앱 컨테이너 중지·시작·작업 조회(사용 권한만 있는 사용자는 거부)
- 위임된 `read+stop`의 HTTP 접수·동일 키 재시도, `read` 누락 시 저장 전 거부, 상세/events/목록의 일치, 기존 cursor의 회사 중지·권한 회수 재검사, `miy.dev` scope 조합. 이 추가 시험의 합성 앱에는 worker를 실행하지 않음
- 관리자 전용과 감사 기록(조회 API의 행위자·동작 필터와 cursor 포함)
- 잠금 대기 중 관리자 권한 회수(사용자 활성 상태 변경·앱 등록 승인·앱 생성·이름 변경·컴포넌트 게시 포함), 변경·감사 기록의 원자성, 일반·페이지 설치 조회의 중지 정책 일치
- 잠금 대기 중 개발 권한 회수(설치 생성·DB 준비·위임 발급), 판정된 쓰기의 커밋까지 사용자 비활성화 대기
- 회사 사용 중지된 prod 설치의 승인 기록 조회·변경(관리자), 주체별 승인 기록·작업 조회와 운영 기록 조회 판정의 일치

## CLI·MCP의 사용자 위임

일반 로그인으로 얻은 사용자 자격증명과, 그 사용자가 특정 작업에 발급한 위임 자격증명을 구분한다. 위임은 하나의 앱·환경과 정확한 action 목록, 이름, 60~86,400초의 고정 유효기간을 가진다. 사용자당 활성 위임은 최대 32개다. refresh는 만료 시점이나 범위를 늘리지 않는다. 부모 CLI 세션이 더 일찍 만료되면 그 만료를 넘지 못한다.

지원 action:

```text
read app_read app_write directory_read build upload dev_session prepare_database
accept deploy promote start stop restart remove logs
```

`read`는 상태·설치·작업 조회의 별도 권한이다. 일반 작업 흐름에 관찰이 필요하면 함께 지정한다. `start`가 `stop`이나 `remove`를 허용하지 않는다. 관리자가 발급해도 지정한 앱·환경·action을 벗어날 수 없다. 위임 자격증명으로 다시 위임하거나 설치 권한을 승인하고, 계정·전역 관리·등록 API를 호출할 수 없다. 단일 앱 위임으로 다른 앱의 API 접근 범위까지 얻는 것은 지원하지 않는다.

실행 작업 요청은 접수한 작업의 상태를 응답하므로 실행 action과 `read`를 함께 부여해야 한다. preflight와 POST 모두 이를 검사하고, `read`가 없으면 작업을 저장하기 전에 거부한다. 접수 응답은 커밋 전에 읽은 작업으로 만들어 커밋 후 별도 권한 조회 때문에 403이 되는 경우를 없앤다. 후속 상세·events·목록 조회는 요청마다 현재 위임과 사용자 권한을 다시 검사한다. DB 준비(`POST /api/v1/database/prepare`)도 같은 규칙이다. 접수 응답이 저장된 DB 작업이므로 `prepare_database`와 `read`가 함께 필요하고, `read`가 없으면 작업을 저장하기 전에 거부한다. 작업 저장과 판정은 한 트랜잭션이며, 응답은 그 트랜잭션에서 읽은 작업이다. 시험 결과 기록(`POST /api/v1/apps/{app}/test-acceptance`)도 남는 기록이 실행 관리 정보(`canManage`일 때만 보이는 `acceptance`)라서 `accept`와 `read`가 함께 필요하다. 실행 잠금과 앱 상태 확인을 기다린 뒤 기록 직전에 다시 판정한다. 개발 세션 생성 응답 복구(`GET /api/v1/apps/{app}/dev-sessions/by-request/{request}`)는 세션 조회와 같은 판정(`dev_session`)을 한다. 위임 자격증명은 관리자 권한을 갖지 않는다. 관리자 API뿐 아니라 권한 목록(`GET /api/v1/grants`)에서도 위임된 관리자는 자기 권한만 본다. 앱·설치·작업의 앱 단위 판정에서는 위임 범위 안에서 사용자의 관리자 권한을 그대로 쓴다.

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

실행 결과와 검증 소스는 최종 [코어 권한 검증 기록](../releases/core-permissions-2026-11-03-validation.json)에 기록한다. 실제 Windows 실행과 새 호스트·운영 배포는 이번 검증에 포함하지 않는다. 다단계 Core 정책은 격리 PostgreSQL/HTTP 및 서명 검증 backend fixture로, SDK HTTP 전송은 실제 loopback TCP 검사로 구분한다. 두 검사를 운영 Docker 네트워크의 전체 연결 인수로 합산하지 않는다.
