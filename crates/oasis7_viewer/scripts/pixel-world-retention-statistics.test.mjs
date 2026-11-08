import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { measureRetainedJson } from '../software_safe_src/pixel_world_retention.js';
import { createWorldViewClient } from '../software_safe_src/pixel_world_world_view.js';
describe('read-only retention components', () => {
  it('counts UTF8 logical encoding without conflating reference identity with bytes', () => {
    const shared = { name: '汉字\n"', values: [null, true, 12] }, roots = [shared, shared];
    const before = JSON.stringify(roots), stats = measureRetainedJson(roots);
    assert.equal(stats.logicalJsonBytes, 2 * new TextEncoder().encode(JSON.stringify(shared)).length);
    assert.equal(stats.uniqueObjects, 1); assert.equal(stats.uniqueArrays, 1); assert.equal(stats.heapBytes, null); assert.equal(JSON.stringify(roots), before);
  });
  it('refuses cycles and bounds observation work', () => { const cycle = {}; cycle.self = cycle; assert.throws(() => measureRetainedJson([cycle]), /cycle/); assert.throws(() => measureRetainedJson([Array(524289).fill(null)]), /budget/); });
  it('does not expose pending old-generation paging data after reset', async () => {
    let finish; const client = createWorldViewClient({ request: () => new Promise(resolve => { finish = resolve; }) });
    client.reset('secret-old-scope'); const pending = client.query(); assert.equal(client.statistics().paging.active, true);
    client.reset('new-scope'); const current = client.statistics(); assert.equal(current.paging.active, false); assert.equal(current.published.entities, 0); assert(!JSON.stringify(current).includes('secret'));
    finish({}); await pending; assert.equal(client.statistics().paging.active, false);
  });
});
