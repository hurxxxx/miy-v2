// CLI guidance regression tests. Creates local templates only; never registers/deploys apps.
import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync,existsSync,openSync,closeSync} from 'node:fs';
import {resolve,join} from 'node:path';
const root=resolve(import.meta.dirname,'..'),out=join(root,'.local/evidence/cli-guidance',Date.now().toString(36));mkdirSync(out,{recursive:true});
const binary=join(out,'miy');copyFileSync(join(root,'target/debug/miy'),binary);
const env={...process.env,PATH:`${process.env.HOME}/.cargo/bin:${process.env.PATH}`,CARGO_TARGET_DIR:join(root,'.local/cli-guidance-cargo'),CI:'true'};
let sequence=0;const checks=[];
const ok=message=>{checks.push(message);console.log('PASS '+message);};
function run(executable,args,cwd=root){const fd=openSync(join(out,`command-${++sequence}.log`),'w');try{return execFileSync(executable,args,{cwd,env,encoding:'utf8',stdio:['ignore','pipe',fd],maxBuffer:8*1024*1024});}finally{closeSync(fd);}}
const cli=(args,cwd)=>JSON.parse(run(binary,['app',...args],cwd));
const hash=path=>createHash('sha256').update(readFileSync(path)).digest('hex');
function fails(args,cwd,pattern){let failure;try{execFileSync(binary,['app',...args],{cwd,env,stdio:'pipe',encoding:'utf8'});}catch(e){failure=e;}assert(failure);assert.match(failure.stderr,pattern);}
async function mcp(cwd){
 const child=spawn(join(cwd,'.miy-tools/miy'),['app','mcp'],{cwd,env,stdio:['pipe','pipe','pipe']});let buffer='',seq=0;const pending=new Map();
 child.stdout.on('data',bytes=>{buffer+=bytes;let n;while((n=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,n);buffer=buffer.slice(n+1);if(line.trim()){const v=JSON.parse(line);if(v.id)pending.get(v.id)?.(v);}}});
 function call(method,params){const id=++seq;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('MCP timeout')),10000);pending.set(id,v=>{clearTimeout(timer);pending.delete(id);resolve(v);});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
 try{
  assert((await call('initialize',{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'cli-guidance-probe',version:'1'}})).result);
  child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const reply=await call('tools/call',{name:'get_cli_help',arguments:{}});assert(!reply.result.isError);assert.deepEqual(JSON.parse(reply.result.content[0].text),cli(['describe'],cwd));
 }finally{child.stdin.end();await new Promise(resolve=>child.on('exit',resolve));}
}
const app=join(out,'fresh');cli(['init',app,'--id','cli-guidance-probe','--name','CLI guidance probe']);
const help=cli(['describe'],app),lock=JSON.parse(readFileSync(join(app,'miy.cli.json')));
assert.equal(lock.cliVersion,help.cliVersion);assert.equal(lock.bundleVersion,help.bundleVersion);
for(const [path,expected] of Object.entries(lock.files))assert.equal(hash(join(app,path)),expected,path);
assert.equal(readFileSync(join(app,'.agents/skills/miy-cli/SKILL.md'),'utf8'),readFileSync(join(app,'.claude/skills/miy-cli/SKILL.md'),'utf8'));
assert.equal(readFileSync(join(app,'docs/CLI-REFERENCE.md'),'utf8'),run(binary,['app','describe','--format','markdown'],app));
// Both agent entry points must lead to files actually installed by init, without MCP/auth.
for(const [file,skill] of [['AGENTS.md','.agents/skills/miy-cli/SKILL.md'],['CLAUDE.md','.claude/skills/miy-cli/SKILL.md']]){
 const guidance=readFileSync(join(app,file),'utf8');assert(guidance.includes(skill));assert(guidance.includes('docs/CLI-REFERENCE.md'));assert(existsSync(join(app,skill)));
}
// Compare actual command invocations, not just the generator's data structure.
for(const row of help.commands){if(row.command.split(' ').includes('help'))continue;const actual=run(binary,[...row.command.split(' ').slice(1),'--help'],app);assert.equal(actual.replace(/\s+/g,' ').trim(),row.help.replace(/\s+/g,' ').trim(),row.command);}
ok('init installs versioned CLI skill/manual for both clients; generated commands match actual --help');
await mcp(app);ok('stdio MCP and offline CLI return exactly the same versioned command/workflow reference');
const manual=join(app,'docs/CLI-REFERENCE.md'),original=readFileSync(manual);writeFileSync(manual,'custom local notes');
fails(['check'],app,/cli_guidance_modified/);fails(['setup'],app,/cli_guidance_modified_or_conflict/);assert.equal(readFileSync(manual,'utf8'),'custom local notes');writeFileSync(manual,original);
rmSync(join(app,'.claude/skills/miy-cli/references/commands.md'));cli(['setup'],app);assert(existsSync(join(app,'.claude/skills/miy-cli/references/commands.md')));
const before=readFileSync(join(app,'miy.cli.json'));cli(['setup'],app);assert.deepEqual(readFileSync(join(app,'miy.cli.json')),before);ok('edited guidance is refused, missing files recover and repeated setup is idempotent');
const oldBinary=process.env.MIY_PREVIOUS_CLI;
if(oldBinary){
 const old=join(out,'compatible-old');run(oldBinary,['app','init',old,'--id','old-cli-probe','--name','Old CLI probe']);
 const foundation=JSON.parse(readFileSync(join(old,'miy.bundle.json')));assert.equal(foundation.bundleVersion,help.bundleVersion,'previous CLI must use same SDK bundle');
 writeFileSync(join(old,'src/main.rs'),readFileSync(join(old,'src/main.rs'),'utf8')+'\n// unsaved business change preserved by CLI setup\n');
 const business=['src/main.rs','web/src/AppRoot.tsx','manifest.json','miy.bundle.json','docs/CLI.md','sdk/browser/index.ts','migrations/0001_initial.sql'];const hashes=business.map(f=>hash(join(old,f))),oldHash=hash(join(old,'.miy-tools/miy'));
 const conflict=join(old,'.agents/skills/miy-cli/SKILL.md');mkdirSync(resolve(conflict,'..'),{recursive:true});writeFileSync(conflict,'custom skill');fails(['setup'],old,/cli_guidance_modified_or_conflict/);assert.equal(hash(join(old,'.miy-tools/miy')),oldHash);rmSync(conflict);
 cli(['setup'],old);assert.deepEqual(business.map(f=>hash(join(old,f))),hashes);assert.equal(run(join(old,'.miy-tools/miy'),['--version'],old),`miy ${help.cliVersion}\n`);
 for(const [file,expected] of Object.entries(foundation.protectedFiles))assert.equal(hash(join(old,file)),expected);
 ok('same SDK CLI update preserves dirty business files and old protected docs; unmanaged conflicts never replace the old executable');
 // Same-bundle upgrade must synchronize tooling instead of returning before it does so.
 run('git',['init'],old);run('git',['add','.'],old);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Local guidance fixture'],old);
 copyFileSync(oldBinary,join(old,'.miy-tools/miy'));const upgraded=cli(['upgrade'],old);assert(upgraded.alreadyCurrent);assert.equal(upgraded.cliVersion,help.cliVersion);assert.equal(run(join(old,'.miy-tools/miy'),['--version'],old),`miy ${help.cliVersion}\n`);ok('same-bundle app upgrade updates project CLI and guidance together');
}else console.log('SKIP prior-version transition: set MIY_PREVIOUS_CLI to an older CLI with the same SDK bundle');
const previousBundleBinary=process.env.MIY_PREVIOUS_BUNDLE_CLI;
if(previousBundleBinary){
 const old=join(out,'previous-bundle');run(previousBundleBinary,['app','init',old,'--id','agent-guidance-upgrade','--name','Agent guidance upgrade']);
 const foundation=JSON.parse(readFileSync(join(old,'miy.bundle.json')));assert.equal(foundation.bundleVersion,'0.6.0');
 const current=JSON.parse(readFileSync(join(app,'miy.bundle.json')));
 assert.deepEqual(Object.keys(current.protectedFiles).sort(),Object.keys(foundation.protectedFiles).sort());
 assert.deepEqual(Object.keys(current.protectedFiles).filter(p=>current.protectedFiles[p]!==foundation.protectedFiles[p]).sort(),['AGENTS.md','CLAUDE.md'],'documentation patch must preserve every runtime/SDK/component protected hash');
 if(process.env.MIY_GUIDANCE_LIVE_CHECK==='1'){
  run(previousBundleBinary,['app','login','--account','developer'],old);run(previousBundleBinary,['app','components','sync'],old);
 }
 run('git',['init'],old);run('git',['add','.'],old);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Previous bundle fixture'],old);
 const tracked=run('git',['ls-files'],old).trim().split('\n');
 const unchanged=tracked.filter(p=>!['AGENTS.md','CLAUDE.md','miy.bundle.json','miy.cli.json','docs/CLI-REFERENCE.md'].includes(p)&&!p.startsWith('.agents/skills/miy-cli/')&&!p.startsWith('.claude/skills/miy-cli/'));
 const before=unchanged.map(p=>hash(join(old,p)));
 fails(['setup'],old,/bundle_version_mismatch/);
 const oldManual=readFileSync(join(old,'docs/CLI-REFERENCE.md')),oldExecutable=hash(join(old,'.miy-tools/miy'));
 writeFileSync(join(old,'docs/CLI-REFERENCE.md'),'custom guidance');run('git',['add','.'],old);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Custom guidance fixture'],old);
 fails(['upgrade'],old,/cli_guidance_modified_or_conflict/);assert.equal(hash(join(old,'.miy-tools/miy')),oldExecutable);assert.deepEqual(JSON.parse(readFileSync(join(old,'miy.bundle.json'))),foundation);
 writeFileSync(join(old,'docs/CLI-REFERENCE.md'),oldManual);run('git',['add','.'],old);run('git',['-c','user.name=MIY probe','-c','user.email=probe@localhost','commit','-m','Restore generated guidance'],old);
 const upgraded=cli(['upgrade'],old);assert.equal(upgraded.bundleVersion,help.bundleVersion);assert.equal(upgraded.upgradedFrom,'0.6.0');assert.deepEqual(unchanged.map(p=>hash(join(old,p))),before);
 for(const file of ['AGENTS.md','CLAUDE.md'])assert.equal(readFileSync(join(old,file),'utf8'),readFileSync(join(app,file),'utf8'));
 cli(['setup'],old);assert.equal(run(join(old,'.miy-tools/miy'),['--version'],old),`miy ${help.cliVersion}\n`);await mcp(old);
 if(process.env.MIY_GUIDANCE_LIVE_CHECK==='1')assert(cli(['components','check'],old).valid);
 ok('0.6.0 documentation upgrade preserves business/configuration/dependencies/components, rejects guidance conflicts and installs both agent entry points');
}else console.log('SKIP documentation bundle transition: set MIY_PREVIOUS_BUNDLE_CLI to a CLI with SDK bundle 0.6.0');
if(process.env.MIY_GUIDANCE_LIVE_CHECK==='1'){
 cli(['login','--account','developer'],app);cli(['components','sync'],app);assert(cli(['check'],app).valid);ok('fresh template app check passes with generated guidance and installed component locks');
}else console.log('SKIP full app check: set MIY_GUIDANCE_LIVE_CHECK=1 for local platform login/components and toolchain checks');
writeFileSync(join(out,'result.json'),JSON.stringify({cliVersion:help.cliVersion,bundleVersion:help.bundleVersion,checks,registeredApps:0},null,2));console.log('Evidence: '+out);
