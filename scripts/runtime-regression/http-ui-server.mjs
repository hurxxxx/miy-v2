// Isolated output and port: this never replaces a running platform UI.
import { execFileSync, spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const site = resolve(root, '.local/evidence/http-ip-ui/site');
execFileSync('pnpm', ['--filter', '@miy/platform', 'exec', 'vite', 'build', '--outDir', site, '--emptyOutDir'], { cwd: root, stdio: 'inherit' });
const child = spawn('pnpm', ['--filter', '@miy/platform', 'exec', 'vite', 'preview', '--outDir', site, '--host', '0.0.0.0', '--port', '18149', '--strictPort'], { cwd: root, stdio: 'inherit' });
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
child.on('exit', code => process.exit(code ?? 0));
