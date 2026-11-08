import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createOwnedCdpInput, enterKeyEvents } from './viewer-owned-cdp-input.mjs';

test('Enter retains standard key identity and omits platform-native keycodes', () => {
  assert.deepEqual(enterKeyEvents(), [
    { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r', unmodifiedText: '\r' },
    { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 },
  ]);
  assert(enterKeyEvents().every(event => !Object.hasOwn(event, 'nativeVirtualKeyCode')));
});

test('remote endpoints reject before opening any CDP connection', async () => {
  await assert.rejects(createOwnedCdpInput(async () => JSON.stringify({ data: { cdpUrl: 'ws://example.com:9222/devtools/browser/x' } }), async () => 'unused'), /owned local browser/);
});

async function fakeBrowser(targets, action) {
  const original = globalThis.WebSocket, messages = [], sockets = [];
  globalThis.WebSocket = class extends EventTarget {
    constructor() { super(); sockets.push(this); queueMicrotask(() => this.dispatchEvent(new Event('open'))); }
    send(raw) {
      const request = JSON.parse(raw); messages.push(request);
      const result = request.method === 'Target.getTargets' ? { targetInfos: targets }
        : request.method === 'Target.attachToTarget' ? { sessionId: 'owned-session' } : {};
      queueMicrotask(() => this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify({ id: request.id, result }) })));
    }
    close() { this.closed = true; }
  };
  try { await action(messages, sockets); } finally { globalThis.WebSocket = original; }
}
const run = async () => JSON.stringify({ data: { cdpUrl: 'ws://127.0.0.1:9222/devtools/browser/owned' } });
test('only the unique owned URL target receives the two exact native key messages', async () => {
  await fakeBrowser([{ type: 'page', url: 'http://owned/', targetId: 'page-owned' }, { type: 'page', url: 'http://other/', targetId: 'other' }], async (messages, sockets) => {
    const input = await createOwnedCdpInput(run, async () => 'http://owned/');
    try { await input.enter(); } finally { input.close(); }
    assert.equal(messages.find(m => m.method === 'Target.attachToTarget').params.targetId, 'page-owned');
    const keys = messages.filter(m => m.method === 'Input.dispatchKeyEvent');
    assert.deepEqual(keys.map(m => m.params), enterKeyEvents());
    assert(keys.every(m => m.sessionId === 'owned-session'));
    assert(sockets.every(socket => socket.closed));
  });
});
test('ambiguous pages fail closed and close the owned connection before input', async () => {
  await fakeBrowser([{ type: 'page', url: 'http://owned/', targetId: 'a' }, { type: 'page', url: 'http://owned/', targetId: 'b' }], async (messages, sockets) => {
    await assert.rejects(createOwnedCdpInput(run, async () => 'http://owned/'), /uniquely match/);
    assert(!messages.some(m => m.method.startsWith('Input.')));
    assert(sockets.every(socket => socket.closed));
  });
});
