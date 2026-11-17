import assert from 'node:assert/strict';
import test from 'node:test';
import {createNativeResponses,projectNativeResponses} from './fixtures/native_response_diagnostics.mjs';
import {assertNativeStage} from './fixtures/native_stage_diagnostics.mjs';

test('records only bounded fixed metadata and never keeps secrets',()=>{
  let now=10;const recorder=createNativeResponses(()=>now);
  const event=recorder.begin('/capabilities');now=25;event.headers(200);
  now=30;event.body(JSON.stringify({csrf_token:'a'.repeat(64),cookie:'SECRET',nested:{token:'SECRET'}}));
  assert.deepEqual(recorder.snapshot(),{omitted:0,events:[{route:'/capabilities',phase:'body',status_class:'2xx',elapsed_ms:20,csrf_shape:'valid'}]});
  assert.ok(!JSON.stringify(recorder.snapshot()).includes('SECRET'));
});
test('classifies absent malformed invalid and oversized capability bodies',()=>{
  const recorder=createNativeResponses();
  for(const [body,shape] of [['{}','missing'],['{"csrf_token":"SECRET"}','invalid'],['SECRET','invalid_json'],['x'.repeat(32769),'oversize']]){
    recorder.begin('/capabilities').body(body);assert.equal(recorder.snapshot().events.at(-1).csrf_shape,shape);
  }
});
test('bounds retained events and rejects arbitrary routes and output fields',()=>{
  const recorder=createNativeResponses(()=>0);
  recorder.begin('/SECRET').body('SECRET');assert.equal(recorder.snapshot().events.length,0);
  for(let i=0;i<20;i++)recorder.begin('/native-key/complete').headers(500);
  assert.equal(recorder.snapshot().events.length,16);assert.equal(recorder.snapshot().omitted,4);
  const result=projectNativeResponses({omitted:-1,events:[{route:'SECRET',phase:'SECRET',status_class:'SECRET',elapsed_ms:Infinity,csrf_shape:'SECRET',cookie:'SECRET'}]});
  assert.ok(!JSON.stringify(result).includes('SECRET'));
});
test('pending and failed transport remain distinct without reading errors',()=>{
  const recorder=createNativeResponses(()=>1);const event=recorder.begin('/capabilities');
  assert.equal(recorder.snapshot().events[0].phase,'started');event.failed();
  assert.equal(recorder.snapshot().events[0].phase,'transport_error');
});
test('successful assertion does not read or emit diagnostic facts',async()=>{
  let reads=0;const runtime={nativeResponses(){reads++;throw Error('secret');},suspend(){reads++;}};
  await assertNativeStage(runtime,{stage:'approved'},'approved',{emit(){reads++;}});assert.equal(reads,0);
});
test('failure emits second-whitelisted facts and retains strict original assertion',async()=>{
  let output;const runtime={nativeResponses:()=>({events:[{route:'SECRET',headers:'SECRET'}]}),suspend:async()=>{throw Error('SECRET');}};
  await assert.rejects(assertNativeStage(runtime,{stage:'approved'},'native_session_ready',{emit:s=>{output=s;}}),{code:'ERR_ASSERTION'});
  const report=JSON.parse(output);assert.equal(report.stage,'approved');assert.equal(report.responses.events[0].route,'unknown');assert.ok(!output.includes('SECRET'));
});
