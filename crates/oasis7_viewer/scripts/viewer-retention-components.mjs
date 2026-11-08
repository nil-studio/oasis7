// Component comparisons never equate encoded JSON, heap, linear memory and RSS.
export function compareRetentionComponent(initial, final) {
  const limit = 0.10;
  if (!Number.isFinite(initial) || !Number.isFinite(final) || initial < 0 || final < 0) return { status: 'unknown', reason: 'component_unavailable' };
  if (initial === 0) return final === 0 ? { status: 'within_budget', growth: 0 } : { status: 'unknown', reason: 'zero_initial_component', initial, final };
  const growth = (final - initial) / initial;
  return { status: growth <= limit ? 'within_budget' : 'over_budget', initial, final, growth, limit };
}
export function numericRetentionRecord(input) {
  const allowed = new Set(['timeMs','memoryPhaseMatched','memoryReadMs','afterGeneration','afterScopeSequence','afterInstanceSequence','afterTimeOrigin','afterLoading','afterQueryCurrent','afterObserved','afterEntities','afterBodies','timeOrigin','instanceSequence','cameraZoom','cameraPanX','cameraPanY','viewportWidth','viewportHeight','phase','scopeSequence','generation','viewJsonBytes','viewObjects','viewArrays','pagingJsonBytes','pagingObjects','pagingArrays','pagingActive','pagingEntities','pagingRelations','pagingAggregates','pagingEvents','pagingCursors','observationMs','jsUsedHeapBytes','jsTotalHeapBytes','domNodes','documents','listeners','wasmLinearBytes','rendererBytes','gpuBytes','ownedRssBytes']);
  for (const [key, value] of Object.entries(input)) {
    if (!allowed.has(key)) throw Error('unrecognized retention field');
    if (key === 'phase') { if (!['warmup','home','away','filtered','restored'].includes(value)) throw Error('unknown retention phase'); }
    else if (['cameraPanX','cameraPanY'].includes(key)) { if (!Number.isFinite(value)) throw Error('camera pan must be finite numeric'); }
    else if (['pagingActive','memoryPhaseMatched','afterLoading','afterQueryCurrent','afterObserved'].includes(key)) { if (typeof value !== 'boolean') throw Error('paging active must be boolean'); }
    else if (value !== null && (typeof value !== 'number' || !Number.isFinite(value) || value < 0)) throw Error('retention field must be nonnegative numeric or unknown');
  }
  return { ...input };
}

export function summarizeRetentionWindow(first,last,{actualSpanMs,samples}) {
  const eligible=actualSpanMs>=1800000&&samples>=12&&first.length===6&&last.length===6&&last[0].timeMs>first.at(-1).timeMs;
  const median=(rows,key)=>{if(!['viewJsonBytes','viewObjects'].includes(key)&&rows.some(row=>row.memoryPhaseMatched!==true))return null;const values=rows.map(x=>x[key]).filter(Number.isFinite).sort((a,b)=>a-b);return values.length===rows.length&&values.length?values[Math.floor(values.length/2)]:null;};
  const components={};
  for(const key of ['jsUsedHeapBytes','wasmLinearBytes','rendererBytes','gpuBytes','ownedRssBytes'])components[key]=eligible?compareRetentionComponent(median(first,key),median(last,key)):{status:'unknown',reason:'short_or_overlapping_method_probe',initial:median(first,key),final:median(last,key)};
  const diagnosticTrends={};
  for(const key of ['viewJsonBytes','viewObjects','jsTotalHeapBytes','domNodes','listeners']){const initial=median(first,key),final=median(last,key);diagnosticTrends[key]={initial,final,growth:initial>0&&final!==null?(final-initial)/initial:null,interpretation:'diagnostic trend only; not a memory budget or total cache cap'};}
  return {eligible,components,diagnosticTrends,failedComponents:Object.entries(components).filter(([,value])=>value.status==='over_budget').map(([key])=>key)};
}

export function classifyRetentionPhase(before,after) {
  const valid=row=>row&&row.identityValid===true&&row.sourceMatches===true&&Number.isSafeInteger(row.instance)&&Number.isSafeInteger(row.scope)&&Number.isSafeInteger(row.generation)&&Number.isFinite(row.origin)&&row.camera&&Number.isFinite(row.camera.zoom);
  if(!valid(before)||before.loading||!before.currentQuery||!before.complete||!before.observed||!valid(after)||before.instance!==after.instance||before.scope!==after.scope||before.origin!==after.origin||JSON.stringify(before.camera)!==JSON.stringify(after.camera)||after.stale||after.error)return 'precondition_lost';
  if(after.generation!==before.generation||after.loading||!after.currentQuery||!after.observed)return 'poll_crossed';
  if(!after.complete||after.entities!==before.entities||after.bodies!==before.bodies||after.labels!==before.labels)return 'precondition_lost';
  return 'matched';
}
