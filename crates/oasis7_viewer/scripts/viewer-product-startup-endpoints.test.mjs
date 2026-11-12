import test from 'node:test';
import assert from 'node:assert/strict';
import {assertViewerScene,readOwnedViewerScene} from './viewer-product-startup-endpoints.mjs';
const sample=()=>({ready:true,visibility:'visible',world:'W',metrics:{status:'observed',source_cut:'C',agent_body_sprites:1000,in_view_agent_body_sprites:1000,laid_out_in_view_labels:200},identityValid:true,stats:{scopeBound:true,loading:false,published:{sourceCutMatches:true,queryGenerationMatches:true,completeness:'complete',entities:1000}},target:{width:44,height:44},canvas:{},dpr:1,gpu:'Apple M3'});
test('actual complete frame can be1000 bodies without asserting200 until density preparation',()=>{const row=sample();row.metrics.laid_out_in_view_labels=100;assert.doesNotThrow(()=>assertViewerScene(row,{world:'W',cut:'C'}));assert.throws(()=>assertViewerScene(row,{world:'W',cut:'C',requireLabels:true}));});
test('scope/query/cut mismatch, partial/stale frame or forged player identity fails closed',()=>{for(const change of [r=>r.world='old',r=>r.metrics.source_cut='old',r=>r.stats.published.queryGenerationMatches=false,r=>r.stats.published.completeness='partial',r=>r.stats.loading=true,r=>r.identityValid=false,r=>r.metrics.in_view_agent_body_sprites=999,r=>r.visibility='hidden',r=>r.gpu='SwiftShader']){const row=sample();change(row);assert.throws(()=>assertViewerScene(row,{world:'W',cut:'C',requireGpu:true}));}});
test('before rendererReady no bridge import or metric getter is touched',async()=>{
 const original=globalThis.document;let reads=0;
 globalThis.document={visibilityState:'visible',querySelector:()=>{reads++;return {dataset:{rendererReady:'false'}};}};
 try{const row=await readOwnedViewerScene({evaluate:(fn,args)=>fn(args)},'C');assert.equal(row.ready,false);assert.equal(reads,1);assert.equal('metrics' in row,false);}finally{if(original===undefined)delete globalThis.document;else globalThis.document=original;}
 // If the absolute browser import executes under Node, this test rejects
 // before reaching these assertions: it is deliberately not mocked away.
});
