import AppRoot from "./AppRoot";
import type { AppProps } from "@miy/sdk";
export default function ReferenceApp(props: AppProps) {
  return <AppRoot {...props} title="기준정보" />;
}
