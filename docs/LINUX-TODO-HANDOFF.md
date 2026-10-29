# Linux Codex 전달 프롬프트

아래 요청은 Windows에서 확인한 네이티브 로컬 개발 이후의 배포 검증이다.
Windows PC에는 Docker Desktop·WSL을 설치하지 않았다. Windows 개발 세션은
정상 종료했고 서버 개발 DB와 테스트 레코드를 보존했다.

```text
MIY Todo 앱의 Linux 빌드·개발서버 배포 검증을 이어서 진행해 주세요.

저장소: https://github.com/hurxxxx/miy-v2.git
브랜치: codex/native-local-preview
네이티브 구현 커밋: b873a811be533e3f907ff36b39408db185c81571
이 브랜치는 main의 564df40 변경도 포함합니다. 기존 작업이나 운영 배포를
덮어쓰지 말고 별도 작업 디렉터리에서 최신 브랜치를 받아 사용하세요.

먼저 docs/NATIVE-DEVELOPMENT.md, examples/native-todo/README.md,
docs/releases/cli-0.6.1-native-windows-validation.json을 읽어 주세요.
이 변경의 CLI는 0.6.1, 개발 번들은 0.7.1입니다.

목표 서버는 https://v2.dwdcc.kr 이며 기존 등록 앱 ID는 todo-win-20261029입니다.
Windows에서는 사용자가 선택한 admin 계정으로 실제 로그인했고 등록 승인까지
완료했습니다. 사용자별 목록이므로 기존 레코드는 같은 admin 사용자로 확인해야 합니다.

1. Linux용 CLI를 이 브랜치에서 --no-default-features로 빌드하세요.
   명시적으로 해당 서버 context를 선택하고 실제 CLI 로그인을 사용하세요.
   인증정보를 읽거나 로그에 출력하지 말고, 필요하면 사용자에게 브라우저 승인을 요청하세요.

2. 이 CLI로 별도 독립 앱 디렉터리를 생성하세요.
   ID todo-win-20261029 / 이름 Windows Todo 검증을 사용하고,
   examples/native-todo의 src/, web/src/, migrations/0002_todos.sql,
   manifest.json만 생성 프로젝트의 업무 파일에 복사하세요.
   생성된 SDK·Dockerfile·초기 SQL·lock·CLI 관리 파일은 직접 수정하지 마세요.
   기존 앱을 지우거나 다른 ID로 새 등록하지 마세요.

3. 앱의 AGENTS.md·miy-cli/miy-app 스킬과 실제 MCP 계약·도움말을 따르세요.
   component sync, doctor --for develop, app fmt, app check를 실행하고
   실제 명령과 종료 코드를 기록하세요. 소스 전체를 앱의 Git에 커밋하세요.
   기존 릴리스가 있으면 상태를 확인하고 manifest 버전을 올리세요.

4. 이 Linux 서버에 이미 있는 Docker Engine으로 app build를 수행하세요.
   doctor/API가 보고한 서버 Linux CPU에 맞는 --platform을 사용하세요.
   Windows에 Docker Desktop을 설치하는 방식은 사용하지 않습니다.
   Docker Engine이나 빌드 권한이 없다면 구체적인 차단 사유를 보고하세요.

5. app deploy --environment dev로 해당 앱만 개발 영역에 배포하세요.
   job/request ID를 보존하고 job watch/get 및 app status로 종료 성공과
   실제 releaseId·immutable imageId·버전을 확인하세요.
   통신 실패 시 새 배포를 무작정 만들지 말고 원래 요청의 job resume을 사용하세요.
   다른 활성 개발 세션이 있으면 강제로 빼앗지 말고 충돌을 보고하세요.

6. 배포된 실제 개발 앱을 브라우저로 열어 확인하세요.
   유지되어야 할 개발 DB UUID:
     129f8abd-62c2-40d1-9175-d73d85196940
   유지되어야 할 Todo UUID:
     8dcfffe2-3854-4ce5-b878-2da9f915c151
   제목: 네이티브 실행 후에도 남는 할 일 20261029
   completed: false
   이 레코드는 Windows 네이티브 미리보기에서 만들었고 중지·재시작 후에도 유지됐습니다.
   배포 후 같은 DB와 레코드인지 CLI/허용된 앱 API와 UI로 확인하고,
   별도 임시 레코드로 추가·조회·완료·취소·삭제·새로고침을 검증하세요.
   유지 대상 레코드는 수정하거나 삭제하지 마세요.

7. 실제 확인한 항목만 인수 기록에 남기세요. 브라우저 도구나 로그인이 없어
   UI를 확인하지 못하면 NOT_RUN으로 표시하고 Windows에서 열 최종 URL을 주세요.
   최종 보고에는 소스 commit, CLI SHA-256, 명령/종료 코드, job/release/image ID,
   DB·Todo ID 일치 여부, 개발 앱 URL, 화면 증거, 실패·미실행 항목을 포함하세요.

플랫폼 자체 재배포·core DB 변경·다른 앱 변경·운영 승격은 수행하지 마세요.
개발자 최소 권한과 운영 DB 분리는 이번 admin 개발 배포 검증과 구분하세요.
```
