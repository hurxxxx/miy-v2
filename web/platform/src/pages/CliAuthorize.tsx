import { useState } from "react";
import { useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { Button, Panel } from "../ui";
import { useSurface, useUser } from "../data/session";
import { CliDeviceApproval } from "./LoginRecovery";

export function CliAuthorize() {
  const user = useUser();
  const { portalOrigin } = useSurface();
  const location = useLocation();
  const id = location.pathname.split("/").at(-1);
  const [result, setResult] = useState("");
  const [busy, setBusy] = useState(false);
  async function approve() {
    setBusy(true);
    try {
      await request(`/api/v1/cli-authorizations/${id}/approve`, user.csrf, { method: "POST" });
      setResult("CLI 연결을 승인했습니다. 터미널로 돌아가세요.");
    } catch (e) {
      setResult(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title="CLI 로그인 승인">
      <p>플랫폼 서버: <strong>{portalOrigin}</strong></p>
      <p>
        {user.name} 계정으로 CLI가 플랫폼 API를 사용하도록 연결합니다. 권한은 이 계정의 앱·환경 권한을 따릅니다.
      </p>
      <p>
        요청 ID: <code>{id}</code>
      </p>
      <p>터미널의 요청 ID와 같은지 확인하세요. 연결은 8시간 뒤 만료되며 app logout으로 해제할 수 있습니다.</p>
      {result ? (
        <p role="status">{result}</p>
      ) : (
        <Button disabled={busy} onClick={approve}>
          이 CLI 연결 승인
        </Button>
      )}
    </Panel>
  );
}

export function CliDeviceApprove() {
  return <CliDeviceApproval user={useUser()} />;
}
