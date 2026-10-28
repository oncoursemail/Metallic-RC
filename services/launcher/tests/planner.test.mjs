import test from 'node:test';
import assert from 'node:assert/strict';
import { CATALOG } from '../src/catalog.mjs';
import { planInstallation, planRemoval, validateArtifact } from '../src/planner.mjs';
const PLATFORM='linux-x64',ORIGIN='https://downloads.example.invalid',HASH='a'.repeat(64);
const roots=['metallic-game','minecraft-java','metallic-host'];
const receipt=id=>({id,platform:PLATFORM,version:'1.0.0',sha256:HASH,verified:true});
const artifact=id=>({componentId:id,platform:PLATFORM,version:'1.0.0',sha256:HASH,sizeBytes:4096,fileName:id+'.zip',url:ORIGIN+'/'+id+'.zip'});
function availableCatalog(){return structuredClone(CATALOG).map(item=>({...item,distribution:{[PLATFORM]:artifact(item.id)}}));}
function readyPlan(extra={}){return planInstallation({selected:['metallic-game'],platform:PLATFORM,catalog:availableCatalog(),trustedOrigins:[ORIGIN],...extra});}
test('empty selection is valid and does nothing',()=>{
 const p=planInstallation();for(const k of ['requested','required','blockers','downloads','installs'])assert.deepEqual(p[k],[]);assert.equal(p.canApply,false);
});
test('all eight component combinations remain independent',()=>{
 for(let mask=0;mask<8;mask++){const selected=roots.filter((_,i)=>mask&(1<<i)),p=readyPlan({selected});
  assert.deepEqual(p.requested,selected);assert.deepEqual(p.required.filter(id=>roots.includes(id)),selected);
  assert.equal(p.required.includes('minecraft-runtime'),selected.includes('minecraft-java'));
  assert.deepEqual(p.downloads.map(a=>a.componentId),p.required);assert.deepEqual(p.removals,[]);
 }
});
test('Minecraft gets its runtime first without game or host',()=>{
 const p=readyPlan({selected:['minecraft-java']});assert.deepEqual(p.required,['minecraft-runtime','minecraft-java']);assert.deepEqual(p.operations[0].requiredBy,['minecraft-java']);assert.equal(p.operations[0].requested,false);assert.equal(p.downloadBytes,8192);
});
test('unknown IDs and directly selected runtimes are rejected',()=>{
 for(const id of ['typo','__proto__','minecraft-runtime'])assert.throws(()=>planInstallation({selected:[id]}),/Unknown optional component/);
 assert.throws(()=>planInstallation({selected:'metallic-game'}),/array/);
});
test('duplicate selections normalize without duplicate downloads',()=>{
 const p=readyPlan({selected:['metallic-game','metallic-game']});assert.deepEqual(p.requested,['metallic-game']);assert.equal(p.downloads.length,1);
});
test('hidden optional-component dependencies are rejected',()=>{
 const catalog=availableCatalog();catalog[0].requires=['metallic-host'];assert.throws(()=>readyPlan({catalog}),/cannot be hidden dependencies/);
});
test('unknown dependencies and cycles are rejected even when unselected',()=>{
 const unknown=availableCatalog();unknown[0].requires=['missing-runtime'];assert.throws(()=>readyPlan({catalog:unknown}),/Unknown dependency/);
 const cycle=availableCatalog();cycle.find(i=>i.id==='minecraft-runtime').requires=['minecraft-runtime'];assert.throws(()=>readyPlan({catalog:cycle}),/Dependency cycle/);
});
test('shipping catalog honestly blocks unreleased installers',()=>{
 const p=planInstallation({selected:roots,platform:PLATFORM});assert.equal(p.canApply,false);assert.equal(p.blockers.length,4);assert.deepEqual(p.downloads,[]);assert.deepEqual(p.installs,[]);assert.ok(CATALOG.every(i=>i.distribution===null));
});
test('missing and unsupported platforms fail safely',()=>{
 assert.equal(planInstallation({selected:['metallic-game']}).blockers[0].code,'platform-required');assert.throws(()=>readyPlan({platform:'made-up-os'}),/Unsupported target platform/);
});
test('downloaded bytes are not an installed component',()=>{
 const p=readyPlan({downloaded:[receipt('metallic-game')]});assert.equal(p.operations[0].status,'ready');assert.equal(p.operations[0].action,'install');assert.equal(p.installs.length,1);assert.equal(p.downloads.length,0);assert.equal(p.canApply,true);
});
test('verified installed receipt avoids reinstallation',()=>{
 const p=readyPlan({installed:[receipt('metallic-game')]});assert.equal(p.operations[0].status,'installed');assert.equal(p.operations[0].action,'keep');assert.deepEqual(p.downloads,[]);assert.deepEqual(p.installs,[]);
});
test('unverified and duplicate inventories are rejected',()=>{
 assert.throws(()=>readyPlan({downloaded:[{...receipt('metallic-game'),verified:false}]}),/verified/);
 assert.throws(()=>readyPlan({installed:[receipt('metallic-game'),receipt('metallic-game')]}),/duplicate/);
});
test('stale or wrong-platform downloads must be downloaded again',()=>{
 for(const change of [{sha256:'b'.repeat(64)},{platform:'windows-x64'},{version:'0.9.0'}])assert.equal(readyPlan({downloaded:[{...receipt('metallic-game'),...change}]}).downloads.length,1);
});
test('previous installed release plans an update',()=>{
 assert.equal(readyPlan({installed:[{...receipt('metallic-game'),version:'0.9.0'}]}).installs[0].action,'update');
});
test('deselecting never implicitly uninstalls',()=>{
 const installed=[receipt('metallic-host')];assert.deepEqual(readyPlan({installed}).removals,[]);assert.deepEqual(readyPlan({selected:[],installed}).removals,[]);
});
test('blocked dependencies prevent partial execution',()=>{
 const catalog=availableCatalog();catalog.find(i=>i.id==='minecraft-runtime').distribution=null;
 const p=readyPlan({selected:['metallic-game','minecraft-java'],catalog});assert.equal(p.canApply,false);assert.equal(p.operations.find(i=>i.id==='minecraft-java').code,'dependency-unavailable');assert.deepEqual(p.downloads,[]);assert.deepEqual(p.installs,[]);
});
test('unsafe artifact URLs filenames integrity and targets are rejected',()=>{
 const options={id:'metallic-game',platform:PLATFORM,trustedOrigins:[ORIGIN]};
 const bad=[{url:'http://downloads.example.invalid/game.zip'},{url:'https://other.example.invalid/game.zip'},{url:'https://user:secret@downloads.example.invalid/game.zip'},{url:ORIGIN+'/game.zip#fragment'},{url:'file:///tmp/game.zip'},{fileName:'../game.zip'},{fileName:'a\\game.zip'},{fileName:'CON.zip'},{fileName:'game.zip.'},{sha256:'missing'},{sizeBytes:-1},{sizeBytes:0},{sizeBytes:Number.MAX_SAFE_INTEGER+1},{componentId:'metallic-host'},{platform:'windows-x64'}];
 for(const change of bad)assert.throws(()=>validateArtifact({...artifact('metallic-game'),...change},options));
});
test('an allowlist does not make local or IP artifact destinations acceptable',()=>{
 for(const origin of ['https://127.0.0.1','https://127.1','https://localhost','https://updates.internal'])assert.throws(()=>validateArtifact({...artifact('metallic-game'),url:origin+'/game.zip'},{id:'metallic-game',platform:PLATFORM,trustedOrigins:[origin]}));
});
test('explicit removal preserves data and unrelated components',()=>{
 const installed=['metallic-game','minecraft-runtime','minecraft-java','metallic-host'].map(receipt);
 const p=planRemoval({componentId:'minecraft-java',platform:PLATFORM,installed});assert.deepEqual(p.operations.map(i=>i.id),['minecraft-java','minecraft-runtime']);assert.equal(p.preserveUserData,true);assert.ok(p.operations.every(i=>i.preserveUserData));
 assert.deepEqual(planRemoval({componentId:'metallic-game',platform:PLATFORM,installed}).operations.map(i=>i.id),['metallic-game']);
});
test('shared runtimes remain when another installed component needs them',()=>{
 const catalog=availableCatalog();catalog.find(i=>i.id==='metallic-game').requires=['minecraft-runtime'];
 const p=planRemoval({componentId:'minecraft-java',platform:PLATFORM,catalog,installed:['metallic-game','minecraft-runtime','minecraft-java'].map(receipt)});assert.deepEqual(p.operations.map(i=>i.id),['minecraft-java']);
});
test('aggregate download totals cannot overflow safe integers',()=>{
 const catalog=availableCatalog();for(const item of catalog)item.distribution[PLATFORM].sizeBytes=Number.MAX_SAFE_INTEGER;
 assert.throws(()=>readyPlan({selected:['minecraft-java'],catalog}),/safe integer limit/);
});
