import assert from 'node:assert/strict';
import {frameDiagnostic} from './lib/frame-diagnostics.mjs';

const capability = 'a'.repeat(43);
const uri = `https://app.example.test/_miy/a/${capability}/ui/chunk.js`;
assert.equal(frameDiagnostic(`Failed import ${uri}?x=1`),
  'Failed import https://app.example.test/_miy/a/[redacted]/ui/chunk.js?x=1');
assert.equal(frameDiagnostic(`Failed ${uri}\nFailed ${uri}`),
  'Failed https://app.example.test/_miy/a/[redacted]/ui/chunk.js\nFailed https://app.example.test/_miy/a/[redacted]/ui/chunk.js');
for (const kind of ['frame', 'access', 'refresh', 'call', 'delegate']) {
  const token = `miy_${kind}_${'b'.repeat(43)}`;
  assert.equal(frameDiagnostic(`Bearer ${token}`), 'Bearer [redacted credential]');
  assert(!frameDiagnostic('x'.repeat(280) + token).slice(0, 300).includes('b'.repeat(10)));
}
assert.equal(frameDiagnostic('HTTP 401 /_miy/api/tasks'), 'HTTP 401 /_miy/api/tasks');
console.log('PASS frame diagnostics redact asset capabilities and credentials before reporting or truncating');
