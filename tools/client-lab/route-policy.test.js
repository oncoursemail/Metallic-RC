'use strict';
const test=require('node:test');const assert=require('node:assert/strict');const policy=require('./route-policy.js');
const port=4187, local='http://127.0.0.1:'+port;
test('discovery and service routing preserve only the required local request path',()=>{
  assert.equal(policy.route('https://ns.rec.net/',port).url,local+'/');
  assert.equal(policy.route('https://ns.rec.net?build=2026',port).url,local+'/?build=2026');
  assert.equal(policy.route('https://api.rec.net/api/versioncheck/v4',port).url,local+'/recnet/api/api/versioncheck/v4');
  assert.equal(policy.route('https://auth.rec.net/account/login?x=1',port).url,local+'/recnet/auth/account/login?x=1');
});
test('the already-owned origin is not rewritten twice',()=>{
  assert.equal(policy.route(local+'/recnet/rooms/api/test',port).action,'unchanged');
});
test('userinfo, suffix tricks, wrong local ports and unknown hosts cannot escape',()=>{
  for(const uri of ['https://ns.rec.net.evil.invalid/','https://ns.rec.net@evil.invalid/','http://127.0.0.1:1234/','https://ns.rec.net:443@127.0.0.1/','https://evil.invalid/a?password=secret']) {
    const result=policy.route(uri,port);assert.equal(result.action,'redirect');assert.equal(result.url,local+'/blocked-external/');
  }
});
test('relative and non-network URI values are not treated as network requests',()=>{
  for(const uri of ['file:///C:/asset','/relative','asset-name',''])assert.equal(policy.route(uri,port).action,'unchanged');
});
test('ambiguous/bounded inputs rejected',()=>{
  for(const uri of [' https://ns.rec.net/','https://ns.rec.net\\@evil.invalid/','https://ns.rec.net/\nsecret','x'.repeat(8193)])assert.equal(policy.route(uri,port).action,'reject');
  for(const port of [-1,80,65536,1.5,'4187'])assert.throws(()=>policy.route('https://ns.rec.net/',port));
});
test('diagnostics never include query/fragment values or long identifier segments',()=>{
  assert.equal(policy.diagnosticPath('http://127.0.0.1:4187/recnet/auth/api/login?password=secret#secret'),'/recnet/auth/api/login');
  assert.equal(policy.diagnosticPath(local+'/account/12345/'+('f'.repeat(64))+'?token=secret'),'/account/:redacted/:redacted');
  assert(!policy.diagnosticPath(local+'/x/'+('a'.repeat(500))+'?private=yes').includes('private'));
});
test('all emitted absolute destinations stay on the owned loopback origin',()=>{
  const hosts=['ns.rec.net','api.rec.net','auth.rec.net','evil.invalid','127.0.0.1:1','rooms.rec.net'];
  for(let i=0;i<1000;i++){
    const uri=(i%2?'http':'https')+'://'+hosts[i%hosts.length]+'/api/path'+i+'?nonce='+i;
    const r=policy.route(uri,port);const u=new URL(r.url);assert.equal(u.origin,local);
    assert.equal(u.username,'');assert.equal(u.password,'');
  }
});
