import { createServer } from "../../../web/platform/node_modules/vite/dist/node/index.js";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const server = await createServer({
  configFile: false,
  root: fileURLToPath(new URL("./", import.meta.url)),
  cacheDir: `${root}.local/evidence/common-ui/vite-cache`,
  resolve: {
    alias: {
      react: `${root}web/platform/node_modules/react`,
      "react-dom": `${root}web/platform/node_modules/react-dom`,
      "@miy/sdk": `${root}packages/sdk/src/index.ts`,
      "@miy/ui": `${root}packages/ui/src/index.tsx`,
    },
  },
  esbuild: { jsx: "automatic" },
  server: {
    host: "127.0.0.1",
    port: 19871,
    strictPort: true,
    fs: { allow: [root] },
  },
});
await server.listen();
const stop = async () => {
  await server.close();
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
