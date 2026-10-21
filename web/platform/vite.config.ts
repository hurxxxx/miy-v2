import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { federation } from "@module-federation/vite";
export default defineConfig({
  plugins: [
    react(),
    federation({
      name: "miy_shell",
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
