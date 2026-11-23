import { defineConfig, mergeConfig } from "vite";
import previewConfig from "./sdk/preview/vite.config.ts";
import { excalidrawFonts } from "./vite.excalidraw-fonts.ts";

export default defineConfig(mergeConfig(previewConfig, {
  plugins: [excalidrawFonts()],
}));
