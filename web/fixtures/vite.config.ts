import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";
export default defineConfig(({ mode }) => {
  const profile =
    mode === "health"
      ? "health"
      : mode === "diagnosis"
        ? "diagnosis"
        : "reference";
  const entry =
    profile === "health"
      ? "HealthApp"
      : profile === "reference"
        ? "ReferenceApp"
        : "DiagnosticApp";
  return {
    base: "./",
    plugins: [
      react(),
      federation({
        name: `miy_${profile}`,
        dts: false,
        filename: "remoteEntry.js",
        manifest: true,
        exposes: { "./AppRoot": `./src/${entry}.tsx` },
        shared: {
          react: { singleton: true, requiredVersion: "19.3.1" },
          "react-dom": { singleton: true, requiredVersion: "19.3.1" },
        },
      }),
    ],
    build: {
      outDir: `dist/${profile}`,
      target: "esnext",
      rollupOptions: { input: `src/${entry}.tsx` },
    },
  };
});
