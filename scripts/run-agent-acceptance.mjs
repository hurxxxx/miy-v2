#!/usr/bin/env node
// Evaluator-side setup and independent acceptance. Never mounted into agent sandboxes.
import { spawn } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { readFile, writeFile, mkdir, chmod, copyFile, readdir } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

process.umask(0o077);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const origin = 'https://v2.dwdcc.kr';
const args = process.argv.slice(2), mode = args.shift();
const values = {};
for (let i = 0; i < args.length; i += 2) values[args[i]] = args[i + 1];
if (!['prepare', 'evaluate-v1', 'evaluate-v2', 'cleanup', 'keepalive'].includes(mode) || !values['--output']) {
  console.error('Usage: node scripts/run-agent-acceptance.mjs prepare|evaluate-v1|evaluate-v2|cleanup --output ABS [--agent codex|claude] [--cli ABS]');
  process.exit(2);
}
const output = resolve(values['--output']);
const statePath = join(output, 'private', 'state.json');
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const evaluatorSha256 = digest(await readFile(fileURLToPath(import.meta.url)));
const error = code => Object.assign(new Error(code), { safeCode: code });
function check(condition, code) { if (!condition) throw error(code); }
async function json(path) { return JSON.parse(await readFile(path, 'utf8')); }
async function save(path, data) { await writeFile(path, JSON.stringify(data, null, 2) + '\n', { mode: 0o600 }); }
async function directory(path) { await mkdir(path, { recursive: true, mode: 0o700 }); await chmod(path, 0o700); }
async function command(program, argv, { cwd = root, env = process.env, timeout = 120000 } = {}) {
  const child = spawn(program, argv, { cwd, env, stdio: ['ignore', 'pipe', 'ignore'], detached: true });
  let stdout = '', exceeded = false;
  const kill = () => { try { process.kill(-child.pid, 'SIGKILL'); } catch {} };
  const timer = setTimeout(kill, timeout);
  child.stdout.on('data', bytes => { stdout += bytes; if (stdout.length > 16 * 1024 * 1024) { exceeded = true; kill(); } });
  const code = await new Promise((yes, no) => { child.once('close', yes); child.once('error', () => no(error('process_start_failed'))); });
  clearTimeout(timer);
  check(!exceeded && code === 0, 'evaluator_command_failed');
  return stdout.trim();
}
let state, browser, operationStage='initialization';
const contexts = [];
function env(config) {
  const e = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith('MIY_')));
  return { ...e, MIY_CONFIG_DIR: config, MIY_CREDENTIAL_BACKEND: 'file' };
}
async function cli(config, argv, cwd = root) {
  const reply = JSON.parse(await command(state.cli, ['--output', 'json', '--no-input', '--context', 'default', ...argv], { cwd, env: env(config) }));
  check(reply.ok && reply.schemaVersion === 1 && reply.error === null, 'evaluator_cli_failed');
  return reply.data;
}
async function startBrowser() { browser ||= await chromium.launch({ channel: 'chrome', headless: true }); return browser; }
async function api(context, path, { method = 'GET', data, release } = {}) {
  const me = await (await context.request.get(origin + '/api/v1/me')).json();
  const response = await context.request.fetch(origin + path, { method, data,
    headers: { origin, 'x-miy-csrf': me.csrf, ...(release ? { 'x-miy-release': release } : {}) }, timeout: 30000 });
  check(response.status() >= 200 && response.status() < 300, 'acceptance_api_status_mismatch');
  const body = await response.text();
  return body.length ? JSON.parse(body) : null;
}
async function loginBrowser(account, password) {
  await startBrowser();
  const context = await browser.newContext(); contexts.push(context);
  const page = await context.newPage(); page.setDefaultTimeout(45000);
  await page.goto(origin + '/auth/login');
  await page.locator('input[name=username]').fill(account);
  await page.locator('input[name=password]').fill(password);
  await page.locator('#kc-login').click();
  await page.waitForURL(u => u.origin === origin && u.pathname === '/');
  const me = await api(context, '/api/v1/me');
  check(me.name === account && me.development === false && me.admin === (account === 'admin'), 'ordinary_oidc_identity_required');
  return { context, page, me };
}
async function loginCli(config, user) {
  await directory(config);
  await cli(config, ['context', 'init', '--url', origin]);
  const pending = await cli(config, ['auth', 'login', '--device-code', '--no-open', '--no-wait']);
  check(pending.status === 'pending' && new URL(pending.verificationUrl).origin === origin, 'unexpected_device_target');
  await user.page.goto(pending.verificationUrl);
  await user.page.locator('input[name=userCode]').fill(pending.userCode);
  await user.page.getByRole('button', { name: '이 계정으로 CLI 연결 승인', exact: true }).click();
  await user.page.getByRole('status').filter({ hasText: 'CLI 연결을 승인했습니다' }).waitFor();
  const authenticated = await cli(config, ['auth', 'wait', pending.loginId]);
  check(authenticated.status === 'authenticated' && authenticated.userId === user.me.id, 'device_identity_mismatch');
  const key = digest('session\0' + origin + '\0' + user.me.id);
  const record = await json(join(config, 'credentials', key + '.json'));
  check(record.backend === 'unix_file' && typeof record.value === 'string', 'private_credential_backend_mismatch');
  return JSON.parse(record.value);
}
function businessPrompt(agent, phase, appId) {
  const common = `당신은 제공된 MIY 프로젝트에서 업무 앱을 만드는 독립 ${agent} 개발자입니다.\n` +
    `앱 ID는 ${appId}이며 평가자가 등록과 개발 권한 승인을 준비했습니다. 이 폴더의 AGENTS.md, CLAUDE.md, miy-cli/miy-app 스킬과 MCP 계약을 먼저 읽으세요. ` +
    `실제 miy_app MCP의 get_contract, search_api, list_components, get_cli_help 도구를 사용하고 반환한 계약을 따르세요. ` +
    `SDK·Dockerfile·기존 SQL·보호 파일과 플랫폼 코드는 수정하지 마세요. 설정/로그인 토큰을 읽거나 출력하지 마세요. ` +
    `설정된 context의 miy CLI만 서버에 접근하는 데 사용하세요. 주소가 miy-acceptance.invalid인 것은 이 앱에 제한된 평가 프록시입니다. 다른 서버 주소나 계정으로 전환하지 마세요. ` +
    `운영 배포·권한 변경·앱 제거·새 등록은 수행하지 마세요. Docker/SSH/WSL을 사용할 수 없으며 서버 원격 빌드를 사용해야 합니다.\n`;
  const v1 = `업무 과제: '비품대장' Rust + React 앱을 구현하세요. 로그인 사용자는 자기 비품만 조회·수정·삭제할 수 있어야 합니다. ` +
    `비품은 UUID id, 비품명 name(공백 제외1~100자), 수량 quantity(정수0~100000), 소유자 owner를 가집니다. 소유자는 SDK Identity에서 정하고 클라이언트 입력으로 바꾸지 못하게 하세요. ` +
    `GET /assets는 {assets:[{id,name,quantity}]}를 반환합니다. POST /assets는 {name,quantity}를 받아 생성한 비품을 반환합니다. ` +
    `PATCH /assets/{id}는 name/quantity의 부분 수정을 지원하며 변경한 비품을 반환하고 DELETE /assets/{id}는 삭제합니다. ` +
    `빈 이름·음수/실수 수량·잘못된 타입은 거부하세요. 새 SQL migration으로 app 스키마에 테이블을 만드세요. ` +
    `화면 제목은 '비품대장', 입력 label은 '비품명'과 '수량', 등록 버튼은 '등록'으로 하고 목록·수정·삭제·저장 결과/오류를 제공하세요. ` +
    `제공된 공용 컴포넌트와 SDK runtime.api를 사용하세요. 의미 있는 업무 단위 검사를 추가하고 app fmt, app check 및 해당 검사를 실행하세요. ` +
    `소스를 Git에 커밋한 뒤 app build --remote --no-wait로 서버 빌드를 제출하고 job watch/get/logs로 성공을 확인하세요. ` +
    `완료된 receipt로 app deploy --environment dev --no-wait를 실행하고 job watch 및 app status로 실제 running 상태까지 확인하세요. ` +
    `단계가 실패하면 진단/복구 안내를 사용하고 실패를 숨기거나 검사를 제거하지 마세요. ` +
    `최종 답변은 실제 수행한 검사·source commit·build/job/release/image 식별자와 dev URL을 요약하세요.\n`;
  const v2 = `기존 비품대장에 선택적인 보관 위치 location(최대100자)을 추가하세요. 기존 레코드와 UUID·비품명·수량을 보존해야 합니다. ` +
    `기존 적용 migration은 수정하지 않고 새 migration을 추가하세요. API의 GET/POST/PATCH와 화면의 '보관 위치' 입력·목록 표시를 함께 갱신하세요. ` +
    `이전 레코드의 location은 null 또는 빈 문자열이어도 됩니다. 새 기능 검사와 기존 검사를 수행하고 manifest의 앱 버전을 올리세요. ` +
    `Git에 커밋한 후 서버 원격 빌드와 dev 배포를 완료하세요. 평가자가 이전 버전에서 만든 레코드는 수정·삭제하지 마세요. ` +
    `최종 답변에 실제 검사와 두 번째 source/build/job/release/image 식별자 및 데이터 보존 확인 범위를 요약하세요.\n`;
  return common + (phase === 1 ? v1 : v2);
}
async function prepare() {
  await directory(output); check((await readdir(output)).length === 0, 'output_must_be_empty');
  await directory(join(output, 'private'));
  await writeFile(join(output, '.gitignore'), '*\n');
  state = { schemaVersion: 1, startedAt: new Date().toISOString(), origin, status: 'prepared', agents: {}, cleanup: [] };
  const supplied = resolve(values['--cli'] || '.local/evidence/platform-server-20261102T115051Z/published-cli/miy');
  await directory(join(output, 'private/bin')); state.cli = join(output, 'private/bin/miy');
  await copyFile(supplied, state.cli); await chmod(state.cli, 0o700);
  state.cliSha256 = digest(await readFile(state.cli));
  const guide = await (await fetch(origin + '/api/v1/cli/install')).json();
  check(guide.cliVersion === '0.6.3' && guide.downloads.some(r => r.target === 'x86_64-unknown-linux-gnu' && r.binarySha256 === state.cliSha256), 'unverified_published_cli');
  state.publishedSourceCommit = guide.sourceCommit;
  state.repositoryCommit = await command('git', ['rev-parse', 'HEAD']);
  await save(statePath, state);
  const credentials = await json(join(root, '.local/credentials.json'));
  const developer = await loginBrowser('developer', credentials.developer);
  const admin = await loginBrowser('admin', credentials.admin);
  const developerConfig = join(output, 'private/developer-cli'), adminConfig = join(output, 'private/admin-cli');
  state.developerConfig = developerConfig; state.adminConfig = adminConfig;
  await save(statePath, state);
  await developer.context.storageState({ path: join(output, 'private/developer-browser.json') });
  await admin.context.storageState({ path: join(output, 'private/admin-browser.json') });
  await loginCli(developerConfig, developer); await loginCli(adminConfig, admin);
  const stamp = Date.now().toString(36);
  for (const agent of ['codex', 'claude']) {
    const base = join(output, 'agents', agent), app = join(base, 'app'), sandbox = join(base, 'sandbox');
    const privateRoot = join(base, 'private'); await directory(privateRoot); await directory(sandbox);
    const appId = `qa-ai-${agent}-${stamp}`, rootConfig = join(privateRoot, 'upstream-cli');
    const socket = join(base, 'proxy.sock'), proxyConfig = join(privateRoot, 'proxy.json');
    // Record the credential directory before issuing a session. Cleanup can
    // revoke through this CLI vault if preparation stops before proxy.json.
    state.agents[agent] = { appId, app, sandbox, privateRoot, rootConfig, proxyConfig, socket, status: 'preparing' };
    await save(statePath, state);
    await cli(developerConfig, ['app', 'init', app, '--id', appId, '--name', `AI ${agent} 비품대장`]);
    const registration = await cli(developerConfig, ['app', 'register'], app);
    check(typeof registration.id === 'string', 'registration_id_missing');
    await cli(adminConfig, ['registration', 'approve', registration.id]);
    const real = await loginCli(rootConfig, developer);
    const proxyToken = 'miy_access_' + randomBytes(32).toString('hex'), proxyOrigin = 'http://miy-acceptance.invalid';
    const miy = join(sandbox, 'miy'); await directory(miy); await directory(join(miy, 'credentials'));
    await save(join(miy, 'config.json'), { version: 1, clientId: randomUUID(), active: 'default', contexts: {
      default: { origin: proxyOrigin, caFile: null, userId: real.userId },
    } });
    const synthetic = { ...real, origin: proxyOrigin, accessToken: proxyToken,
      refreshToken: 'miy_refresh_' + randomBytes(32).toString('hex'), expiresAt: Math.floor(Date.now() / 1000) + 86400,
      refreshExpiresAt: Math.floor(Date.now() / 1000) + 86400 * 30, reauthRequired: false };
    const key = digest('session\0' + proxyOrigin + '\0' + real.userId);
    await save(join(miy, 'credentials', key + '.json'), { version: 1, backend: 'unix_file', value: JSON.stringify(synthetic) });
    await save(proxyConfig, { upstream: origin, appId, accessToken: real.accessToken, refreshToken: real.refreshToken,
      accessExpiresAt: real.expiresAt, proxyToken, socket, auditFile: join(base, 'proxy-audit.jsonl'), stateFile: join(base, 'proxy-state.json') });
    await command('git', ['init', '-b', 'main'], { cwd: app });
    const gitEnv = { ...process.env, GIT_AUTHOR_NAME: 'MIY acceptance', GIT_AUTHOR_EMAIL: 'acceptance@miy.invalid',
      GIT_COMMITTER_NAME: 'MIY acceptance', GIT_COMMITTER_EMAIL: 'acceptance@miy.invalid' };
    await command('git', ['add', '.'], { cwd: app, env: gitEnv });
    await command('git', ['commit', '-m', 'Initialize verified MIY bundle'], { cwd: app, env: gitEnv });
    for (const phase of [1, 2]) await writeFile(join(privateRoot, `prompt-v${phase}.txt`), businessPrompt(agent, phase, appId), { mode: 0o600 });
    state.agents[agent] = { appId, app, sandbox, privateRoot, rootConfig, proxyConfig, socket,
      bundleBeforeSha256: digest(await readFile(join(app, 'miy.bundle.json'))), status: 'prepared' };
    await save(statePath, state); console.log('PREPARED ' + agent + ' ' + appId);
  }
  await developer.context.storageState({ path: join(output, 'private/developer-browser.json') });
  await admin.context.storageState({ path: join(output, 'private/admin-browser.json') });
}
async function evaluate(phase) {
  const agent = values['--agent']; check(['codex', 'claude'].includes(agent), 'agent_required');
  const a = state.agents[agent];
  if (phase === 2) a.v1 = await json(join(output, 'agents', agent, 'v1-acceptance.json'));
  // A rerun must never leave an older PASS available to cleanup on failure.
  await save(join(output, 'agents', agent, `v${phase}-acceptance.json`), {
    status: 'RUNNING', phase, startedAt: new Date().toISOString(), evaluationHarnessSha256: evaluatorSha256,
  });
  await startBrowser();
  const context = await browser.newContext({ storageState: join(output, 'private/developer-browser.json') }); contexts.push(context);
  operationStage='application_state';
  const current = await api(context, '/api/v1/apps/' + a.appId);
  const installation = current.installations?.find(i => i.appId === a.appId && i.environment === 'dev');
  check(installation && installation.runtimeStatus === 'running' && installation.dbStatus === 'ready', 'dev_installation_not_ready');
  check(/^qa-ai-(codex|claude)-[a-z0-9]+$/.test(a.appId), 'invalid_acceptance_app_id');
  operationStage='database_identity';
  const dbQuery = `SELECT json_build_object('id',id,'name',database_name,'environment',environment) FROM db_resources WHERE app_id='${a.appId}' AND environment='dev';`;
  const database = JSON.parse(await command('docker', ['exec','-u','postgres','miy-v2-validation-postgres-1','psql','-X','-d','miy_core','-U','postgres','-A','-t','-c',dbQuery]));
  check(typeof database.id === 'string' && database.environment === 'dev', 'development_database_identity_missing');
  if (phase === 2) check(database.id === a.v1.database.id && database.name === a.v1.database.name, 'development_database_replaced');
  const path = '/api/v1/installations/' + installation.id + '/api/assets';
  const appApi = (suffix = '', method = 'GET', data) => api(context, path + suffix, { method, data, release: installation.releaseId });
  operationStage='asset_api_checks';
  let list = await appApi(); check(Array.isArray(list.assets), 'asset_list_shape');
  for (const row of list.assets) if (row.name.startsWith('qa-ui-'+agent+'-')) await appApi('/'+row.id,'DELETE');
  list = await appApi();
  const developer = await api(context, '/api/v1/me');
  for (const body of [{name:'',quantity:1},{name:'invalid-negative',quantity:-1},{name:'invalid-fraction',quantity:1.5},{name:'invalid-owner',quantity:1,owner:'forged'}]) {
    const response = await context.request.post(origin + path, {data:body,headers:{origin,'x-miy-csrf':developer.csrf,'x-miy-release':installation.releaseId}});
    check(response.status() >= 400 && response.status() < 500, 'invalid_asset_input_accepted');
  }
  check(JSON.stringify((await appApi()).assets) === JSON.stringify(list.assets), 'invalid_input_mutated_records');
  const otherContext = await browser.newContext({storageState:join(output,'private/admin-browser.json')}); contexts.push(otherContext);
  const otherList = await api(otherContext,path,{release:installation.releaseId});
  check(Array.isArray(otherList.assets) && otherList.assets.length===0,'cross_user_asset_list_leaked');

  let preserved;
  if (phase === 1) {
    const marker = 'qa-preserve-' + randomBytes(5).toString('hex');
    const created = await appApi('', 'POST', { name: marker, quantity: 3 });
    check(typeof created.id === 'string' && created.name === marker && created.quantity === 3, 'asset_create_shape');
    preserved = { id: created.id, name: marker, quantity: 3 };
    const temporary = await appApi('', 'POST', { name: 'qa-temporary-' + agent, quantity: 5 });
    check(typeof temporary.id === 'string', 'temporary_asset_missing');
    const changed = await appApi('/' + temporary.id, 'PATCH', { quantity: 6 });
    check(changed.quantity === 6, 'asset_patch_failed');
    await appApi('/' + temporary.id, 'DELETE');
    const after = await appApi(); check(!after.assets.some(x => x.id === temporary.id) && after.assets.some(x => x.id === preserved.id), 'asset_delete_or_preservation_failed');
  } else {
    const before = a.v1; check(before && installation.id === before.installation.id && installation.releaseId !== before.installation.releaseId, 'v2_installation_or_release_mismatch');
    preserved = before.preserved;
    const original = list.assets.find(x => x.id === preserved.id);
    check(original && original.name === preserved.name && original.quantity === preserved.quantity, 'v1_record_not_preserved');
    const moved = await appApi('/' + preserved.id, 'PATCH', { location: '본사 창고' });
    check(moved.location === '본사 창고' && moved.name === preserved.name && moved.quantity === preserved.quantity, 'location_patch_or_data_preservation_failed');
    const temporary = await appApi('', 'POST', { name: 'qa-location-' + agent, quantity: 2, location: '지점' });
    check(temporary.location === '지점', 'location_create_failed');
    await appApi('/' + temporary.id, 'DELETE');
  }
  operationStage='cross_user_write_checks';
  const other = await api(otherContext,'/api/v1/me');
  const forbiddenUpdate = await otherContext.request.patch(origin+path+'/'+preserved.id,{data:{quantity:99},headers:{origin,'x-miy-csrf':other.csrf,'x-miy-release':installation.releaseId}});
  check(forbiddenUpdate.status()===404,'cross_user_asset_update_allowed');
  const forbiddenDelete = await otherContext.request.delete(origin+path+'/'+preserved.id,{headers:{origin,'x-miy-csrf':other.csrf,'x-miy-release':installation.releaseId}});
  check(forbiddenDelete.status()===404,'cross_user_asset_delete_allowed');
  const unchanged=(await appApi()).assets.find(x=>x.id===preserved.id);
  check(unchanged?.name===preserved.name && unchanged.quantity===preserved.quantity,'denied_cross_user_write_mutated_record');
  operationStage='build_deploy_provenance';
  const proxyState = await json(join(output,'agents',agent,'proxy-state.json'));
  let matchingBuild, matchingDeploy;
  for (const id of proxyState.builds || []) {
    const build = await api(context,'/api/v1/builds/'+id);
    if (build.sourceCommit===installation.sourceCommit && build.imageId===installation.imageId && build.status==='succeeded') matchingBuild=build;
  }
  for (const id of proxyState.runtimeJobs || []) {
    const job = await api(context,'/api/v1/runtime/jobs/'+id);
    if (job.releaseId===installation.releaseId && job.imageId===installation.imageId && job.status==='succeeded' && job.environment==='dev' && job.action==='deploy') matchingDeploy=job;
  }
  check(matchingBuild && matchingDeploy,'build_deploy_installation_provenance_mismatch');
  if(phase===2) check(await command('git',['rev-parse','HEAD'],{cwd:a.app})===installation.sourceCommit,'deployed_source_is_not_agent_head');
  const page = await context.newPage(); page.setDefaultTimeout(45000);
  const url = origin + '/environments/dev/apps/' + a.appId;
  operationStage='browser_list';
  await page.goto(url); await page.getByRole('heading', { name: '비품대장', exact: true }).waitFor();
  await page.getByText(preserved.name, { exact: true }).waitFor();
  if (phase === 2) await page.getByText('본사 창고', { exact: true }).waitFor();
  await page.reload(); await page.getByText(preserved.name, { exact: true }).waitFor();
  operationStage='browser_form_create';
  const uiMarker='qa-ui-'+agent+'-'+randomBytes(3).toString('hex');
  await page.getByLabel('비품명',{exact:true}).first().fill(uiMarker);
  await page.getByLabel('수량',{exact:true}).first().fill('4');
  if(phase===2) await page.getByLabel('보관 위치',{exact:true}).first().fill('임시 보관');
  await page.getByRole('button',{name:'등록',exact:true}).click();
  await page.getByText(uiMarker,{exact:true}).waitFor();
  const uiRecord=(await appApi()).assets.find(x=>x.name===uiMarker);
  check(uiRecord?.quantity===4,'browser_create_did_not_reach_database');
  operationStage='browser_form_edit';
  const row=page.locator('tr').filter({hasText:uiMarker}).first();
  const rowIndex=await page.locator('tr').evaluateAll((rows,marker)=>rows.findIndex(r=>r.textContent.includes(marker)),uiMarker);
  check(rowIndex>=0,'browser_edit_row_missing');
  await row.getByRole('button',{name:/수정/}).click();
  // Editing renders the name as an input value, so a text-filtered row no longer matches.
  const editingRow=page.locator('tr').nth(rowIndex);
  if(await editingRow.locator('input[type=number]').count()) {
    await editingRow.locator('input[type=number]').fill('7');
    await editingRow.getByRole('button',{name:/저장/}).click();
  } else {
    // A valid alternative reuses the create form for the selected record.
    const quantity=page.getByLabel('수량',{exact:true}).first();
    await quantity.fill('7');
    await quantity.locator('xpath=ancestor::form[1]').locator('button[type=submit]').click();
  }
  await page.getByText(uiMarker,{exact:true}).waitFor();
  let updated=false;
  for(let attempt=0;attempt<100;attempt++) { if((await appApi()).assets.find(x=>x.id===uiRecord.id)?.quantity===7){updated=true;break;}await page.waitForTimeout(100); }
  check(updated,'browser_edit_did_not_reach_database');
  operationStage='browser_form_delete';
  page.once('dialog',dialog=>dialog.accept());
  await page.locator('tr').filter({hasText:uiMarker}).first().getByRole('button',{name:/삭제/}).click();
  await page.getByText(uiMarker,{exact:true}).waitFor({state:'detached'});
  check(!(await appApi()).assets.some(x=>x.id===uiRecord.id),'browser_delete_did_not_reach_database');
  operationStage='recording_artifacts';
  await page.screenshot({ path: join(output, 'agents', agent, `v${phase}-desktop.png`), fullPage: true });
  check(digest(await readFile(join(a.app, 'miy.bundle.json'))) === a.bundleBeforeSha256, 'protected_bundle_lock_changed');
  const bundle = await json(join(a.app, 'miy.bundle.json'));
  for (const [path, hash] of Object.entries(bundle.protectedFiles)) {
    check(digest(await readFile(join(a.app, path))) === hash, 'protected_bundle_file_changed');
  }
  await command('git', ['diff', '--exit-code', 'HEAD', '--'], { cwd: a.app });
  const record = { status: 'PASS', checkedAt: new Date().toISOString(), evaluationHarnessSha256:evaluatorSha256, phase, url,
    build: {id:matchingBuild.id,sourceCommit:matchingBuild.sourceCommit,imageId:matchingBuild.imageId,status:matchingBuild.status,platform:matchingBuild.platform},
    deployJob:{id:matchingDeploy.id,status:matchingDeploy.status,releaseId:matchingDeploy.releaseId,imageId:matchingDeploy.imageId},
    installation: { id: installation.id, releaseId: installation.releaseId, imageId: installation.imageId,
      sourceCommit: installation.sourceCommit, dbStatus: installation.dbStatus, runtimeStatus: installation.runtimeStatus },
    database, preserved, checks: ['authenticated_developer_api', 'invalid_input_rejected_without_mutation', 'cross_user_list_and_update_delete_denied', 'create_update_delete', 'browser_list_and_reload', 'browser_form_create_edit_delete_reaches_database', 'build_deploy_image_and_source_match', 'protected_bundle_lock_and_file_hashes_preserved', 'tracked_source_matches_agent_commit',
      ...(phase === 2 ? ['same_installation_new_release', 'v1_uuid_name_quantity_preserved', 'new_location_create_update'] : [])] };
  a['v' + phase] = record; a.status = phase === 2 ? 'PASS' : 'v1_passed';
  // Each concurrent evaluator owns only its agent/phase report. Aggregate on cleanup.
  await save(join(output, 'agents', agent, `v${phase}-acceptance.json`), record);
  console.log('PASS ' + agent + ' v' + phase + ' ' + url);
}
async function cleanup() {
  await startBrowser();
  state.cleanup ||= []; state.cleanupFailures ||= [];
  const attempt=async(label,action)=>{try{await action();state.cleanup.push(label);}catch{state.cleanupFailures.push(label);}await save(statePath,state);};
  for (const [agent,a] of Object.entries(state.agents)) {
    for(const phase of [1,2]){try{a['v'+phase]=await json(join(output,'agents',agent,`v${phase}-acceptance.json`));}catch(e){if(e.code!=='ENOENT')throw e;}}
    a.status=a.v2?.status==='PASS'?'PASS':a.v1?.status==='PASS'?'v1_passed':'prepared';
    await attempt(agent+':proxy_cli_revoked',async()=>{
      let config;
      try{config=await json(a.proxyConfig);}catch(failure){
        if(failure.code!=='ENOENT')throw failure;
        // An interrupted prepare has not copied its token into proxy.json yet.
        await cli(a.rootConfig,['auth','logout']);return;
      }
      const response=await fetch(origin+'/auth/cli/revoke',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({refreshToken:config.refreshToken}),signal:AbortSignal.timeout(20000)});
      check(response.ok,'owned_proxy_cli_session_revoke_failed');
    });
    await attempt(agent+':upstream_local_credentials_removed',()=>cli(a.rootConfig,['auth','logout','--local-only']));
  }
  for(const account of ['developer','admin']){
    await attempt(account+':cli_logged_out',()=>cli(state[account+'Config'],['auth','logout']));
    await attempt(account+':platform_browser_logged_out',async()=>{
      const context=await browser.newContext({storageState:join(output,'private/'+account+'-browser.json')});contexts.push(context);
      const response=await context.request.get(origin+'/api/v1/me');
      if(response.status()===200){const me=await response.json();const out=await context.request.post(origin+'/auth/logout',{headers:{origin,'x-miy-csrf':me.csrf}});check(out.status()===200,'owned_browser_logout_failed');}
      else check(response.status()===401,'owned_browser_session_unavailable');
      check((await context.request.get(origin+'/api/v1/me')).status()===401,'owned_browser_still_authenticated');
    });
  }
  state.finishedAt=new Date().toISOString();state.status=Object.values(state.agents).every(a=>a.status==='PASS') && !state.cleanupFailures.length?'PASS':'INCOMPLETE';
  await save(statePath,state);console.log('CLEANUP '+state.status);check(!state.cleanupFailures.length,'acceptance_cleanup_incomplete');
}

try {
  if (mode === 'prepare') await prepare();
  else { state = await json(statePath); if (mode === 'cleanup') await cleanup(); else if (mode === 'keepalive') { await startBrowser(); for (const account of ['developer', 'admin']) { const context = await browser.newContext({ storageState: join(output, 'private/' + account + '-browser.json') }); contexts.push(context); await api(context, '/api/v1/me'); } console.log('PASS evaluator browser sessions active'); } else await evaluate(mode === 'evaluate-v1' ? 1 : 2); }
} catch (failure) {
  const record={ status: 'FAIL', operation: mode, checkedAt:new Date().toISOString(), evaluationHarnessSha256:evaluatorSha256,
    code: failure.safeCode || 'acceptance_failed', stage:operationStage, errorType:failure.constructor?.name || 'Error' };
  if(['evaluate-v1','evaluate-v2'].includes(mode) && ['codex','claude'].includes(values['--agent']) && state?.agents?.[values['--agent']]) {
    await save(join(output,'agents',values['--agent'],mode.replace('evaluate-','')+'-acceptance.json'),record).catch(()=>{});
  }
  console.error(JSON.stringify(record)); process.exitCode = 1;
} finally {
  for (const context of contexts) await context.close().catch(() => {});
  if (browser) await browser.close().catch(() => {});
}
