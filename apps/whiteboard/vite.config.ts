import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "node:path";
import { excalidrawFonts } from "./web/vite.excalidraw-fonts.ts";

export default defineConfig({
  base: "./",
  resolve: { alias: { "@miy/sdk": resolve("sdk/browser/index.ts"), "@miy/ui": resolve("sdk/ui/index.tsx"), "@miy/components": resolve("platform-components") } },
  plugins: [react(), tailwindcss(), excalidrawFonts(), federation({name:"miy_whiteboard",dts:false,filename:"remoteEntry.js",manifest:true,exposes:{"./AppRoot":"./web/src/AppRoot.tsx"},shared:{react:{singleton:true,requiredVersion:"19.3.1"},"react-dom":{singleton:true,requiredVersion:"19.3.1"}}})],
  build: {outDir:"dist/ui",target:"esnext",chunkSizeWarningLimit:4096,rollupOptions:{input:"web/src/AppRoot.tsx"}},
});
