import { useEffect, useState } from "react";
import { request } from "@miy/sdk";
import { Button, Panel } from "@miy/ui";

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
        {guide.windowsInstaller && <Panel title="1. Windows 설치">
          <p>Windows 11 x64에서 설치 파일을 실행하세요. 관리자 권한 없이 현재 사용자에게 설치합니다.</p>
          <p><a className="cli-install-download" href={guide.windowsInstaller.url} download={guide.windowsInstaller.file}>Windows 설치 파일 다운로드</a></p>
          <p><a href={guide.windowsInstaller.checksumUrl}>설치 파일 SHA-256</a></p>
          <ol>
            <li>설치 파일을 실행하고 아래 서버 주소를 입력하세요. 처음 설치하는 PC에서는 서버 주소 입력란이 비어 있습니다.</li>
            <li>연결 확인 후 설치를 누르세요. 설치 폴더와 연결 이름은 자동으로 관리합니다.</li>
            <li>설치가 끝나면 새 터미널을 열고 아래 로그인 명령을 실행하세요.</li>
          </ol>
          <Command label="설치에 입력할 서버 주소" command={guide.serverUrl} />
          <p>사내 IP의 HTTP 주소와 도메인의 HTTPS 주소를 모두 사용할 수 있습니다. 다른 회사 서버에 연결하려면 해당 회사에서 안내한 주소를 입력하세요.</p>
        </Panel>}
        <details open={!guide.windowsInstaller}>
          <summary>스크립트 설치와 다른 운영체제</summary>
        <Panel title="1. 배포 패키지 준비">
          {guide.downloads?.length ? <>
            <p>운영체제·CPU에 맞는 설치 파일을 선택하세요. Windows 설치 명령은 다운로드와 해시 확인을 함께 수행합니다.</p>
            <ul>{guide.downloads.map((download) => <li key={download.target}>
              <a href={download.url}>{download.file}</a>{" · "}<a href={download.checksumUrl}>SHA-256</a>
            </li>)}</ul>
            <p>검증 후보 소스: <code>{guide.sourceCommit}</code></p>
          </> : <p>설치 파일을 준비 중입니다. 배포 파일이 등록되면 이 화면에서 다운로드할 수 있습니다.</p>}
          <p>설치 스크립트도 검증한 패키지에 포함된 파일을 사용하세요.</p>
        </Panel>
        <div className="cli-install-platforms">
          <Panel title="2. Linux · macOS">
            <p>검증한 tar.gz를 푼 폴더에서 Bash로 실행하세요.</p>
            <Command label="Linux·macOS 설치 명령" command={guide.commands.unix} />
            <p>기본 설치 경로는 <code>~/.local/bin</code>입니다. 이 경로가 PATH에 있어야 <code>miy</code> 명령을 사용할 수 있습니다.</p>
          </Panel>
          <Panel title="2. Windows">
            <p>{guide.downloads?.length ? "Windows x64의 PowerShell에서 아래 명령을 실행하세요. 설치 후 새 터미널을 여세요." : "Windows 배포 파일이 준비된 후 설치 명령을 실행하세요."}</p>
            <Command label="Windows 설치 명령" command={guide.commands.windows} />
            <p>기본 설치 경로는 <code>%LOCALAPPDATA%\Programs\miy\bin</code>입니다. 명령 뒤에 <code>-AddToPath</code>를 추가하면 사용자 PATH에 등록하며, 새 터미널에서 사용할 수 있습니다.</p>
          </Panel>
        </div>
        </details>
        <Panel title={guide.windowsInstaller ? "2. CLI 로그인" : "3. 로그인"}>
          <p>설치 중 입력한 주소 또는 설치 명령에 명시된 주소를 연결로 등록합니다. CLI에는 기본 IP나 도메인이 없습니다. 주소 없이 설치하면 직접 입력하거나 설치 후 <code>miy context init</code>으로 등록하세요.</p>
          {guide.serverUrl.startsWith("http://") && <p>이 서버는 사내망 HTTP 구성으로 통신이 암호화되지 않습니다.</p>}
          <p>PATH가 설정된 터미널에서 실행하고, 열린 브라우저에서 계정과 인증 코드를 확인해 연결을 승인하세요.</p>
          <Command label="CLI 로그인 명령" command={guide.commands.login} />
          <p>기존 연결이 있다면 <code>miy context list</code>로 로그인할 서버를 먼저 확인하세요.</p>
        </Panel>
      </>}
    </section>
  );
}
