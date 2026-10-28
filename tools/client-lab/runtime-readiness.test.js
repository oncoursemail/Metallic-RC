'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');

const REQUIRED=['WSASetLastError','connect','WSAConnect','sendto','WSASendTo','getaddrinfo','GetAddrInfoW','WSAIoctl'];
const source=fs.readFileSync(path.join(__dirname,'runtime-agent.js'),'utf8');
const pointer=(name,isNull=false)=>({name,isNull:()=>isNull,toString:()=>name});

// Execute the real agent against inert Frida-shaped objects. No process or socket is opened.
function fixture(options={}){
  const messages=[],replacements=[],attachments=[],originalCalls=[],queries=[],flushStates=[];
  const locations=Object.fromEntries(REQUIRED.map(name=>[name,pointer(name)]));
  if(options.alias)locations.WSAConnect=locations.connect;
  const module={name:'ws2_32.dll',findExportByName(name){
    queries.push(name);
    if(options.lookupFailure===name)throw new Error('fixture lookup');
    return options.missing===name?null:options.nullExport===name?pointer(name,true):locations[name];
  },getExportByName(name){
    const location=this.findExportByName(name);
    if(!location || location.isNull())throw new Error('Fixture export unavailable: '+name);
    return location;
  }};
  let observer,context,flushes=0;
  const api={
    replace(location,callback){
      if(options.replaceFailure===location.name)throw new Error('fixture replacement');
      replacements.push({location,callback});
    },
    attach(location,callbacks){
      if(options.attachFailure)throw new Error('fixture observer');
      attachments.push({location,callbacks});
      return options.emptyListener?null:{detach(){}};
    },
    flush(){
      flushes++;
      flushStates.push(vm.runInContext('rpc.exports.status().networkGuards',context));
      if(options.flushFailure===flushes)throw new Error('fixture flush');
    }
  };
  context=vm.createContext({CONFIG:{port:4187,fixtureOnly:true},
    Process:{id:123,pointerSize:8,attachModuleObserver(callbacks){observer=callbacks;return{detach(){}};}},
    NativeFunction:function(){if(options.nativeFailure)throw new Error('fixture native function');return()=>{};},
    SystemFunction:function(location,result,args){
      if(options.systemFailure===location.name)throw new Error('fixture system function');
      return function(...values){originalCalls.push({name:location.name,args:values,signature:args});return{value:23,lastError:7};};
    },
    NativeCallback:function(callback,result,args){if(options.callbackFailure)throw new Error('fixture callback');return callback;},
    Interceptor:api,rpc:{exports:{}},send:value=>messages.push(value),setInterval:()=>0,Date});
  vm.runInContext(source,context);
  return{messages,replacements,attachments,originalCalls,queries,flushStates,options,module,
    add:(value=module)=>observer.onAdded(value),status:()=>vm.runInContext('rpc.exports.status()',context),
    flushes:()=>flushes,callback:name=>replacements.find(row=>row.location.name===name).callback};
}

function assertStopped(f){
  assert.equal(f.status().networkGuards,false);
  assert.equal(f.status().stopped,true);
  assert.equal(f.messages.filter(row=>row.kind==='agent_failure').length,1);
  assert.equal(f.messages.filter(row=>row.kind==='winsock_guards_installed').length,0);
  const count=f.replacements.length;f.add();assert.equal(f.replacements.length,count);
}

for(const name of REQUIRED){
  test('missing '+name+' fails before the first hook',()=>{
    const f=fixture({missing:name});f.add();assertStopped(f);
    assert.equal(f.replacements.length,0);assert.equal(f.attachments.length,0);assert.equal(f.flushes(),0);
  });
  test('null pointer for '+name+' fails before the first hook',()=>{
    const f=fixture({nullExport:name});f.add();assertStopped(f);
    assert.equal(f.replacements.length,0);assert.equal(f.attachments.length,0);
  });
}
test('export lookup errors and aliased guard addresses fail before registration',()=>{
  for(const options of [{lookupFailure:'WSAIoctl'},{alias:true}]){
    const f=fixture(options);f.add();assertStopped(f);assert.equal(f.replacements.length,0);
  }
});
for(const name of ['connect','WSAConnect','sendto','WSASendTo','getaddrinfo','GetAddrInfoW']){
  test(name+' replacement failure never becomes ready',()=>{
    const f=fixture({replaceFailure:name});f.add();assertStopped(f);
  });
}
test('native wrapper and callback construction errors fail closed',()=>{
  for(const options of [{nativeFailure:true},{systemFailure:'getaddrinfo'},{callbackFailure:true}]){
    const f=fixture(options);f.add();assertStopped(f);
  }
});
test('extension observer registration must return a listener',()=>{
  for(const options of [{attachFailure:true},{emptyListener:true}]){
    const f=fixture(options);f.add();assertStopped(f);assert.equal(f.flushes(),0);
  }
});
test('initial flush failure does not publish network readiness',()=>{
  const f=fixture({flushFailure:1});f.add();assertStopped(f);assert.deepEqual(f.flushStates,[false]);
});
test('all exports, six replacements, observer and flush precede readiness; repeat modules are inert',()=>{
  const f=fixture();f.add();
  assert.deepEqual(f.queries,REQUIRED);assert.equal(f.replacements.length,6);assert.equal(f.attachments.length,1);
  assert.deepEqual(f.flushStates,[false]);assert.equal(f.status().networkGuards,true);assert.equal(f.status().stopped,false);
  assert.equal(f.status().ready,false);assert.equal(f.messages.filter(row=>row.kind==='winsock_guards_installed').length,1);
  f.add();assert.equal(f.queries.length,REQUIRED.length);assert.equal(f.replacements.length,6);assert.equal(f.flushes(),1);
});
test('unrelated modules do not install or advertise network guards',()=>{
  const f=fixture();f.add({name:'other.dll'});assert.equal(f.replacements.length,0);
  assert.equal(f.status().networkGuards,false);assert.equal(f.status().stopped,false);
});

const socket=port=>({isNull:()=>false,readU16:()=>2,add:offset=>({readU8:()=>({2:port>>8,3:port&255,4:127,5:0,6:0,7:1}[offset])})});
function extension(f,location=pointer('ConnectEx'),guid='b907a225f3dd60468ee976e58c74063e',result=0){
  const callbacks=f.attachments[0].callbacks,state={};
  const value=number=>({toUInt32:()=>number});
  callbacks.onEnter.call(state,[null,value(0xc8000006),{readByteArray:()=>Buffer.from(guid,'hex')},value(16),{readPointer:()=>location},value(8)]);
  callbacks.onLeave.call(state,{toInt32:()=>result});
}
test('ConnectEx is guarded once after a successful extension query',()=>{
  const f=fixture();f.add();extension(f);
  assert.equal(f.replacements.length,7);assert.equal(f.flushes(),2);assert.equal(f.status().networkGuards,true);
  assert.equal(f.messages.filter(row=>row.kind==='connectex_guard_installed').length,1);
  extension(f);assert.equal(f.replacements.length,7);assert.equal(f.flushes(),2);
  assert.equal(f.messages.filter(row=>row.kind==='connectex_guard_installed').length,1);
});
test('failed or unrelated extension queries do not install a new callback',()=>{
  const f=fixture();f.add();extension(f,pointer('ConnectEx'),undefined,-1);extension(f,pointer('other'),'00'.repeat(16));
  assert.equal(f.replacements.length,6);assert.equal(f.flushes(),1);assert.equal(f.status().networkGuards,true);
});
for(const name of REQUIRED){
  test('ConnectEx cannot alias the required '+name+' export',()=>{
    const f=fixture();f.add();extension(f,pointer(name));
    assert.equal(f.status().stopped,true);assert.equal(f.status().networkGuards,false);
    assert.equal(f.replacements.length,6);assert.equal(f.flushes(),1);
    assert.equal(f.messages.filter(row=>row.kind==='agent_failure').length,1);
    assert.equal(f.messages.filter(row=>row.kind==='connectex_guard_installed').length,0);
    extension(f);assert.equal(f.replacements.length,6);assert.equal(f.flushes(),1);
  });
}
test('ConnectEx null address, replacement or flush failure revokes readiness',()=>{
  for(const kind of ['null','replace','flush']){
    const f=fixture();f.add();
    if(kind==='replace')f.options.replaceFailure='ConnectEx';
    if(kind==='flush')f.options.flushFailure=2;
    extension(f,pointer('ConnectEx',kind==='null'));
    assert.equal(f.status().stopped,true);assert.equal(f.status().networkGuards,false);
    assert.equal(f.messages.filter(row=>row.kind==='connectex_guard_installed').length,0);
    const count=f.replacements.length;extension(f);assert.equal(f.replacements.length,count);
  }
});
test('socket guards preserve the existing signatures, bounds and error values',()=>{
  const f=fixture();f.add();extension(f);
  const signatures={connect:3,WSAConnect:7,sendto:6,WSASendTo:9,ConnectEx:7};
  for(const [name,count] of Object.entries(signatures)){
    const index=name==='sendto'?4:name==='WSASendTo'?5:1;
    const values=Array(count).fill(null);values[index]=socket(4189);values[index+1]=16;
    const call={},before=f.originalCalls.length;
    assert.equal(f.callback(name).apply(call,values),name==='ConnectEx'?0:-1);
    assert.equal(call.lastError,10013);assert.equal(f.originalCalls.length,before);
    values[index]=socket(4187);assert.equal(f.callback(name).apply(call,values),23);assert.equal(call.lastError,7);
    assert.equal(f.originalCalls.at(-1).signature.length,count);
  }
});
test('both DNS guards preserve loopback admission and refused-name errors',()=>{
  const f=fixture();f.add();
  for(const name of ['getaddrinfo','GetAddrInfoW']){
    const node=host=>({isNull:()=>false,readUtf8String:()=>host,readUtf16String:()=>host});
    const call={},before=f.originalCalls.length;
    assert.equal(f.callback(name).call(call,node('not-allowed.invalid'),null,null,null),11001);
    assert.equal(call.lastError,11001);assert.equal(f.originalCalls.length,before);
    assert.equal(f.callback(name).call(call,node('localhost'),null,null,null),23);assert.equal(call.lastError,7);
    assert.equal(f.originalCalls.at(-1).signature.length,4);
  }
});
