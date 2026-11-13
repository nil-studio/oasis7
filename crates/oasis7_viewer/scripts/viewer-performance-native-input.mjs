import { createOwnedCdpInput, enterKeyEvents } from './viewer-owned-cdp-input.mjs';
// Measure trusted browser input to visible selection feedback, not Node/CLI round trips.
// Two rAF callbacks provide a paint opportunity; this is not display-photon or Runtime latency.
export async function collectNativeSelectionSamples({ evaluate, run, count, minimumDurationMs = 0, betweenSamples = async () => {}, onSample = () => {}, selectionSource = "core", preferCenter = false }) {
  if (!["core", "renderer"].includes(selectionSource)) throw Error("unknown selection observation source");
  const samples = [];
  const more = () => samples.length < count || (samples.at(-1)?.endTime ?? 0) - (samples[0]?.startTime ?? 0) < minimumDurationMs;
  let nativeInput;
  try { for (let index = 0; more(); index += 1) {
    const target = await evaluate(`JSON.stringify((()=>{
      const selected=${JSON.stringify(selectionSource)}==='renderer'?document.querySelector('button[data-agent-id][data-renderer-target][data-selected=true]')?.dataset.agentId:window.__AW_TEST__.getState().selectedId;
      const candidates=[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>{
        const r=b.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
        return {id:b.dataset.agentId,x,y,visible:r.width>0&&r.height>0&&x>20&&x<innerWidth-20&&y>20&&y<innerHeight-20&&getComputedStyle(b).visibility!=='hidden'};
      }).filter(b=>b.visible&&b.id!==selected);
      if(${JSON.stringify(preferCenter)})candidates.sort((a,b)=>(a.x-innerWidth/2)**2+(a.y-innerHeight/2)**2-((b.x-innerWidth/2)**2+(b.y-innerHeight/2)**2));
      return candidates[0]||null;
    })())`);
    if (!target) throw Error('no visible unselected native Agent target; interaction is unmeasured');
    const selector = `button[data-agent-id=${JSON.stringify(target.id)}][data-renderer-target]`;
    await evaluate(`(()=>{
      const expected=${JSON.stringify(target.id)};
      window.__VIEWER_PERF_INPUT__={expectedId:expected,status:'armed'};
      const listener=e=>{
        const button=e.target.closest?.('button[data-agent-id][data-renderer-target]');
        if(e.key!=='Enter'||button?.dataset.agentId!==expected)return;
        document.removeEventListener('keydown',listener,true);
        if(button.dataset.selected==='true'){window.__VIEWER_PERF_INPUT__={status:'invalid_already_selected',expectedId:expected};return;}
        const start=e.timeStamp;
        const row=window.__VIEWER_PERF_INPUT__={expectedId:expected,eventType:e.type,isTrusted:e.isTrusted,startTime:start,timeOrigin:performance.timeOrigin,status:'pending'};
        if(${JSON.stringify(selectionSource)}==='renderer'){const observationStarted=performance.now();const m=window.__QA_READ_METRICS__?.();row.displayAtInput=m?{status:m.status,cut:m.source_cut,version:m.render_version,profile:m.display_profile,markers:m.projected_marker_count,bodies:m.agent_body_sprites,inView:m.in_view_agent_body_sprites,labels:m.laid_out_in_view_labels}:null;row.displayObservationMs=performance.now()-observationStarted;}
        const clickListener=event=>{if(event.target.closest?.('button[data-agent-id][data-renderer-target]')?.dataset.agentId===expected){row.clickTime=event.timeStamp;row.clickTrusted=event.isTrusted;document.removeEventListener('click',clickListener,true);}};document.addEventListener('click',clickListener,true);
        const deadline=performance.now()+5000;
        const check=()=>{
          const b=document.querySelector(${JSON.stringify(selector)});
          if(b?.dataset.selected==='true'){
            row.domFeedbackTime=performance.now();
            requestAnimationFrame(()=>requestAnimationFrame(end=>{
              const state=window.__AW_TEST__.getState();
              const current=document.querySelector(${JSON.stringify(selector)});
              const rendererId=current?.dataset.selected==='true'?current.dataset.agentId:null;
              Object.assign(row,{selectionSource:${JSON.stringify(selectionSource)},coreSelectedKind:state.selectedKind,coreSelectedId:state.selectedId,selectedKind:${JSON.stringify(selectionSource)}==='renderer'?(rendererId?'agent':null):state.selectedKind,selectedId:${JSON.stringify(selectionSource)}==='renderer'?rendererId:state.selectedId,endTime:end,durationMs:end-start,paintOpportunities:2,status:'complete'});
            }));return;
          }
          if(performance.now()>deadline){row.status='timeout';return;}
          requestAnimationFrame(check);
        }; requestAnimationFrame(check);
      }; document.addEventListener('keydown',listener,true);return true;
    })()`);
    let preparation;
    if (selectionSource === 'renderer') {
      nativeInput ||= await createOwnedCdpInput(run, evaluate);
      // Prepare the same target on the already owned CDP session immediately
      // before native input, avoiding the CLI focus-to-input scheduling gap.
      preparation = `(async()=>{const deadline=performance.now()+10000;while(performance.now()<deadline){const b=document.querySelector(${JSON.stringify(selector)}),m=window.__QA_READ_METRICS__?.();if(b&&m?.status==='observed'&&m.display_profile==='world_view_v1'&&m.projected_marker_count===1000&&m.agent_body_sprites===1000&&m.in_view_agent_body_sprites===1000&&m.laid_out_in_view_labels===200){const r=b.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;if(r.width>0&&r.height>0&&x>20&&x<innerWidth-20&&y>20&&y<innerHeight-20){b.focus({preventScroll:true});if(document.activeElement===b)return true}}await new Promise(resolve=>setTimeout(resolve,25));}throw Error('same native target did not become ready before input');})()`;
    } else await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus({preventScroll:true});true`);
    if (nativeInput) await nativeInput.prepareAndEnter(preparation);
    else await run(['press', 'Enter']);
    const sample = await evaluate(`(async()=>{const deadline=performance.now()+6000;while(performance.now()<deadline){const r=window.__VIEWER_PERF_INPUT__;if(r?.status==='complete'||r?.status==='timeout'||r?.status==='invalid_already_selected')return JSON.stringify(r);await new Promise(resolve=>setTimeout(resolve,10));}throw Error('native selection feedback timed out');})()`);
    if (sample.status !== 'complete' || !sample.isTrusted || sample.selectedKind !== 'agent' || sample.selectedId !== target.id || !Number.isFinite(sample.durationMs) || sample.durationMs < 0) {
      throw Error('invalid native selection measurement: '+JSON.stringify(sample));
    }
    if(samples.length && sample.timeOrigin!==samples[0].timeOrigin)throw Error('page identity changed during interaction window');
    samples.push({index, ...sample, nativeTransport: nativeInput ? 'owned_cdp' : 'cli', ...(nativeInput ? { nativeKeyEvents: enterKeyEvents() } : {})}); await onSample(samples);
    if(more())await betweenSamples({index,samples});
  } } catch (error) {
    const canAttach = error !== null && (typeof error === 'object' || typeof error === 'function');
    try {
      if (!canAttach) throw Error('non-object original error; diagnostic cannot be attached');
      const diagnosticProgram = selectionSource === 'renderer'
        ? `JSON.stringify((()=>{const r=window.__VIEWER_PERF_INPUT__,b=[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].find(b=>b.dataset.agentId===r?.expectedId);return{input:r,active:{tag:document.activeElement?.tagName,id:document.activeElement?.dataset?.agentId??null},expectedTargetAttached:!!b,transport:"renderer"==='renderer'?'owned_cdp':'cli'}})())`
        : `JSON.stringify((()=>{const r=window.__VIEWER_PERF_INPUT__,b=[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].find(b=>b.dataset.agentId===r?.expectedId);return{input:r,active:{tag:document.activeElement?.tagName,id:document.activeElement?.dataset?.agentId??null},expectedTargetAttached:!!b,transport:"core"==='renderer'?'owned_cdp':'cli'}})())`;
      error.nativeInputDiagnostic = await evaluate(diagnosticProgram);
      if (typeof error.nativeInputDiagnostic === 'string') error.nativeInputDiagnostic = JSON.parse(error.nativeInputDiagnostic);
    } catch (diagnosticError) { if (canAttach) error.nativeInputDiagnosticError = String(diagnosticError); }
    throw error;
  } finally { nativeInput?.close(); }
  return samples;
}
