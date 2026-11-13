'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');

// Only diagnostic/control logic: no Frida, native process, or network connection.
function fixture(){
  const messages=[];
  const context=vm.createContext({CONFIG:{port:4187,fixtureOnly:true},Process:{id:123,attachModuleObserver:()=>({})},rpc:{exports:{}},send:value=>messages.push(value),setInterval:()=>0,Date});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'runtime-agent.js'),'utf8'),context);
  return{messages,context,run:code=>vm.runInContext(code,context)};
}
test('loading diagnostics does not assert a native adapter or network guard is ready',()=>{
  const f=fixture();const state=f.run('rpc.exports.status()');
  assert.equal(state.ready,false);assert.equal(state.networkGuards,false);assert.equal(state.stopped,false);
  assert.equal(f.messages[0].kind,'agent_loaded');
});
test('agent failure remains observable after ordinary diagnostics fill their cap',()=>{
  const f=fixture();f.run("for(let i=0;i<700;i++)event('fixture',{i});");assert.equal(f.messages.length,512);
  f.run("failure('fixture',new Error('expected failure')); ");
  assert.equal(f.messages.length,513);assert.equal(f.messages[512].kind,'agent_failure');assert.equal(f.run('rpc.exports.status().stopped'),true);
});
test('repeat failures do not grow an unbounded error stream',()=>{
  const f=fixture();f.run("for(let i=0;i<700;i++)failure('fixture',new Error('expected')); ");
  assert.equal(f.messages.filter(x=>x.kind==='agent_failure').length,1);
});
