'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');

// Run the actual installed HTTP observer with inert Frida-shaped pointers.
// No Frida package, native process, private metadata, or network is accessed.
function fixture() {
  const ids = ['210451', '241858', '241859', '129313', '126411'];
  const bytes = Buffer.alloc(32, 0x90);
  const digest = crypto.createHash('sha256').update(bytes).digest('hex');
  const hooks = new Map(), messages = [];
  const methods = Object.fromEntries(ids.map((id, i) => [id, {rva: i * 64, prefixSha256: digest}]));
  const context = vm.createContext({
    CONFIG: {port: 4187, methods},
    Process: {id: 123, findRangeByAddress: () => ({protection: 'r-x'}), attachModuleObserver: () => ({})},
    Checksum: {compute: (algorithm, value) => crypto.createHash(algorithm).update(Buffer.from(value)).digest('hex')},
    NativeFunction: function () { return () => { throw new Error('HTTP observer must not construct strings'); }; },
    Interceptor: {
      attach(address, callbacks) { hooks.set(address.rva, callbacks); return {detach() {}}; },
      flush() {}
    },
    rpc: {exports: {}}, send: value => messages.push(value), setInterval: () => 0, Date
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'route-policy.js'), 'utf8'), context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'runtime-agent.js'), 'utf8'), context);
  context.fixtureModule = {base: {add: rva => ({rva, readByteArray: size => bytes.subarray(0, size)})}};
  vm.runInContext('installClient(fixtureModule)', context);
  assert.equal(hooks.size, 4);
  assert.equal(context.rpc.exports.status().ready, true);
  assert.equal(messages.filter(x => x.kind === 'client_route_adapter_ready').length, 1);
  return {observe: hooks.get(methods['129313'].rva).onEnter, messages, status: context.rpc.exports.status};
}

function request(method, padding) {
  const memory = Buffer.alloc(112);
  memory[24] = method;
  Buffer.from(padding).copy(memory, 25);
  const value = 'http://127.0.0.1:4187/recnet/api/config';
  const managedString = {
    isNull: () => false,
    add: offset => offset === 16 ? {readS32: () => value.length} : {readUtf16String: () => value}
  };
  const uri = {add: () => ({readPointer: () => managedString})};
  return {
    memory,
    pointer: {add: offset => ({
      readPointer: () => uri,
      readU8: () => memory.readUInt8(offset),
      readS32: () => memory.readInt32LE(offset)
    })}
  };
}

for (const [name, method, padding] of [
  ['GET with nonzero adjacent padding', 0, [0, 0, 0x7f]],
  ['POST with nonzero adjacent padding', 2, [0xff, 0xff, 0xff]],
  ['GET with zero padding', 0, [0, 0, 0]],
  ['POST with zero padding', 2, [0, 0, 0]]
]) {
  test(name, () => {
    const f = fixture(), input = request(method, padding), before = Buffer.from(input.memory);
    f.observe([input.pointer]);
    const events = f.messages.filter(x => x.kind === 'native_http_request');
    assert.equal(events.length, 1);
    assert.equal(events[0].method, method);
    assert.equal(events[0].ownedOrigin, true);
    assert.equal(events[0].path, '/recnet/api/config');
    assert.deepEqual(input.memory, before);
    assert.equal(f.status().routeCount, 0);
    assert.equal(f.status().stopped, false);
    assert.equal(f.messages.some(x => x.kind === 'agent_failure'), false);
  });
}
