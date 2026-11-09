import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";
// The platform frame runtime: an MF host served by Core on every app frame origin
// at /_miy/runtime/. It loads one installation's `./AppRoot` and no other app.
export default defineConfig({
  base: "/_miy/runtime/",
  plugins: [
    react(),
    federation({
      name: "miy_frame",
      dts: false,
      remotes: {},
      shared: {
        react: { singleton: true, requiredVersion: "19.3.1" },
        "react-dom": { singleton: true, requiredVersion: "19.3.1" },
      },
    }),
  ],
  build: { target: "esnext" },
});
