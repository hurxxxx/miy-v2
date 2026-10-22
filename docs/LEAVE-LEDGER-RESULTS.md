# CLI → Codex → 휴가 관리 대장 → Docker 배포 실증

검증일: 2026-10-22 KST. **별도 작업공간에서 생성한 휴가 관리 앱을 실제 Codex CLI가 개발했고, 로컬 Docker에 배포해 miy 라우터에서 실행했다.** V0~V1 위에 CLI·앱 SDK·이미지 배포·API 게이트웨이·기본 수명주기 제어를 추가한 실증이다. 정식 V2~V7 전체 인수 완료를 뜻하지 않는다.

이 문서는 bundle 0.2.1 당시의 실험 기록이다. 여기서 발견한 CLI 등록 공백·지침/검사 불일치는 후속 bundle 0.3.0에서 보강하고 [새 앱의 개발·배포 과정으로 재검증](IMPLEMENTATION-RESULTS.md#개발등록배포-과정-재검증-bundle-030)했다. 아래 개입·미구현 항목은 최초 실험 당시 기준으로 보존한다.

## 접속과 소스

- 개발 화면: [http://localhost:18000](http://localhost:18000). **관리자로 시작** → **휴가 관리 대장**을 선택한다.
- 앱 경로: [http://localhost:18000/apps/leave-ledger](http://localhost:18000/apps/leave-ledger).
- 독립 소스: `~/projects/miy-apps/leave-ledger`. [앱 README](../../miy-apps/leave-ledger/README.md)에 API와 업무 규칙을 기록했다.
- 최종 소스 커밋: `a10f34e220ad4ef0e3f18b10cab3c6d7e624ce0f`.
- 최종 릴리스: `0.1.1`, 이미지 태그 `miy-app/leave-ledger:0.1.1`.
- 실제 실행 이미지 ID: `sha256:657d6031505f764f966b1a2447b62c361909a25cd6ae521739290609b9079d14`.

검증 레코드는 합성 데이터이며 화면에서 `(검증)` 또는 `(화면 검증)`으로 표시한다. 앱 소스는 플랫폼 저장소에 복사하지 않았으며 별도의 Git 저장소로 관리한다.

## 실제 개발·배포 경로

1. Rust CLI의 `miy app init`으로 독립 폴더에 Rust·React 템플릿, Rust/브라우저 SDK, 공통 UI, 개발 지침, 잠금 파일을 생성했다.
2. 관리자 API로 앱을 등록하고 개발자에게 해당 앱의 개발 환경 권한, 사용자 A/B에게 사용 권한을 부여했다.
3. `scripts/develop-app.mjs`가 **실제 `codex exec`**를 호출했다. Codex는 실제 OAuth로 MCP에 연결해 `get_contract`, `search_api`를 조회하고 앱의 Rust·React·SQL·테스트·README를 작성했다. 모델 공급자의 기존 로그인은 유지했다.
4. 개발 지침·SDK·공개 계약을 제공하고 macOS 외부 샌드박스에서 플랫폼 소스와 Docker 소켓을 차단했다. 중첩 샌드박스 오류를 피하기 위해 Codex 자체 샌드박스 대신 외부 OS 정책을 적용했다. 엄격한 전체 호스트 읽기 격리나 정식 독립 개발 평가의 완료를 주장하지 않는다.
5. 평가자가 소스를 검토·커밋하고 `miy app build`를 실행했다. 한 CLI 명령이 React 모듈을 빌드하고 Rust Linux 실행 파일·UI·manifest·migration·소스 커밋 정보를 같은 Docker 이미지에 포함했다.
6. 개발 계정으로 `miy app deploy`를 실행했다. 서버는 권한·이미지 ID를 검사하고 영속 작업을 등록했다. Rust 실행기는 Docker CLI로 같은 Engine의 불변 이미지 ID를 읽고 이미지 내부 `/miy`의 UI·manifest·migration을 해시 검증 후 추출했다.
7. 실행기가 앱·환경별 DB와 분리된 역할을 준비하고 migration을 적용한 뒤 제한된 컨테이너를 시작했다. 요청자는 DB·역할·컨테이너를 수동 생성하지 않았다. 초기 네트워크 설정 보완은 아래 개입 기록에 남겼다.
8. 플랫폼은 등록 정보에서 UI 모듈을 찾아 `/apps/leave-ledger`에 로드했다. iframe 없이 메뉴·헤더를 유지하며, `runtime.api()`는 권한·CSRF·릴리스 검사를 거쳐 Rust 컨테이너로 전달된다. 컨테이너는 설치·릴리스별 서명된 사용자 정보를 SDK로 검증한다.

## 구현한 앱 기능

직원명·부서·연차/오전 반차/오후 반차·기간·사유·상태를 관리한다. 신청·목록·상세·수정·삭제, 관리자 승인·사유가 필요한 반려, 연도·상태·직원명 필터, 승인/신청/잔여 일수 요약을 제공한다. 일반 사용자는 본인 데이터만 사용하며 변경은 신청 상태에서만 가능하다.

연차는 평일만 계산하고 반차는 같은 평일 하루에 0.5일이다. 기간 중복은 사용자별 DB 잠금으로 동시 요청까지 검사한다. 기본 15일은 예제 정책으로 화면에 명시했다. 공휴일 달력·실제 인사 정책 연동은 없고 음수 잔여를 허용하며, 목록은 연도별 최신 1,000건까지 표시한다.

## 검증 결과

플랫폼 밖 앱을 개발한 Codex가 읽을 수 없던 `scripts/check-leave-ledger.mjs`로 **38개 독립 인수 검사**를 통과했다.

| 영역 | 확인한 결과 |
| --- | --- |
| 업무·DB | 실제 DB 기반 생성·수정·삭제, 평일·반차 계산, 잔여 계산, 승인·반려·상태 전이 제한 |
| 권한 | 타 사용자 조회·수정·삭제 거부, 목록 격리, 본문 사용자/관리자 위조 거부, 일반 사용자 승인 거부 |
| 동시성 | 같은 사용자의 겹치는 요청 2개 중 하나만 생성되고 다른 하나는 409 |
| 게이트웨이 | 이전 UI 릴리스의 변경 요청 거부, 다른 Origin 변경 거부, 컨테이너 직접 접근의 위조 사용자 헤더 거부 |
| 실제 인증 | 개발 버튼과 별도로 실제 Keycloak OAuth 세션을 이용한 Docker 앱 데이터 접근 성공 |
| Docker | 실행 이미지 ID 일치, 비관리자 UID, 읽기 전용 루트, 256 MiB·1 CPU·128 PID 제한 확인 |
| UI | 실제 브라우저 신청·상세·직접 링크 새로고침, iframe 없음, 배포 전후 플랫폼 빌드 해시 동일 |
| 제어 | 플랫폼 웹의 중지·시작·재시작, 중지 중 API 503, 재기동 후 동일 레코드 보존 |

0.1.0에서 0.1.1 이미지로 교체하면서 기존 승인·반려·신청 데이터 3건의 ID·소유자·상태·일수를 보존했다. 별도의 CLI 생성 및 잘못된 계약/진입점/이미지 참조 거부 검사 5개도 통과했다. 앱 Rust 단위 검사 6개, 플랫폼 단위 검사 5개, 타입 검사·Clippy·기존 OAuth/UI 회귀 검사와 실제 DB 격리·migration·실행기 강제 종료 회귀 검사도 통과했다.

실행 증거는 `.local/evidence/leave-ledger.json`, `leave-update.json`, `cli-contract.json`, `leave-*.png`, Codex JSONL 및 로컬 실행 로그에 둔다. 자격증명·세션·원시 도구 기록은 Git에 포함하지 않는다. 통합 화면의 시각 검토에서 발견한 플랫폼의 전역 header/main 스타일은 플랫폼 영역에 한정하도록 보완했다.

## 발견한 문제와 개입

- macOS의 중첩 `sandbox-exec`가 도구 실행을 막아, 외부 OS 정책을 유지한 실행 방식으로 변경했다. 최초 실패 기록은 보존했다.
- 개발 번들의 도구체인 고정과 캐시 제외를 보완했고, SQLx가 소유하는 트랜잭션에 중첩 BEGIN/COMMIT를 넣지 않도록 Codex에 수정 요청했다.
- 비대화형 의존성 설치의 디렉터리 재구성 확인은 CLI에서 CI 실행 설정으로 처리했다.
- Docker의 `internal` 네트워크만 연결한 컨테이너는 호스트 포트를 게시하지 않아 전용 일반 bridge로 수정했다. 현재 앱 네트워크의 외부 송신 차단을 검증한 것은 아니다.
- 템플릿의 `remoteEntry.js` 직접 등록은 ESM 타입을 잃어 UI 로딩에 실패했다. bundle 0.2.1에서 `mf-manifest.json`을 계약으로 고정하고 CLI·서버 검사를 보완했다. Codex가 manifest·문서만 수정한 0.1.1을 새 이미지로 배포했다.
- Docker가 시작·재시작 때 임의 호스트 포트를 재할당해, 실행기가 매번 실제 게시 포트를 다시 읽고 저장하도록 수정했다.

첫 시도 무개입 성공으로 집계하지 않는다. 업무 로직을 평가자가 대신 작성하지 않았고, 위 번들·계약 피드백은 실제 Codex에 다시 전달해 수정·재검사했다.

## 재현 명령과 현재 범위

플랫폼 기동 후 기존 앱의 상태·재배포·제어는 다음과 같이 실행한다.

```sh
cd ~/projects/miy-apps/leave-ledger
.miy-tools/miy app login --account developer
.miy-tools/miy app check
.miy-tools/miy app build
.miy-tools/miy app deploy
.miy-tools/miy app status
.miy-tools/miy app control stop
.miy-tools/miy app control start
```

개발 계정의 CLI 로그인은 명시적으로 켠 localhost 개발 프로필에 한정한다. 실제 MCP OAuth와 웹 OAuth 검사를 대체하지 않는다. 앱·권한의 최초 등록은 관리자 API를 사용하며 아직 등록·권한 편집 전용 관리 화면은 없다.

현재 배포는 Docker Desktop Linux Engine의 **같은 로컬 이미지 저장소**를 개발 빌드와 서버 실행기가 공유한다. 원격 레지스트리 전송·독립 Linux 서버 배포·운영 승격·백업/복원·실패 migration 후 전체 복구·호스트 재시작·3회 독립 반복은 완료하지 않았다. 신뢰된 앱 UI가 플랫폼과 같은 브라우저 공간을 공유한다. 정식 V2~V7 통과 여부는 [집중 검증 계획](VALIDATION.md)에서 별도로 판단한다.
