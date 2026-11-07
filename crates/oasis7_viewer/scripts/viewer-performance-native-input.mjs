// Measure trusted browser input to visible selection feedback, not Node/CLI round trips.
// Two rAF callbacks provide a paint opportunity; this is not display-photon or Runtime latency.
export async function collectNativeSelectionSamples({ evaluate, run, count, minimumDurationMs = 0, betweenSamples = async () => {}, onSample = () => {} }) {
  const samples = [];
  const more = () => samples.length < count || (samples.at(-1)?.endTime ?? 0) - (samples[0]?.startTime ?? 0) < minimumDurationMs;
  for (let index = 0; more(); index += 1) {
    const target = await evaluate(`JSON.stringify((()=>{
      const selected=window.__AW_TEST__.getState().selectedId;
      return [...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>{
        const r=b.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
        return {id:b.dataset.agentId,x,y,visible:r.width>0&&r.height>0&&x>20&&x<innerWidth-20&&y>20&&y<innerHeight-20&&getComputedStyle(b).visibility!=='hidden'};
      }).find(b=>b.visible&&b.id!==selected)||null;
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
        const clickListener=event=>{if(event.target.closest?.('button[data-agent-id][data-renderer-target]')?.dataset.agentId===expected){row.clickTime=event.timeStamp;row.clickTrusted=event.isTrusted;document.removeEventListener('click',clickListener,true);}};document.addEventListener('click',clickListener,true);
        const deadline=performance.now()+5000;
        const check=()=>{
          const b=document.querySelector(${JSON.stringify(selector)});
          if(b?.dataset.selected==='true'){
            row.domFeedbackTime=performance.now();
            requestAnimationFrame(()=>requestAnimationFrame(end=>{
              const state=window.__AW_TEST__.getState();
              Object.assign(row,{selectedKind:state.selectedKind,selectedId:state.selectedId,endTime:end,durationMs:end-start,paintOpportunities:2,status:'complete'});
            }));return;
          }
          if(performance.now()>deadline){row.status='timeout';return;}
          requestAnimationFrame(check);
        }; requestAnimationFrame(check);
      }; document.addEventListener('keydown',listener,true);return true;
    })()`);
    await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus({preventScroll:true});true`);
    await run(['press', 'Enter']);
    const sample = await evaluate(`(async()=>{const deadline=performance.now()+6000;while(performance.now()<deadline){const r=window.__VIEWER_PERF_INPUT__;if(r?.status==='complete'||r?.status==='timeout'||r?.status==='invalid_already_selected')return JSON.stringify(r);await new Promise(resolve=>setTimeout(resolve,10));}throw Error('native selection feedback timed out');})()`);
    if (sample.status !== 'complete' || !sample.isTrusted || sample.selectedKind !== 'agent' || sample.selectedId !== target.id || !Number.isFinite(sample.durationMs) || sample.durationMs < 0) {
      throw Error('invalid native selection measurement: '+JSON.stringify(sample));
    }
    if(samples.length && sample.timeOrigin!==samples[0].timeOrigin)throw Error('page identity changed during interaction window');
    samples.push({index, ...sample}); await onSample(samples);
    if(more())await betweenSamples({index,samples});
  }
  return samples;
}
