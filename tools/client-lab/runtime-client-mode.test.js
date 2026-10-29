'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const crypto=require('node:crypto');

const required=['WSASetLastError','connect','WSAConnect','sendto','WSASendTo','getaddrinfo','GetAddrInfoW','WSAIoctl'];
const ids=['210451','241858','241859','129313','126411'];
const source=fs.readFileSync(path.join(__dirname,'runtime-agent.js'),'utf8');
function fixture(options={}) {
  const messages=[],replacements=[],attachments=[],nativeFunctions=[],nativeCalls=[],reads=[],timers=[];
  const bytes=Buffer.alloc(32,0x90),digest=crypto.createHash('sha256').update(bytes).digest('hex');
  const pointer=name=>({name,isNull:()=>false,toString:()=>name});
  const methods=Object.fromEntries(ids.map((id,i)=>[id,{rva:i*64,prefixSha256:digest}]));
  const config={port:4187,fixtureOnly:false,methods,buildId:'23191908'};
  if(Object.hasOwn(options,'mode'))config.clientAdapterMode=options.mode;
  const network={name:'ws2_32.dll',findExportByName:name=>options.missing===name?null:pointer(name)};
  const game={name:'GameAssembly.dll',get base(){
    reads.push('base');return{add:rva=>({name:'managed:'+rva,isNull:()=>false,
      readByteArray:size=>{reads.push('prefix');return bytes.subarray(0,size);}})};
  }};
  let observer,flushes=0,context;
  const api={
    replace(location,callback){
      if(options.replaceFailure===location.name)throw new Error('authored replacement failure');
      replacements.push({name:location.name,callback});
    },
    attach(location,callbacks){
      if(options.attachFailure)throw new Error('authored observer failure');
      attachments.push({name:location.name,callbacks});return{detach(){}};
    },
    flush(){if(++flushes===options.flushFailure)throw new Error('authored flush failure');}
  };
  context=vm.createContext({CONFIG:config,Process:{id:123,pointerSize:8,
    attachModuleObserver(callbacks){observer=callbacks;return{detach(){}};},
    findRangeByAddress(){reads.push('range');return{protection:'r-x'};},
    findModuleByName(){reads.push('moduleLookup');return game;}},
    NativeFunction:function(location){nativeFunctions.push(location.name);return(...args)=>{nativeCalls.push({name:location.name,args});return pointer('returned');};},
    SystemFunction:function(location){return()=>({value:23,lastError:7});},
    NativeCallback:function(callback){return callback;},
    Checksum:{compute:(_algorithm,value)=>crypto.createHash('sha256').update(Buffer.from(value)).digest('hex')},
    Memory:{allocUtf16String(){throw new Error('Unexpected managed allocation');}},
    Interceptor:api,rpc:{exports:{}},send:value=>messages.push(value),setInterval:callback=>{timers.push(callback);return 1;},Date});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'route-policy.js'),'utf8'),context);
  vm.runInContext(source,context);
  return{messages,replacements,attachments,nativeFunctions,nativeCalls,reads,timers,options,
    addNetwork:()=>observer?.onAdded(network),addGame:()=>observer?.onAdded(game),
    status:()=>context.rpc.exports.status(),observerPresent:()=>!!observer,
    extension(location=pointer('ConnectEx')) {
      const callbacks=attachments.find(a=>a.name==='WSAIoctl').callbacks,state={};
      const value=n=>({toUInt32:()=>n});
      callbacks.onEnter.call(state,[null,value(0xc8000006),{readByteArray:()=>Buffer.from('b907a225f3dd60468ee976e58c74063e','hex')},value(16),{readPointer:()=>location},value(8)]);
      callbacks.onLeave.call(state,{toInt32:()=>0});
    }};
}

test('network-only keeps actual guard installation and truthful non-fixture admission',()=>{
  const f=fixture({mode:'network-only'});f.addGame();f.addNetwork();f.addGame();
  assert.equal(f.status().clientAdapterMode,'network-only');assert.equal(f.status().networkGuards,true);
  assert.equal(f.status().ready,false);assert.equal(f.status().routeCount,0);assert.equal(f.status().stopped,false);
  assert.equal(f.replacements.length,6);assert.deepEqual(f.attachments.map(a=>a.name),['WSAIoctl']);
  assert.deepEqual(f.nativeFunctions,['WSASetLastError']);assert.deepEqual(f.nativeCalls,[]);assert.deepEqual(f.reads,[]);
  assert.equal(f.timers.length,0);
  const loaded=f.messages.find(m=>m.kind==='agent_loaded');assert.equal(loaded.fixtureOnly,false);assert.equal(loaded.clientAdapterMode,'network-only');
  assert.equal(f.messages.filter(m=>m.kind==='winsock_guards_installed').length,1);
  assert.equal(f.messages.some(m=>['client_route_adapter_ready','native_http_request','native_discovery_deserialized','uri_routed'].includes(m.kind)),false);
});

for(const mode of [undefined,'full'])test('full behavior remains compatible: '+String(mode),()=>{
  const f=fixture(mode===undefined?{}:{mode});f.addNetwork();f.addGame();
  assert.equal(f.status().clientAdapterMode,'full');assert.equal(f.status().networkGuards,true);assert.equal(f.status().ready,true);
  assert.equal(f.replacements.length,6);assert.equal(f.attachments.length,5);
  assert.equal(f.attachments.filter(a=>a.name.startsWith('managed:')).length,4);
  assert.equal(f.reads.filter(r=>r==='prefix').length,5);
  assert.deepEqual(f.nativeFunctions,['WSASetLastError','managed:0']);assert.equal(f.timers.length,1);
  assert.equal(f.messages.filter(m=>m.kind==='client_route_adapter_ready').length,1);
});

for(const missing of required)test('network-only missing guard fails closed: '+missing,()=>{
  const f=fixture({mode:'network-only',missing});f.addNetwork();f.addGame();
  assert.equal(f.status().networkGuards,false);assert.equal(f.status().stopped,true);
  assert.equal(f.replacements.length,0);assert.equal(f.attachments.length,0);assert.deepEqual(f.reads,[]);
});
for(const replaceFailure of required.slice(1,7))test('network-only replacement error revokes readiness: '+replaceFailure,()=>{
  const f=fixture({mode:'network-only',replaceFailure});f.addNetwork();f.addGame();
  assert.equal(f.status().networkGuards,false);assert.equal(f.status().stopped,true);
  assert.deepEqual(f.reads,[]);assert.equal(f.messages.some(m=>m.kind==='winsock_guards_installed'),false);
});
for(const options of [{attachFailure:true},{flushFailure:1}])test('network-only registration/flush failure cannot admit a process '+JSON.stringify(options),()=>{
  const f=fixture({mode:'network-only',...options});f.addNetwork();
  assert.equal(f.status().networkGuards,false);assert.equal(f.status().stopped,true);
});
test('network-only retains ConnectEx installation/dedupe and revokes on alias',()=>{
  const f=fixture({mode:'network-only'});f.addNetwork();f.extension();f.extension();
  assert.equal(f.replacements.length,7);assert.equal(f.status().networkGuards,true);
  assert.equal(f.messages.filter(m=>m.kind==='connectex_guard_installed').length,1);
  f.extension({name:'connect',isNull:()=>false,toString:()=> 'connect'});
  assert.equal(f.status().networkGuards,false);assert.equal(f.status().stopped,true);
});
test('network-only retains destination and DNS denials',()=>{
  const f=fixture({mode:'network-only'});f.addNetwork();
  const socket={isNull:()=>false,readU16:()=>2,add:offset=>({readU8:()=>({2:16,3:92,4:127,5:0,6:0,7:1}[offset])})};
  const call={};assert.equal(f.replacements.find(r=>r.name==='connect').callback.call(call,null,socket,16),-1);assert.equal(call.lastError,10013);
  const node={isNull:()=>false,readUtf8String:()=> 'unowned.invalid',readUtf16String:()=> 'unowned.invalid'};
  for(const name of ['getaddrinfo','GetAddrInfoW'])assert.equal(f.replacements.find(r=>r.name===name).callback.call(call,node,null,null,null),11001);
});
for(const mode of [null,undefined,false,true,0,1,{},[],'','FULL','network_only'])test('invalid explicit agent mode is rejected before any observer: '+JSON.stringify(mode),()=>{
  const f=fixture({mode});f.addNetwork();f.addGame();
  assert.equal(f.status().stopped,true);assert.equal(f.status().networkGuards,false);assert.equal(f.observerPresent(),false);
  assert.equal(f.messages.filter(m=>m.kind==='agent_failure').length,1);assert.equal(f.messages.some(m=>m.kind==='agent_loaded'),false);
  assert.deepEqual(f.nativeFunctions,[]);assert.equal(f.timers.length,0);
});
