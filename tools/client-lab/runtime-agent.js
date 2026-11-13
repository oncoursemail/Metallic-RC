'use strict';
// CONFIG and MetallicRoutePolicy are injected by the reviewed, hash-checking runner.
// Attaches only to that runner's new process. No TLS/auth/security-success patching.
const kept = [], installed = new Set();
let eventCount = 0, routeCount = 0, ready = false, stopped = false;
function event(kind, details = {}) {
  if (eventCount++ < 512) send({kind, timeMs:Date.now(), ...details});
}
function failure(scope, error) {
  if (stopped) return;
  stopped = true;
  // One failure must remain observable even after ordinary diagnostics hit their cap.
  send({kind:'agent_failure', timeMs:Date.now(), scope, error:String(error).slice(0,160)});
}
function isAllowedSocket(address, length) {
  if (address.isNull() || length < 8) return false;
  const family = address.readU16();
  const port = address.add(2).readU8()*256 + address.add(3).readU8();
  if (port !== CONFIG.port) return false;
  if (family === 2 && length >= 16)
    return address.add(4).readU8() === 127 && address.add(5).readU8() === 0 && address.add(6).readU8() === 0 && address.add(7).readU8() === 1;
  if (family === 23 && length >= 28) {
    const bytes = new Uint8Array(address.add(8).readByteArray(16));
    return bytes.slice(0,15).every(x=>x===0) && bytes[15]===1;
  }
  return false;
}
function installNetwork(module) {
  if (module.name.toLowerCase() !== 'ws2_32.dll' || installed.has('winsock')) return;
  const setError = new NativeFunction(module.getExportByName('WSASetLastError'),'void',['int']);
  function wrap(name, signature, addressIndex, lengthIndex, zeroMeansFailure=false, supplied=null) {
    const location = supplied || module.findExportByName(name);
    if (!location || installed.has(location.toString())) return;
    const original = new SystemFunction(location,'int',signature);
    const callback = new NativeCallback(function(...args) {
      let allowed=false;
      try {allowed=isAllowedSocket(args[addressIndex],args[lengthIndex]);}catch (_) {}
      if (!allowed) {event('network_destination_blocked',{api:name});this.lastError=10013;return zeroMeansFailure?0:-1;}
      const result=original(...args);this.lastError=result.lastError;return result.value;
    },'int',signature);
    Interceptor.replace(location,callback);kept.push(callback,original);installed.add(location.toString());
  }
  wrap('connect',['pointer','pointer','int'],1,2);
  wrap('WSAConnect',['pointer','pointer','int','pointer','pointer','pointer','pointer'],1,2);
  wrap('sendto',['pointer','pointer','int','int','pointer','int'],4,5);
  wrap('WSASendTo',['pointer','pointer','uint','pointer','uint','pointer','int','pointer','pointer'],5,6);
  for(const [name,wide] of [['getaddrinfo',false],['GetAddrInfoW',true]]) {
    const location=module.findExportByName(name);if(!location)continue;
    const signature=['pointer','pointer','pointer','pointer'];const original=new SystemFunction(location,'int',signature);
    const callback=new NativeCallback(function(node,service,hints,result) {
      let host='';try{if(!node.isNull())host=wide?node.readUtf16String():node.readUtf8String();}catch(_){}
      if(!['127.0.0.1','::1','localhost'].includes((host||'').toLowerCase())) {event('dns_destination_blocked',{api:name});this.lastError=11001;return 11001;}
      const answer=original(node,service,hints,result);this.lastError=answer.lastError;return answer.value;
    },'int',signature);
    Interceptor.replace(location,callback);kept.push(callback,original);installed.add(location.toString());
  }
  // ConnectEx may be returned as an extension function rather than an export.
  const ioctl=module.findExportByName('WSAIoctl');
  if(ioctl)kept.push(Interceptor.attach(ioctl,{
    onEnter(args){
      this.output=null;
      if(args[1].toUInt32()===0xc8000006 && args[3].toUInt32()===16 && args[5].toUInt32()>=Process.pointerSize) {
        try {
          const guid=Array.from(new Uint8Array(args[2].readByteArray(16))).map(x=>x.toString(16).padStart(2,'0')).join('');
          if(guid==='b907a225f3dd60468ee976e58c74063e')this.output=args[4];
        }catch(_){}
      }
    },
    onLeave(ret){if(this.output && ret.toInt32()===0){try{wrap('ConnectEx',['pointer','pointer','int','pointer','uint','pointer','pointer'],1,2,true,this.output.readPointer());Interceptor.flush();event('connectex_guard_installed');}catch(e){failure('connectex_guard',e);}}}
  }));
  installed.add('winsock');Interceptor.flush();event('winsock_guards_installed');
}
function readManagedString(pointer,limit=8192) {
  if(pointer.isNull())return null;
  const length=pointer.add(16).readS32();if(length<0||length>limit)throw new Error('Managed string length outside bound');
  return pointer.add(20).readUtf16String(length);
}
function prefixMatches(base,method) {
  const address=base.add(method.rva),range=Process.findRangeByAddress(address);
  if(!range || !range.protection.includes('r'))return false;
  return Checksum.compute('sha256',address.readByteArray(32))===method.prefixSha256;
}
function installClient(module) {
  if(ready||stopped||CONFIG.fixtureOnly)return;
  try {
    const methods=CONFIG.methods,base=module.base;
    const needed=['210451','241858','241859','129313','126411'];
    if(!needed.every(id=>prefixMatches(base,methods[id])))return;
    const ctor=new NativeFunction(base.add(methods['210451'].rva),'pointer',['pointer','pointer']);
    kept.push(ctor);
    function routeConstructor(args) {
      const value=readManagedString(args[1]);if(value===null)return;
      const result=MetallicRoutePolicy.route(value,CONFIG.port);
      if(result.action==='unchanged')return;
      const destination=result.action==='reject'?MetallicRoutePolicy.origin(CONFIG.port)+'/blocked-external/':result.url;
      const utf16=Memory.allocUtf16String(destination);
      const managed=ctor(utf16,ptr(0));
      if(managed.isNull()||readManagedString(managed)!==destination)throw new Error('Native managed string construction did not match');
      args[1]=managed;routeCount++;
      event('uri_routed',{reason:result.reason,path:MetallicRoutePolicy.diagnosticPath(destination),sequence:routeCount});
    }
    for(const id of ['241858','241859'])kept.push(Interceptor.attach(base.add(methods[id].rva),{
      onEnter(args){try{routeConstructor(args);}catch(e){failure('uri_constructor',e);}}
    }));
    kept.push(Interceptor.attach(base.add(methods['129313'].rva),{
      onEnter(args){
        try {
          const request=args[0];let uri=request.add(16).readPointer();
          if(request.add(97).readU8()!==0){const redirected=request.add(104).readPointer();if(!redirected.isNull())uri=redirected;}
          const value=readManagedString(uri.add(16).readPointer());
          const local=(value||'').startsWith(MetallicRoutePolicy.origin(CONFIG.port)+'/');
          event('native_http_request',{method:request.add(24).readS32(),ownedOrigin:local,path:MetallicRoutePolicy.diagnosticPath(value)});
        }catch(e){failure('http_observer',e);}
      }
    }));
    kept.push(Interceptor.attach(base.add(methods['126411'].rva),{
      onLeave(ret){
        try {
          if(ret.isNull()){event('native_discovery_deserialized',{response:false});return;}
          const status=readManagedString(ret.add(16).readPointer(),4096);
          let localFields=0;
          for(let i=1;i<32;i++){
            const value=readManagedString(ret.add(16+i*8).readPointer(),8192);
            if(value && value.startsWith(MetallicRoutePolicy.origin(CONFIG.port)+'/recnet/'))localFields++;
          }
          event('native_discovery_deserialized',{response:true,statusNonempty:!!status,localServiceFields:localFields});
        }catch(e){failure('discovery_observer',e);}
      }
    }));
    ready=true;Interceptor.flush();event('client_route_adapter_ready',{buildId:CONFIG.buildId,methodsVerified:needed.length,port:CONFIG.port});
  }catch(e){failure('client_adapter_install',e);}
}
const observer=Process.attachModuleObserver({onAdded(module){
  try{installNetwork(module);if(module.name.toLowerCase()==='gameassembly.dll')installClient(module);}catch(e){failure('module_observer',e);}
}});kept.push(observer);
const timer=setInterval(()=>{
  if(stopped)return;
  const module=Process.findModuleByName('GameAssembly.dll');if(module)installClient(module);
},10);
rpc.exports={status(){return{ready,stopped,routeCount,networkGuards:installed.has('winsock'),pid:Process.id};}};
event('agent_loaded',{pid:Process.id,fixtureOnly:!!CONFIG.fixtureOnly});
