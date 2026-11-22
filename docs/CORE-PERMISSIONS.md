# 코어 권한 계약과 설치 전환

- 정리일: 2026-11-15. **AB-05 정책과 prod/dev/console 세 용도, 설치별 앱 프레임의 권한 집행을 구현·격리 검증·개발 서버에 반영했다.** 현재 배포 소스는 [운영 안내](SERVER-OPERATIONS.md), 작업 상태와 순서는 [PLAN](PLAN.md)이 관리한다.
- 이 문서는 권한 판정의 정본이다. 출처·세션·실행·호환 전환은 [앱 경계 설계](APP-BOUNDARIES.md)가 담당한다.
- 변경 전 소스 `22c219d`의 계약은 [소스 스냅샷(CLI/core 0.7.0)](archive/CORE-PERMISSIONS-0.7.0.md)에 보존한다. 아래의 변경되지 않은 설치 승인·위임·원래 호출자 계약은 재사용한다. AB-05 최초 구현의 실행 범위는 [초기 구현·검증 기록](releases/internal-minimum-security-implementation-2026-11-06-validation.json)에 기록했다.

사내에서 회사가 관리하는 앱을 대상으로 업무 앱 UI는 설치별 출처 iframe에서([앱 프레임 설계](APP-FRAMES.md)), 플랫폼 화면은 통합 라우팅으로 실행하고 단일 IP HTTPS를 유지한다. prod 업무 포털·dev 포털·관리/CLI 승인/Keycloak 콘솔의 세 출처와 `portal | dev_portal | console` 브라우저 용도를 서버가 검사한다. 각 포털 세션은 자기 환경의 앱 사용만 허용하고 콘솔 세션은 앱 업무 인증에 쓰지 않는다. AB-05의 prod 사용/관리 분리와 아래 권한 계약을 유지한다. 공통 ACL은 사용자·업무 자원의 권한을 다루며 브라우저의 출처 격리를 대신하지 않는다.

현행 권한 코드를 수정하면 아래 "현행 소스와 차이" 열을 갱신하고 해당 source commit과 실행 기록을 연결한다. archive는 `22c219d` 당시 고정본이며 덮어쓰지 않는다. 버전 문자열만으로 소스·검사·배포 범위가 같다고 판단하지 않는다. 실제 배포 상태는 운영 안내가 관리한다.

## 확정 권한 행렬

모든 허용은 사용자 활성·자격증명 용도/범위·현재 앱/환경 정책과 대상 자원 조건의 교집합이다. UI·SDK가 판단을 대신하지 않는다. 아래 표는 현행 구현 계약이며 게시 CLI·보관 소스 스냅샷과 구분한다. 2026-11-12 프레임 최초 도입부터 브라우저의 앱 업무 API·연결·디렉터리·UI 산출물은 포털 쿠키가 아니라 앱 프레임 토큰으로만 호출한다([APP-FRAMES](APP-FRAMES.md) §3).

| 행위 | 확정 계약 | 현행 소스와 차이 |
| --- | --- | --- |
| prod 업무 앱 입장 | 명시적 유효 use 또는 전체 허용 정책. 관리자·deploy 자동 입장 없음. 회사 중지는 누구도 우회하지 못함 | 운영 반영. prod는 명시적 use/all만 허용하며 관리자 권한을 use로 자동 복사하지 않음 |
| dev 미리보기·시험 | 관리자 또는 해당 앱의 dev develop은 **dev 포털**에서 개발 미리보기, 명시적 use@dev는 시험 입장. 회사 중지 중 수정 예외는 관리자/개발자만. 포털 세션은 prod Use만, dev 포털 세션은 dev Use만 판정한다(2026-11-09 운영 반영) | console 세션은 업무 인증에 사용하지 않는다. 실제 dev 포털 주소는 [운영 안내](SERVER-OPERATIONS.md)를 따른다 |
| 보안 조건 미충족 설치의 실행 | 실제 위험 계약이 남은 경로만 제한하고 앱 ID·DB·파일을 보존. 담당자는 기존 dev 설치·CLI/위임과 승격 흐름으로 보완 | 전 앱 보류 게이트·SDK 일괄 전환을 추가하지 않음. MF 자체는 차단 사유가 아니며 제한이 필요한 경우만 범위·해제 조건 기록 |
| 앱 운영 관리 | dev develop, prod deploy 또는 관리자. 자격증명 scope·위임 action을 교차 검사 | 기존 중앙 정책 재사용 |
| 운영 작업·events·설치 승인 상세 | Manage(read) 필요. Use만으로 허용하지 않음 | 소스 반영. Observe=Manage(read), Use만으로 조회 거부 |
| 릴리스 소스 다운로드·복구 | 해당 앱의 dev `Develop`과 일반 CLI 로그인 | `app.release-source.v1` 후보. 업무용 앱 세션과 위임 자격증명은 거부하며, 복구 업로드 완료 직전 현재 권한을 다시 확인한다 |
| 실제 런타임 로그 | Manage(logs) | 기존 제한 유지 |
| 시작·중지·승격·삭제 등 접수 | 해당 action과 관리 read, 사전 상태/revision 조건. 표시값은 서버 판정, 실행 전 재검사 | canManage 하나로 모든 변경을 허용하지 않음 |
| 담당 앱 목록 | 앱·환경별 관리 read. 사용 중지·미설치 앱 포함 | 소스 반영. 관리 read로 설치 목록을 필터링하고 미설치 앱도 콘솔에 표시 |
| 전역 권한·사용자·그룹·설치 승인 변경 | 위임되지 않은 현재 관리자와 기존 scope 조건. 같은 트랜잭션에서 검사·감사 | 앱 담당자에게 자동 부여하지 않음 |
| 개인 CLI 인증 승인 | 본인 계정의 콘솔 인증 경로. 콘솔 사용 자체는 전역 관리 권한이 아님 | device 승인만 console에서 허용하고 portal/dev_portal 승인 거부. legacy 로그인은 410 종료, 일반 직원도 본인 device 승인 가능 |
| 앱 내부 공간·자원 | 해당 앱의 원래 사용자 ACL | 플랫폼 admin을 앱 owner로 자동 변환하지 않음 |
| 본인 개인 설정·브라우저 세션 | 위임되지 않은 활성 사용자가 본인 기록만(`PlatformAction::Personal`). 세 출처 모두 허용하고 쿠키 요청은 CSRF를 요구한다. 표시 이름 변경은 감사에 남는다. 현재 세션은 목록에서 끝낼 수 없고 로그아웃으로 끝낸다 | 소스 반영(미배포). `GET /api/v1/me`의 `preferences`, `PATCH /api/v1/me/preferences`, `GET /api/v1/me/sessions`, `DELETE /api/v1/me/sessions/{id}`. 설정 값(고정 앱·고정 패널 포함)은 표시용이며 판정 입력이 아니다 |
| 런처 분류 | 읽기는 위임되지 않은 사용자 누구나(세 출처). 만들기·바꾸기·지우기·배치는 콘솔의 위임되지 않은 활성 관리자(`Administer`)이며, 존재하는 앱 ID만 한 분류에 하나씩 넣는다. 변경은 감사에 남는다. 분류는 앱을 묶어 보여 줄 뿐 입장 권한을 주지 않는다 | 소스 반영(미배포). `GET·POST /api/v1/launcher/categories`, `PATCH·DELETE /api/v1/launcher/categories/{id}`, `PUT /api/v1/launcher/categories/layout`. 관리자 판정을 앱 존재 확인보다 먼저 한다. 전체 배치 저장은 조회로 받은 `revision`을 `expectedRevision`으로 보내며, 다른 변경을 먼저 저장했다면 409로 돌려준다 |

그룹에는 use·develop만, prod deploy는 사람별로 부여한다. 계층·상위 조직·책임자에게 권한을 자동 상속하지 않는다. 플랫폼 관리자의 prod 사용도 명시적 use 또는 전체 허용이 필요하다. 관리자가 자신의 use를 부여할 때에도 감사를 남기며 그 행위가 앱 내부 ACL을 부여하지는 않는다. 기존 관리자를 use로 자동 복사하지 않는다. 외부 지원자의 배포/호스트 권한은 데이터에 영향을 줄 수 있는 신뢰 경계이며 이 행렬만으로 데이터 기밀성을 보장하지 않는다.

**사용자 전환(M6, 소스 반영·미배포).** 콘솔의 위임되지 않은 활성 관리자(`Administer`, Origin·CSRF)가 사용자 화면에서 "이 사용자로 로그인"을 누르면 그 사용자의 업무 포털 또는 dev 포털 세션이 생긴다. 이후 판정은 그 사용자의 일반 세션과 같으며 관리자에게 use나 앱 ACL을 부여하지 않는다. 위 표의 '관리자 자동 입장 없음'과 'admin을 앱 owner로 변환하지 않음'의 명시적 예외다.

- 시작: `POST /api/v1/users/{id}/sign-in-as` `{environment}`가 60초짜리 1회용 코드를 만들고 관리 감사에 `user.sign_in_as`(관리자·대상·환경)를 남긴다. 본인, 없는 사용자, 사용 중지된 사용자, 비밀번호 재설정 중인 사용자는 거절한다.
- 세션: 콘솔은 대상 포털의 `/switch-user`를 열어 코드를 주소 조각으로 전한다. 포털 화면이 주소에서 코드를 지운 뒤 같은 출처의 `POST /auth/switch`로 보내며, Core는 발급한 용도의 포털에서만 코드를 한 번 쓴다. 브라우저에서 그 포털에 이미 로그인한 세션을 끝내고 대상 사용자의 일반 세션(8시간, 같은 CSRF·로그아웃)을 만든다. 세션에는 시작한 관리자(`impersonator_user_id`)를 기록한다.
- 끝: 대상 사용자를 중지하거나 시작한 관리자를 중지·강등하는 트랜잭션에서 전환 세션과 남은 코드를 회수한다. 대상 계정의 비밀번호 재설정이 진행 중이면 프레임·브라우저 후속 호출과 포털 알림 채널에서 기존 세션 사용을 거부하며, 재설정 완료 이전에 발급된 대상 계정 전환 코드도 거부한다. 각 경로는 부모 세션과 전환을 시작한 관리자의 현재 상태를 확인한다. 전환 세션 로그아웃은 Core에서 끝난다.
- 같은 포털 출처의 열린 탭은 전환 성공 알림을 받은 뒤 Core에서 본인 세션을 다시 읽는다. 탭 간 알림에는 사용자 데이터나 전환 코드가 없다.

## 설치가 요청한 권한과 관리자가 승인한 권한

고장 신고 후보는 익명 접수만 별도 공개한다. 내용 조회·상태/메모 변경은 현재 활성 관리자(`Administer`)이며 앱 develop/deploy나 일반 사용자에게 허용하지 않는다. 변경은 기존 `policy::Write`의 트랜잭션 재확인·감사를 사용한다. [신고 접수와 관리 계약](SUPPORT-REPORTS.md).

manifest의 `requestedPermissions`는 요구 사항이다. 지원하는 권한은 `database`, `platform:directory:read`, `platform:files:read`·`write`, `platform:notifications:write`, `app:<앱>:read`·`write`, 그리고 프레임 장치 권한 `frame:clipboard-write`·`frame:fullscreen`(승인되면 그 설치의 프레임 iframe `allow`로만 쓰인다, [APP-FRAMES §3.5](APP-FRAMES.md#35-헤더와-iframe-속성))이다. 인증된 backend `connections` 경로의 다른 앱 호출·directory 조회·소유 앱 Storage는 **현재 사용자 권한 ∩ 위임 자격증명 범위 ∩ 현재 릴리스 요청 ∩ 해당 설치/릴리스 승인 ∩ 대상 앱 사용자 정책**을 모두 만족해야 한다.

앱 프레임의 업무 API는 프레임 토큰으로 발신 설치를 증명한다. 프레임은 자기 설치의 API만 직접 부르고, 다른 앱은 A의 설치 승인을 거친 연결(`callers=[A]`)로만 호출한다. 포털 쿠키는 업무 API에 쓰이지 않는다. 2026-11-12 운영 반영 전의 같은 페이지 MF에서는 발신 앱을 증명하지 못했다. 이 한계와 사내 승인 앱 신뢰는 APP-BOUNDARIES를 따르며 backend 연결 검증의 PASS를 브라우저 발신 앱 격리로 확대하지 않는다.

승인 키는 `(installationId, releaseId)`다. dev와 prod, 서로 다른 설치, 새 릴리스 사이에 승인을 복사하지 않는다. 승인하지 않은 기능은 403 `installation_permission_denied`로 차단한다. 앱의 설치·배포 자체는 부분 승인 또는 승인 없음으로도 가능하다. 자체 DB의 준비·migration·연결은 기존 앱/환경 개발·배포 권한과 격리 정책을 따른다. `database`를 이 외부 호출 승인 목록에 추가해 다른 앱의 DB에 접근할 수는 없다.

계획된 변경(미구현): 알림 발행 권한 `platform:notifications:write`를 허용 목록에 추가한다. 지금은 `unsupported_permission`으로 거부된다([범용 기능 계획 M2](COMMON-FEATURES-PLAN.md#m2-플랫폼앱-연결과-알림)).

`0016_installation_permissions.sql`은 기존 설치를 자동 승인하지 않는다. 실제 서버에 migration을 적용하면 기존 외부 연결도 명시적으로 승인하기 전에는 거부된다. 기존 앱 DB와 레코드는 수정하지 않는다. 관리자는 적용 전에 실제 사용하는 연결을 검토하고, 적용 후 정확한 설치·릴리스에 필요한 부분집합만 승인해야 한다. 이전 릴리스로 돌아가면 그 릴리스의 현재 승인 기록이 적용된다.

권한 API는 다음과 같다. `releaseId` 조회를 생략하면 현재 설치 릴리스를 조회한다.

```text
GET /api/v1/installations/{id}/permissions?releaseId={releaseId}
PUT /api/v1/installations/{id}/permissions
```

현재 소스·운영에서 조회는 그 설치의 관리 읽기(`Manage(read)`)를 요구한다. 이전 운영 `7b43789`의 Use 허용은 2026-11-07 전환으로 제거했다. 업무 입장과 관리 조회는 독립이므로 회사 사용 중지된 prod 앱도 관리자·배포 담당자는 승인 기록을 읽는다. 수정은 위임되지 않은 관리자와 `miy.dev`가 필요하다. 수정 응답은 커밋 전에 같은 트랜잭션에서 읽은 기록이며, 커밋 뒤에 다시 판정하지 않는다. 수정 본문은 정확한 릴리스·관찰한 revision·승인할 전체 목록이다. 빈 목록은 모두 철회한다.

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

계획된 변경(미구현): 콘솔에 릴리스 권한 승인 화면을 추가한다. 같은 API·정책·승인 이력을 쓴다([범용 기능 계획 P1](COMMON-FEATURES-PLAN.md#p1-셸설정콘솔)).

## 사용자·그룹의 앱 권한과 회사 사용 중지

그룹·입장 정책은 migration `0018_groups_and_app_access.sql`로 구현한 기반을 재사용한다. 아래 AB-05 변경은 소스에 구현했으며 이전 배포·실행 내역은 [0.7.0 계약 이력](archive/CORE-PERMISSIONS-0.7.0.md#사용자그룹의-앱-권한과-회사-사용-중지)에서 확인한다.

- **정본:** 그룹과 주 소속의 정본은 코어 DB다. Keycloak은 인증만 맡는다.
- **그룹 종류:**
  - 조직(인사, `hr`) 그룹: 주 소속(`users.primary_group_id`)이 정확히 그 그룹인 사용자만 포함한다.
  - 수동(`local`) 그룹: 관리자가 직접 추가한 사용자만 포함한다.
  - 계층은 조직 그룹에만 있다. 상위·하위 조직이나 책임자에게 권한이 이어지지 않는다. 계층 변경은 잠금 후 최신 관계로 순환을 검사해 `409 group_hierarchy_cycle`로 거부한다.
- **그룹 권한:** `use`와 `develop`만 줄 수 있다. 운영 배포(`deploy`)는 사람별로만 부여하며, 그룹에 요청하면 `400 group_permission_not_allowed`다.
- **판정:** 권한 판정은 `effective_grants` 뷰(직접 권한 ∪ 현재 그룹 멤버십 ∪ 전체 허용)를 매번 계산한다. 그룹 권한을 사용자 권한 행으로 복사하지 않는다. 멤버십·주 소속·그룹 활성 상태가 바뀌면 다음 요청부터 반영되고, 대기 작업도 실행 시점에 같은 뷰로 다시 검사한다. 판정에서는 판정 모듈(`policy.rs`, 아래 "판정 구조")만 이 뷰를 읽는다. 대기 작업·개발 세션·DB 작업의 재확인도 이 모듈을 거친다. `/me`의 권한 목록은 같은 뷰를 표시만 한다.
- **입장 대상:** 앱·환경마다 `selected`(기본, 권한을 받은 사용자·그룹만)와 `all`(사용 중인 모든 사용자) 중 하나다. `all`은 명시적으로 고를 때만 적용된다.
- **회사 사용 중지:** 앱 사용 경로(화면 자산, 앱 API 호출, 앱 간 호출 대상)는 `403 app_disabled`로 차단한다. 포털의 일반·페이지 설치 목록은 Use 판정으로 사용 불가능한 설치를 제외한다. 콘솔의 앱 상세는 Manage(read), 비브라우저 CLI의 앱 상세는 Use 또는 Manage(read)로 설치를 표시하므로 사용이 차단된 담당 설치도 canOpen=false로 조회할 수 있다. 관리용 앱 이름·ID 목록은 유지한다. 관리자도 운영(prod) 사용 중지를 우회하지 못한다. 다만 그 앱의 개발 권한이 있는 사용자는 테스트(dev) 환경에서 앱을 고칠 수 있다.
- **사용 중지 중 실행 관리:** 사용 중지는 업무 사용을 막으며, 기존 dev develop/prod deploy 또는 관리자의 관리 작업을 막지 않는다. 담당자 관리 목록은 사용 가능한 설치 목록과 독립적으로 제공하며 `appDisabled`·`canOpen`을 서버가 계산한다. `GET /api/v1/runtime/installations`를 앱·환경별 Manage(read)로 필터링한다. 콘솔 또는 비브라우저 credential로 조회하며 비관리자는 담당 설치만, Use-only 사용자는 빈 목록을 받는다.
- **운영 기록 조회의 일치:** `Observe`는 관리 읽기로 제한하며 Use를 허용하지 않는다. 목록·상세·events·cursor·승인 기록에 같은 판정을 적용한다. 관리 scope와 위임 read가 필요하고 업무 이용 가능 상태는 별도 최소 응답으로 제공한다. 기존 CLI·MCP의 전환과 실제 호환 검사는 [경계 설계](APP-BOUNDARIES.md#api와-자동화-계약)를 따른다.
- **판정 구조:** 앱 단위 판정은 `crates/miy/src/policy.rs` 한 곳이 한다.
  - 요청마다 사용자 활성·관리자, `effective_grants`, 회사 사용 중지, 위임 범위와 유효 세션을 **하나의 SELECT와 동일 DB 스냅샷**에서 읽고 자격증명의 `miy.dev`를 적용한다. HTTP 조회·위임 검사·대기 작업 재확인이 같은 읽기를 사용한다. 과거의 활성 상태와 비활성화 뒤 새로 부여된 권한을 합쳐 승인하지 않는다.
  - 동작별 순수 판정으로 결정한다. 동작은 사용(`Use`), 개발 쓰기(`Develop`), 실행·운영 관리(`Manage(동작)`), 운영 기록 조회(`Observe`: 실행 작업·설치 승인 기록), 기록이 남는 요청 접수(`Request(동작)`: 실행·DB 작업, 시험 결과 기록)다. 호출부는 동작을 지정할 뿐 판정 함수를 고르지 않는다.
  - 상세 판정, 목록(앱·설치·작업)의 노출 범위, 설치 목록의 표시값이 모두 이 판정에서 나온다. 목록 SQL에는 권한 조건이 없고, 판정이 허용한 앱·환경 목록으로만 거른다.
  - 대기 작업(실행·DB·빌드·로그 요청·개발 세션)은 시작 전과 긴 대기(잠금, 컴파일) 뒤에 `recheck_queued` 하나로 다시 판정한다. 저장된 행위자의 활성 상태와 환경 관리 권한(관리자, dev `develop`, prod `deploy`), 저장된 위임의 범위를 본다. 접수 때의 `Manage(동작)`에서 자격증명의 `miy.dev`만 뺀 판정이다.
  - **실행 대상은 서버 빌드 산출물뿐이다(2026-11-16 소스).** dev 배포와 prod 승격은 그 앱의 성공한 서버 빌드가 만든 이미지만 받는다. 배포 권한이 있어도 다른 곳에서 만든 이미지는 실행할 수 없으며, 사전 점검·접수·worker 실행에서 각각 확인해 `server_build_required`로 거부한다. 다른 앱의 빌드가 만든 같은 이미지도 인정하지 않는다. 이미지 업로드 API와 위임 action `upload`는 제거했다. 이 변경 전에 올린 이미지로 실행 중인 설치의 시작·중지·재시작·제거는 그대로이고, 새 배포와 승격에는 서버 빌드가 필요하다.
  - 확정 거부는 `user_disabled` 또는 `permission_revoked`로 기록한다. 권한을 읽지 못하면 `database_unavailable`(로그 요청은 `diagnostic_unavailable`)이고 회수로 분류하지 않는다. 거부된 DB 작업은 실행한 것이 없으므로 설치의 DB 상태를 바꾸지 않는다.
  - 쓰기는 `policy::Write`로만 커밋한다. 쓰기와 잠금을 먼저 하고 같은 트랜잭션 안에서 판정하며, 판정을 통과한 트랜잭션만 커밋할 수 있다. 앱 단위 쓰기(실행·DB 작업과 시험 결과 접수, 릴리스·설치 생성, 로그 요청·개발 세션·빌드 접수, 앱 호출 토큰, 위임 발급)는 위 동작으로, 관리자 변경(아래 "관리")과 앱 등록 요청은 플랫폼 동작으로 판정한다.
  - 판정 전에 행위자의 사용자 행→위임 행 순으로 공유 잠금을 얻고, 잠금 대기가 끝난 뒤 새 SELECT로 사실 전체를 읽는다. 잠금은 커밋까지 유지하므로 사용자 비활성화·관리자 회수·위임 철회는 진행 중인 쓰기가 끝날 때까지 기다린다. 위임 발급은 사용자→부모 세션 순으로 먼저 잠그고 마지막 판정도 유지해, 사용자 비활성화의 세션 회수와 교착되지 않게 한다. 권한·그룹 변경은 기다리지 않는다. 판정 직후 회수되면 그 쓰기는 커밋되고, 대기 작업은 실행 전에 다시 확인한다.
  - 인증 흐름, 본인 자격증명·임대 회수, 읽기 전용 스냅샷, worker 기록은 `scripts/check-write-decisions.py`의 목록에 이유와 함께 둔다. 이 검사(`check-static.sh rust`)는 목록 밖에서 트랜잭션을 직접 열거나, 주체를 받는 함수가 `Write` 없이 쓰면 실패한다.
- **표시값:** 서버가 계산하는 안내 값이며, 실제 요청은 서버가 다시 판정한다.
  - `canOpen`은 사용 판정이다. `canManage`는 관리 읽기(`Manage(read)`), `canPromote`는 prod 승격 요청 접수(`Request(promote)`) 판정이다. `acceptance`는 `canManage`일 때만 담는다.
  - 위임 자격증명도 같은 판정을 따른다. 범위에 `read`가 있고 해당 환경의 관리 권한이 있으면 `canManage`가 참이다. `canPromote`는 위임 범위의 `promote`·`read`에 더해 실제 `deploy` 권한과 `miy.dev`까지 본다. 이전에는 위임이면 `canManage`가 항상 거짓이었고, 페이지 목록의 `canPromote`는 위임 범위만 봤다.
- **사용자 등록:** 로그인 서버(Keycloak)는 인증만 한다. 사람은 Core에 등록된 뒤에만 로그인할 수 있다. 등록 방법은 둘이다. 관리자가 만든 초대 링크를 받은 사람이 직접 계정을 만들면 그 순간 등록되고([계정 발급](ACCOUNTS.md)), 이미 로그인 서버에 있는 계정은 관리자가 subject ID(소문자 UUID)와 표시 이름으로 등록한다. 로그인 시도만으로는 아무 행도 만들지 않는다. 등록된 사람은 사용 중 상태의 비관리자이며 앱 권한이 없다. 관리자 지정과 앱 등록 신청 자격(`canDevelop`)은 별도 변경이고, 사용 중인 마지막 관리자의 관리자 지정은 해제할 수 없다. 플랫폼 관리 화면의 "사용자 초대"와 "사용자 계정"에서 다룬다. 초대·재설정 링크는 사용하는 순간 발급 관리자의 현재 권한과 대상 계정의 사용 상태를 다시 확인하며, 계정 링크가 설정되지 않은 서버에서는 계정 생성·비밀번호를 로그인 서버의 관리 절차가 담당한다.
- **관리:** 위임되지 않은 관리자만 바꿀 수 있다. 초대·비밀번호 재설정 링크의 발급과 취소, 사용자 등록·이름·관리자·앱 등록 신청 자격 변경, 그룹·앱 정책·개인 권한 변경, 사용자 활성 상태 변경, 앱 등록 승인·거절, 앱 생성·표시 이름 변경, 공용 컴포넌트 게시는 커밋 전에 현재 `admin AND enabled`를 다시 확인하고 사용자 행 잠금을 커밋까지 유지한다. 대상 잠금을 기다리는 동안 관리자 권한이 회수됐다면 변경과 감사 기록을 함께 취소한다. 주 소속·사용자 활성 상태·역할 변경은 행위자·대상 사용자 행을 같은 ID 순서로 잠근다. 사용자 활성 상태는 변경 전에 확인하므로 관리자가 자신의 계정을 비활성화할 수 있다(마지막 활성 관리자는 제외).
- **감사:** 개인 권한(`grant.enabled`·`grant.disabled`. 앱 등록 승인으로 요청자에게 생기는 dev `develop` 포함), 그룹·그룹 권한(`group.*`·`group_grant.*`), 사용자 등록·이름·역할(`user.registered`·`user.renamed`·`user.admin.enabled|disabled`·`user.developer.enabled|disabled`. 값이 실제로 바뀐 때만), 계정 링크(`account_link.created`·`account_link.revoked`와 본인이 남기는 `user.password_reset`), 사용자 활성 상태(`user.enabled`·`user.disabled`)와 주 소속(`user.primary_group_set`), 앱 정책(`app.admission_set`·`app.disabled`·`app.enabled`) 변경을 같은 트랜잭션에 기록한다. 개인 권한에는 사용자·앱·환경·권한, 사용자 활성 상태·주 소속에는 변경 전후 값을 남긴다. 플랫폼 관리 화면의 "사용자·그룹 권한"에서 다룬다.

```text
GET|POST /api/v1/groups                  PATCH /api/v1/groups/{id}
PUT /api/v1/groups/{id}/parent            GET|PUT /api/v1/groups/{id}/members
GET|POST /api/v1/users                   PATCH /api/v1/users/{id}
POST /api/v1/users/{id}/state             PUT /api/v1/users/{id}/primary-group
GET|PUT /api/v1/group-grants
GET /api/v1/apps/{app}/access             PUT /api/v1/apps/{app}/admission
PUT /api/v1/apps/{app}/control            GET /api/v1/runtime/installations
```

감사 기록은 `GET /api/v1/audit`(관리자 전용)로 기간·행위자·동작 접두어(예: `grant.`, `group.`, `app.`)를 걸러 최신순으로 조회한다. `nextCursor`로 이전 기록을 이어 읽는다. 요청 미들웨어 기록은 `METHOD:status`와 경로, 도메인 변경은 위 동작 이름이다. 플랫폼 관리 화면의 "감사 기록"은 검색 조건과 해당 결과의 cursor를 함께 관리하며 새 검색이 실패하면 이전 페이지를 이어 붙이지 않는다. migration `0019_audit_query.sql`은 행위자·시각 인덱스만 추가한다. 별도 대상 종류·결과·요약 필드와 보존 정책은 V1-A1의 후속 범위다. 보존 기간은 PRD 미결정 사항이라 아직 삭제하지 않는다.

기존 권한 회귀의 실행 기록은 [보관 계약](archive/CORE-PERMISSIONS-0.7.0.md)과 [구조 기반 보완 기록](releases/core-foundation-review-2026-11-06-validation.json)에 보존한다. 권한 관련 인수는 [BV-01·04·05·06·07·08·10·11·12](APP-BOUNDARIES.md#경계-인수)의 이번 실행 범위는 경계 인수표에 기록한다. 전환 전 임시 브라우저 차단과 신규 자격증명의 기본 거부는 앱 경계 설계의 적용 시기를 따른다.

## CLI·MCP의 사용자 위임

일반 로그인으로 얻은 사용자 자격증명과, 그 사용자가 특정 작업에 발급한 위임 자격증명을 구분한다. 위임은 하나의 앱·환경과 정확한 action 목록, 이름, 60~86,400초의 고정 유효기간을 가진다. 사용자당 활성 위임은 최대 32개다. refresh는 만료 시점이나 범위를 늘리지 않는다. 부모 CLI 세션이 더 일찍 만료되면 그 만료를 넘지 못한다.

지원 action:

```text
read app_read app_write directory_read build dev_session prepare_database
accept deploy promote start stop restart remove logs
```

`read`는 상태·설치·작업 조회의 별도 권한이다. 일반 작업 흐름에 관찰이 필요하면 함께 지정한다. `start`가 `stop`이나 `remove`를 허용하지 않는다. 관리자가 발급해도 지정한 앱·환경·action을 벗어날 수 없다. 위임 자격증명으로 다시 위임하거나 설치 권한을 승인하고, 계정·전역 관리·등록 API를 호출할 수 없다. 단일 앱 위임으로 다른 앱의 API 접근 범위까지 얻는 것은 지원하지 않는다.

실행 작업 요청은 접수한 작업의 상태를 응답하므로 실행 action과 `read`를 함께 부여해야 한다. preflight와 POST 모두 이를 검사하고, `read`가 없으면 작업을 저장하기 전에 거부한다. 접수 응답은 커밋 전에 읽은 작업으로 만들어 커밋 후 별도 권한 조회 때문에 403이 되는 경우를 없앤다. 후속 상세·events·목록 조회는 요청마다 현재 위임과 사용자 권한을 다시 검사한다. DB 준비(`POST /api/v1/database/prepare`)도 같은 규칙이다. 접수 응답이 저장된 DB 작업이므로 `prepare_database`와 `read`가 함께 필요하고, `read`가 없으면 작업을 저장하기 전에 거부한다. 작업 저장과 판정은 한 트랜잭션이며, 응답은 그 트랜잭션에서 읽은 작업이다. 시험 결과 기록(`POST /api/v1/apps/{app}/test-acceptance`)도 남는 기록이 실행 관리 정보(`canManage`일 때만 보이는 `acceptance`)라서 `accept`와 `read`가 함께 필요하다. 실행 잠금과 앱 상태 확인을 기다린 뒤 기록 직전에 다시 판정한다. 개발 세션 생성 응답 복구(`GET /api/v1/apps/{app}/dev-sessions/by-request/{request}`)는 세션 조회와 같은 판정(`dev_session`)을 한다. 위임 자격증명은 관리자 권한을 갖지 않는다. 관리자 API뿐 아니라 권한 목록(`GET /api/v1/grants`)에서도 위임된 관리자는 자기 권한만 본다. 앱·설치·작업의 관리 판정에서는 위임 범위 안에서 사용자의 관리자 권한을 재사용한다. **AB-05(2026-11-07 운영 반영):** prod Use는 위임 여부와 무관하게 명시적 use/all이 필요하며 관리자라는 이유로 허용하지 않는다. 이전 운영 `7b43789`의 관리자 자동 Use는 더 이상 적용하지 않는다.

```sh
miy --context platform auth delegations create --app inventory --environment dev \
  --action read --action build --action deploy --name inventory-agent \
  --expires-in 3600 --credential-file ./private/inventory-agent.json
miy --context platform auth delegations list
miy --context platform auth delegations revoke DELEGATION_UUID
```

비밀은 표준 출력에 표시하지 않고 지정한 비공개 파일에 저장한다. 대상 작업은 별도 `MIY_CONFIG_DIR`에 같은 서버 context를 만든 뒤 `auth delegations import --credential-file ...`로 가져온다. 기존 사용자 credential을 덮어쓰지 않는다. 가져오기 후 원본 비밀 파일은 자동 삭제하지 않으므로 사용자가 필요 기간만 보관한다. 파일이나 토큰을 Git·문서·채팅·MCP 도구 응답에 복사하지 않는다.

로컬 stdio MCP는 프로젝트 CLI의 동일한 credential을 사용한다. 직접 HTTP MCP도 Core `miy_access_` credential을 인식하며 기존 OIDC 인증 경로도 유지한다. 서버는 요청마다 위임 만료·철회, 현재 사용자 상태·grant를 다시 확인한다. 목록과 cursor도 위임된 앱·환경으로 제한한다.

runtime·DB 준비·서버 빌드·로그 작업에 원래 `delegation_id`를 저장한다. 다른 위임으로 같은 요청 식별자를 재전송해 권한을 바꾸지 못한다. worker는 실행·복구 시 현재 위임을 확인하고 빌드는 결과 게시 직전에도 확인한다. 개발 DB 세션의 heartbeat·연결도 부모 세션과 위임을 재검사한다. 위임 철회, 부모/발급된 CLI 세션 철회, refresh 재사용, 사용자 비활성화·grant 회수·만료가 제한에 반영된다.

이미 완료한 외부 부작용을 되돌리거나 진행 중인 모든 작업을 즉시 강제 취소하는 기능은 아니다. 일반 사용자 credential로 접수한 기존 작업은 종전의 사용자/grant 재검사 정책을 유지한다. 서비스 계정과 사용자 위임의 재위임은 별도 설계 범위다.

## 앱 간 호출의 원래 사용자와 호출 경로

Core가 앱에 전달하는 서명된 identity에는 원래 사용자·수신 설치/릴리스·환경이 들어간다. 인증된 backend 후속 호출에서는 검증한 호출 설치/릴리스 순서와 유효 외부 권한도 보존한다. 브라우저의 identity·사용자·호출자 헤더를 신뢰하지 않으며, 현행 브라우저 발신 설치는 검증된 프레임 자격증명으로 판단한다. 발신 앱을 증명할 수 없었던 직접 요청은 전환 전 같은 페이지 MF 구조의 한계다.

앱 서버가 사용자 요청 안에서 부르는 코어 알림 API와, 코어가 포털 사용자의 요청으로 앱이 선언한 읽기 전용 엔드포인트를 부르는 플랫폼 → 앱 호출은 [아래 절](#플랫폼-알림과-플랫폼--앱-호출)을 따른다(소스 반영, 미배포). 플랫폼 호출의 서명된 identity에는 `platform=true`가 들어가고 관리자 표시는 없다.

앱이 가진 `MIY_GATEWAY_SECRET`은 자기 앱에서 Core의 요청을 확인하는 용도다. 앱이 이 키로 만든 JWT를 Core 인증이나 다른 사용자 위임으로 사용할 수 없다. 후속 호출에는 Core가 별도로 발급하고 hash만 저장한 `miy_call_` 증표가 필요하다. 이 증표는 원래 만료를 연장하지 않고 최대 30초 동안 유효하다.

후속 호출은 증표를 받은 설치의 `connections/.../api/...`와 읽기 directory, 소유 앱의 `platform/files` 경로에만 허용한다. 직접 대상 API·권한 승인·작업·관리 API에는 사용할 수 없다. 각 단계는 원래 사용자 권한, 원본 CLI/browser/frame/legacy 세션, 모든 이전 설치의 현재 릴리스·승인 및 이전 호출에 사용한 권한을 다시 검사한다. 프레임에서 시작한 체인은 원 프레임의 설치·환경·릴리스와 첫 호출자의 결합도 다시 검사한다. 최초 인증 뒤 배포가 겹쳐도 요청 문맥을 새 릴리스로 바꾸지 않는다. 이 결합 보완은 [리뷰 보완 소스·검증](releases/app-frames-review-fixes-2026-11-12-validation.json)을 거쳐 운영 `7defdb0`에 반영했다([배포 기록](releases/app-frames-review-fixes-2026-11-12-deployment.json)). 원본 로그아웃이나 중간 앱 권한 철회도 기존 증표의 다음 사용을 거부한다. IdP 변경을 Core 사용자 상태에 자동 동기화하는 기능은 아니다.

다음 단계의 유효 권한은 이전 유효 권한과 수신 앱의 승인 권한의 교집합이다. 예를 들어 A→B→C 호출에는 A와 B 모두 C 호출 권한을 요청·승인받아야 한다. B만 가진 더 강한 권한으로 A를 대신할 수 없다. 순환 호출과 8 hop을 넘는 확장은 거부한다. 한 사용자당 살아 있는 증표는 1,000개로 제한하고 만료된 증표는 발급 중 정리한다.

일반 JSON 연결 요청 본문은 최대 4MiB·15초로 수집한 뒤 실제 전달 직전에 권한과 호출 경로를 다시 검사한다. 이미 앱으로 전달한 요청의 롤백까지 보장하지는 않는다.

Rust SDK의 `Identity.context`로 검증된 호출자 정보를 읽는다. `Identity::app_api`와 `platform_api("directory")`는 비공개 증표와 현재 release를 붙여 `MIY_CORE_ORIGIN`으로 요청한다. 런타임은 서버의 명시적인 origin을 전달한다. 기본 HTTPS이며 서버가 HTTP를 명시적으로 허용한 경우에만 `MIY_CORE_ALLOW_HTTP=1`을 전달한다. redirect는 따라가지 않고 오류·Debug 출력에 증표나 응답 body를 포함하지 않는다. 실제 설치 환경에서는 앱 컨테이너에서 이 origin에 접근할 수 있어야 한다.

로컬 preview의 외부 API도 승인된 dev 설치를 요구한다. 로컬 manifest만으로 권한을 늘리지 않는다. 아직 dev 설치가 없으면 외부 연결을 사용할 수 없다. preview 코드가 배포된 릴리스와 같다고 주장하는 후속 증표는 발급하지 않는다.

## 플랫폼 알림과 플랫폼 → 앱 호출

[범용 기능 계획 M2](COMMON-FEATURES-PLAN.md#m2-플랫폼앱-연결과-알림)의 계약이다. 2026-11-22 정본으로 확정했고 구현은 소스 단계다(미배포).

**알림 발행**

- 권한은 `platform:notifications:write`다. 릴리스가 요청하고 관리자가 그 설치·릴리스에 승인해야 한다(다른 플랫폼 권한과 같은 승인 절차).
- 발행은 앱 서버가 사용자 요청을 처리하는 동안 받은 `miy_call_` 문맥으로만 한다. 프레임 토큰·포털 세션·CLI·위임 자격증명으로는 발행할 수 없다. 요청이 끝난 뒤의 발행·재전송은 서비스 주체(V1-A2) 뒤로 미룬다.
- 수신자는 활성 사용자이면서 같은 환경에서 그 앱의 `use` 판정을 통과해야 한다(회사 사용 중지 포함). 사용자 ID(최대 500) 외에 플랫폼 그룹 ID(`recipientGroups`, 최대 50)를 줄 수 있고, 코어가 보낼 때의 현재 구성원으로 펼친 뒤(최대 5,000명, 호출한 사람 자신은 그룹으로는 더하지 않는다) 같은 조건으로 거른다. 그룹 수신자는 `platform:directory:read` 승인도 필요하다(2026-11-22). 조건에 맞지 않는 수신자는 말없이 빼고 응답에는 접수 건수만 준다. 수신자가 그 종류를 꺼 두었으면 저장하지 않는다.
- 내용은 종류(`kind`), 묶음 키(`groupKey`), 짧은 표시 문구(본문 미리보기 없이), 앱 내부 목적지 경로, 원본 자원 ID, 요청 ID다. 같은 수신자·묶음 키의 알림은 새 알림이 덮어쓴다. 같은 요청 ID는 한 번만 접수하고 그 결과를 다시 조회할 수 있다.
- 회수·읽음: 앱은 같은 문맥으로 묶음 키 단위로 알림을 회수하거나(원본 수정·삭제, 수신자의 접근 상실) 호출한 사용자 본인의 알림을 읽음 처리한다. 포털에서 읽은 것은 앱의 읽음 상태를 바꾸지 않는다.
- 발행이 실패해도 앱은 업무 저장을 되돌리지 않는다. 유실된 알림은 다시 보내지 않는다.

**알림함과 배지**

- 알림함은 환경별이다. 업무 포털은 prod, dev 포털은 dev 알림만 읽고 바꾼다. 본인 알림만 보인다.
- 목록과 미읽음 수는 저장된 알림을 코어가 아는 사실(현재 `use` 판정, 계정·앱 사용 중지)로 거른 뒤 계산한다. 패널 열기·페이지 이동은 `check=true`로 해당 페이지의 원본을 재확인한다. 재확인 예산에 닿으면 마지막으로 확인한 위치의 커서를 반환한다. 알림을 열 때는 보던 알림의 `id`로 재확인하며, 같은 묶음의 교체 알림은 새 ID를 받는다. 거부된 버전만 지우므로 이전 확인·읽음 요청은 교체 알림에 적용되지 않는다.
- 수신 설정은 앱별·종류별 끄기이며 코어가 저장 단계에서 거른다. 앱은 이 설정을 읽지 않는다.
- 보존 기간은 90일이다. 지난 알림은 코어 작업이 지운다.
- 포털은 알림을 텍스트로만 그리고 클릭하면 그 앱 프레임을 목적지 경로로 연다. 앱의 React 코드를 포털에서 실행하지 않는다.

**플랫폼 → 앱 호출**

- 앱은 manifest의 `platformEndpoints`에 고정 경로를 선언한다. 선언은 릴리스 계약의 일부이며 따로 승인하지 않는다(앱이 자기 데이터를 그 사용자에게 보여 주는 읽기이기 때문이다).
- 코어는 포털 사용자의 요청에서만 부른다. 그 사용자가 포털 환경(업무 포털 prod, dev 포털 dev)에서 해당 설치의 `use` 판정을 통과할 때만이다. 앱에는 그 사용자의 서명된 신원을 보내고 `admin`은 항상 거짓, 호출자는 플랫폼(`platform=true`)으로 표시한다. 설치 릴리스에 `platform:directory:read`가 승인돼 있으면 그 사람의 현재 그룹 ID(최대 4,096개)도 `groups`로 넣는다. 이 호출은 후속 증표가 없어 앱이 디렉터리에 물을 수 없기 때문이며, 디렉터리의 본인 memberships와 같은 범위다(2026-11-22).
- 읽기 전용이다. 앱 데이터를 바꾸는 일은 앱 프레임 안에서만 하고 포털이 업무 쓰기를 대행하지 않는다.
- 응답은 표시 전용 데이터(제목, 보조 문구, 경로, 시각, 건수)이며 크기·시간·동시 호출 수의 한도를 둔다(64KiB, 3초, 프로세스당 16개, 같은 프로세스에서 사용자당 4개). 호출 슬롯을 기다린 뒤 현재 설치와 `use`를 읽는다. 결과를 적용하기 전에 `use`와 응답한 릴리스가 여전히 유효한지 확인한다. 앱이 멈췄거나 한도를 넘으면 해당 카드나 검색 앱에서 "가져오지 못함"을 표시한다.
- 지금 쓰는 곳: 알림 재확인(`notificationCheck`, M2), 홈 요약 카드(`homeCard`, M5), 통합 검색(`search`, M12). 통합 검색은 업무 포털·dev 포털의 `GET /api/v1/search?q=&limit=`(콘솔 거부)에서 홈 카드와 같은 설치 범위(최대 12개)를 부르고, 본문은 다듬은 검색어(1–200자, 제어 문자 거부)·앱별 개수(1–10)·표시 설정이다. 응답 항목은 제목 120자·발췌 240자·앱 기준 경로·시각·종류 40자로 자르며, 앱이 돌려주지 않은 항목의 존재·건수는 드러내지 않는다. 홈 카드는 업무 포털·dev 포털의 `GET /api/v1/home/cards`(콘솔 거부)에서 그 환경의 `use` 판정을 통과한 설치 중 카드를 선언한 것만(최대 12개) 부른다. 본문은 그 사람의 표시 설정(`timeZone`, `locale`)이다. 응답은 제목 80자·건수·항목 5개(제목 120자, 보조 문구 160자, 앱 기준 경로, RFC 3339 시각)로 잘라 텍스트로만 그린다. 호출 직전에 `use`를 잃었거나 선언이 없어진 앱은 카드를 빼고, 응답하지 못한 앱은 `unavailable`로 표시한다.

## 공통 업무 권한과 앱 연계

**상태: G4 디렉터리, 앱 소유 G5·G6 ACL·공유와 작은 첨부 Storage는 구현·게시·개발 서버 반영을 마쳤다.** [공통 기능 격리 검증](releases/common-components-2026-11-14-validation.json)과 [0.10.0 반영](releases/pilot-0.10.0-2026-11-15-deployment.json)을 구분한다. 배포 프레임·preview에서 이 기능들을 함께 사용하는 전체 경로는 아직 미인수다. API 전용 Invoke·서비스 자동화·업무 이벤트는 미구현이며 A2·Invoke·이벤트·대용량 파일은 각 기능 순서에 개발한다. PLM·PMS·PDM·MES·DAS는 독립 앱이고 아래 요구 전체가 v1에서 구현된 기능이라는 뜻은 아니다.

### 현행 기반과 공백

| 영역 | 확인한 현행 | 개선할 부분 |
| --- | --- | --- |
| 공통 인증·앱 입장 | 사용자·그룹과 앱/환경 권한, 앱별 DB, 원래 사용자 identity 제공 | 앱 DB의 자원·역할·grant를 SDK `Acl`로 집행. 각 앱 handler가 반드시 연결하며 플랫폼 admin을 앱 owner로 승격하지 않음 |
| 디렉터리 | `directory.rs`의 사용자·그룹·현재 원사용자 멤버십 DTO, 검색·cursor·ids 조회와 브라우저/Rust SDK를 제공. 설치 승인·현재 Use·위임을 매 요청 재확인 | 선택기는 앱 프레임 안의 공용 UI. 서버의 현재 판정과 구분하며 관리자 API를 대행하지 않음. HR/로컬 그룹 자동 상속 없음 |
| backend 앱 간 호출 | connections 경로에서 같은 환경 대상 Use, 설치/릴리스 요청·승인과 호출 체인을 검사. app read/write는 GET·HEAD와 나머지 메서드 구분 | API Invoke·업무 action별 승인은 후속. 브라우저 발신 설치의 증명과 직접 API 제한은 앱 프레임 계약에서 집행 |
| 실행 주체·비동기 | 사용자 대행·제한 위임은 구현. runtime job은 배포·승격·시작·중지 등의 운영 작업 | 서비스 계정(V1-A2), 업무 이벤트·outbox·재시도 제품은 미구현. 운영 worker를 업무 큐로 간주하지 않음 |
| 데이터 전달 | Rust SDK JSON 연결은 요청·응답 4MiB, 15초 timeout, 자동 재시도 없음 | 작은 첨부는 별도 10MiB 스트리밍 경로. 대용량 CAD/도면·장기 처리·재개 전송은 후속 |

현행 prod 대상 `Use`는 명시적 use 또는 전체 허용 정책을 요구하며 관리자·deploy만으로 허용하지 않는다(AB-05 운영 반영). dev는 관리자 또는 해당 앱 develop 미리보기와 명시적 use@dev를 허용하고, 브라우저에서는 dev_portal 용도만 사용할 수 있다. backend 연결도 현재 대상 Use를 요구하며 API 전용 Invoke는 후속이다.

### 공통 ACL과 앱의 책임

- 플랫폼은 인증된 사용자·그룹의 정본, 앱·환경 진입과 설치 승인, 기존 위임·감사를 제공한다. 앱 서버용 현재 멤버십 확인은 구현했고 서비스 주체는 후속이다. 앱이 클라이언트 입력의 사용자 ID·그룹·역할을 그대로 믿지 않도록 공통 ACL에 필요한 검증된 context를 확장한다.
- SDK `Acl`은 `create_resource_in`, `require/require_in`, `list_resources`, `sharing/replace_in/replacement_result`를 제공한다. 앱이 역할→동작을 선언하고 SDK SQL을 새 앱 migration으로 적용한다. 플랫폼이 미리 만든 `app` 스키마를 사용하며 migration owner에 DB CREATE 권한을 요구하지 않는다. 업무 생성/변경과 ACL·감사·결과 기록을 같은 트랜잭션에 묶고 현재 멤버십을 조회한다. `acl.manage`와 revision·requestId로 공유 변경을 직렬화하며 보호된 생성자와 활성 직접 사용자 관리자를 유지한다. 앱은 동일 도구를 사용하되 실제 판정은 앱 서버에서 수행하며, UI 버튼·SDK 호출 관례만으로 우회 경로를 닫았다고 판단하지 않는다.
- 앱은 업무 데이터와 선택적 공간·멤버·역할·자원 ACL 관계를 자기 앱/환경 DB에서 소유한다. 공간이 없는 앱도 지원한다. 코어에 전역 워크스페이스나 모든 업무 ACL 저장소를 추가하지 않는다. 같은 이름의 역할이나 같은 공간 ID가 다른 앱 권한으로 자동 이어지지 않는다. 사용자·그룹·서비스 주체는 플랫폼의 안정된 ID로 참조하고, 업무 역할·자원·ACL 관계는 대상 앱·환경 범위 안에서 해석한다.
- 목록·검색·상세·다운로드·수정·대기 작업 모두 같은 현재 권한을 확인한다. 목록의 제목·건수·cursor도 접근 불가 자원을 드러내지 않아야 한다. 계정·그룹 멤버십·앱 사용·리소스 ACL 회수 뒤의 요청과 작업은 다시 판정하며, 과거 토큰·선택 값·파생 권한 복사로 접근을 유지하지 않는다. 멤버십 확인 실패 시 허용하지 않는다.
- PLM의 설계 변경 승인 가능 상태·자기 승인 금지 같은 업무 규칙은 PLM이 소유한다. 공통 ACL이 `approve`를 허용해도 업무 상태 검사를 생략하지 않는다. 플랫폼 admin이 앱 owner·설계 승인자·공간 관리자가 되지는 않는다.
- **앱 코드 신뢰와 사용자 ACL은 다른 경계다.** 현재 업무 앱은 설치별 출처 프레임으로 포털·다른 앱의 DOM과 프레임 자격증명에서 분리한다. 공통 ACL은 그 안에서 사용자·자원 권한을 집행하는 도구이며 출처 격리를 대신하지 않는다. 같은 사이트 배치의 쿠키 간섭·프로세스 분리 한계와 외부 앱 수용 조건은 [앱 프레임 설계](APP-FRAMES.md)를 따른다. 같은 JavaScript 실행 공간을 공유한 것은 전환 전 MF 구조의 이력이다.

### 디렉터리·작은 첨부의 현행 계약

디렉터리는 `kind=users|groups|memberships`, 검색 `q`, 1~100의 `limit`(생략하면 50), cursor, 최대 100개의 `ids`를 받는다. 쿼리 없는 SDK의 비정형 호출(`platformApi("directory")`, `platform_api("directory")`)은 한 페이지만 읽고, 항목이 더 있으면 잘린 목록을 돌려주지 않고 `directory_page_incomplete`로 실패한다. memberships는 원사용자만 조회하며 사용자 지정 매개변수를 받지 않는다. 사용자 id/name/primaryGroup, 그룹 id/name/source와 subjectId/installationId/environment를 반환한다. 권한을 가진 앱에 공개하는 DTO이며 비활성 사용자·관리자 플래그·인사 비밀 필드를 내보내지 않는다. ACL은 조회 실패 때 거부하고 그룹을 사용자 목록으로 영구 복제하지 않는다.

파일은 앱 backend가 자원 ACL을 판정한 뒤 `platform:files:read/write` 승인을 가진 후속 증표로 Storage를 사용한다. 일반 브라우저/CLI의 Storage 직접 접근은 거부한다. 프레임 `/_miy/files/...`는 **소유 앱** `/api/files/...`로만 전달한다. 별도의 preview capability는 현재 개발자와 승인된 dev 설치의 directory/files만 허용하며 브라우저에 노출하지 않는다. 프리뷰는 실제 dev installationId와 로컬 release를 구분한다.

Core schema 23(2026-11-15 개발 서버 반영)이 파일 metadata를 소유하고 bytes는 artifacts의 `.files/<installation UUID>/` 아래(schema 34부터 파일 UUID 앞 두 자리 하위 디렉터리, 아래 대용량 전송 절)에 `<file UUID>.blob`(미완료는 `.part`)으로 보관한다. 앱 DB는 자원 연결과 ACL을 소유한다. 한 요청 전송(`single`)은 10MiB/파일, 각 전송 단계 4개 슬롯·20초 상한이며 일반 JSON 4MiB/15초 제한을 확대하지 않는다. 설치별 한도와 코어 전체 한도는 아래 대용량 전송 절의 규칙(schema 33)을 따른다. 실제 파일명은 경로로 사용하지 않는다. 파일 응답은 content/integrity 헤더 허용 목록과 sandbox/nosniff만 통과시키며 쿠키·Clear-Site-Data는 전달하지 않는다.

예약 requestId와 첨부 requestId는 독립적이며 충돌/결과 조회를 제공한다. 앱은 예약 의도를 먼저 저장하고, bytes ready 후 앱 참조를 커밋한 다음 Core attach를 기록한다. 응답 유실은 조회로 해결하고 자동 재시도·보상 삭제를 하지 않는다. incomplete pending만 24시간 만료 대상이고 ready/attached는 자동 고아 삭제하지 않는다. 만료 시각이 지난 예약은 즉시 `expired`로 보이고 quota에서 빠진다. bytes 삭제와 명시 취소 후 남은 정리는 요청 경로가 아니라 worker가 약 1분마다 수행한다. 한 파일의 삭제 실패는 그 파일만 남기고 나머지 정리와 다른 앱의 예약을 막지 않으며, 감사에는 사용자 없이 플랫폼 행위(`file.expired`·`file.cancelled`)로 남는다. 실패하거나 끊긴 업로드는 예약이 만료되기 전까지 같은 예약으로 다시 보낼 수 있다. 전송 중인 파일의 상태를 바꾸려는 요청은 몇 초 기다린 뒤 409 `file_upload_in_progress`를 받는다. 예약·첨부·취소는 lock을 쥔 채 DB 연결을 추가로 얻지 않고, 디스크 여유는 트랜잭션 밖에서 측정한다. 명시 취소는 먼저 복구 가능한 내부 cancelling 상태를 커밋해 첨부를 차단한 뒤 bytes를 삭제하고 quota를 반환한다. 정리 실패는 재조회/재실행으로 이어지며 삭제 후 ready로 되돌아가지 않는다. 앱 참조가 있으면 Core가 ready여도 취소를 거부해야 한다. 첨부된 파일은 취소로 지울 수 없다. 앱이 자기 참조를 삭제한 트랜잭션을 커밋한 뒤 첨부 때의 reference로 `detach`를 호출하면 Core가 bytes를 삭제하고 quota를 반환한다(결과는 `cancelled`, 다시 호출해도 안전). reference가 다르면 거부한다. 앱 제거는 bytes를 보존한다. 읽기/전송 시작과 업로드 완료의 현재 권한을 검사하지만 이미 전송된 바이트를 회수한다고 주장하지 않는다. Core metadata·bytes·앱 참조를 함께 백업/복원해야 하며 전체 백업 제품은 PLAN의 별도 잔여다.

SDK 사용·마이그레이션 예시는 [앱 계약](../templates/rust-react/docs/CONTRACT.md)과 [대표 프로젝트·업무 앱](../examples/common-work/README.md)에 있다. PLM/PDM 대형 파일, 서비스 주체·업무 이벤트·서버 간 파일 공유는 이 작은 첨부 구현의 완료 범위가 아니다.

### 대용량 전송(M10)

[범용 기능 계획 §2.3](COMMON-FEATURES-PLAN.md#23-파일-저장)의 코어 설계를 정본으로 확정했다. 2026-11-22 분할 예약·커밋된 오프셋·한 조각 쓰기·확정·한도, 다운로드 증표, 하위 디렉터리와 주기 무결성 확인을 소스에 반영했다(core schema 33·34, 미배포).

- **전송 방식.** 예약(`POST platform/files`)에 `transfer`를 둔다. `single`(기본)은 지금의 10MiB 한 요청 업로드이고, `parts`는 2GiB까지 조각으로 보낸다. `single` 예약에 조각을, `parts` 예약에 한 요청 업로드를 보내면 409 `file_transfer_mismatch`다.
- **조각.** `PUT platform/files/{id}/parts`에 `Content-Range: bytes S-E/T` 하나를 보낸다. `T`는 예약 크기, `S`는 커밋된 `received`와 같아야 하고(아니면 409 `file_part_offset`), 길이는 1B~8MiB다(프레임 파일 gateway의 10MiB 안). 파일 행을 잠근 채 디스크의 꼬리를 `S`로 잘라 쓰고 동기화한 뒤 새 오프셋을 커밋한다. 첫 생성은 부모 디렉터리까지 동기화한다. DB보다 디스크가 짧으면 503 `file_part_bytes_missing`으로 멈춘다. 응답을 잃으면 메타데이터의 `received`에서 이어 보낸다.
- **확정.** 모든 바이트가 온 뒤(`received == size`, 아니면 409 `file_parts_incomplete`) `POST platform/files/{id}/complete`는 SHA-256 준비를 시작하고 202와 `pending`을 반환한다. 호출자는 같은 파일 ID로 다시 요청한다. 해시가 준비되면 현재 쓰기 권한을 다시 확인하고 rename·디렉터리 동기화와 `ready` 커밋을 마친 뒤 200을 반환한다. 준비된 해시는 예약에 보관하며, 재시작이나 응답 유실 뒤에도 같은 요청 경로를 사용한다. 취소·만료·첨부·회수는 작은 첨부와 같다.
- **한도.** 설치별 한도는 콘솔 관리자가 `GET·PUT·DELETE /api/v1/installations/{id}/file-quota`로 정하고 감사(`file.quota_set`)에 남긴다. 정하지 않으면 기본값(20GiB·100,000개)이다. 기존 파일은 그대로 두고 새 예약만 거절한다(409 `file_quota_exceeded`). 코어 전체에는 고정 총량이 없다. 새 예약은 직렬화하며, 아직 쓰지 않은 예약량을 더해도 파일 볼륨에 max(2GiB, 5%)의 여유와 inode가 남을 때만 받는다(503 `file_storage_low`).
- **다운로드 증표.** 큰 파일은 메모리 Blob이 아니라 다운로드 증표로 받는다. 앱 backend가 파일 ACL을 판정한 뒤 `POST platform/files/{id}/download` `{reference?}`를 부르면 5분짜리 증표(`/_miy/downloads/<토큰>`)를 받는다. 이 요청은 그 설치의 프레임이 직접 시작한 호출 체인이어야 하고(아니면 403 `file_download_frame_required`), 릴리스가 `platformEndpoints.fileAccess`를 선언해야 한다(아니면 409 `file_access_endpoint_missing`). 첨부된 파일은 첨부 때의 참조와 같아야 한다. 증표는 프레임 세션·사용자·설치·릴리스·파일·참조에 묶여 저장된다(hash만). 프레임 출처의 GET·Range마다 같은 출처의 Fetch Metadata, 프레임과 부모 세션, 릴리스, 설치 `use`, 읽기 승인, 파일 상태와 참조를 다시 확인하고, 전송 직전 `fileAccess`로 소유 앱에 그 사람의 서명된 신원(`platform=true`)과 `{fileId, reference}`를 보내 `{allowed: true}`를 받아야 보낸다(아니면 403 `file_access_denied`). Range는 하나만 받고(여러 범위는 전체), ETag는 SHA-256, 응답은 `attachment`·`no-store`·sandbox·nosniff이며 프레임 listener의 로그는 꺼져 있다. 독립 예산(동시 8개)과 60초 무응답 한도를 둔다. backend의 `files_download`는 계속 10MiB까지다.
- **배치.** 바이트는 `.files/<설치 UUID>/<파일 UUID 앞 두 자리>/<파일 UUID>.blob|.part`에 둔다. 새 쓰기는 항상 하위 디렉터리로 가고, 읽기는 하위 디렉터리에 없으면 이전 배치(`.files/<설치 UUID>/`)를 본다. worker가 매 정리 주기에 이전 배치의 파일을 그 파일의 행 잠금(쓰기·정리와 같은 잠금) 아래에서 옮기고 두 디렉터리를 동기화한다. 잠긴 행은 다음 주기로 미루고 행이 없는 파일은 그대로 둔다. 삭제는 두 배치를 모두 지운다.
- **묶음 전송 확인.** 프레임의 `runtime.downloadAndWait`는 같은 증표로 전체 파일을 내려받고, 발급 프레임의 자격증명으로 `/_miy/downloads/<토큰>/status`를 조회한다. Core가 응답 본문의 마지막 바이트를 내보낸 뒤에 다음 파일을 시작할 수 있다. 중단된 스트림은 실패로 기록한다. 상태 조회가 끊긴 경우 `checkOnly` 옵션으로 같은 증표의 상태를 다시 확인할 수 있다. 확인값은 Core 프로세스의 단기 상태이며 기기의 디스크 저장 결과는 브라우저가 관리한다.
- **무결성.** worker가 약 1시간마다 실행당 4GiB 안에서 가장 오래 확인하지 않은 `ready`·`attached` 파일의 SHA-256을 다시 계산한다(`verified_at`). 바이트가 없거나 합이 다르면 `corrupt_at`을 기록하고 플랫폼 행위로 감사(`file.corrupt`)에 남기며, 그 파일의 내려받기·다운로드 증표·첨부는 409 `file_corrupt`로 멈춘다. 복구는 백업에서 바이트를 되살린 뒤 운영자가 표시를 지우는 절차다(제품 기능 아님).

### 앱 간 업무 호출과 자동화

UI `Use`와 업무 API `Invoke`의 분리를 검토한다. 예를 들어 PLM 사용자가 MES 화면에 들어갈 권한은 없어도 명시적으로 승인된 생산 요청 action을 호출할 수 있도록 한다. Invoke는 대상 앱이 선언한 action과 자원 범위를 필요로 하며 대상 리소스 ACL과 업무 상태를 우회하지 않는다. 현재 `read/write` 승인, 기존 Use 사용자와 게시 CLI의 호환·전환 규칙은 구현 전에 함께 확정해야 한다. 현재 서버는 계속 대상 Use를 요구하므로 이 예시는 아직 사용할 수 없다.

사용자 대행은 원래 사용자를 끝까지 보존하고, 무인 연계는 소유자·목적·앱·환경·action·자원 범위·만료/철회를 갖춘 별도 서비스 주체로 수행한다(V1-A2). 사용자 권한 거부를 더 강한 서비스 계정으로 조용히 재시도하지 않는다. Hermes·LLM 공급자 키는 업무 권한이 아니며 실행 엔진이 사용자나 서비스 주체를 임의 지정할 수 없다. 현행 승인 교집합·동일 환경·순환 및 8 hop 제한은 기본 기반으로 유지하고 범위를 늘리는 변경은 따로 검토한다.

비동기 PLM→MES 연계는 발행 앱의 업무 변경과 transactional outbox 기록, 수신 앱의 이벤트 ID 기반 중복 방지, 제한된 재시도·실패 조회·재처리와 감사 계약을 검토한다. 실행 주체·원래 원인·이벤트/스키마 버전을 보존하고 소비·재시도 시 현재 권한과 대상 상태를 다시 확인한다. 앱 간 DB 공유나 전역 원자적 트랜잭션, 정확히 한 번 전달을 전제하지 않는다. 일반 알림 projection(V1-F3)은 이 업무 이벤트 실행 엔진과 구분한다.

PDM/CAD 파일은 JSON 호출 본문에 넣지 않고 Storage API의 파일 ID·버전과 제한된 접근 핸들로 연결한다. 선택·전달뿐 아니라 다운로드·스트리밍 시 현재 사용자/서비스 권한과 원본 앱 ACL을 확인한다. v1 선택 파일의 10MiB 기준(V1-F1)은 대용량 CAD 지원을 뜻하지 않는다. 대용량·중단/재개·보존/복원 계약은 V1-F1·F2의 확장 검토 범위다.

### 대표 인수와 검토할 결정

V1-G5·G6의 대표 앱은 기존 결정인 프로젝트·업무 앱을 유지한다. 그 위에 PLM→PDM/MES의 작은 연계 fixture를 추가하며 전체 PLM 구현을 코어 완료 조건으로 삼지 않는다.

| 인수 | 통과 조건 | 상태 |
| --- | --- | --- |
| G4·G5·G6 공통 ACL | 같은 앱에 입장한 두 사용자의 자원 접근 차이, 공간 없는 앱, 다른 앱/환경 ID 오용 거부. 그룹 회수 후 직접 API·목록·검색·다운로드·대기 작업 차단 | Core 디렉터리 회귀·SDK 실DB·대표 앱 HTTP/실DB·브라우저 모의 HTTP 범위 PASS. 실제 인증/프레임/preview 전체 연결과 미구현 업무 대기 작업은 NOT_RUN |
| PLM→PDM 사용자 대행 | 원래 사용자·호출 앱·설치 승인 보존과 대상 파일/자원 ACL 집행. 플랫폼 관리자도 앱 owner로 취급하지 않음 | NOT_RUN |
| PLM→MES Invoke | MES UI Use 없이 허용된 action·자원만 실행. 다른 action·환경·자원과 미승인 호출 거부. PLM 자기 승인·잘못된 업무 상태 거부 | NOT_RUN |
| A2·업무 이벤트 | 서비스 주체의 범위·만료·회수 적용, 사용자 실패의 자동 권한 상승 금지, 중복 이벤트·재시도에도 업무 효과 중복 방지와 감사 | NOT_RUN |
| F1·F2 파일 연계 | 파일 ID·버전·주체·앱/환경 제한, 다운로드 전 회수 반영, JSON 한도와 대용량 파일 경로 구분 | 소유 앱의 작은 첨부·별도 5MiB 전송·회수·응답 유실 복구 PASS. 실제 전체 연결·앱 간 파일 전달·대용량 CAD·통합 복원 제품은 NOT_RUN |

세부 명령·증거·실패 이력은 [공통 기능 검증 기록](releases/common-components-2026-11-14-validation.json)과 그 뒤의 [리뷰 후속 기록](releases/common-components-2026-11-15-review-fixes.json)을 따른다. 분리한 Core·대표 앱·브라우저 시험은 실제 배포 전체 경로 인수와 구분한다.

공통 SDK는 앱 선언 역할과 직접 사용자/그룹 grant를 지원한다. 중첩 관계·익명 링크·관리자 소유권 복구는 기본 기능에 포함하지 않는다. Invoke와 현행 Use의 호환, 서비스 주체 수명주기, 이벤트 전달·재처리, 대용량 전송/통합 복원은 각 기능을 구현할 때 확정한다. 이 후속 결정을 모두 마쳐야 G4나 최소 보안을 진행할 수 있는 것은 아니다. 실제 순서는 PLAN에서만 관리한다.

## SDK 전환

게시된 CLI 0.7.0 / 번들 0.8.0 / Rust SDK 0.3.0의 권한·호출자 계약 전환은 [0.7.0 보관본](archive/CORE-PERMISSIONS-0.7.0.md#sdk-전환)에 당시 범위로 보존한다. 게시 출처와 상태는 [운영 안내](SERVER-OPERATIONS.md)가 관리한다.

AB-07은 AppRoot를 유지하면서 앱 데이터·적용 SQL·게시물 바이트를 보존하는 계약이다. 현재 세 출처·목적별 세션·환경 제한·콘솔 device 승인을 구현·운영 반영했고 legacy 로그인은 410으로 종료했다. 개발 단계에서는 구 API·CLI·SDK 호환을 강제하지 않으며 새 계약은 CLI·SDK·웹·문서와 함께 새 불변 버전으로 제공한다. 브라우저 재로그인·실패 복구와 실제 반영 범위는 [앱 경계 설계](APP-BOUNDARIES.md#전환과-복구)를 따른다. 실제 Windows·외부 환경 등 미실행 인수는 별도로 남아 있다.

## 검증 기록

0.7.0 권한 보강 당시의 결과와 소스는 [코어 권한 검증 기록](releases/core-permissions-2026-11-03-validation.json)에 보존한다. 해당 실행에는 실제 Windows·새 호스트·운영 배포가 포함되지 않았다. 다단계 Core 정책의 격리 PostgreSQL/HTTP·서명 검증 backend fixture와 SDK HTTP 전송의 실제 loopback TCP 검사를 운영 Docker 네트워크의 전체 연결 인수로 합산하지 않는다. AB-05와 세 출처 경계의 후속 검증·배포는 [경계 인수](APP-BOUNDARIES.md#경계-인수)의 기록을 따르며 이전 PASS를 이월하지 않는다.
