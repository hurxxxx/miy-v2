# 현재 아키텍처 읽기

- 기준: 2026-11-09 사내 최소 경계. **회사 통제 앱·iframe 없는 동적 UI·단일 IP HTTPS, prod/dev/console 세 출처 구현·운영 반영**([dev 포털 배포](../releases/dev-portal-2026-11-09-deployment.json)).

현재 계약의 Mermaid 원본은 [앱 실행·관리 경계](../APP-BOUNDARIES.md#출처와-자격증명)에 있다. 복제 그림을 별도 정본으로 유지하지 않는다. 권한은 [코어 권한](../CORE-PERMISSIONS.md#확정-권한-행렬), 작업 순서는 [추진 계획](../PLAN.md)을 따른다.

단일 Core·IdP와 MF/AppRoot 통합 라우팅을 유지한다. 출처는 **prod 업무 포털**, **dev 앱 포털**, **관리·device CLI 승인·Keycloak `/identity` 콘솔** 세 개다. 각 포털 세션은 자기 환경의 앱 사용만 허용한다. 앱 코드·능동 앱 콘텐츠는 콘솔에 적재하지 않으며 별도 IdP는 기능 개발의 선행 조건이 아니다.

같은 페이지의 앱 UI는 회사가 통제하는 신뢰 코드이며 각 포털 안의 앱 박스가 독립 보안 영역을 뜻하지 않는다. 단일 IP의 포트는 origin을 나누지만 쿠키를 나누지 않으므로 세션 용도·환경·정확한 Origin·CSRF·backend 쿠키 미전달을 함께 적용한다. 같은 상위 도메인의 쿠키 영향도 남는다. 실제 주소·서버/UI 배포 소스·schema는 [운영 안내](../SERVER-OPERATIONS.md)가 관리한다.

2026-10의 SVG 원본·PNG·PDF 묶음은 [과거 구성도](../archive/architecture-2026-10/README.md)에 보존한다. 현재 설계의 그림으로 재사용하지 않으며 향후 내보내기 산출물에는 Mermaid 원본·설계 버전을 함께 표시한다.
