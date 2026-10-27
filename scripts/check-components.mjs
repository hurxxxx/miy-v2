import { fixtureEnv, workflowArgs, workflowData, fixtureCredential, collectApiPage } from "./lib/cli-workflow.mjs";
// Uses a new synthetic app and new component names; never upgrades existing user apps.
import assert from "node:assert/strict";
import { CLI_EXIT, cliData, assertCliFailure } from "./lib/cli-result.mjs";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, copyFileSync, openSync, closeSync, rmSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";
import { chromium } from "@playwright/test";

const root = resolve(import.meta.dirname,"..");
const suffix = Date.now().toString(36);
const evidence = join(root,".local/evidence/component-registry",suffix);
const app = resolve(root,"../miy-apps/component-probe-"+suffix);
const appId = "component-probe-"+suffix;
const component = "badge-"+suffix;
const origin = "http://localhost:18000";
const env = fixtureEnv(evidence,{...process.env,PATH:`${process.env.HOME}/.cargo/bin:${process.env.PATH}`,CI:"true",CARGO_TARGET_DIR:join(root,".local/components-cargo")});
mkdirSync(evidence,{recursive:true});
const binary = join(evidence,"miy");
copyFileSync(join(root,"target/debug/miy"),binary);
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const binaryHash = digest(binary);
const corePid = readFileSync(join(root,".local/serve.pid"),"utf8");
const hostHash = digest(join(root,"web/platform/dist/index.html"));
const passed = [];
const ok = (message) => { passed.push(message); console.log("PASS "+message); };
let sequence=0;
function run(program,args,cwd=root) {
  const file=openSync(join(evidence,`command-${++sequence}.log`),"w");
  try{return execFileSync(program,args,{cwd,env,encoding:"utf8",stdio:["ignore","pipe",file],maxBuffer:8*1024*1024});}
  catch(e){throw Error(`Command ${sequence} failed (${program} ${args[0]}). Inspect private evidence log.`,{cause:e});}
  finally{closeSync(file);}
}
const cli=(args,cwd=app)=>workflowData(cliData(run(binary,workflowArgs(args,cwd,env,binary),cwd)),args);
const fails=(args,expectedExit,pattern)=>{
  let failure;
  try{execFileSync(binary,workflowArgs(args,app,env,binary),{cwd:app,env,stdio:["ignore","pipe","pipe"],encoding:"utf8"});}
  catch(e){failure=e;}
  return assertCliFailure(failure,expectedExit,pattern);
};
async function login(account) {
  const r=await fetch(origin+"/auth/development",{method:"POST",headers:{origin,"content-type":"application/json"},body:JSON.stringify({account})});
  assert.equal(r.status,200);
  const cookie=r.headers.get("set-cookie").split(";")[0];
  const me=await (await fetch(origin+"/api/v1/me",{headers:{cookie}})).json();
  return {cookie,csrf:me.csrf};
}
async function api(user,path,body,expected=200) {
  const response=await fetch(origin+path,{method:body?"POST":"GET",headers:{cookie:user.cookie,origin,"x-miy-csrf":user.csrf,"content-type":"application/json"},body:body?JSON.stringify(body):undefined});
  assert.equal(response.status,expected,path);
  const value = await response.json();
  return body ? value : collectApiPage(value, path, async next => {
    const response = await fetch(origin + next, { headers: { cookie: user.cookie } });
    assert.equal(response.status, 200);
    return response.json();
  });
}
const admin=await login("admin");
const developer=await login("developer");
const initial=await api(admin,"/api/v1/installations");
const baseline=initial.filter(a=>a.kind==="docker_app").map(a=>({id:a.id,image:a.imageId,release:a.releaseId}));
const adminDir=join(evidence,"admin");mkdirSync(adminDir);
cli(["auth","login","--account","admin"],adminDir);
const source=join(evidence,"source");mkdirSync(source);
function packageSource(version,label,bundles=[JSON.parse(readFileSync(join(app,"miy.bundle.json"))).bundleVersion]) {
  const meta={protocol:"miy.component/v1",name:component,version,description:"Runtime registry distribution probe",uiContract:"miy.ui/v1alpha1",bundleVersions:bundles,files:["index.tsx","README.md"]};
  writeFileSync(join(source,"component.json"),JSON.stringify(meta,null,2));
  writeFileSync(join(source,"index.tsx"),`export function RegistryBadge(){return <strong data-testid="registry-badge">${label}</strong>;}\n`);
  writeFileSync(join(source,"README.md"),`Import RegistryBadge from @miy/components/${component}. Release ${version}; no API permission required.\n`);
}
function locked(){return JSON.parse(readFileSync(join(app,"platform-components/miy.components.lock.json")));}
async function mcp() {
  const child=spawn(binary,["mcp"],{cwd:app,env,stdio:["pipe","pipe","ignore"]});
  const messages=[];let buffer="";
  child.stdout.on("data",chunk=>{buffer+=chunk;let i;while((i=buffer.indexOf("\n"))>=0){messages.push(JSON.parse(buffer.slice(0,i)));buffer=buffer.slice(i+1);}});
  const send=x=>child.stdin.write(JSON.stringify({jsonrpc:"2.0",...x})+"\n");
  const receive=async id=>{for(let i=0;i<200;i++){const value=messages.find(v=>v.id===id);if(value)return value;await new Promise(r=>setTimeout(r,25));}throw Error("MCP timeout");};
  try {
    send({id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"component-check",version:"1"}}});
    assert((await receive(1)).result);send({method:"notifications/initialized"});
    send({id:2,method:"tools/call",params:{name:"list_components",arguments:{query:component}}});
    const list=JSON.parse((await receive(2)).result.content[0].text);assert(list.some(v=>v.name===component&&v.version==="1.0.0"));
    send({id:3,method:"tools/call",params:{name:"get_component",arguments:{name:component,version:"1.0.0"}}});
    const detail=JSON.parse((await receive(3)).result.content[0].text);assert(detail.usage.includes("RegistryBadge"));assert(detail.installCommand.includes(component));
    send({id:4,method:"tools/call",params:{name:"get_contract",arguments:{}}});
    const contract=JSON.parse((await receive(4)).result.content[0].text);
    assert.deepEqual(contract.protectedFiles,JSON.parse(readFileSync(join(app,"miy.bundle.json"))).protectedFiles);
  } finally {child.stdin.end();child.kill();}
}
const browser=await chromium.connectOverCDP("http://127.0.0.1:19222");
try {
  cli(["init",app,"--id",appId,"--name","공용 컴포넌트 검증"],root);
  assert(!readFileSync(join(app,"sdk/ui/index.tsx"),"utf8").includes("function Panel"));
  cli(["auth","login","--account","developer"]);
  assert.equal((await fetch(origin+"/api/v1/components")).status,401);
  cli(["component","sync"]);
  assert.equal(locked().components.ui.version,"1.0.1");
  assert.equal(digest(join(app,"platform-components/ui/index.tsx")),digest(join(root,"registry/components/ui/1.0.1/index.tsx")));
  ok("fresh template declares UI; authenticated CLI fetches the independently stored implementation");

  packageSource("1.0.0","badge v1");
  fails(["component","publish",source],CLI_EXIT.forbidden,/forbidden/);
  cli(["component","publish",source],adminDir);
  cli(["component","publish",source],adminDir); // exact re-publication is idempotent
  const published=cli(["component","show",component,"--version","1.0.0"]);
  await api(admin,"/api/v1/components",{...published.package,description:"changed same version"},409);
  await api(admin,"/api/v1/components",{...published.package,version:"1.0.1",files:{...published.package.files,"../escape.tsx":"bad"}},400);
  assert(cli(["component","list"]).components.some(v=>v.name===component));
  await mcp();
  cli(["component","add",component]);
  assert.equal(locked().components[component].version,"1.0.0");
  assert.equal(digest(binary),binaryHash);
  assert.equal(digest(join(app,".miy-tools/miy")),binaryHash);
  ok("admin-only immutable publication; live CLI and MCP discover a new component without a binary change");

  const managed=join(app,"platform-components",component,"index.tsx");
  const original=readFileSync(managed);
  rmSync(managed);
  fails(["component","check"],CLI_EXIT.environment,/component_file_missing/);
  cli(["component","sync"]);assert.deepEqual(readFileSync(managed),original);
  writeFileSync(managed,"user edit");
  fails(["component","sync"],CLI_EXIT.environment,/component_modified/);
  assert.equal(readFileSync(managed,"utf8"),"user edit");writeFileSync(managed,original);
  const lockFile=join(app,"platform-components/miy.components.lock.json");
  const savedLock=readFileSync(lockFile);const tampered=locked();tampered.components[component].sha256="0".repeat(64);writeFileSync(lockFile,JSON.stringify(tampered));
  fails(["component","sync"],CLI_EXIT.environment,/locked_component_digest_changed/);writeFileSync(lockFile,savedLock);
  ok("missing files restored at pinned digest; edits and digest substitution refused without overwriting code");

  writeFileSync(join(app,"web/src/AppRoot.tsx"),`import type {AppProps} from "@miy/sdk";\nimport {Panel} from "@miy/ui";\nimport {RegistryBadge} from "@miy/components/${component}";\nexport default function AppRoot({runtime}:AppProps){return <Panel title="Component registry probe"><RegistryBadge/><p>{runtime.location.pathname}</p></Panel>;}\n`);
  const request=cli(["register"]);await api(admin,`/api/v1/app-registrations/${request.id}/decision`,{approve:true});
  run("git",["init","-b","main"],app);run("git",["add","."],app);run("git",["commit","-m","test: registry component v1"],app);
  cli(["build"]);assert.equal(cli(["deploy"]).status,"succeeded");
  const ctx=await browser.newContext();const [name,...value]=developer.cookie.split("=");await ctx.addCookies([{name,value:value.join("="),url:origin}]);
  const page=await ctx.newPage();const errors=[];page.on("pageerror",e=>errors.push(e.message));
  const url=`${origin}/environments/dev/apps/${appId}`;
  await page.goto(url);await page.getByTestId("registry-badge").filter({hasText:"badge v1"}).waitFor();
  assert.equal(await page.locator("iframe").count(),0);
  ok("fetched basic UI and new component build into a Docker app and render in the platform router");

  packageSource("1.1.0","badge v2");cli(["component","publish",source],adminDir);
  packageSource("9.0.0","incompatible",["99.0.0"]);cli(["component","publish",source],adminDir);
  const installedBefore=digest(managed);cli(["component","sync"]);assert.equal(digest(managed),installedBefore);assert.equal(locked().components[component].version,"1.0.0");
  fails(["component","update",component,"--version","9.0.0"],CLI_EXIT.environment,/incompatible_component/);assert.equal(digest(managed),installedBefore);
  cli(["component","update",component]);assert.equal(locked().components[component].version,"1.1.0");
  await page.reload();await page.getByTestId("registry-badge").filter({hasText:"badge v1"}).waitFor();
  const credentialPath=fixtureCredential(app,env).path;const credentials=readFileSync(credentialPath);
  rmSync(credentialPath);try{assert(cli(["component","check"]).valid);}finally{writeFileSync(credentialPath,credentials,{mode:0o600});}
  ok("publication and sync keep v1 pinned; explicit update selects compatible v2; deployed app stays v1; checks need no login");
  const manifest=JSON.parse(readFileSync(join(app,"manifest.json")));manifest.version="0.1.1";writeFileSync(join(app,"manifest.json"),JSON.stringify(manifest,null,2));
  run("git",["add","."],app);run("git",["commit","-m","test: explicitly deploy component v2"],app);
  cli(["build"]);assert.equal(cli(["deploy"]).status,"succeeded");
  await page.reload();await page.getByTestId("registry-badge").filter({hasText:"badge v2"}).waitFor();
  await page.goto(url+"/detail/1");await page.getByTestId("registry-badge").filter({hasText:"badge v2"}).waitFor();await page.reload();await page.getByTestId("registry-badge").waitFor();
  assert.deepEqual(errors,[]);await page.screenshot({path:join(evidence,"deployed-v2.png")});await ctx.close();
  assert.equal(digest(binary),binaryHash);assert.equal(readFileSync(join(root,".local/serve.pid"),"utf8"),corePid);assert.equal(digest(join(root,"web/platform/dist/index.html")),hostHash);
  const final=await api(admin,"/api/v1/installations");for(const before of baseline){const after=final.find(a=>a.id===before.id);assert.equal(after.imageId,before.image);assert.equal(after.releaseId,before.release);}
  ok("explicit app deployment applies v2; CLI hash, core PID, platform UI and existing app releases remain unchanged");

  const business=(directory)=>{
    const data={};for(const folder of ["src","web/src","migrations"]){const walk=p=>{for(const item of readdirSync(p,{withFileTypes:true})){const file=join(p,item.name);if(item.isDirectory())walk(file);else data[file.slice(directory.length)]=digest(file);}};walk(join(directory,folder));}return data;
  };
  for(const [version,original] of [["0.4.0","arch-mvix06lk-ui"],["0.3.0","my-local-test"]]) {
    const clone=join(evidence,"upgrade-"+version);
    run("git",["clone","--quiet","--no-hardlinks",resolve(root,"../miy-apps",original),clone]);
    assert.equal(JSON.parse(readFileSync(join(clone,"miy.bundle.json"))).bundleVersion,version);
    const before=business(clone);
    cli(["auth","login","--account","developer"],clone);
    assert.equal(cli(["upgrade"],clone).bundleVersion,JSON.parse(readFileSync(join(app,"miy.bundle.json"))).bundleVersion);assert.deepEqual(business(clone),before);
    assert(cli(["check"],clone).valid);
    ok(`${version} -> current bundle upgrade preserves business Rust/React and SQL in a clone; full app check passes`);
  }
  writeFileSync(join(evidence,"result.json"),JSON.stringify({passed,app,appId,url,component,binaryHash,corePid:corePid.trim(),hostHash,checkedAt:new Date().toISOString()},null,2));
  writeFileSync(join(root,".local/evidence/component-registry/latest.json"),JSON.stringify({evidence,app,url},null,2));
  console.log("Evidence: "+evidence);
} finally {await browser.close();}
