import { fixtureEnv, workflowArgs, workflowData, collectApiPage } from "./lib/cli-workflow.mjs";
// Platform/CLI regression probe. Creates only uniquely named temporary fixtures and cleans them.
import assert from 'node:assert/strict';
import {CLI_EXIT,cliData,assertCliFailure} from './lib/cli-result.mjs';
import {execFileSync,spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,copyFileSync,openSync,closeSync,existsSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {chromium} from '@playwright/test';
const root=resolve(import.meta.dirname,'..');
const evidence=join(root,'.local/evidence/operations',Date.now().toString(36));mkdirSync(evidence,{recursive:true});
const appId='ops-probe-'+Date.now().toString(36),app=join(evidence,'app'),adminDir=join(evidence,'admin');mkdirSync(adminDir);
const binary=join(evidence,'miy');copyFileSync(join(root,'target/debug/miy'),binary);
const origin='http://localhost:18000',env=fixtureEnv(evidence,{...process.env,PATH:`${process.env.HOME}/.cargo/bin:${process.env.PATH}`,CI:'true',CARGO_TARGET_DIR:join(root,'.local/operations-cargo')});
let sequence=0,preview,local,browser,installation,image;const checks=[];
const ok=message=>{checks.push(message);console.log('PASS '+message);};
const hash=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
function run(program,args,cwd=root){const fd=openSync(join(evidence,`command-${++sequence}.log`),'w');try{return execFileSync(program,args,{cwd,env,encoding:'utf8',stdio:['ignore','pipe',fd],maxBuffer:8*1024*1024});}finally{closeSync(fd);}}
function cli(args,cwd=app){return workflowData(cliData(run(binary,workflowArgs(args,cwd,env,binary),cwd)),args);}
function fails(args,expectedExit,pattern,cwd=app){let error;try{execFileSync(binary,workflowArgs(args,cwd,env,binary),{cwd,env,encoding:'utf8',stdio:['ignore','pipe','pipe']});}catch(e){error=e;}return assertCliFailure(error,expectedExit,pattern);}
async function until(check,message,ms=180000){const end=Date.now()+ms;while(Date.now()<end){if(await check())return;await new Promise(r=>setTimeout(r,100));}throw Error(message);}
async function login(account){const r=await fetch(origin+'/auth/development',{method:'POST',headers:{origin,'content-type':'application/json'},body:JSON.stringify({account})});assert.equal(r.status,200);const cookie=r.headers.get('set-cookie').split(';')[0];const me=await(await fetch(origin+'/api/v1/me',{headers:{cookie}})).json();return {cookie,csrf:me.csrf};}
async function api(user,path,body,status=200){const r=await fetch(origin+path,{method:body?'POST':'GET',headers:{cookie:user.cookie,origin,'x-miy-csrf':user.csrf,'content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});assert.equal(r.status,status,path);const value=await r.json();return body?value:collectApiPage(value,path,async next=>{const response=await fetch(origin+next,{headers:{cookie:user.cookie}});assert.equal(response.status,200);return response.json();});}
function sql(query,database='miy_core'){return execFileSync('docker',['exec','-i','miy-v2-validation-postgres-1','psql','-U','postgres','-d',database,'-At','-v','ON_ERROR_STOP=1'],{input:query,encoding:'utf8',stdio:['pipe','pipe','pipe']}).trim();}
async function startPreview(){const fd=openSync(join(evidence,`preview-${++sequence}.log`),'w');preview=spawn(binary,['app','dev','--runtime','docker','--database','local','--port','18123','--no-open'],{cwd:app,env,stdio:['ignore',fd,fd]});closeSync(fd);await until(()=>{assert.equal(preview.exitCode,null,'preview exited: inspect private log');return existsSync(join(app,'.miy/preview.json'));},'preview startup timeout',240000);return JSON.parse(readFileSync(join(app,'.miy/preview.json')));}
async function stopPreview(){cli(['dev-stop']);await until(()=>preview.exitCode!==null,'runner did not exit',5000);assert.equal(preview.exitCode,0);assert(!existsSync(join(app,'.miy/preview-run.json')));assert(!existsSync(join(app,'.miy/preview.json')));assert.equal(await fetch('http://127.0.0.1:18123/__miy/context').then(()=>true,()=>false),false);}
async function guards(page,base,shell=false){
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 let dialogs=0,decision='dismiss';page.on('dialog',async d=>{dialogs++;await d[decision]();});
 await page.goto(base);await page.getByTestId('draft').waitFor();
 await page.getByRole('button',{name:'Next',exact:true}).click();await page.waitForURL(base+'/next');
 await page.getByTestId('draft').fill('unsaved');
 let before=dialogs;
 await page.getByRole('button',{name:'Home',exact:true}).click();assert.equal(dialogs,before+1);assert.equal(await page.getByTestId('draft').inputValue(),'unsaved');
 before=dialogs;await page.evaluate(()=>history.back());await until(()=>dialogs===before+1,'back did not prompt');await page.waitForURL(base+'/next');assert.equal(await page.getByTestId('draft').inputValue(),'unsaved');
 if(shell){before=dialogs;await page.getByRole('link',{name:'테스트 영역',exact:true}).click();assert.equal(dialogs,before+1);assert.equal(await page.getByTestId('draft').inputValue(),'unsaved');}
 // Native document unload uses the same registration.
 before=dialogs;await page.evaluate(()=>{setTimeout(()=>location.reload(),0);});await until(()=>dialogs===before+1,'reload did not prompt');assert.equal(await page.getByTestId('draft').inputValue(),'unsaved');
 decision='accept';await page.getByRole('button',{name:'Home',exact:true}).click();await page.waitForURL(url=>url.pathname.replace(/\/$/,'')===new URL(base).pathname.replace(/\/$/,''));await until(async()=>await page.getByTestId('draft').inputValue()==='','accepted navigation did not reset draft');
 // Forward cancellation from a clean history entry, then save/dispose permits it.
 await page.evaluate(()=>history.back());await page.waitForURL(base+'/next');
 await page.getByTestId('draft').fill('forward draft');decision='dismiss';before=dialogs;await page.evaluate(()=>history.forward());await until(()=>dialogs===before+1,'forward did not prompt');await page.waitForURL(base+'/next');assert.equal(await page.getByTestId('draft').inputValue(),'forward draft');
 await page.getByRole('button',{name:'Save',exact:true}).click();before=dialogs;await page.evaluate(()=>history.forward());await page.waitForURL(url=>url.pathname.replace(/\/$/,'')===new URL(base).pathname.replace(/\/$/,''));assert.equal(dialogs,before);
 if(shell){
  await page.getByTestId('draft').fill('logout draft');decision='dismiss';before=dialogs;
  await page.getByRole('button',{name:'로그아웃',exact:true}).click();assert.equal(dialogs,before+1);assert.equal(await page.getByTestId('draft').inputValue(),'logout draft');
  decision='accept';before=dialogs;await page.getByRole('button',{name:'로그아웃',exact:true}).click();await page.getByRole('button',{name:/개발자/}).waitFor();assert.equal(dialogs,before+1,'logout prompted twice');
  await page.context().request.post(origin+'/auth/development',{headers:{origin},data:{account:'developer'}});await page.goto(base);await page.getByTestId('draft').waitFor();
  await page.getByTestId('draft').fill('leaving shell');decision='accept';await page.getByRole('link',{name:'테스트 영역',exact:true}).click();await page.waitForURL(origin+'/environments/dev');before=dialogs;await page.getByRole('link',{name:'운영 영역',exact:true}).click();await page.waitForURL(origin+'/environments/prod');assert.equal(dialogs,before,'unmounted app left a guard');}
 assert.deepEqual(errors,[]);ok(`${shell?'platform':'local preview'}: app/shell/history/unload guards, cancel/allow/save and cleanup`);
}
const admin=await login('admin'),developer=await login('developer');
const baseline=(await api(admin,'/api/v1/installations')).map(v=>({id:v.id,release:v.releaseId,image:v.imageId}));
const corePid=readFileSync(join(root,'.local/serve.pid'),'utf8');
try{
 cli(['auth','login','--account','admin'],adminDir);
 cli(['component','publish',join(root,'registry/components/ui/1.0.1')],adminDir);
 cli(['init',app,'--id',appId,'--name','Operations probe'],root);
 cli(['auth','login','--account','developer']);cli(['component','sync']);
 const registration=cli(['register']);await api(admin,`/api/v1/app-registrations/${registration.id}/decision`,{approve:true});
 writeFileSync(join(app,'web/src/AppRoot.tsx'),`import {useState,useEffect} from 'react';import type {AppProps} from '@miy/sdk';
export default function AppRoot({runtime}:AppProps){const [draft,setDraft]=useState('');useEffect(()=>runtime.registerNavigationGuard(()=>draft?'Discard unsaved probe?':undefined),[runtime.registerNavigationGuard,draft]);useEffect(()=>setDraft(''),[runtime.location.pathname]);return <section><h1>Operations probe</h1><p data-testid="path">{runtime.location.pathname}</p><input data-testid="draft" value={draft} onChange={e=>setDraft(e.target.value)}/><button onClick={()=>runtime.navigate('/next')}>Next</button><button onClick={()=>runtime.navigate('/')}>Home</button><button onClick={()=>setDraft('')}>Save</button></section>;}`);
 let info=await startPreview();
 local=await chromium.launch({channel:'chrome',headless:true});const ctx=await local.newContext(),page=await ctx.newPage();await page.goto(info.url);await page.getByTestId('draft').waitFor();await guards(page,`http://127.0.0.1:18123/apps/${appId}`);
 fails(['dev','--database','local','--port','18124','--no-open'],CLI_EXIT.conflict,/preview_already_running/);
 const project='miy-preview-'+createHash('sha256').update(app).digest('hex').slice(0,16);
 // Test retention using only this synthetic local DB, not any user's app data.
 execFileSync('docker',['exec',project+'-db','psql','-U','postgres','-d','postgres','-c',"CREATE TABLE lifecycle_probe(value text); INSERT INTO lifecycle_probe VALUES ('retained');"],{stdio:'ignore'});
 await stopPreview();
 info=await startPreview();assert.match(run('docker',['exec',project+'-db','psql','-U','postgres','-d','postgres','-At','-c','SELECT value FROM lifecycle_probe']),/retained/);await stopPreview();
 ok('dev → dev-stop → dev: runner/port/lock/container cleanup, duplicate-run rejection and DB retention');
 await local.close();local=undefined;
 run('git',['init'],app);run('git',['add','.'],app);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Synthetic platform operations probe'],app);
 image=cli(['build']).imageId;cli(['deploy']);const status=cli(['status']);installation=status.find(v=>v.environment==='dev');assert(installation.localBuild.matchesRunningImage);
 copyFileSync(join(app,'manifest.json'),join(adminDir,'manifest.json'));
 fails(['rename','--name','denied'],CLI_EXIT.forbidden,/forbidden/);
 cli(['rename','--name','Renamed operations probe'],adminDir);
 let current=cli(['status'])[0];assert.equal(current.name,'Renamed operations probe');assert.equal(current.manifestName,'Operations probe');assert.equal(current.displayNameDiffers,true);
 ok('administrator-only explicit display name; manifest, release and developer rights preserved');
 browser=await chromium.connectOverCDP('http://127.0.0.1:19222');const web=await browser.newContext();await web.request.post(origin+'/auth/development',{headers:{origin},data:{account:'developer'}});const screen=await web.newPage();await guards(screen,origin+`/environments/dev/apps/${appId}`,true);await web.close();
 const report={releaseId:installation.releaseId,checks:[{scope:'navigation and preview lifecycle',actor:'developer',result:'passed',evidence:'check-operations.mjs real browsers: cancellation, save, cleanup and local DB retention'},{scope:'production',actor:'administrator',result:'not_run',evidence:'This probe is dev only'}]};
 const reportPath=join(evidence,'acceptance.json');writeFileSync(reportPath,JSON.stringify(report));
 assert.equal(cli(['accept','--note','Legacy note-only compatibility']).structured,false);
 assert(cli(['accept','--note','Synthetic platform boundary tests','--report',reportPath]).structured);
 current=cli(['status'])[0];assert.deepEqual(current.acceptance.checks,report.checks);assert(current.acceptance.recordedBy);assert(current.acceptance.checkedAt);
 const failed={...report,checks:[{...report.checks[0],result:'failed'}]};await api(developer,`/api/v1/apps/${appId}/test-acceptance`,{...failed,note:'must reject'},400);
 assert.deepEqual(cli(['status'])[0].acceptance.checks,report.checks);
 writeFileSync(reportPath,JSON.stringify({...report,releaseId:'00000000-0000-4000-8000-000000000000'}));fails(['accept','--note','stale','--report',reportPath],CLI_EXIT.conflict,/release_mismatch/);
 ok('structured acceptance records exact release/recorder/scopes; rejects failed or stale reports');
 assert.equal(readFileSync(join(root,'.local/serve.pid'),'utf8'),corePid);ok('app deployment required no core restart');
 // Upgrade an independent 0.5 fixture, never the user's app.
 const oldBinary=process.env.MIY_PREVIOUS_CLI;
 if(oldBinary && existsSync(oldBinary)){
  const upgrade=join(evidence,'upgrade');run(oldBinary,['app','init',upgrade,'--id','upgrade-probe','--name','Upgrade probe'],root);
  run(oldBinary,['app','login','--account','developer'],upgrade);run(oldBinary,['app','components','sync'],upgrade);
  run('git',['init'],upgrade);run('git',['add','.'],upgrade);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Old bundle fixture'],upgrade);
  const preserved=['src/main.rs','web/src/AppRoot.tsx','manifest.json','migrations/0001_initial.sql'];const before=preserved.map(f=>hash(join(upgrade,f)));
  cli(['auth','login','--account','developer'],upgrade);
  fails(['upgrade'],CLI_EXIT.environment,/before_upgrade/,upgrade);assert.equal(JSON.parse(readFileSync(join(upgrade,'miy.bundle.json'))).bundleVersion,'0.5.0');
  run(oldBinary,['app','components','update','ui','--version','1.0.1'],upgrade);run('git',['add','.'],upgrade);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Compatible component'],upgrade);
  assert.equal(cli(['upgrade'],upgrade).bundleVersion,JSON.parse(readFileSync(join(app,'miy.bundle.json'))).bundleVersion);assert.deepEqual(preserved.map(f=>hash(join(upgrade,f))),before);cli(['check'],upgrade);ok('0.5 → current bundle explicit upgrade rejects incompatible components, preserves business files and passes full check');
 } else { console.log('SKIP bundle upgrade: set MIY_PREVIOUS_CLI to a bundle 0.5 CLI binary'); }
}finally{
 if(preview&&preview.exitCode===null){try{cli(['dev-stop']);}catch{} }
 if(local)await local.close();if(browser)await browser.close();
 // Only this run's unique fixture may be removed; retain independent users' apps and DBs.
 assert(/^ops-probe-[a-z0-9]+$/.test(appId));
 const artifacts=sql(`SELECT artifact_hash FROM releases WHERE app_id='${appId}';`).split('\n').filter(Boolean);
 const ids=sql(`SELECT id FROM installations WHERE app_id='${appId}';`).split('\n').filter(Boolean);
 for(const id of ids){assert(/^[a-f0-9-]{36}$/.test(id));try{cli(['remove']);}catch{}const suffix=id.replaceAll('-','');
  sql(`DROP DATABASE IF EXISTS miy_app_${suffix} WITH (FORCE);`,'postgres');
  for(const kind of ['r','m','o'])sql(`DROP ROLE IF EXISTS miy_${kind}_${suffix};`,'postgres');
 }
 sql(`BEGIN; DELETE FROM release_checks WHERE release_id IN (SELECT id FROM releases WHERE app_id='${appId}'); DELETE FROM runtime_jobs WHERE app_id='${appId}'; DELETE FROM jobs WHERE installation_id IN (SELECT id FROM installations WHERE app_id='${appId}'); DELETE FROM db_resources WHERE installation_id IN (SELECT id FROM installations WHERE app_id='${appId}'); DELETE FROM installations WHERE app_id='${appId}'; DELETE FROM releases WHERE app_id='${appId}'; DELETE FROM image_uploads WHERE app_id='${appId}'; DELETE FROM grants WHERE app_id='${appId}'; DELETE FROM app_registrations WHERE app_id='${appId}'; DELETE FROM apps WHERE id='${appId}'; COMMIT;`);
 const project='miy-preview-'+createHash('sha256').update(app).digest('hex').slice(0,16);
 for(const args of [['volume','rm',project+'-data'],['network','rm',project],['image','rm',`miy-app/${appId}:0.1.0`],['image','rm',`miy-preview/${project.slice(12)}:local`]])try{run('docker',args);}catch{}
 const artifactSetting=readFileSync(join(root,'.local/runtime.env'),'utf8').split('\n').find(line=>line.startsWith('MIY_ARTIFACTS='))?.slice('MIY_ARTIFACTS='.length);
 if(artifactSetting)for(const hash of artifacts){assert(/^[a-f0-9]{64}$/.test(hash));const dir=resolve(root,artifactSetting,hash);assert.equal(JSON.parse(readFileSync(join(dir,'manifest.json'))).id,appId);rmSync(dir,{recursive:true});}
 const after=(await api(admin,'/api/v1/installations')).map(v=>({id:v.id,release:v.releaseId,image:v.imageId}));assert.deepEqual(after,baseline);
 writeFileSync(join(evidence,'result.json'),JSON.stringify({checks,fixtureRemoved:true,existingInstallationsUnchanged:true},null,2));
 console.log('Evidence: '+evidence);
}
