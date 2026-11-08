import {test} from 'node:test';
import assert from 'node:assert/strict';
import {runInNewContext} from 'node:vm';
import {retentionFocusScript,validateRetentionFocusAnchor} from './viewer-retention-focus.mjs';
function context({readyAfter=0,count=1000,identity=false}={}) {
 let now=0,attempts=0,focused=0;const reads=[];
 const document={activeElement:null,querySelector(){attempts++;return attempts>readyAfter?button:null;}};
 const button={isConnected:true,getBoundingClientRect:()=>({left:10,top:10,right:54,bottom:54,width:44,height:44}),focus(){focused++;document.activeElement=this;}};
 const window={__AW_TEST__:{getState:()=>({authReady:identity,authPlayerId:null,authPublicKey:null,authBoundAgentId:null,authRegistrationStatus:'guest'})},__OASIS7_WORLD_VIEW_RETENTION__:(cut,measure)=>{reads.push({cut,measure});return{loading:false,stale:false,hasError:false,localGeneration:2,scopeSequence:1,instanceSequence:1,published:{queryGenerationMatches:true,sourceCutMatches:true,completeness:'complete',entities:count}};},__QA_READ_METRICS__:()=>({status:'observed',display_profile:'world_view_v1',source_cut:'cut',projected_marker_count:1000,agent_body_sprites:1000,in_view_agent_body_sprites:1000,laid_out_in_view_labels:200,render_version:9})};
 return {globals:{window,document,innerWidth:800,innerHeight:600,performance:{now:()=>now,timeOrigin:1000},setTimeout:resolve=>{now+=25;resolve();}},reads,get focused(){return focused;},get elapsed(){return now;}};
}
test('waits for the same attached target then focuses once, using cheap reads only',async()=>{const c=context({readyAfter:2});const result=await runInNewContext(retentionFocusScript('button','cut'),c.globals);assert.equal(result.focused,true);assert.equal(c.focused,1);assert.equal(c.elapsed,50);assert(c.reads.every(row=>row.measure===false));});
test('never-ready preparation is bounded and sends no focus/input',async()=>{const c=context({readyAfter:Infinity});await assert.rejects(runInNewContext(retentionFocusScript('button','cut'),c.globals),/did not become ready/);assert.equal(c.focused,0);assert.equal(c.elapsed,10000);});
test('complete999 or acquired identity stops before focus instead of retrying',async()=>{for(const options of [{count:999},{identity:true}]){const c=context(options);await assert.rejects(runInNewContext(retentionFocusScript('button','cut'),c.globals),/cardinality|identity/);assert.equal(c.focused,0);assert.equal(c.elapsed,0);}});

test('scope, instance and page changes reject before the caller can send Enter',()=>{const before={scope:2,instance:1,origin:1000};for(const change of [{scope:3},{instance:2},{origin:1001}]){let sent=0;assert.throws(()=>{validateRetentionFocusAnchor(before,{focused:true,...before,...change});sent++;},/changed/);assert.equal(sent,0);}assert.deepEqual(validateRetentionFocusAnchor(before,{focused:true,...before,generation:99},before),before);});
