import { createOwnedCdpSession } from './viewer-owned-cdp-session.mjs';

// Owned browser input only. Native platform keycodes are deliberately omitted:
// on macOS an explicit native code can enter the browser's default key path.
export const enterKeyEvents = () => [
  { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r', unmodifiedText: '\r' },
  { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 },
];

export async function createOwnedCdpInput(run, evaluate) {
  const session = await createOwnedCdpSession(run, evaluate);
  return {
    async enter() { for (const params of enterKeyEvents()) await session.page('Input.dispatchKeyEvent', params); },
    async frameBoundary() {
      const r = await session.page('Runtime.evaluate', {expression:'new Promise(resolve=>requestAnimationFrame(frame=>resolve({frame,now:performance.now(),wall:Date.now()})))',awaitPromise:true,returnByValue:true});
      return r.result.value;
    },
    async click(point) {
      const p = {x:point.x,y:point.y,button:'left',clickCount:1};
      await session.page('Input.dispatchMouseEvent',{type:'mousePressed',buttons:1,...p});
      await session.page('Input.dispatchMouseEvent',{type:'mouseReleased',buttons:0,...p});
    },
    close: session.close,
  };
}
