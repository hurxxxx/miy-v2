# 프로젝트·업무 공통 기능 예제

CLI/번들 **0.10.0**, SDK **0.4.0**, 공용 UI **1.1.0** 후보를 사용하는 업무 소스다. 플랫폼 기능을 다시 구현하지 않고 앱 DB의 프로젝트·업무·ACL·첨부 관계를 공통 SDK와 UI에 연결한다. 별도 워크스페이스를 만들지 않는다. 개발 서버에 등록·배포된 앱이라는 뜻은 아니다.

새 독립 프로젝트를 `miy app init PATH --id common-work --name "프로젝트와 업무"`로 만든 뒤 이 디렉터리의 `src/`, `web/src/`, `manifest.json`, 새 migration 0002·0003을 복사한다. 생성 SDK·초기 SQL·lock은 유지하고 공용 UI 1.1.0을 설치한다. manifest의 directory/files read/write를 dev 설치에서 승인해야 한다. 앱 Use는 자원 공유와 별도로 부여한다. 이미 사용 중인 앱의 SQL을 덮어쓰지 않는다.

소유자는 생성 시 정해지고 공유를 관리한다. 편집자는 업무·첨부를 변경하며 조회자는 읽기·다운로드만 한다. 앱은 모든 handler에서 현재 ACL을 확인하고, 목록은 ACL SQL로 먼저 필터링한다. 플랫폼 관리자도 앱 소유자로 취급하지 않는다. 업무 상태 규칙은 앱이 추가한다.

공유 변경은 revision과 requestId를 사용한다. 파일은 예약 의도를 앱 DB에 기록한 다음 Core에 예약하고, bytes 전송→앱 참조 커밋→Core attach 순서로 처리한다. 모르는 결과는 원래 ID로 조회한다. 프로젝트·업무 생성을 조회했을 때 서버에 만들어진 것이 없다고 확인되면 같은 ID로 다시 보내거나 요청을 취소할 수 있다. 같은 ID를 쓰므로 뒤늦게 도착한 첫 요청은 중복 생성 대신 충돌로 끝난다. 요청 ID는 공용 UI의 `requestId`로 만든다(HTTP 주소의 프레임에는 `crypto.randomUUID`가 없다). 참조가 커밋된 파일은 Core에 ready로 남아 있어도 취소하지 않는다. `FileTransferPanel`은 실제로 측정하지 않은 바이트 진행률을 표시하지 않는다. 세션 복구에는 사용자·설치·자원별 요청 ID와 표시 metadata만 저장하며 토큰과 파일 bytes는 넣지 않는다.

API metadata는 `/projects/{id}/files`, 바이너리는 앱 `/api/files/projects/{id}/{file}/content`를 사용한다. UI는 `runtime.fileApi`로 소유 앱만 호출한다. 10MiB 이하의 작은 첨부와 raster 미리보기가 범위이며 대형 CAD·재개 업로드·익명 공유는 지원하지 않는다.

`python3 scripts/check-common-work.py`는 저장소 루트에서 임시 앱·PostgreSQL·서명된 identity로 실제 업무 HTTP handler를 확인한다. Core directory/Storage는 이 앱 검사에서 로컬 fixture이고, 실제 Core 권한·저장 실증은 `scripts/runtime-regression/run.py`, SDK ACL SQL 실증은 `scripts/test-sdk-acl.py`가 담당한다. 브라우저는 공용 UI 검사 안의 대표 앱 fixture를 사용한다. 이 검사를 실제 OIDC·배포 프레임 전체 인수나 운영 반영으로 확대하지 않는다.

전체 구현 상태는 [PLAN](../../docs/PLAN.md), 권한·저장 계약은 [CORE-PERMISSIONS](../../docs/CORE-PERMISSIONS.md), 명령·결과는 [CHECKS](../../docs/CHECKS.md)와 [검증 기록](../../docs/releases/common-components-2026-11-14-validation.json), [리뷰 후속 기록](../../docs/releases/common-components-2026-11-15-review-fixes.json)을 따른다.
