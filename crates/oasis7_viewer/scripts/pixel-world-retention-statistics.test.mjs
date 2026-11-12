import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { measureRetainedJson } from '../software_safe_src/pixel_world_retention.js';
import { createScopePendingFixture } from './scope-pending-fixture.mjs';
import { createWorldViewClient, createWorldViewTransport } from '../software_safe_src/pixel_world_world_view.js';
describe('read-only retention components', () => {
  it('counts UTF8 logical encoding without conflating reference identity with bytes', () => {
    const shared = { name: '汉字\n"', values: [null, true, 12] }, roots = [shared, shared];
    const before = JSON.stringify(roots), stats = measureRetainedJson(roots);
    assert.equal(stats.logicalJsonBytes, 2 * new TextEncoder().encode(JSON.stringify(shared)).length);
    assert.equal(stats.uniqueObjects, 1); assert.equal(stats.uniqueArrays, 1); assert.equal(stats.heapBytes, null); assert.equal(JSON.stringify(roots), before);
  });
  it('matches sparse JSON arrays and omitted undefined fields', () => { const value = { missing: undefined, slots: Array(3) }; assert.equal(measureRetainedJson([value]).logicalJsonBytes, new TextEncoder().encode(JSON.stringify(value)).length); });
  it('refuses cycles and bounds observation work', () => { const cycle = {}; cycle.self = cycle; assert.throws(() => measureRetainedJson([cycle]), /cycle/); assert.throws(() => measureRetainedJson([Array(524289).fill(null)]), /budget/); });
  it('observes actual partial Maps and releases only its current-generation registration', async () => {
    const fixture = createScopePendingFixture(); let finish;
    const client = createWorldViewClient({ request: query => { const response = fixture.reply({type:'request_world_view',query},'owned').view; return query.cursor ? new Promise(resolve=>{finish=()=>resolve(response);}) : Promise.resolve(response); } });
    client.reset('public',fixture.describe().world); const pending = client.query({});
    await new Promise(resolve=>setImmediate(resolve));
    const partial=client.statistics('scope-qa-cut-1'); assert.equal(partial.published.queryGenerationMatches,true); assert.equal(partial.published.sourceCutMatches,true); assert.equal(client.statistics('wrong-cut').published.sourceCutMatches,false); assert.equal(partial.published.entities,1); assert.equal(partial.published.completeness,'partial'); assert.equal(partial.paging.maps[0],1); assert(partial.paging.logicalJsonBytes>0); assert(!JSON.stringify(partial).includes('scope-A'));
    const previousScope=partial.scopeSequence; client.reset('another'); assert.equal(client.statistics().scopeSequence,previousScope+1); assert.equal(client.statistics().published.queryGenerationMatches,false); assert.equal(client.statistics().paging.active,false); finish();await pending;assert.equal(client.statistics().published.entities,0);
  });
  it('cheap phase stamps never traverse DTO values or trigger a read', () => {
    let requests=0;const client=createWorldViewClient({request:async()=>{requests++;return null;}}),original=globalThis.TextEncoder;
    globalThis.TextEncoder=class {constructor(){throw Error('DTO traversal attempted');}};
    try{const stamp=client.statistics('known-cut',false);assert.equal(stamp.measured,false);assert.equal(stamp.published.logicalJsonBytes,null);assert.equal(stamp.published.visitedNodes,0);assert.equal(requests,0);}finally{globalThis.TextEncoder=original;}
  });
  it('distinguishes client remounts without exposing scope identity', () => { const a=createWorldViewClient({request:async()=>null}), b=createWorldViewClient({request:async()=>null}); assert.notEqual(a.statistics().instanceSequence,b.statistics().instanceSequence); });
  it('does not expose pending old-generation paging data after reset' , async () => {
    let finish; const client = createWorldViewClient({ request: () => new Promise(resolve => { finish = resolve; }) });
    client.reset('secret-old-scope'); const pending = client.query(); assert.equal(client.statistics().paging.active, true);
    client.reset('new-scope'); const current = client.statistics(); assert.equal(current.paging.active, false); assert.equal(current.published.entities, 0); assert(!JSON.stringify(current).includes('secret'));
    finish({}); await pending; assert.equal(client.statistics().paging.active, false);
  });
});

describe('inflight scalar lifecycle coverage',()=>{
 it('retains retired Map counts after invalidation until the held page completes',async()=>{
  const fixture=createScopePendingFixture();let release;const client=createWorldViewClient({request:q=>{const page=fixture.reply({type:'request_world_view',query:q},'local').view;return q.cursor?new Promise(done=>{release=()=>done(page)}):Promise.resolve(page);}});
  client.reset('private-scope',fixture.describe().world);const task=client.query({});await new Promise(done=>setImmediate(done));
  const current=client.statistics(null,false).inflight;assert.equal(current.currentQueries,1);assert.deepEqual(current.currentMapEntries,[1,0,0,0]);client.invalidate();
  const retired=client.statistics(null,false).inflight;assert.equal(retired.currentQueries,0);assert.equal(retired.retiredQueries,1);assert.equal(retired.retiredPageReferences,2);assert.deepEqual(retired.retiredMapEntries,[1,0,0,0]);assert(!JSON.stringify(retired).includes('scope'));release();await task;assert.equal(client.statistics(null,false).inflight.retiredQueries,0);
 });
 it('reset does not pretend held closures released and rejected completion releases registration',async()=>{let reject;const client=createWorldViewClient({request:()=>new Promise((_,no)=>reject=no)});client.reset('old');const task=client.query();client.reset('new');assert.equal(client.statistics(null,false).inflight.retiredQueries,1);reject(Error('late'));await task;assert.equal(client.statistics(null,false).inflight.retiredQueries,0);assert.equal(client.statistics().published.entities,0);});
 it('transport pending counts follow success, error, clear and send failure without keys',async()=>{let message;const transport=createWorldViewTransport({send:m=>message=m});const a=transport.request({request_generation:'1'});assert.deepEqual(Object.keys(transport.statistics()).sort(),['pendingRequests','semantics']);assert.equal(transport.statistics().pendingRequests,1);transport.handle({type:'world_view',view:{request_generation:message.query.request_generation}});await a;assert.equal(transport.statistics().pendingRequests,0);const e=transport.request({request_generation:'error'});const errorRejected=assert.rejects(e);transport.handle({type:'world_view_error',error:{request_generation:message.query.request_generation,code:'unavailable'}});await errorRejected;assert.equal(transport.statistics().pendingRequests,0);const b=transport.request({request_generation:'2'});const rejected=assert.rejects(b);transport.clear();await rejected;assert.equal(transport.statistics().pendingRequests,0);const failing=createWorldViewTransport({send:()=>{throw Error('send')}});await assert.rejects(failing.request({request_generation:'3'}));assert.equal(failing.statistics().pendingRequests,0);});
});

it('transport timeout releases pending scalar without inventing successful data',async()=>{const transport=createWorldViewTransport({send:()=>{},timeoutMs:1});await assert.rejects(transport.request({request_generation:'timeout'}),/timed out/);assert.equal(transport.statistics().pendingRequests,0);});
