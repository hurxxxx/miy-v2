import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { randomUUID, createHash } from 'node:crypto';
const require = createRequire('/projects/miy-v2-cli-experience-20261029/package.json');
const { chromium, expect: baseExpect, request } = require('@playwright/test');
const expect = baseExpect.configure({timeout:20000});
const dir='/projects/miy-cli-experience-run-20261029/evidence';
const origin='https://v2.dwdcc.kr', app='todo-win-20261029';
const before=JSON.parse(await readFile(dir+'/live-before.json','utf8'));
const report={startedAt:new Date().toISOString(),browser:'Linux Playwright Chromium',origin,checks:[],screenshots:[],failures:[],notRun:['Human IdP successful login and replay of its actual consumed callback; synthetic invalid callback checks are separate.']};
const browser=await chromium.launch({headless:true});
const context=await browser.newContext({viewport:{width:1365,height:1000}});
const client=await request.newContext({baseURL:origin});
const page=await context.newPage();
let me, refreshToken;
async function save(){await writeFile(dir+'/live-after.json',JSON.stringify(report,null,2)+'\n');}
async function check(name,fn){try{await fn();report.checks.push({name,status:'PASS'});console.log('PASS '+name);}catch(e){report.checks.push({name,status:'FAIL',category:e.name});report.failures.push(name);console.error('FAIL '+name+' '+e.name);throw e;}finally{await save();}}
async function shot(name){await page.screenshot({path:dir+'/'+name,fullPage:true});report.screenshots.push(name);}
async function json200(response){expect(response.status()).toBe(200);return response.json();}
async function login(p,ctx,label=/관리자로 시작/){await p.goto(origin);await p.getByRole('button',{name:label}).click();await expect(p.getByRole('button',{name:'로그아웃',exact:true})).toBeVisible();return json200(await ctx.request.get(origin+'/api/v1/me'));}
async function logout(ctx,identity){if(identity?.csrf)expect((await ctx.request.post(origin+'/auth/logout',{headers:{origin,'x-miy-csrf':identity.csrf}})).status()).toBe(200);}
async function status(id,expected){const r=await context.request.get(origin+'/api/v1/cli/devices/'+id);expect(r.headers()['cache-control']).toBe('no-store');const value=await json200(r);expect(Object.keys(value).sort()).toEqual(['id','clientName','status','expiresAt','expiresIn'].sort());expect(value.status).toBe(expected);return value;}
try {
 await check('same-admin-login',async()=>{me=await login(page,context);report.identity={id:me.id,name:me.name,admin:me.admin,development:me.development};expect(report.identity).toEqual(before.identity);});
 await check('preserved-release-database-and-all-todos',async()=>{
  const state=await json200(await context.request.get(origin+'/api/v1/apps/'+app));
  const old=before.installations.find(i=>i.environment==='dev'), current=state.installations.find(i=>i.environment==='dev');
  const keys=['id','appId','environment','releaseId','imageId','version','sourceCommit','generation','desiredState','runtimeStatus','dbStatus'];
  report.installation=Object.fromEntries(keys.map(k=>[k,current[k]]));
  expect(report.installation).toEqual(Object.fromEntries(keys.map(k=>[k,old[k]])));
  const p=await json200(await context.request.post(origin+'/api/v1/apps/'+app+'/runtime/preflight',{headers:{origin,'x-miy-csrf':me.csrf},data:{target:{appId:app,environment:'dev'},action:'start'}}));
  report.database=p.expectedState.database;expect(p.expectedState).toEqual(before.preflight.expectedState);
  const todos=await json200(await context.request.get(origin+'/api/v1/installations/'+current.id+'/api/todos'));
  report.todos=todos.items;expect(todos.items).toEqual(before.todos);
 });
 await check('desktop-todo-screen',async()=>{
  await page.goto(origin+'/environments/dev/apps/'+app);
  await expect(page.getByRole('heading',{name:'오늘의 할 일',exact:true})).toBeVisible();
  await expect(page.getByRole('region',{name:'내 할 일 목록'})).toHaveAttribute('aria-busy','false');
  await expect(page.locator('[data-todo-id="8dcfffe2-3854-4ce5-b878-2da9f915c151"]').getByRole('checkbox')).not.toBeChecked();
  await expect(page.locator('#platform-navigation')).toBeVisible();await shot('live-todo-desktop.png');
 });
 await check('390px-todo-and-collapsible-navigation',async()=>{
  await page.setViewportSize({width:390,height:844});
  await expect(page.locator('#platform-navigation')).toBeHidden();
  const toggle=page.getByRole('button',{name:'메뉴 열기',exact:true});await expect(toggle).toBeVisible();
  const heading=await page.getByRole('heading',{name:'오늘의 할 일',exact:true}).boundingBox();
  expect(heading.x).toBeGreaterThanOrEqual(0);expect(heading.x+heading.width).toBeLessThanOrEqual(390);
  const widths=await page.evaluate(()=>({body:document.documentElement.scrollWidth,viewport:innerWidth}));
  expect(widths.body).toBeLessThanOrEqual(widths.viewport);report.mobileWidths=widths;
  await shot('live-todo-390px.png');await toggle.click();await expect(page.locator('#platform-navigation')).toBeVisible();await shot('live-menu-390px.png');
  await page.getByRole('button',{name:'메뉴 닫기',exact:true}).click();await expect(page.locator('#platform-navigation')).toBeHidden();
 });
 await check('naturally-expired-login-request-and-no-secret-status',async()=>{
  expect(Date.now()-Date.parse(before.expiryRequest.createdAt)).toBeGreaterThan(301000);
  const response=await context.request.get(origin+'/api/v1/cli/devices/'+before.expiryRequest.id);
  expect([200,404]).toContain(response.status());
  if(response.status()===200){const value=await status(before.expiryRequest.id,'expired');expect(value.expiresIn).toBe(0);report.expiredRequest={id:value.id,status:value.status,expiresAt:value.expiresAt,expiresIn:value.expiresIn};}
  else report.expiredRequest={id:before.expiryRequest.id,status:'expired-row-already-cleaned',httpStatus:404};
  await page.goto(origin+before.expiryRequest.verificationPath);
  await expect(page.getByRole('status')).toContainText('로그인 요청이 만료');
  await expect(page.getByLabel('인증 코드')).toHaveCount(0);await shot('live-expired-request-390px.png');
 });
 await check('live-device-status-browser-only-code-repair-and-consumed-state',async()=>{
  const device=await json200(await client.post('/auth/cli/device',{data:{clientId:randomUUID(),clientName:'CLI 0.6.3 live status regression'}}));
  expect((await client.get('/api/v1/cli/devices/'+device.id)).status()).toBe(401);
  await status(device.id,'pending');await page.goto(origin+device.verificationPath);
  await expect(page.getByRole('timer')).toBeVisible();await shot('live-pending-request-390px.png');
  await page.getByLabel('인증 코드').fill(device.userCode==='AAAAA-AAAAA'?'BBBBB-BBBBB':'AAAAA-AAAAA');
  await page.getByRole('button',{name:'이 계정으로 CLI 연결 승인'}).click();
  await expect(page.getByRole('alert')).toContainText('코드가 일치하지 않습니다');
  await page.getByLabel('인증 코드').fill(device.userCode);await page.getByRole('button',{name:'이 계정으로 CLI 연결 승인'}).click();
  await expect(page.getByRole('status')).toContainText('CLI 연결을 승인했습니다');await status(device.id,'approved');
  const other=await browser.newContext();let otherMe;try{const p=await other.newPage();otherMe=await login(p,other,/사용자 A/);expect((await other.request.get(origin+'/api/v1/cli/devices/'+device.id)).status()).toBe(404);}finally{await logout(other,otherMe);await other.close();}
  const tokens=await json200(await client.post('/auth/cli/token',{data:{grantType:'device_code',id:device.id,deviceCode:device.deviceCode}}));
  refreshToken=tokens.refreshToken;expect(tokens.userId).toBe(me.id);
  expect((await client.get('/api/v1/cli/devices/'+device.id,{headers:{authorization:'Bearer '+tokens.accessToken}})).status()).toBe(403);
  await status(device.id,'consumed');await page.reload();await expect(page.getByRole('status')).toContainText('이미 승인한 요청');
  await expect(page.getByLabel('인증 코드')).toHaveCount(0);await shot('live-consumed-request-390px.png');
  report.deviceStatus={anonymous:401,bearer:403,otherApprovedUser:404,cacheControl:'no-store',secretsAbsent:true,statuses:['pending','approved','consumed']};
 });
 await check('actual-server-invalid-oauth-callback-clears-cookie-and-fixed-recovery',async()=>{
  const recovery=await browser.newContext({viewport:{width:390,height:844}});
  try {
   const initiated=await recovery.request.get(origin+'/auth/login',{maxRedirects:0});expect(initiated.status()).toBe(303);
   expect((await recovery.cookies(origin+'/auth')).some(c=>c.name==='miy_login')).toBe(true);
   for(const path of ['/auth/callback?error=expired_code&state=untrusted','/auth/callback?code=consumed&state=unknown','/auth/callback']){
    const r=await recovery.request.get(origin+path,{maxRedirects:0});expect(r.status()).toBe(303);expect(r.headers().location).toBe('/?login=expired');expect(r.headers()['cache-control']).toBe('no-store');
    expect((await recovery.cookies(origin+'/auth')).some(c=>c.name==='miy_login')).toBe(false);
   }
   expect((await recovery.request.get(origin+'/api/v1/me')).status()).toBe(401);
   const p=await recovery.newPage();await p.goto(origin+'/?login=expired');await expect(p.getByRole('alert')).toContainText('로그인 요청이 만료됐거나 이미 처리됐습니다');
   await p.screenshot({path:dir+'/live-oauth-recovery-390px.png',fullPage:true});report.screenshots.push('live-oauth-recovery-390px.png');
   await p.goto(origin+'/auth/login');await expect(p.locator('input[name="username"]')).toBeVisible();
   await p.screenshot({path:dir+'/live-idp-login-awaiting-user.png',fullPage:true});report.screenshots.push('live-idp-login-awaiting-user.png');
   report.idpLoginFormReached=true;
  }finally{await recovery.close();}
 });
 await check('installation-guide-and-installer-download-actual-sha256',async()=>{
  const guide=await json200(await client.get('/api/v1/cli/install'));expect(guide.cliVersion).toBe('0.6.3');expect(guide.sourceCommit).toBe('8d7d04271f9467d8bbdd16308e73715b33e4dc0c');
  report.installGuide=guide;
  await page.goto(origin+'/cli/install');await expect(page.getByRole('link',{name:'Windows 설치 파일 다운로드'})).toBeVisible();
  await shot('live-install-guide-390px.png');
  const link=await page.getByRole('link',{name:'Windows 설치 파일 다운로드'}).getAttribute('href');
  const response=await client.get(new URL(link,origin).href);expect(response.status()).toBe(200);
  const bytes=await response.body();const digest=createHash('sha256').update(bytes).digest('hex');
  const index=JSON.parse(await readFile('/projects/miy-v2/.local/cli-installers/installer-index.json','utf8'));expect(digest).toBe(index.sha256);
  report.installerDownload={url:new URL(link,origin).href,sha256:digest,bytes:bytes.length};
 });
}catch(e){process.exitCode=1;}
finally {
 if(refreshToken){try{expect((await client.post('/auth/cli/revoke',{data:{refreshToken}})).status()).toBe(200);report.sessionRevoked=true;}catch{report.failures.push('revoke-test-session');process.exitCode=1;}}
 try{await logout(context,me);report.developmentSessionsLoggedOut=true;}catch{report.failures.push('logout-test-development-session');process.exitCode=1;}
 report.finishedAt=new Date().toISOString();report.status=report.failures.length?'FAIL':'PASS';await save();await client.dispose();await context.close();await browser.close();
}
