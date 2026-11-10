// Controlled standard WS + owned Chromium CDP native input. No application API mutation.
// DENSITY_QA_PENDING=1, CASE=confirm|replace|scope|absence via DENSITY_QA_PENDING_CASE.
// DENSITY_QA_PENDING_INPUT=keyboard|mouse. A native window miss exits nonzero,
// keeps every event, and does not retry or certify an unmeasured cancellation.
// Agent complete-projection omission is not an authoritative world_deleted event.
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createOwnedCdpInput } from './viewer-owned-cdp-input.mjs';
const assert = (value, message) => { if (!value) throw Error(message); };
const timeout = (promise, name) => Promise.race([promise, new Promise((_, reject) => { const t = setTimeout(() => reject(Error(name + ' timed out')), 12000); t.unref(); })]);

export async function probePendingWorldSelection({ run, evaluate, state, waitProjection, armPendingGate, freezeCompleteBoundary, unfreeze, fixture, reconnect, waitNegotiation, out }) {
  const mode = process.env.DENSITY_QA_PENDING_CASE || 'confirm';
  const inputMode = process.env.DENSITY_QA_PENDING_INPUT || 'keyboard';
  assert(['keyboard','mouse'].includes(inputMode), 'unknown native pending input');
  assert(['confirm', 'replace', 'scope', 'absence'].includes(mode), 'unknown pending probe case');
  const result = { case: mode, inputMode, scope: 'controlled actual WS third-page hold + CDP trusted Enter; not signed service/P5 or latency/P6. Prepared held replies are not wire delivery; wire.jsonl is authoritative.', target: 'qa-agent-0500' };
  const selector = 'button[data-agent-id="qa-agent-0500"][data-renderer-target]';
  const cdp = await createOwnedCdpInput(run, evaluate); let gate;
  try {
    await freezeCompleteBoundary();
    result.before = JSON.parse(await evaluate(`(async()=>{const end=Date.now()+10000;while(Date.now()<end){const m=window.__QA_READ_METRICS__();if(m.status==='observed'&&m.projected_marker_count===1000&&m.agent_body_sprites===1000&&m.in_view_agent_body_sprites===1000&&document.querySelector(${JSON.stringify(selector)}))return JSON.stringify({metrics:m,core:window.__AW_TEST__.getState(),targetPresent:true});await new Promise(r=>setTimeout(r,50))}throw Error('initial full observed scene did not settle')})()`));
    gate = armPendingGate('qa-agent-', async page => { result.thirdRequest = { request: page.request, connection: page.connection, receivedAt: page.receivedAt, cdpDispatchAt: Date.now() }; if(inputMode==='mouse')await cdp.click(result.focusPoint);else await cdp.enter(); });
    const disclosure = '.pixel-world-exploration-disclosure';
    if (!await evaluate(`document.querySelector('${disclosure}').open`)) await run(['click', disclosure + ' > summary']);
    await run(['fill', '.pixel-world-exploration form input', 'qa-agent-']);
    await evaluate(`document.querySelector('.pixel-world-exploration form input').focus({preventScroll:true});true`); await run(['press', 'Enter']);
    await timeout(gate.firstReceived, 'held first query page');
    await run(['click', disclosure + ' > summary']);
    result.focusPoint = await evaluate(`(()=>{const b=document.querySelector(${JSON.stringify(selector)});if(!b)throw Error('old presented0500 target missing before first partial');b.focus({preventScroll:true});window.__QA_PENDING_NATIVE__=[];const record=e=>{if(e.type==='keydown'&&e.key!=='Enter')return;const id=e.target.closest?.('[data-agent-id]')?.dataset.agentId;__QA_PENDING_NATIVE__.push({type:e.type,trusted:e.isTrusted,target:id,time:e.timeStamp,wall:Date.now(),oldDomPresent:!!document.querySelector(${JSON.stringify(selector)}),metrics:window.__QA_READ_METRICS__(),explorer:document.querySelector('.pixel-world-exploration [role=status]')?.textContent})};document.addEventListener('keydown',record,true);document.addEventListener('click',record,true);document.addEventListener('pointerdown',record,true);window.__QA_PENDING_DISPOSE__=()=>{document.removeEventListener('keydown',record,true);document.removeEventListener('click',record,true);document.removeEventListener('pointerdown',record,true)};const r=b.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
    result.firstReleaseFrame = await cdp.frameBoundary(); result.firstReleaseAt = Date.now();
    gate.releaseFirst(); await timeout(gate.thirdReceived, 'held third query page and native input'); assert(!gate.inputError, gate.inputError);
    result.pagesAtInput = gate.pages; result.input = JSON.parse(await evaluate('JSON.stringify(window.__QA_PENDING_NATIVE__)'));
    assert(result.pagesAtInput.length === 3 && result.pagesAtInput.slice(0, 2).every(p => p.response.view.entities.length === 200 && !p.response.view.entities.some(e => e.entity_id === result.target)), 'first400 disclosed the pending target');
    assert(result.input.some(e => e.type === (inputMode==='mouse'?'pointerdown':'keydown') && e.trusted && e.target === result.target && e.oldDomPresent)
      && result.input.some(e => e.type === 'click' && e.trusted && e.target === result.target), 'native input missed the actual old presented target; no retry permitted');
    await evaluate(`(async()=>{const end=Date.now()+5000;while(Date.now()<end){if(document.querySelector('[data-world-selection-status=pending]'))return true;await new Promise(r=>setTimeout(r,25))}throw Error('no real pending feedback after trusted partial-scene input')})()`);
    result.pending = await state(); result.feedback = await evaluate(`(()=>{const e=document.querySelector('[data-world-selection-status=pending]'),r=e.getBoundingClientRect();return{text:e.textContent,rect:r.toJSON(),selected0500:document.querySelector(${JSON.stringify(selector)})?.dataset.selected,canvas:document.querySelector('#pixel-world-embedded-runtime-canvas').getBoundingClientRect().toJSON()}})()`);
    assert(result.feedback.selected0500 !== 'true', 'pending was presented as confirmed selection');
    await run(['screenshot', '--full', resolve(out, 'pending-third-page.png')]);
    if (mode === 'confirm') {
      gate.releaseThird(); gate.dispose(); unfreeze();
      await evaluate(`(async()=>{const end=Date.now()+10000;while(Date.now()<end){if(document.querySelector(${JSON.stringify(selector)})?.dataset.selected==='true'&&!document.querySelector('[data-world-selection-status=pending]'))return true;await new Promise(r=>setTimeout(r,25))}throw Error('same-cut later page did not confirm pending target')})()`);
      await waitProjection(1000); result.confirmed = await state();
    } else if (mode === 'replace') {
      const other = 'button[data-agent-id="qa-agent-0001"][data-renderer-target]';
      await run(['click', other]);
      await evaluate(`(async()=>{const end=Date.now()+5000;while(Date.now()<end){if(document.querySelector(${JSON.stringify(other)})?.dataset.selected==='true'&&!document.querySelector('[data-world-selection-status=pending]'))return true;await new Promise(r=>setTimeout(r,25))}throw Error('new explicit selection did not cancel pending')})()`);
      result.replacement = await state(); gate.releaseThird(); gate.dispose(); unfreeze(); await waitProjection(1000);
      await run(['wait', '300']); result.afterLatePage = await state();
      assert(result.afterLatePage.targets.some(t => t.id === 'qa-agent-0001' && t.selected === 'true') && !result.afterLatePage.targets.some(t => t.id === result.target && t.selected === 'true'), 'late page replaced the newer user selection');
    } else if (mode === 'scope') {
      fixture.revoke(); const reconnecting = reconnect(); unfreeze(); await reconnecting; await waitNegotiation(true);
      await evaluate(`(async()=>{const end=Date.now()+10000;while(Date.now()<end){if(!document.querySelector('[data-world-selection-status=pending]')&&document.querySelectorAll('.pixel-world-renderer-target').length===0)return true;await new Promise(r=>setTimeout(r,25))}throw Error('scope refusal did not cancel pending and clear targets')})()`);
      result.scopeRefused = await state(); gate.replayThirdOnCurrentSockets(); gate.dispose(); await run(['wait', '300']);
      result.afterLatePage = await state(); assert(result.afterLatePage.targets.length === 0, 'old pending response crossed refused scope');
      fixture.restoreWorldView(); await reconnect(); await waitNegotiation(true); await waitProjection(1000); result.recovered = await state();
    } else {
      fixture.remove(result.target); result.newCutResponse = gate.releaseThirdFresh(); gate.dispose();
      await evaluate(`(async()=>{const end=Date.now()+10000;while(Date.now()<end){if(document.querySelector('[data-world-selection-status=unavailable]'))return true;await new Promise(r=>setTimeout(r,25))}throw Error('new-cut cursor refusal did not terminate pending')})()`);
      result.unavailable = await state(); await run(['screenshot', '--full', resolve(out, 'pending-complete-absence-unavailable.png')]); unfreeze();
      const search = async text => { const d='.pixel-world-exploration-disclosure';if(!await evaluate(`document.querySelector('${d}').open`))await run(['click',d+' > summary']);await run(['fill','.pixel-world-exploration form input',text]);await evaluate(`document.querySelector('.pixel-world-exploration form input').focus({preventScroll:true});true`);await run(['press','Enter']); };
      await search('qa-agent'); await waitProjection(999); result.completeAbsence = await state();
      assert(!result.completeAbsence.targets.some(t => t.id === result.target), 'complete new-cut projection retained omitted Agent');
      result.absenceBoundary = 'complete projection omission, not authoritative world_deleted';
      fixture.restore(); await search('agent-'); await waitProjection(1000); result.recovered = await state();
    }
    const final = await state();
    assert(final.core.authReady === false && final.core.selectedId === null, 'pending inspection acquired business identity');
    if (mode !== 'confirm') assert(!final.targets.some(t => t.id === result.target && t.selected === 'true'), 'cancelled pending target became selected');
    result.status = 'passed_pending_' + mode + '_scope_only'; return result;
  } catch (error) { result.status = String(error).includes('native input missed the actual old presented target') ? 'unmeasured_native_window_missed' : 'failed'; result.error = String(error); result.lastState = await state().catch(() => null); result.native = await evaluate('JSON.stringify(window.__QA_PENDING_NATIVE__||[])').then(JSON.parse).catch(() => null); throw error; }
  finally { gate?.dispose(); unfreeze(); await evaluate('window.__QA_PENDING_DISPOSE__?.();true').catch(() => {}); cdp.close(); writeFileSync(resolve(out, 'pending-third-page-summary.json'), JSON.stringify(result, null, 2)); }
}
