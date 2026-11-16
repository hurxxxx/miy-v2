import { useEffect, useState } from "react";
import { request } from "@miy/sdk";
import { Button, Panel } from "../ui";

type InstallGuide = {
  schemaVersion: number;
  serverUrl: string;
  cliVersion: string;
  commands: { unix: string; windows: string; login: string };
  downloads?: { target: string; file: string; sha256: string; url: string; checksumUrl: string }[];
  sourceCommit?: string;
  windowsInstaller?: { file: string; sha256: string; url: string; checksumUrl: string };
};

function Command({ label, command }: { label: string; command: string }) {
  const [message, setMessage] = useState("");
  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setMessage("복사했습니다.");
    } catch {
      setMessage("내용을 선택해 직접 복사해 주세요.");
    }
  }
  return (
    <div className="cli-install-command">
      <pre aria-label={label}><code>{command}</code></pre>
      <Button type="button" aria-label={`${label} 복사`} onClick={copy}>복사</Button>
      {message && <p role="status">{message}</p>}
    </div>
  );
}

export function CliInstall() {
  const [guide, setGuide] = useState<InstallGuide>();
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    request<InstallGuide>("/api/v1/cli/install")
      .then((result) => { if (active) setGuide(result); })
      .catch(() => { if (active) setError("CLI 설치 정보를 불러오지 못했습니다. 잠시 후 다시 접속해 주세요."); });
    return () => { active = false; };
  }, []);
  return (
    <section className="cli-install" aria-labelledby="cli-install-title">
      <h1 id="cli-install-title">CLI 설치</h1>
      <p>터미널과 AI 도구에서 앱을 개발하고 이 플랫폼에 배포합니다.</p>
      {error && <p role="alert">{error}</p>}
      {!guide && !error && <p role="status">설치 정보를 불러오는 중…</p>}
      {guide && <>
        <p>연결할 서버: <strong data-testid="cli-install-server">{guide.serverUrl}</strong> · CLI {guide.cliVersion}</p>
        <Panel title="1. 명령 한 줄로 설치">
          <p>사용하는 PC의 터미널에 아래 명령을 붙여 넣으세요. 설치 파일 다운로드·해시 확인·서버 등록을 자동으로 처리합니다.</p>
          <div className="cli-install-platforms">
            <Panel title="Linux · macOS">
              {guide.downloads?.some((item) => item.target !== "x86_64-pc-windows-msvc") ? <>
                <Command label="Linux·macOS 설치 명령" command={guide.commands.unix} />
                <p>OS와 CPU를 자동으로 선택합니다. <code>~/.local/bin</code>에 설치하고 사용자 셸의 PATH에 추가합니다. 설치 후 새 터미널을 여세요.</p>
              </> : <p>이 서버에 Linux·macOS CLI 패키지가 아직 게시되지 않았습니다. 서버 관리자에게 게시를 요청하세요.</p>}
            </Panel>
            <Panel title="Windows">
              {guide.downloads?.some((item) => item.target === "x86_64-pc-windows-msvc") ? <>
                <Command label="Windows 설치 명령" command={guide.commands.windows} />
                <p>Windows x64의 PowerShell에서 실행하세요. 관리자 권한 없이 현재 사용자에게 설치하고 PATH에 추가합니다.</p>
              </> : <p>이 서버에 Windows CLI 패키지가 아직 게시되지 않았습니다. 서버 관리자에게 게시를 요청하세요.</p>}
            </Panel>
          </div>
        </Panel>
        <details>
          <summary>설치 파일 직접 다운로드</summary>
          {guide.downloads?.length ? <ul>{guide.downloads.map((download) => <li key={download.target}>
            <a href={download.url}>{download.file}</a>{" · "}<a href={download.checksumUrl}>SHA-256</a>
          </li>)}</ul> : <p>게시된 패키지가 없습니다.</p>}
          {guide.sourceCommit && <p>패키지 소스: <code>{guide.sourceCommit}</code></p>}
          {guide.windowsInstaller && <>
            <p><a className="cli-install-download" href={guide.windowsInstaller.url} download={guide.windowsInstaller.file}>Windows 설치 파일 다운로드</a>{" · "}<a href={guide.windowsInstaller.checksumUrl}>설치 파일 SHA-256</a></p>
            <p>GUI 설치기를 사용하는 경우 아래 서버 주소를 입력하세요. 처음 설치하는 PC에서는 서버 주소 입력란이 비어 있습니다.</p>
            <Command label="설치에 입력할 서버 주소" command={guide.serverUrl} />
          </>}
        </details>
        <Panel title="기존 CLI 업데이트">
          <p>로컬 미리보기와 CLI를 사용하는 Codex·Claude Code를 종료한 뒤, 위 설치 명령을 같은 사용자로 다시 실행하세요. 기존 연결과 인증 정보는 유지됩니다.</p>
          <Command label="전역 CLI 버전 확인" command="miy --version" />
          <p>위에 표시된 버전인지 확인하고, 각 앱 프로젝트 폴더에서 새 전역 CLI로 실행하세요. 프로젝트의 기존 실행 파일로 실행하면 새 버전으로 바뀌지 않습니다.</p>
          <Command label="프로젝트 CLI·MCP 갱신" command="miy app setup" />
          <p>Codex·Claude Code를 다시 열어 MCP와 가이드를 새로 불러오세요. SDK 번들이 다르다는 오류가 나오면 변경을 검토·커밋한 뒤 명시적으로 <code>miy app upgrade</code>를 사용하세요.</p>
          <p>프로젝트 실행 파일(<code>.miy-tools/miy</code>, Windows는 <code>.miy-tools/miy.exe</code>)의 버전과 <code>auth status</code>를 확인하세요. 업데이트만으로 재로그인이 필요한 것은 아닙니다. 인증 저장소 오류가 계속되면 같은 실행 파일의 <code>report send --message "인증 저장소 오류"</code>로 로그인 없이 신고할 수 있습니다.</p>
        </Panel>
        <Panel title="2. CLI 로그인">
          <p>위 명령은 이 서버 주소를 연결로 등록합니다. CLI에는 기본 IP나 도메인이 없습니다. 기존 연결과 인증 정보는 유지합니다.</p>
          {guide.serverUrl.startsWith("http://") && <p>이 서버는 사내망 HTTP 구성으로 통신이 암호화되지 않습니다.</p>}
          <p>처음 인증하거나 재인증이 필요하다는 오류가 있을 때 실행하세요. 열린 브라우저에서 계정과 인증 코드를 확인해 연결을 승인합니다.</p>
          <Command label="CLI 로그인 명령" command={guide.commands.login} />
          <p>기존 연결이 있다면 <code>miy context list</code>로 로그인할 서버를 먼저 확인하세요.</p>
        </Panel>
      </>}
    </section>
  );
}
