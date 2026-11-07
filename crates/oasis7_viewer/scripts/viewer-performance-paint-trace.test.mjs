import { test } from 'node:test';
import assert from 'node:assert/strict';
import { matchInputPaintTrace } from './viewer-performance-paint-trace.mjs';
const sample = {index:0,expectedId:'A',startTime:100,clickTime:102,endTime:180,durationMs:80,isTrusted:true,clickTrusted:true};
const event = {type:'click',target:'A',startTime:102,duration:40,interactionId:9};
test('separates DOM acceptance, native paint estimate and extra rAF opportunities', () => {
 const result=matchInputPaintTrace([sample],{supported:true,mutations:[{target:'A',time:108,selected:true}],events:[event]});
 assert.equal(result.rows[0].domAcceptedMs,8);assert.equal(result.rows[0].nextPaintEstimateMs,42);
 assert.deepEqual(result.rows[0].nextPaintIntervalMs,[38,46]);assert.equal(result.rows[0].legacyRafOpportunityMs,80);
});
test('missing or identity-mismatched paint stays unknown rather than zero', () => {
 for(const events of [[],[{...event,target:'B'}],[{...event,startTime:110}]]) {
  const result=matchInputPaintTrace([sample],{supported:true,mutations:[{target:'A',time:108,selected:true}],events});
  assert.equal(result.paintMatched,0);assert.equal(result.unknownPaint,1);assert.equal(result.rows[0].nextPaintEstimateMs,null);
 }
});
test('a paint before feedback and ambiguous duplicate event cannot prove feedback presentation', () => {
 for(const trace of [{mutations:[{target:'A',time:150,selected:true}],events:[event]},{mutations:[{target:'A',time:108,selected:true}],events:[event,event]}]) {
  assert.equal(matchInputPaintTrace([sample],trace).paintMatched,0);
 }
 assert.throws(()=>matchInputPaintTrace([sample],{overflow:true}),/overflow/);
});

test('feedback coalesced away before native next paint is not signed as presented',()=>{
 const result=matchInputPaintTrace([sample],{supported:true,mutations:[{target:'A',time:108,selected:true},{target:'A',time:130,selected:false}],events:[event]});
 assert.equal(result.paintMatched,0);assert.equal(result.rows[0].domAcceptedMs,8);
});

test('feedback inside rounded paint interval is unknown until it precedes lower bound',()=>{
 for(const time of [139,142,144]){
  const result=matchInputPaintTrace([sample],{supported:true,mutations:[{target:'A',time,selected:true}],events:[event]});
  assert.equal(result.paintMatched,0);assert.equal(result.rows[0].nextPaintEstimateMs,null);
 }
 const boundary=matchInputPaintTrace([sample],{supported:true,mutations:[{target:'A',time:138,selected:true}],events:[event]});
 assert.equal(boundary.paintMatched,1);
});
