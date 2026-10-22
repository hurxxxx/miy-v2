import type { AppProps } from "@miy/sdk";
import { Panel } from "@miy/ui";
export default function AppRoot({runtime}:AppProps) {
  return <Panel title="새 앱"><p>개발 준비가 완료됐습니다.</p><p>{runtime.location.pathname}</p></Panel>;
}
