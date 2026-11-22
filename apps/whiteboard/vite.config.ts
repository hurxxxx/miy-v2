import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";
import tailwindcss from "@tailwindcss/vite";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";

// The frame CSP allows fonts from the frame's own origin only. Ship
// Excalidraw's fonts beside the UI chunks under their original names
// (`excalidraw-assets/fonts/<Family>/<file>.woff2`); see
// web/src/platform/excalidraw/self-hosted-assets.ts.
function excalidrawFonts(): Plugin {
  const fonts = resolve("node_modules/@excalidraw/excalidraw/dist/prod/fonts");
  const files = (directory: string): string[] =>
    readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory() ? files(join(directory, entry.name)) : [join(directory, entry.name)],
    );
  return {
    name: "miy-excalidraw-fonts",
    apply: "build",
    generateBundle() {
      for (const file of files(fonts)) {
        this.emitFile({
          type: "asset",
          fileName: ["excalidraw-assets", "fonts", ...relative(fonts, file).split(sep)].join("/"),
          source: readFileSync(file),
        });
      }
    },
  };
}

export default defineConfig({
  base: "./",
  resolve: { alias: { "@miy/sdk": resolve("sdk/browser/index.ts"), "@miy/ui": resolve("sdk/ui/index.tsx"), "@miy/components": resolve("platform-components") } },
  plugins: [react(), tailwindcss(), excalidrawFonts(), federation({name:"miy_whiteboard",dts:false,filename:"remoteEntry.js",manifest:true,exposes:{"./AppRoot":"./web/src/AppRoot.tsx"},shared:{react:{singleton:true,requiredVersion:"19.3.1"},"react-dom":{singleton:true,requiredVersion:"19.3.1"}}})],
  build: {outDir:"dist/ui",target:"esnext",chunkSizeWarningLimit:4096,rollupOptions:{input:"web/src/AppRoot.tsx"}},
});
