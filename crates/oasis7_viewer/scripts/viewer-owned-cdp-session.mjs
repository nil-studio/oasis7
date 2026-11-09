// A bounded connection to the browser created by this tool session. No arbitrary target routing.
export async function createOwnedCdpSession(run, evaluate) {
  const reply = JSON.parse(await run(['--json', 'get', 'cdp-url']));
  const url = new URL(reply.data?.cdpUrl);
  if (url.protocol !== 'ws:' || url.hostname !== '127.0.0.1' || !url.port || url.username || url.password) throw Error('CDP endpoint was not the owned local browser');
  const ws = new WebSocket(url), pending = new Map(); let sequence = 0, closed = false; const listeners = new Set();
  const close = () => { if (closed) return; closed = true; listeners.clear(); ws.close(); for (const p of pending.values()) { clearTimeout(p.timer); p.no(Error('owned CDP closed')); } pending.clear(); };
  const send = (method, params = {}, sessionId) => new Promise((yes, no) => {
    if (closed) { no(Error('owned CDP closed')); return; }
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); no(Error(method + ' timed out')); }, 10000);
    pending.set(id, { yes, no, timer });
    try { ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); }
    catch (error) { pending.delete(id); clearTimeout(timer); no(error); }
  });
  ws.addEventListener('close', close);
  ws.addEventListener('error', close);
  ws.addEventListener('message', event => {
    const r = JSON.parse(event.data), p = pending.get(r.id);
    if (!r.id && !closed) for (const listener of listeners) listener(r);
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
    const pageMethods = new Set(['Input.dispatchKeyEvent','Input.dispatchMouseEvent','Runtime.evaluate','Runtime.getHeapUsage','Performance.enable','Performance.getMetrics','Memory.getDOMCounters','Page.getLayoutMetrics','Page.startScreencast','Page.stopScreencast','Page.screencastFrameAck']);
    const browserMethods = new Set(['Browser.getVersion','SystemInfo.getProcessInfo','Tracing.getCategories']);
    const checked = (methods, method, params, session) => {
      if (!methods.has(method)) throw Error('CDP method outside owned observation/input contract: ' + method);
      return send(method, params, session);
    };
    return {
      pageCount: targets.targetInfos.filter(t => t.type === 'page').length,
      page: (method, params = {}) => checked(pageMethods, method, params, sessionId),
      async browser(method, params = {}) {
        if (!browserMethods.has(method)) throw Error('CDP method outside owned observation/input contract: ' + method);
        const current = (await send('Target.getTargets')).targetInfos.filter(t => t.type === 'page');
        if (current.length !== 1 || current[0].targetId !== page[0].targetId || current[0].url !== href) throw Error('browser-wide observation requires one owned page: ' + JSON.stringify({pages:current.length,attachedMatches:current.filter(t=>t.targetId===page[0].targetId).length,urlMatches:current.filter(t=>t.url===href).length,blankPages:current.filter(t=>t.url==='about:blank').length}));
        return checked(browserMethods, method, params);
      },
      onPageEvent(listener) { const scoped = event => { if (event.sessionId === sessionId) listener(event); }; listeners.add(scoped); return () => listeners.delete(scoped); },
      onEvent(listener) { listeners.add(listener); return () => listeners.delete(listener); },
      close,
    };
  } catch (error) { close(); throw error; }
}
