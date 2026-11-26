// SDK behavior checks without a platform, credential files or a production build.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const frameRequire = createRequire(new URL('../web/frame-runtime/package.json', import.meta.url));
const { transformWithOxc } = await import(pathToFileURL(frameRequire.resolve('vite')).href);

async function sourceModule(name, instance = '') {
  const source = readFileSync(new URL(`../packages/sdk/src/${name}.ts`, import.meta.url), 'utf8');
  const { code: javascript } = await transformWithOxc(source, `${name}.ts`);
  return import(`data:text/javascript;base64,${Buffer.from(javascript).toString('base64')}#${instance}`);
}
const transport = await sourceModule('transport');
const { completeDirectoryPage, directoryQuery } = await sourceModule('directory');
const { ApiError, isApiError, request, requestBlob, readJson, readBlob, withAbort } = transport;
const originalFetch = globalThis.fetch;
const passed = [];
async function check(name, run) {
  try { await run(); passed.push(name); }
  finally { globalThis.fetch = originalFetch; }
}
const matches = (kind, code, status = null) => (error) => {
  assert(error instanceof ApiError);
  assert.equal(error.kind, kind);
  assert.equal(error.code, code);
  assert.equal(error.status, status);
  assert(!JSON.stringify(error).includes('PRIVATE-BODY'));
  return true;
};

await check('a federated app recognizes the frame SDK error without sharing its constructor', async () => {
  const frameTransport = await sourceModule('transport', 'frame');
  const frameError = new frameTransport.ApiError('http', 'file_download_failed', 409, 'request-1');
  assert.equal(frameError instanceof ApiError, false);
  assert(isApiError(frameError));
  assert.equal(isApiError(Object.assign(new Error(), { name: 'ApiError' })), false);
  globalThis.fetch = async () => { throw frameError; };
  await assert.rejects(request('/resource'), (error) => error === frameError);
});

await check('JSON/Headers handling preserves caller headers and host CSRF', async () => {
  let sent;
  globalThis.fetch = async (path, init) => {
    sent = { path, init };
    return new Response('{"saved":true}', { headers: { 'content-type': 'application/json' } });
  };
  assert.deepEqual(await request('/resource', 'host-csrf', {
    method: 'POST', body: '{}', headers: new Headers({ 'x-custom': 'value', 'x-miy-csrf': 'caller' }),
  }), { saved: true });
  assert.equal(sent.init.headers.get('content-type'), 'application/json');
  assert.equal(sent.init.headers.get('x-custom'), 'value');
  assert.equal(sent.init.headers.get('x-miy-csrf'), 'host-csrf');
  assert.equal(sent.init.credentials, 'same-origin');
});

await check('FormData reaches a real loopback server with its browser boundary', async () => {
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks).toString('utf8');
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ contentType: req.headers['content-type'], hasField: body.includes('field-value') }));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const data = new FormData();
    data.set('field', 'field-value');
    const result = await request(`http://127.0.0.1:${server.address().port}/upload`, undefined,
      { method: 'POST', body: data });
    assert.match(result.contentType, /^multipart\/form-data; boundary=/);
    assert.equal(result.hasField, true);
  } finally { await new Promise((resolve) => server.close(resolve)); }
});

await check('204/empty JSON and binary responses retain their own contracts', async () => {
  assert.equal(await readJson(new Response(null, { status: 204 })), undefined);
  assert.equal(await readJson(new Response('')), undefined);
  assert.equal(await readJson(new Response('  ')), undefined);
  const data = await readBlob(new Response(new Uint8Array([0, 1, 255]), { headers: { 'content-type': 'image/png' } }));
  assert.equal(data.type, 'image/png');
  assert.deepEqual([...new Uint8Array(await data.arrayBuffer())], [0, 1, 255]);
  globalThis.fetch = async (_path, init) => {
    assert.equal(init.headers.get('content-type'), 'text/plain');
    return new Response('binary');
  };
  assert.equal(await (await requestBlob('/file', undefined, { headers: [['content-type', 'text/plain']] })).text(), 'binary');
});

await check('HTTP failures preserve status/code/request ID without write retries', async () => {
  for (const status of [401, 403, 409, 413, 429, 503]) {
    for (const method of ['GET', 'POST']) {
      let count = 0;
      globalThis.fetch = async () => {
        count++;
        return new Response('{"error":{"code":"policy_denied","message":"PRIVATE-BODY"}}',
          { status, headers: { 'x-request-id': 'request-42' } });
      };
      await assert.rejects(request('/action', undefined, { method }), (error) => {
        matches('http', 'policy_denied', status)(error);
        assert.equal(error.requestId, 'request-42');
        assert.equal(error.retryable, method === 'GET' && [429, 503].includes(status));
        return true;
      });
      assert.equal(count, 1);
    }
  }
});

await check('malformed success and proxy HTML failures do not leak raw content', async () => {
  await assert.rejects(readJson(new Response('<h1>PRIVATE-BODY</h1>')), matches('parse', 'invalid_json_response', 200));
  await assert.rejects(readJson(new Response('PRIVATE-BODY', { status: 503 })), matches('http', 'http_503', 503));
  await assert.rejects(readJson(new Response('{"error":{"code":"PRIVATE-BODY <html>"}}', { status: 500 })),
    matches('http', 'http_500', 500));
});

await check('network exceptions are sanitized and writes are never marked retryable', async () => {
  globalThis.fetch = async () => { throw new Error('PRIVATE-BODY url and credentials'); };
  await assert.rejects(request('/read'), (error) => {
    matches('network', 'network_unavailable')(error);
    assert.equal(error.retryable, true);
    return true;
  });
  await assert.rejects(request('/write', undefined, { method: 'PUT' }), (error) => {
    matches('network', 'network_unavailable')(error);
    assert.equal(error.retryable, false);
    return true;
  });
});

await check('aborted requests never send or expose abort reasons', async () => {
  const controller = new AbortController();
  controller.abort('PRIVATE-BODY');
  globalThis.fetch = async () => { assert.fail('aborted request was sent'); };
  await assert.rejects(request('/never', undefined, { signal: controller.signal }), matches('abort', 'request_aborted'));
  await assert.rejects(readJson(new Response('{}'), { signal: controller.signal }), matches('abort', 'request_aborted'));
});

await check('one cancelled consumer does not cancel shared renewal', async () => {
  let finish;
  const shared = new Promise((resolve) => { finish = resolve; });
  const controller = new AbortController();
  const first = withAbort(shared, controller.signal);
  const second = withAbort(shared);
  controller.abort();
  await assert.rejects(first, matches('abort', 'request_aborted'));
  finish('renewed');
  assert.equal(await second, 'renewed');
});

await check('directory values cannot inject routes or another user selector', async () => {
  const query = directoryQuery({ kind: 'users', q: ' name &userId=other ', cursor: 'cursor&kind=groups', ids: ['user&admin=true'] });
  const parsed = new URL(`https://miy.invalid/platform/directory${query}`);
  assert.equal(parsed.pathname, '/platform/directory');
  assert.equal(parsed.searchParams.get('q'), 'name &userId=other');
  assert.equal(parsed.searchParams.get('cursor'), 'cursor&kind=groups');
  assert.equal(parsed.searchParams.get('ids'), 'user&admin=true');
  assert.equal(parsed.searchParams.get('userId'), null);
  for (const invalid of [
    { limit: 0 }, { limit: 101 }, { limit: 1.5 }, { kind: 'admin' },
    { kind: 'memberships', q: 'search' }, { ids: [] }, { ids: ['a,b'] },
    { ids: new Array(101).fill('id') }, { q: '한'.repeat(255) }, { cursor: '' },
  ]) assert.throws(() => directoryQuery(invalid), /invalid_directory_query/);
});

await check('an untyped directory read never returns a first page as the whole list', async () => {
  const whole = { users: [{ id: 'a' }], nextCursor: null };
  assert.equal(completeDirectoryPage(whole), whole);
  const first = { users: [{ id: 'a' }], nextCursor: 'next' };
  assert.throws(() => completeDirectoryPage(first), /directory_page_incomplete/);
  // A caller that passes a query receives pages and continues them itself.
  assert.equal(completeDirectoryPage(first, { kind: 'users' }), first);
});

await check('generated-app helpers match the maintained SDK source', async () => {
  for (const name of ['transport', 'directory'])
    assert.equal(readFileSync(new URL(`../packages/sdk/src/${name}.ts`, import.meta.url), 'utf8'),
      readFileSync(new URL(`../templates/rust-react/sdk/browser/${name}.ts`, import.meta.url), 'utf8'));
});

console.log(`PASS SDK transport and directory: ${passed.length} behavior groups`);
