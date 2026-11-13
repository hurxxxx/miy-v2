# Codex 비품대장 인수 소스

2026-11-02 독립 Codex 세션이 작성하고 서버에 배포한 v2의 업무 파일 원본이다. 평가자는 이 파일을 수정하지 않았다. 앱 소스 commit은 `7b78ac06fc0893e39a39ab804b3019cb1fee1a4b`이며 원래 앱 ID는 `qa-ai-codex-mvz9422x`다. 실행 결과와 개입 내역은 [독립 AI 인수 기록](../../docs/archive/results/INDEPENDENT-AGENTS-0.6.3-ACCEPTANCE.md)에 있다.

이 폴더는 검토용 업무 소스 모음이다. CLI·SDK·의존성·인증·로컬 설정은 포함하지 않는다. 실행하려면 게시 CLI 0.6.3으로 별도 폴더에 새 프로젝트를 생성하고 이 `src/`, `web/src/`, 추가 migration 두 개를 복사한다. 새 앱으로 등록할 경우 생성한 manifest의 새 ID를 유지하고 나머지 업무 설정을 대조한다. SDK, 초기 migration, Dockerfile과 관리 파일은 생성본을 유지한다. 명시적으로 선택한 서버 context에서 공용 컴포넌트를 동기화하고 `app fmt`, `app check`, `cargo test --locked`를 수행한 뒤 커밋하여 원격 빌드·dev 배포한다. 기존 검증 앱에 덮어쓰거나 보존 레코드를 삭제하지 않는다.

v1의 비품명·정수 수량·사용자별 소유권에 v2의 선택적 보관 위치를 추가했다. GET/POST `/assets`, PATCH/DELETE `/assets/{id}`를 제공하며 소유자는 SDK Identity에서 결정한다. 실제 인수는 개발 서버 원격 빌드와 v1→v2 배포를 대상으로 했으며 네이티브 로컬 실행·prod 승격·Windows 실행은 이번 결과에 포함하지 않는다.
