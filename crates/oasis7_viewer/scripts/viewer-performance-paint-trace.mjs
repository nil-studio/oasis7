// Passive browser observations. No synthetic event, renderer mutation or forced paint.
function installBrowserTrace() {
  window.__VIEWER_INPUT_TRACE__?.dispose?.();
  const trace = { native: [], mutations: [], events: [], overflow: false,
    supported: PerformanceObserver.supportedEntryTypes?.includes('event') === true,
    visibility: document.visibilityState, timeOrigin: performance.timeOrigin };
  const append = (list, row) => { if (list.length >= 20000) trace.overflow = true; else list.push(row); };
  const input = event => {
    const button = event.target.closest?.('button[data-agent-id][data-renderer-target]');
    if (!button || (event.type === 'keydown' && event.key !== 'Enter')) return;
    append(trace.native, { type: event.type, target: button.dataset.agentId, startTime: event.timeStamp, trusted: event.isTrusted });
  };
  document.addEventListener('keydown', input, true);
  document.addEventListener('click', input, true);
  const mutations = new MutationObserver(records => {
    for (const record of records) {
      const target = record.target;
      if (target.matches?.('button[data-agent-id][data-renderer-target]')) {
        append(trace.mutations, { target: target.dataset.agentId, time: performance.now(), selected: target.dataset.selected === 'true' });
      }
    }
  });
  mutations.observe(document.body, { attributes: true, subtree: true, attributeFilter: ['data-selected'] });
  let observer;
  if (trace.supported) {
    observer = new PerformanceObserver(list => {
      for (const event of list.getEntries()) append(trace.events, {
        type: event.name, target: event.target?.dataset?.agentId ?? null,
        startTime: event.startTime, duration: event.duration,
        processingStart: event.processingStart, processingEnd: event.processingEnd,
        interactionId: event.interactionId,
      });
    });
    observer.observe({ type: 'event', buffered: false, durationThreshold: 16 });
  }
  trace.dispose = () => { observer?.disconnect(); mutations.disconnect(); document.removeEventListener('keydown', input, true); document.removeEventListener('click', input, true); };
  window.__VIEWER_INPUT_TRACE__ = trace;
  return { supported: trace.supported, timeOrigin: trace.timeOrigin, visibility: trace.visibility };
}
export const installPaintTraceScript = `(${installBrowserTrace.toString()})()`;

export function matchInputPaintTrace(samples, trace) {
  if (trace.overflow) throw Error('input trace overflow; no samples may be silently discarded');
  const rows = samples.map(sample => {
    const mutation = trace.mutations.find(row => row.target === sample.expectedId && row.time >= sample.startTime && row.time <= sample.endTime && row.selected === true);
    const entries = trace.events.filter(row => row.type === 'click' && row.target === sample.expectedId
      && Number.isFinite(sample.clickTime) && Math.abs(row.startTime - sample.clickTime) <= 1 && row.interactionId > 0);
    const event = entries.length === 1 ? entries[0] : null;
    const paintEnd = event ? event.startTime + event.duration : null;
    const stateAtPaint=event?trace.mutations.filter(row=>row.target===sample.expectedId&&row.time>=sample.startTime&&row.time<=paintEnd+4).at(-1)?.selected:null;
    const revokedBeforePaintUpper = mutation && event && trace.mutations.some(row => row.target === sample.expectedId && row.time >= mutation.time && row.time <= paintEnd + 4 && row.selected === false);
    const eligible = Boolean(!revokedBeforePaintUpper && stateAtPaint===true && sample.isTrusted && sample.clickTrusted && mutation && event && Number.isFinite(event.duration) && event.duration >= 16 && paintEnd >= sample.startTime && mutation.time <= paintEnd - 4);
    return { index: sample.index, target: sample.expectedId, startTime: sample.startTime,
      domAcceptedMs: mutation ? mutation.time - sample.startTime : null,
      legacyRafOpportunityMs: sample.durationMs,
      nextPaintEstimateMs: eligible ? paintEnd - sample.startTime : null,
      nextPaintIntervalMs: eligible ? [Math.max(0, paintEnd - sample.startTime - 4), paintEnd - sample.startTime + 4] : null,
      interactionId: event?.interactionId ?? null,
      paintStatus: eligible ? 'matched_feedback_before_next_paint' : !event ? 'unknown_no_unique_exposed_event' : 'unknown_feedback_not_observed_before_paint',
      stateAtPaint, event, mutation };
  });
  const distribution = key => {
    const values=rows.map(row=>row[key]).filter(value=>value!==null&&Number.isFinite(value)).sort((a,b)=>a-b);
    const percentile=q=>values.length?values[Math.max(0,Math.ceil(values.length*q)-1)]:null;
    return {observed:values.length,unknown:rows.length-values.length,p50:percentile(.5),p95:percentile(.95),max:values.at(-1)??null};
  };
  return { distributions:{domAcceptedMs:distribution('domAcceptedMs'),nextPaintEstimateMs:distribution('nextPaintEstimateMs'),legacyRafOpportunityMs:distribution('legacyRafOpportunityMs')}, definition: 'trusted Enter; selected DOM mutation; click EventTiming next rendering update estimate (8ms granularity, events below16ms unexposed); legacy extra two-rAF endpoint retained separately',
    supported: trace.supported, samples: rows.length, paintMatched: rows.filter(row => row.nextPaintEstimateMs !== null).length,
    unknownPaint: rows.filter(row => row.nextPaintEstimateMs === null).length,
    completeness: rows.every(row => row.nextPaintEstimateMs !== null) ? 'all_matched' : 'unknown_samples_retained',
    formalP6: false, rows };
}
