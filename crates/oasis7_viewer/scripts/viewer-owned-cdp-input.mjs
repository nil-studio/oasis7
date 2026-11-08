// Owned browser input only. Native platform keycodes are deliberately omitted:
// on macOS an explicit native code can enter the browser's default key path.
export const enterKeyEvents = () => [
  { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r', unmodifiedText: '\r' },
  { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 },
];

export async function createOwnedCdpInput(run, evaluate) {
  const reply = JSON.parse(await run(['--json', 'get', 'cdp-url']));
  const url = new URL(reply.data?.cdpUrl);
  if (url.protocol !== 'ws:' || url.hostname !== '127.0.0.1' || !url.port || url.username || url.password) throw Error('CDP endpoint was not the owned local browser');
  const ws = new WebSocket(url), pending = new Map(); let sequence = 0;
  const close = () => { ws.close(); for (const p of pending.values()) { clearTimeout(p.timer); p.no(Error('owned CDP closed')); } pending.clear(); };
  const send = (method, params = {}, sessionId) => new Promise((yes, no) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); no(Error(method + ' timed out')); }, 10000);
    pending.set(id, { yes, no, timer });
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  ws.addEventListener('message', event => {
    const r = JSON.parse(event.data), p = pending.get(r.id);
    if (p) { pending.delete(r.id); clearTimeout(p.timer); r.error ? p.no(Error(JSON.stringify(r.error))) : p.yes(r.result); }
  });
  try {
    await new Promise((yes, no) => {
      const timer = setTimeout(() => no(Error('owned CDP open timed out')), 10000);
      ws.addEventListener('open', () => { clearTimeout(timer); yes(); }, { once: true });
      ws.addEventListener('error', () => { clearTimeout(timer); no(Error('owned CDP open failed')); }, { once: true });
    });
    // Samplers may decode JSON strings while raw visual adapters leave them encoded.
    const value = await evaluate('JSON.stringify(location.href)');
    const href = typeof value === 'string' && value.startsWith('\"') ? JSON.parse(value) : value;
    if (typeof href !== 'string') throw Error('owned Viewer URL was not a string');
    const targets = await send('Target.getTargets');
    const page = targets.targetInfos.filter(t => t.type === 'page' && t.url === href);
    if (page.length !== 1) throw Error('CDP target did not uniquely match owned Viewer');
    const { sessionId } = await send('Target.attachToTarget', { targetId: page[0].targetId, flatten: true });
    return {
      async enter() { for (const params of enterKeyEvents()) await send('Input.dispatchKeyEvent', params, sessionId); },
      async frameBoundary() {
        const r = await send('Runtime.evaluate', { expression: 'new Promise(resolve=>requestAnimationFrame(frame=>resolve({frame,now:performance.now(),wall:Date.now()})))', awaitPromise: true, returnByValue: true }, sessionId);
        return r.result.value;
      },
      async click(point) {
        const p = { x: point.x, y: point.y, button: 'left', clickCount: 1 };
        await send('Input.dispatchMouseEvent', { type: 'mousePressed', buttons: 1, ...p }, sessionId);
        await send('Input.dispatchMouseEvent', { type: 'mouseReleased', buttons: 0, ...p }, sessionId);
      },
      close,
    };
  } catch (error) { close(); throw error; }
}
