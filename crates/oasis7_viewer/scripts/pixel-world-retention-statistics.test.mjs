import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { measureRetainedJson } from '../software_safe_src/pixel_world_retention.js';
import { createScopePendingFixture } from './scope-pending-fixture.mjs';
import { createWorldViewClient } from '../software_safe_src/pixel_world_world_view.js';
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
    const partial=client.statistics(); assert.equal(partial.published.entities,1); assert.equal(partial.published.completeness,'partial'); assert.equal(partial.paging.maps[0],1); assert(partial.paging.logicalJsonBytes>0); assert(!JSON.stringify(partial).includes('scope-A'));
    client.reset('another'); assert.equal(client.statistics().paging.active,false); finish();await pending;assert.equal(client.statistics().published.entities,0);
  });
  it('does not expose pending old-generation paging data after reset' , async () => {
    let finish; const client = createWorldViewClient({ request: () => new Promise(resolve => { finish = resolve; }) });
    client.reset('secret-old-scope'); const pending = client.query(); assert.equal(client.statistics().paging.active, true);
    client.reset('new-scope'); const current = client.statistics(); assert.equal(current.paging.active, false); assert.equal(current.published.entities, 0); assert(!JSON.stringify(current).includes('secret'));
    finish({}); await pending; assert.equal(client.statistics().paging.active, false);
  });
});
