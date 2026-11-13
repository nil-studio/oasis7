import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createOwnedCdpInput, enterKeyEvents } from './viewer-owned-cdp-input.mjs';
import { collectNativeSelectionSamples } from './viewer-performance-native-input.mjs';

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

async function fakeBrowser(targets, action, respond = () => undefined) {
  const original = globalThis.WebSocket, messages = [], sockets = [];
  globalThis.WebSocket = class extends EventTarget {
    constructor() { super(); sockets.push(this); queueMicrotask(() => this.dispatchEvent(new Event('open'))); }
    send(raw) {
      const request = JSON.parse(raw); messages.push(request);
      const result = respond(request) ?? (request.method === 'Target.getTargets' ? { targetInfos: targets }
        : request.method === 'Target.attachToTarget' ? { sessionId: 'owned-session' } : {});
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

test('JSON-decoding sampler adapters and raw visual adapters read the same owned URL', async () => {
  for (const decoded of [false, true]) {
    await fakeBrowser([{ type: 'page', url: 'http://owned/', targetId: 'owned' }], async (messages, sockets) => {
      const evaluate = async code => {
        const value = code === 'JSON.stringify(location.href)' ? JSON.stringify('http://owned/') : 'http://owned/';
        return decoded ? JSON.parse(value) : value;
      };
      const input = await createOwnedCdpInput(run, evaluate);
      try { await input.enter(); } finally { input.close(); }
      assert.equal(messages.filter(m => m.method === 'Input.dispatchKeyEvent').length, 2);
      assert(sockets.every(socket => socket.closed));
    });
  }
});

// The CLI adapter deliberately invalidates focus after its evaluate command.
// This models removal/replacement of the focused renderer target before Enter;
// the input must prepare through its already attached native CDP session.
test('renderer prepares the same target on the native session without a CLI focus gap', async () => {
  let focused=false, delivered=false, cliFocusCalls=0;
  await fakeBrowser([{type:'page',url:'http://owned/',targetId:'owned'}],async(messages)=>{
    const evaluate=async code=>{
      if(code.includes('location.href'))return 'http://owned/';
      if(code.includes('candidates[0]'))return {id:'agent-A',x:100,y:100};
      if(code.includes('same native target did not')){cliFocusCalls++;focused=false;return true;}
      if(code.includes('native selection feedback timed out')){
        if(!delivered)throw Error('Enter did not reach the prepared target');
        return {status:'complete',isTrusted:true,selectedKind:'agent',selectedId:'agent-A',durationMs:40,startTime:100,endTime:140,timeOrigin:1};
      }
      return true;
    };
    const rows=await collectNativeSelectionSamples({evaluate,run,count:1,selectionSource:'renderer'});
    assert.equal(rows.length,1);assert.equal(cliFocusCalls,0);
    const native=messages.filter(m=>['Runtime.evaluate','Input.dispatchKeyEvent'].includes(m.method));
    assert.deepEqual(native.map(m=>m.method),['Runtime.evaluate','Input.dispatchKeyEvent','Input.dispatchKeyEvent']);
    assert(native.every(m=>m.sessionId==='owned-session'));
  },request=>{
    if(request.method==='Runtime.evaluate'){focused=true;return {result:{type:'boolean',value:true}};}
    if(request.method==='Input.dispatchKeyEvent'&&request.params.type==='keyDown')delivered=focused;
  });
});
test('failed or unconfirmed native-session preparation sends no key and never retries', async () => {
  for(const response of [{exceptionDetails:{text:'target disappeared'}},{result:{type:'boolean',value:false}}]){
    await fakeBrowser([{type:'page',url:'http://owned/',targetId:'owned'}],async messages=>{
      const input=await createOwnedCdpInput(run,async()=> 'http://owned/');
      try{await assert.rejects(input.prepareAndEnter('true'),/preparation/);}finally{input.close();}
      assert.equal(messages.filter(m=>m.method==='Runtime.evaluate').length,1);
      assert.equal(messages.filter(m=>m.method==='Input.dispatchKeyEvent').length,0);
    },request=>request.method==='Runtime.evaluate'?response:undefined);
  }
});
