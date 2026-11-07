import assert from 'node:assert/strict';
import test from 'node:test';
import {continuePreparedBaseline} from './fixtures/setup_baseline_settlement.mjs';
const reply=value=>Response.json(value);
const pending=()=>reply({stage:'ready_for_setup',pending_operations:['final_observation'],blocked_operations:[{code:'outcome_unknown'}]});

test('baseline observes the same original setup until its actual callback completes',async()=>{
  const ids=[],plan=Object.freeze({setup_id:'original-setup',deadline:'1000'});
  const runtime={continueSetup:async id=>{ids.push(id);return ids.length===1?pending():reply({stage:'prepared'});}};
  const result=await continuePreparedBaseline(runtime,plan,{now:()=>0,pause:async()=>{}});
  assert.equal((await result.json()).stage,'prepared');
  assert.deepEqual(ids,['original-setup','original-setup']);
});

test('baseline never converts an unresolved original to prepared and has a finite observation budget',async()=>{
  let reads=0;
  const result=await continuePreparedBaseline({continueSetup:async()=>{reads++;return pending();}},
    {setup_id:'original',deadline:'1000'},{now:()=>0,pause:async()=>{}});
  assert.equal(reads,16);assert.equal((await result.json()).stage,'ready_for_setup');
});

test('baseline respects the original absolute deadline without extending it',async()=>{
  let clock=0,reads=0;
  const runtime={continueSetup:async()=>{reads++;return pending();}};
  const result=await continuePreparedBaseline(runtime,{setup_id:'original',deadline:'10'},
    {now:()=>clock,pause:async ms=>{assert.equal(ms,10);clock+=ms;}});
  assert.equal(reads,1);assert.equal((await result.json()).stage,'ready_for_setup');
  await assert.rejects(continuePreparedBaseline(runtime,{setup_id:'original',deadline:'10'},
    {now:()=>10,pause:async()=>{throw Error('must not pause');}}),/deadline elapsed/);
  assert.equal(reads,1);
});

test('baseline does not retry rejected authority or terminal blocked states',async()=>{
  for(const response of [new Response('denied',{status:503}),reply({stage:'ready_for_setup',blocked_operations:[{code:'authority_revoked'}]})]) {
    let reads=0;
    const result=await continuePreparedBaseline({continueSetup:async()=>{reads++;return response;}},
      {setup_id:'original',deadline:'1000'},{now:()=>0,pause:async()=>{throw Error('must not retry');}});
    assert.equal(reads,1);assert.equal(result,response);
  }
});
