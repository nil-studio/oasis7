// Local observer/debug measurements, never a signed-service or formal A/B result.
import { appendFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { collectNativeSelectionSamples } from './viewer-performance-native-input.mjs';
import { installPaintTraceScript, matchInputPaintTrace } from './viewer-performance-paint-trace.mjs';

const assert = (value, message) => { if (!value) throw Error(message); };
const distribution = values => {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const at = q => sorted.length ? sorted[Math.ceil(sorted.length * q) - 1] : null;
  return { count: sorted.length, p50: at(.5), p95: at(.95), p99: at(.99), max: sorted.at(-1) ?? null };
};
const densityExpression = `(()=>{const s=window.__AW_TEST__.getState();return {time:performance.now(),origin:performance.timeOrigin,visibility:document.visibilityState,metrics:window.__QA_READ_METRICS__(),targets:document.querySelectorAll('.pixel-world-renderer-target').length,camera:s.pixelWorldCamera,authReady:s.authReady,playerId:s.authPlayerId,publicKey:s.authPublicKey,boundAgent:s.authBoundAgentId,registration:s.authRegistrationStatus}})()`;
function assertDensity(row) {
  assert(row.visibility === 'visible' && row.metrics.status === 'observed'
    && row.metrics.display_profile === 'world_view_v1' && row.targets === 1000
    && row.metrics.projected_marker_count === 1000 && row.metrics.agent_body_sprites === 1000
    && row.metrics.in_view_agent_body_sprites === 1000 && row.metrics.laid_out_in_view_labels === 200,
  'actual 1000 in-view Sprite / 200 laid-out labels prerequisite failed: ' + JSON.stringify(row));
  assert(row.authReady === false && row.playerId === null && row.publicKey === null && row.boundAgent === null
    && row.registration === 'guest', 'observer acquired a player identity');
}

export async function measureDensityPerformance({ evaluate, run, waitProjection, fixture, wire, out, durationMs, overview }) {
  assert(Number.isFinite(durationMs) && durationMs > 0, 'declare positive performance duration');
  const json = async code => { const value = await evaluate(code); return typeof value === 'string' ? JSON.parse(value) : value; };
  const evidence = { durationMs, formalP6: false, environment: { hardware: execFileSync('sysctl',['hw.model','hw.memsize'],{encoding:'utf8'}).trim(), os: execFileSync('sw_vers',[],{encoding:'utf8'}).trim(), power: execFileSync('pmset',['-g','batt'],{encoding:'utf8'}).trim(), network:'unshaped owned loopback', targetRefreshHz:'unknown' }, density: [], gestures: [], updates: [],
    limitations: ['M3/16GiB unshaped loopback observer/debug, not formal 8GiB A/B or normal hosted authorization.',
      'rAF intervals are browser opportunities, not world GPU submit/presentation intervals.',
      'Pagination may expose legitimate partial scenes; monitor retains these periods separately from dense input samples.',
      'No follow/stop, real WorldFeed burst, 50k query, cold/hot or 30-minute memory claim.'] };
  const dense = async () => {
    const cut = fixture.description().cut;
    const row = await json(`(async()=>{const end=Date.now()+10000;let row;while(Date.now()<end){row=${densityExpression};const m=row.metrics;if(m.status==='observed'&&m.source_cut===${JSON.stringify(cut)}&&row.targets===1000&&m.projected_marker_count===1000&&m.agent_body_sprites===1000&&m.in_view_agent_body_sprites===1000&&m.laid_out_in_view_labels===200)return JSON.stringify(row);await new Promise(r=>setTimeout(r,25));}throw Error('dense frame did not settle: '+JSON.stringify(row))})()`);
    assertDensity(row); evidence.density.push(row); return row;
  };
  const gesture = async index => {
    const sign = index % 4 < 2 ? 1 : -1, type = index % 2 ? 'zoom' : 'pan';
    let before = await dense(), wheel = before.camera.zoom < 1.85 ? -20 : 20;
    const proposed = before.camera.zoom * (wheel < 0 ? 1.12 : .89);
    if (type === 'zoom' && (proposed < 1.76 || proposed > 2.08)) {
      const previous = before; await overview(); before = await dense();
      evidence.gestures.push({ type: 'overview', before: previous, after: before, reason: 'native discrete wheel step would leave declared 1000/200 load interval' });
      wheel = -20;
    }
    const point = await json(`(()=>{const r=document.querySelector('#pixel-world-embedded-runtime-canvas').getBoundingClientRect(),c=document.querySelector('#pixel-world-embedded-runtime-canvas');const xs=[2,8,16,32,...Array.from({length:19},(_,i)=>r.width*(i+1)/20),r.width-32,r.width-16,r.width-8,r.width-2],ys=[2,8,16,32,...Array.from({length:19},(_,i)=>r.height*(i+1)/20),r.height-32,r.height-16,r.height-8,r.height-2];for(const y of ys)for(const x of xs){const px=Math.floor(r.left+x),py=Math.floor(r.top+y);if(px+${sign * 6}>r.left&&px+${sign * 6}<r.right&&document.elementFromPoint(px,py)===c&&document.elementFromPoint(px+${sign * 6},py)===c)return{x:px,y:py}}throw Error('no unobscured native camera start')})()`);
    await evaluate(`(()=>{window.__DENSITY_CAMERA_INPUT__=[];const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),types=['pointerdown','pointermove','pointerup','wheel'],handler=e=>{if(__DENSITY_CAMERA_INPUT__.length<64)__DENSITY_CAMERA_INPUT__.push({type:e.type,time:e.timeStamp,trusted:e.isTrusted,buttons:e.buttons,deltaY:e.deltaY})};for(const t of types)c.addEventListener(t,handler,true);window.__DENSITY_CAMERA_DISPOSE__=()=>{for(const t of types)c.removeEventListener(t,handler,true)};return true})()`);
    try {
      await run(['mouse', 'move', String(point.x), String(point.y)]);
      if (type === 'pan') { await run(['mouse', 'down']); await run(['mouse', 'move', String(point.x + sign * 6), String(point.y)]); await run(['mouse', 'up']); }
      else await run(['mouse', 'wheel', String(wheel)]);
      const after = await dense(), input = await json('JSON.stringify(window.__DENSITY_CAMERA_INPUT__)');
      assert(input.some(row => row.trusted && row.type === (type === 'pan' ? 'pointerdown' : 'wheel')), 'camera input was not trusted');
      assert(type === 'pan' ? before.camera.zoom === after.camera.zoom
        && (before.camera.pan_x_px !== after.camera.pan_x_px || before.camera.pan_y_px !== after.camera.pan_y_px)
        : before.camera.zoom !== after.camera.zoom, 'native camera did not change the intended physical parameter');
      evidence.gestures.push({ type, point, before, after, input });
    } finally { await evaluate('window.__DENSITY_CAMERA_DISPOSE__();true'); }
  };
  await dense();
  await evaluate(installPaintTraceScript);
  await evaluate(`(()=>{const t=window.__DENSITY_MONITOR__={started:performance.now(),raf:[],density:[],longTasks:[],overflow:false};let last;const frame=now=>{if(last!==undefined){if(t.raf.length<40000)t.raf.push({time:now,interval:now-last});else t.overflow=true}last=now;t.rafId=requestAnimationFrame(frame)};t.rafId=requestAnimationFrame(frame);t.timer=setInterval(()=>{const m=window.__QA_READ_METRICS__();if(t.density.length<10000)t.density.push({time:performance.now(),visibility:document.visibilityState,status:m.status,version:m.render_version,cut:m.source_cut,markers:m.projected_marker_count,bodies:m.agent_body_sprites,inView:m.in_view_agent_body_sprites,labels:m.laid_out_in_view_labels});else t.overflow=true},250);if(PerformanceObserver.supportedEntryTypes.includes('longtask')){t.observer=new PerformanceObserver(list=>{for(const e of list.getEntries())t.longTasks.push({start:e.startTime,duration:e.duration})});t.observer.observe({type:'longtask',buffered:false})}t.dispose=()=>{cancelAnimationFrame(t.rafId);clearInterval(t.timer);t.observer?.disconnect()};return true})()`);
  let samples = [];
  try {
    samples = await collectNativeSelectionSamples({ evaluate: json, run, count: 20, minimumDurationMs: durationMs,
      selectionSource: 'renderer', preferCenter: true,
      onSample: async rows => {
        samples = rows;
        const sample = rows.at(-1); assert(sample.coreSelectedId === null && sample.coreSelectedKind === null, 'spatial inspection became a business selection');
        appendFileSync(resolve(out, 'performance-native-input.jsonl'), JSON.stringify(sample) + '\n');
        await dense();
      },
      betweenSamples: async ({ index }) => {
        if (index % 2 === 1) await gesture(Math.floor(index / 2));
        if (index % 10 === 9) {
          const start = await json('performance.now()'); fixture.advanceResources(); const cut = fixture.description().cut;
          const after = await dense();
          const pages = wire.filter(row => row.direction === 'out' && row.message.type === 'world_view'
            && row.message.view.source_cut === cut && row.message.view.delivery === 'baseline');
          assert(pages.some(row => row.message.view.completeness === 'complete'), 'updated source did not complete a real baseline');
          const actual = pages.flatMap(row => row.message.view.entities).find(entity => entity.entity_id === 'qa-agent-0999');
          assert(actual?.render_record.resources.amounts.electricity === String(100 + fixture.description().resourceRevision % 2), 'updated resource was not sent over actual WS');
          evidence.updates.push({ start, cut, after, pages: pages.map(row => ({ connection: row.connection, generation: row.message.view.request_generation, entities: row.message.view.entities.length, bytes: row.bytes, completeness: row.message.view.completeness })), actualResource: actual.render_record.resources });
        }
        await dense();
      } });
    await evaluate('new Promise(resolve=>setTimeout(()=>resolve(true),2000))');
  } catch (error) { evidence.error = String(error); throw error; }
  finally {
    const monitor = await json('(()=>{__DENSITY_MONITOR__.dispose();return JSON.stringify(__DENSITY_MONITOR__)})()');
    const trace = await json('JSON.stringify(window.__VIEWER_INPUT_TRACE__)');
    await evaluate('window.__VIEWER_INPUT_TRACE__.dispose();true');
    evidence.monitorOverflow = monitor.overflow;
    evidence.actualInputSpanMs = samples.length ? samples.at(-1).endTime - samples[0].startTime : null;
    evidence.input = matchInputPaintTrace(samples, trace);
    evidence.inputDensity = {total:samples.length,observedDense:samples.filter(s=>s.displayAtInput?.status==='observed'&&s.displayAtInput.profile==='world_view_v1'&&s.displayAtInput.markers===1000&&s.displayAtInput.bodies===1000&&s.displayAtInput.inView===1000&&s.displayAtInput.labels===200).length,pendingOrUnknown:samples.filter(s=>s.displayAtInput?.status!=='observed').length};
    evidence.inputDensity.observedBelowDensity=evidence.inputDensity.total-evidence.inputDensity.observedDense-evidence.inputDensity.pendingOrUnknown;
    evidence.rafOpportunities = distribution(monitor.raf.map(row => row.interval));
    evidence.densityMonitor = { total: monitor.density.length, dense: monitor.density.filter(row => row.status === 'observed' && row.inView === 1000 && row.labels === 200).length,
      pending: monitor.density.filter(row => row.status !== 'observed').length, observedBelowDensity: monitor.density.filter(row => row.status === 'observed' && (row.inView !== 1000 || row.labels !== 200)).length };
    evidence.localFeedback100ms = { dom: evidence.input.distributions.domAcceptedMs.unknown || evidence.input.distributions.domAcceptedMs.p95 === null ? 'unknown' : evidence.input.distributions.domAcceptedMs.p95 <= 100 ? 'pass' : 'fail',
      paint: evidence.input.unknownPaint ? 'unknown' : evidence.input.distributions.nextPaintEstimateMs.p95 === null ? 'unknown' : evidence.input.distributions.nextPaintEstimateMs.p95 <= 100 ? 'pass' : 'fail' };
    writeFileSync(resolve(out, 'performance-input-paint-trace.json'), JSON.stringify({ trace, matched: evidence.input }, null, 2));
    writeFileSync(resolve(out, 'performance-monitor.json'), JSON.stringify(monitor, null, 2));
    writeFileSync(resolve(out, 'performance-window.json'), JSON.stringify(evidence, null, 2));
  }
  assert(!evidence.monitorOverflow, 'monitor overflow');
  assert(evidence.actualInputSpanMs >= durationMs && evidence.gestures.some(row => row.type === 'pan') && evidence.gestures.some(row => row.type === 'zoom') && evidence.updates.length > 0, 'requested actual input window was incomplete');
  return evidence;
}
