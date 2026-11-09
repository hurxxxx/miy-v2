import { Notice, Panel } from "../ui";
import { useUser } from "../data/session";
import { CliDeviceApproval } from "./LoginRecovery";

/**
 * The pre-0.5 browser-approval CLI login is retired: a single click here could
 * hand a CLI credential to whoever started the request. Supported CLIs use the
 * device login, which needs the code shown in the user's own terminal.
 */
export function CliAuthorize() {
  return (
    <Panel title="지원하지 않는 CLI 로그인">
      <Notice tone="warning" role="alert">
        이 로그인 요청은 0.5 이전 CLI의 방식이라 더 이상 승인하지 않습니다. 직접 시작하지
        않은 요청이라면 이 창을 닫으세요.
      </Notice>
      <p>
        최신 CLI로 업그레이드한 뒤 터미널에서 <code>miy auth login</code>을 다시 실행하세요.
        새 로그인은 터미널에 표시된 코드를 이 콘솔에 입력해 승인합니다.
      </p>
    </Panel>
  );
}

export function CliDeviceApprove() {
  return <CliDeviceApproval user={useUser()} />;
}
