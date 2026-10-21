import AppRoot from "./AppRoot";
import type { AppProps } from "@miy/sdk";
export default function DiagnosticApp(props: AppProps) {
  return <AppRoot {...props} title="UI 진단" />;
}
