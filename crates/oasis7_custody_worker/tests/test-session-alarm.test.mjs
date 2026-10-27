import test from 'node:test';
import assert from 'node:assert/strict';
import {prearmAndCommitTestSessionAlarm,reconcileTestSessionAlarm} from '../platform/test_session_alarm.mjs';
function fixture({failSet=false}={}) {
  let alarm=null;
  return {state:{storage:{getAlarm:async()=>alarm,setAlarm:async value=>{if(failSet)throw Error('fixed storage fault');alarm=value;},deleteAlarm:async()=>{alarm=null;}}},read:()=>alarm};
}
test('alarm storage failure never invokes the synchronous SQL commit',async()=>{
  const f=fixture({failSet:true});let calls=0;
  await assert.rejects(prearmAndCommitTestSessionAlarm(f.state,String(Date.now()+60000),()=>{calls++;}));
  assert.equal(calls,0);assert.equal(f.read(),null);
});
test('SQL commit failure retains its conservative wake',async()=>{
  const f=fixture(),at=Date.now()+60000;
  await assert.rejects(prearmAndCommitTestSessionAlarm(f.state,String(at),()=>{throw Error('fixed SQL rollback');}));
  assert.equal(f.read(),at);
});
test('concurrent commits preserve the earlier deadline',async()=>{
  const f=fixture(),at=Date.now()+60000;let calls=0;
  await Promise.all([prearmAndCommitTestSessionAlarm(f.state,String(at+1000),()=>{calls++;}),prearmAndCommitTestSessionAlarm(f.state,String(at),()=>{calls++;})]);
  assert.equal(calls,2);assert.equal(f.read(),at);
});
test('reconcile cannot delete a prearm before its synchronous commit',async()=>{
  const f=fixture(),at=Date.now()+60000;let release,opened=false;
  const gate=new Promise(resolve=>{release=resolve;});
  const set=f.state.storage.setAlarm;
  f.state.storage.setAlarm=async value=>{await gate;await set(value);};
  const commit=prearmAndCommitTestSessionAlarm(f.state,String(at),()=>{opened=true;});
  const reconcile=reconcileTestSessionAlarm(f.state,()=>opened?String(at):'');
  release();await Promise.all([commit,reconcile]);assert.equal(opened,true);assert.equal(f.read(),at);
});
test('unknown state cannot become an empty scheduling decision',async()=>{
  const f=fixture(),at=Date.now()+60000;
  await prearmAndCommitTestSessionAlarm(f.state,String(at),()=>{});
  await assert.rejects(reconcileTestSessionAlarm(f.state,()=>{throw Error('fixed unknown inventory');}));
  assert.equal(f.read(),at);
});
