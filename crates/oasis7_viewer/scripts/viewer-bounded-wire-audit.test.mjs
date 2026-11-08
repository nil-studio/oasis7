import{test}from'node:test';import assert from'node:assert/strict';
import{createBoundedWireAudit}from'./viewer-bounded-wire-audit.mjs';
test('crossing row survives but no later source or observer row is accepted',()=>{
 const lines=[],failures=[];let stops=0;
 const guard=createBoundedWireAudit({limitBytes:4,append:x=>lines.push(x),persistFailure:x=>failures.push(x),stopSource:()=>stops++});
 assert(guard.record(1));assert(guard.record(2));assert.equal(guard.record(3),false);assert.equal(lines.join(''),'1\n2\n3\n');
 assert.equal(guard.accepting(),false);assert.equal(stops,1);const error=guard.failure();
 assert.equal(guard.record({newConnection:true}),false);assert.equal(guard.failure(),error);assert.equal(lines.length,3);assert.equal(stops,1);assert.equal(failures[0].bytes,6);
});
test('persist failure cannot prevent stopping or replace budget error',()=>{
 let stops=0;const guard=createBoundedWireAudit({limitBytes:1,append:()=>{},persistFailure:()=>{throw Error('ENOSPC')},stopSource:()=>stops++});
 assert.equal(guard.record(1),false);assert.equal(stops,1);assert.match(guard.failure().message,/limit exceeded/);assert.match(guard.failure().persistenceError,/ENOSPC/);
});
