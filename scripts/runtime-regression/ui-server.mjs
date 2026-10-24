// Build and serve isolated test outputs; never replace the running platform UI.
import { execFileSync, spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const site = resolve(root, '.local/evidence/runtime-health-ui/site');
const remote = resolve(root, '.local/evidence/runtime-health-ui/remote');
for (const [project, mode, out] of [['@miy/platform', 'production', site], ['@miy/fixtures', 'health', remote]]) {
  execFileSync('pnpm', ['--filter', project, 'exec', 'vite', 'build', '--mode', mode, '--outDir', out, '--emptyOutDir'], { cwd: root, stdio: 'inherit' });
}
const child = spawn('pnpm', ['--filter', '@miy/platform', 'exec', 'vite', 'preview', '--outDir', site, '--host', '127.0.0.1', '--port', '18147', '--strictPort'], { cwd: root, stdio: 'inherit' });
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
child.on('exit', code => process.exit(code ?? 0));
