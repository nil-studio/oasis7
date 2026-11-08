import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createScopePendingFixture } from './scope-pending-fixture.mjs';
import { createWorldViewClient, createWorldViewTransport } from '../software_safe_src/pixel_world_world_view.js';

test('finite baseline has three single-record pages and never negotiates delta', () => {
  const f = createScopePendingFixture();
  assert.deepEqual(f.reply({ type: 'hello_v2', capabilities: ['world_view', 'world_view_pins_v1', 'world_view_delta_v1'] }, 'c').capabilities, ['world_view', 'world_view_pins_v1']);
  let cursor, rows = [], page;
  do { page = f.reply({ type: 'request_world_view', query: { page_size: 200, request_generation: '2:1', ...(cursor ? { cursor } : {}) } }, 'c').view;
    assert.equal(page.entities.length, 1); rows.push(page.entities[0].entity_id); assert(rows.length <= 3, 'finite fixture exceeded three pages'); cursor = page.next_cursor;
  } while (cursor);
  assert.deepEqual(rows, ['scope-A', 'scope-C', 'scope-B']); assert.equal(page.completeness, 'complete');
  assert.equal(Object.keys(f.snapshot().model.agents).length, 0);
});
test('cursor is bound to connection/query and unsupported mutation is refused', () => {
  const f = createScopePendingFixture(), first = f.reply({ type: 'request_world_view', query: { page_size: 200, request_generation: '1' } }, 'one').view;
  assert.deepEqual(f.reply({ type: 'request_world_view', query: { page_size: 200, request_generation: '2', cursor: first.next_cursor } }, 'two').view.unavailable_reasons, ['cursor_binding_mismatch']);
  assert.throws(() => f.reply({ type: 'command' }, 'one'), /non-read/);
});
test('same held request can terminate as scope unavailable; late prepared page is ignored by transport', async () => {
  const f = createScopePendingFixture(), outgoing = []; let third;
  const transport = createWorldViewTransport({ send(message) { outgoing.push(message); const response = f.reply(message, 'one');
    if (outgoing.length === 3) third = { message, response }; else queueMicrotask(() => transport.handle(response)); } });
  const client = createWorldViewClient({ request: query => transport.request(query) }); client.reset('public', f.describe().world);
  const pending = client.query({});
  const deadline = Date.now() + 1000;
  while (!third && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 1));
  assert(third, 'third request was not reached within the fixture budget');
  assert.deepEqual(client.state().view.entities.map(row => row.entity_id), ['scope-A', 'scope-C']);
  assert.equal(third.response.view.entities[0].entity_id, 'scope-B');
  f.denyReads(); const unavailable = f.reply(third.message, 'one');
  assert.equal(unavailable.view.request_generation, third.message.query.request_generation);
  assert.deepEqual(unavailable.view.unavailable_reasons, ['scope_unavailable']);
  transport.handle(unavailable); await pending; assert.match(client.state().error, /scope unavailable/);
  transport.handle(third.response); assert(!client.state().view.entities.some(row => row.entity_id === 'scope-B'));
  assert(outgoing.every(row => row.query.page_size === 200 && !row.query.resume_cursor));
});
