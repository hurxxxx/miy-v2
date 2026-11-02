// Offline guidance, MCP discovery and transactional tooling/SDK upgrade checks.
import assert from 'node:assert/strict';
import {CLI_EXIT,cliData,assertCliFailure} from './lib/cli-result.mjs';
import {execFileSync,spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,copyFileSync,chmodSync,statSync,rmSync,existsSync} from 'node:fs';
import {resolve,join} from 'node:path';
const root=resolve(import.meta.dirname,'..'),out=join(root,'.local/evidence/cli-guidance',Date.now().toString(36));mkdirSync(out,{recursive:true});
const suffix=process.platform==='win32'?'.exe':'',binary=join(out,'miy'+suffix);copyFileSync(process.env.MIY_CLI_BINARY||join(root,'target/debug/miy'+suffix),binary);
const env={...process.env,MIY_CONFIG_DIR:join(out,'config'),MIY_CREDENTIAL_BACKEND:'file',CI:'true'};delete env.MIY_CONTEXT;
const checks=[],hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
const run=(file,args,cwd=root)=>execFileSync(file,args,{cwd,env,encoding:'utf8',stdio:['ignore','pipe','pipe'],maxBuffer:16*1024*1024});
const cli=(args,cwd=root)=>cliData(run(binary,['--output','json',...args],cwd));
const app=(args,cwd)=>cli(['app',...args],cwd);
const ok=message=>{checks.push(message);console.log('PASS '+message);};
function fails(args,cwd,exit,pattern){let failure;try{run(binary,['--output','json',...args],cwd);}catch(error){failure=error;}return assertCliFailure(failure,exit,pattern);}
async function mcp(cwd){
 const child=spawn(join(cwd,'.miy-tools/miy'+suffix),['mcp'],{cwd,env,stdio:['pipe','pipe','pipe']});let buffer='',sequence=0;const pending=new Map();
 child.stdout.on('data',bytes=>{buffer+=bytes;let n;while((n=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,n);buffer=buffer.slice(n+1);if(line.trim()){const value=JSON.parse(line);pending.get(value.id)?.(value);}}});
 function call(method,params){const id=++sequence;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('MCP timeout')),10000);pending.set(id,value=>{clearTimeout(timer);pending.delete(id);resolve(value);});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
 try{
  assert((await call('initialize',{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'guidance-regression',version:'1'}})).result);
  child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  for(const [arguments_,args] of [[{},['describe']],[{command:'app deploy'},['describe','--command','app deploy']],[{search:'job'},['describe','--search','job']]]){
   const reply=await call('tools/call',{name:'get_cli_help',arguments:arguments_});assert(!reply.result.isError);assert.deepEqual(JSON.parse(reply.result.content[0].text),cli(args,cwd));
  }
 }finally{child.stdin.end();await new Promise(resolve=>child.on('exit',resolve));}
}
const fresh=join(out,'한글 app');app(['init',fresh,'--id','cli-guidance-probe','--name','CLI guidance probe']);
const summary=cli(['describe'],fresh),all=cli(['describe','--all'],fresh),lock=JSON.parse(readFileSync(join(fresh,'miy.cli.json'))),bundle=JSON.parse(readFileSync(join(fresh,'miy.bundle.json')));
assert.equal(lock.cliVersion,summary.cliVersion);assert.equal(lock.bundleVersion,bundle.bundleVersion);
assert(!summary.commands.some(row=>row.command.includes(' ')),'default discovery must remain compact');
for(const [path,expected]of Object.entries(lock.files))assert.equal(hash(join(fresh,path)),expected,path);
assert.equal(readFileSync(join(fresh,'.agents/skills/miy-cli/SKILL.md'),'utf8'),readFileSync(join(fresh,'.claude/skills/miy-cli/SKILL.md'),'utf8'));
const normalizedHelp=text=>text.replace(/Usage: miy\.exe\b/g,'Usage: miy').replace(/\s+/g,' ').trim();
for(const row of all.commands){const actual=run(binary,[...row.command.split(' '),'--help'],fresh);assert.equal(normalizedHelp(actual),normalizedHelp(row.help),row.command);}
for(const [file,skill]of [['AGENTS.md','.agents/skills/miy-cli/SKILL.md'],['CLAUDE.md','.claude/skills/miy-cli/SKILL.md']]){const text=readFileSync(join(fresh,file),'utf8');assert(text.includes(skill));assert(existsSync(join(fresh,skill)));}
const manual=readFileSync(join(fresh,'docs/CLI-REFERENCE.md'),'utf8');assert(manual.includes('miy auth login'));assert(manual.includes('miy job watch'));
fails(['app','deploy','--environment','prod'],fresh,CLI_EXIT.usage,/invalid_arguments/);
fails(['app','login'],fresh,CLI_EXIT.usage,/removed_command/);
ok('compact discovery and installed manuals cover actual visible commands; removed names and prod deploy are refused');
await mcp(fresh);ok('MCP default, command and search discovery exactly match the installed CLI');
const manualPath=join(fresh,'docs/CLI-REFERENCE.md'),oldBinary=hash(join(fresh,'.miy-tools/miy'+suffix));
writeFileSync(manualPath,'custom local notes');fails(['app','setup'],fresh,CLI_EXIT.environment,/cli_guidance_modified_or_conflict/);assert.equal(hash(join(fresh,'.miy-tools/miy'+suffix)),oldBinary);assert.equal(readFileSync(manualPath,'utf8'),'custom local notes');writeFileSync(manualPath,manual);
rmSync(join(fresh,'.claude/skills/miy-cli/references/commands.md'));app(['setup'],fresh);assert(existsSync(join(fresh,'.claude/skills/miy-cli/references/commands.md')));
const before=readFileSync(join(fresh,'miy.cli.json'));app(['setup'],fresh);assert.deepEqual(readFileSync(join(fresh,'miy.cli.json')),before);ok('setup rejects changed managed documents, restores missing guidance and is repeatable');
const codex=join(fresh,'.codex/config.toml'),claude=join(fresh,'.mcp.json');
const customCodex=readFileSync(codex,'utf8')+'\n# user-owned setting preserved\n';writeFileSync(codex,customCodex);
const customClaude=JSON.parse(readFileSync(claude));customClaude.mcpServers.other={command:'other-mcp',env:{TOKEN:'synthetic-fixture-only'}};writeFileSync(claude,JSON.stringify(customClaude));
if(process.platform!=='win32')for(const path of [codex,claude])chmodSync(path,0o600);
app(['setup'],fresh);assert.equal(readFileSync(codex,'utf8'),customCodex);assert.deepEqual(JSON.parse(readFileSync(claude)).mcpServers.other,customClaude.mcpServers.other);assert.deepEqual(JSON.parse(readFileSync(claude)).mcpServers.miy_app.args,['mcp']);
if(process.platform!=='win32')for(const path of [codex,claude])assert.equal(statSync(path).mode&0o777,0o600);
ok('tooling transaction preserves unrelated private MCP configuration and installs root mcp arguments');
const previous=process.env.MIY_PREVIOUS_BUNDLE_CLI||process.env.MIY_PREVIOUS_CLI;
if(previous){
 const old=join(out,'previous');run(previous,['app','init',old,'--id','upgrade-probe','--name','Previous bundle']);
 const oldLock=JSON.parse(readFileSync(join(old,'miy.bundle.json')));
 writeFileSync(join(old,'src/main.rs'),readFileSync(join(old,'src/main.rs'),'utf8')+'\n// business change must survive upgrade\n');
 writeFileSync(join(old,'migrations/0002_preserved.sql'),'-- existing applied bytes\r\nCREATE TABLE app.preserved(id integer);\r\n');
 mkdirSync(join(old,'platform-components/custom'),{recursive:true});writeFileSync(join(old,'platform-components/custom/source.ts'),'export const business = "preserve";\n');
 const preserved=['src/main.rs','web/src/AppRoot.tsx','manifest.json','Cargo.toml','Cargo.lock','package.json','pnpm-lock.yaml','tsconfig.json','migrations/0001_initial.sql','migrations/0002_preserved.sql','platform-components/custom/source.ts','miy.components.json'];
 const hashes=preserved.map(path=>hash(join(old,path)));
 run('git',['init'],old);run('git',['add','.'],old);run('git',['-c','user.name=MIY regression','-c','user.email=fixture@localhost','commit','-m','Preserved application fixture'],old);
 if(oldLock.bundleVersion!==bundle.bundleVersion)fails(['app','setup'],old,CLI_EXIT.environment,/bundle_version_mismatch/);
 const oldManual=readFileSync(join(old,'docs/CLI-REFERENCE.md')),oldExecutable=hash(join(old,'.miy-tools/miy'+suffix));
 writeFileSync(join(old,'docs/CLI-REFERENCE.md'),'custom managed document');run('git',['add','.'],old);run('git',['-c','user.name=MIY regression','-c','user.email=fixture@localhost','commit','-m','Conflict fixture'],old);
 fails(['app','upgrade'],old,CLI_EXIT.environment,/cli_guidance_modified_or_conflict/);assert.equal(hash(join(old,'.miy-tools/miy'+suffix)),oldExecutable);assert.deepEqual(JSON.parse(readFileSync(join(old,'miy.bundle.json'))),oldLock);
 writeFileSync(join(old,'docs/CLI-REFERENCE.md'),oldManual);run('git',['add','.'],old);run('git',['-c','user.name=MIY regression','-c','user.email=fixture@localhost','commit','-m','Restore managed document'],old);
 const upgraded=app(['upgrade'],old);assert.equal(upgraded.bundleVersion,bundle.bundleVersion);assert.deepEqual(preserved.map(path=>hash(join(old,path))),hashes);
 for(const [path,expected]of Object.entries(bundle.protectedFiles))assert.equal(hash(join(old,path)),expected,path);
 assert.equal(run(join(old,'.miy-tools/miy'+suffix),['--version'],old),`miy ${summary.cliVersion}\n`);await mcp(old);
 const backup=resolve(old,upgraded.backup||upgraded.transaction?.backup);
 assert(existsSync(join(backup,'manifest.json')));
 ok(`explicit ${oldLock.bundleVersion} → ${bundle.bundleVersion} upgrade preserves business, CRLF SQL, components and dependencies; conflicts preserve old executable`);
}else console.log('SKIP prior-version transition: set MIY_PREVIOUS_BUNDLE_CLI to the preserved baseline binary');
writeFileSync(join(out,'result.json'),JSON.stringify({cliVersion:summary.cliVersion,bundleVersion:bundle.bundleVersion,checks,registeredApps:0},null,2));console.log('Evidence: '+out);
