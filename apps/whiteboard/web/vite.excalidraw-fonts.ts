import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import type { Plugin } from "vite";

function files(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

// The frame CSP restricts fonts to the frame origin. The editor resolves
// each font by its original name under excalidraw-assets/fonts.
export function excalidrawFonts(): Plugin {
  const fonts = resolve("node_modules/@excalidraw/excalidraw/dist/prod/fonts");
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
