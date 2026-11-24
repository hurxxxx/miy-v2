import { build } from 'vite';
import { nodeFileTrace } from '@vercel/nft';
import { copyFile, mkdir, readlink, lstat, symlink } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

const out = 'dist/server';
await build({
  configFile: false,
  build: {
    ssr: 'web/server/codec.ts',
    outDir: out,
    emptyOutDir: true,
    target: 'node22',
    rollupOptions: { output: { entryFileNames: 'codec.mjs' } },
  },
});
// Copy the runtime dependency graph, including pnpm's relative symlinks.
// The deployed app image does not contain the build-time package store.
const { fileList } = await nodeFileTrace([join(out, 'codec.mjs')], {
  base: resolve('.'),
});
for (const path of fileList) {
  if (!path.startsWith('node_modules/')) continue;
  const dest = join(out, path);
  await mkdir(dirname(dest), { recursive: true });
  if ((await lstat(path)).isSymbolicLink())
    await symlink(await readlink(path), dest);
  else await copyFile(path, dest);
}
