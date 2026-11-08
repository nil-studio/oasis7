// Passive browser observations. No synthetic event, renderer mutation or forced paint.
function installBrowserTrace() {
  window.__VIEWER_INPUT_TRACE__?.dispose?.();
  const trace = { native: [], mutations: [], events: [], overflow: false,
    supported: PerformanceObserver.supportedEntryTypes?.includes('event') === true,
    visibility: document.visibilityState, timeOrigin: performance.timeOrigin };
  const traceLimit = 20000;
  const append = (list, row) => { if (list.length >= traceLimit) trace.overflow = true; else list.push(row); };
  const input = event => {
    const button = event.target.closest?.('button[data-agent-id][data-renderer-target]');
    if (!button || (event.type === 'keydown' && event.key !== 'Enter')) return;
    append(trace.native, { type: event.type, target: button.dataset.agentId, startTime: event.timeStamp, trusted: event.isTrusted });
  };
  document.addEventListener('keydown', input, true);
  document.addEventListener('click', input, true);
  // Keep a sparse stream of final connected selection-state changes. Counts
  // retain coalesced/irrelevant DOM records; no history is inferred between callbacks.
  const selector = 'button[data-agent-id][data-renderer-target]';
  const nodesById = new Map(), identities = new WeakMap(), selectedStates = new Map();
  let connectedTargets = 0;
  trace.domObserver = { records: 0, callbacks: 0, maxConnectedTargets: 0, maxIndexedTargets: 0, workMs: 0, maxWorkMs: 0 };
  const reconcileNode = node => {
    const previous = identities.get(node);
    const current = node.isConnected && node.matches?.(selector) ? node.dataset.agentId : null;
    if (previous !== undefined && previous !== current) {
      const nodes = nodesById.get(previous); nodes.delete(node); connectedTargets--;
      if (!nodes.size) nodesById.delete(previous);
      identities.delete(node);
    }
    if (current !== null && previous !== current) {
      if (connectedTargets >= traceLimit) trace.overflow = true;
      else {
        if (!nodesById.has(current)) nodesById.set(current, new Set());
        nodesById.get(current).add(node); identities.set(node, current); connectedTargets++;
      }
    }
    trace.domObserver.maxIndexedTargets = Math.max(trace.domObserver.maxIndexedTargets, connectedTargets);
    return [previous, current].filter(id => id !== undefined && id !== null);
  };
  const isSelected = id => [...(nodesById.get(id) || [])].some(node => node.isConnected && node.dataset.selected === 'true');
  for (const node of document.querySelectorAll(selector)) reconcileNode(node);
  trace.domObserver.maxConnectedTargets = connectedTargets;
  for (const id of nodesById.keys()) selectedStates.set(id, isSelected(id));
  const mutations = new MutationObserver(records => {
    const started = performance.now(), candidates = new Map(), affected = new Map();
    trace.domObserver.callbacks++; trace.domObserver.records += records.length;
    const remember = (node, cause) => {
      if (node.nodeType !== 1) return;
      if (!candidates.has(node)) {
        if (candidates.size >= traceLimit) { trace.overflow = true; return; }
        candidates.set(node, new Set());
      }
      candidates.get(node).add(cause);
    };
    const subtree = (node, cause) => {
      if (node.nodeType !== 1) return;
      if (node.matches('button')) remember(node, cause);
      for (const button of node.querySelectorAll('button')) remember(button, cause);
    };
    for (const record of records) {
      if (record.type === 'attributes') remember(record.target, 'attribute');
      else {
        for (const node of record.addedNodes) subtree(node, 'insert');
        for (const node of record.removedNodes) subtree(node, 'remove');
      }
    }
    for (const [node, causes] of candidates) for (const id of reconcileNode(node)) {
      if (!affected.has(id)) affected.set(id, new Set());
      for (const cause of causes) affected.get(id).add(cause);
    }
    trace.domObserver.maxConnectedTargets = Math.max(trace.domObserver.maxConnectedTargets, connectedTargets);
    for (const [id, causes] of affected) {
      const selected = isSelected(id), present = nodesById.has(id);
      if (selected !== (selectedStates.get(id) ?? false)) append(trace.mutations, {
        target: id, time: performance.now(), selected, present, causes: [...causes],
      });
      if (present) selectedStates.set(id, selected); else selectedStates.delete(id);
    }
    const elapsed = performance.now() - started;
    trace.domObserver.workMs += elapsed; trace.domObserver.maxWorkMs = Math.max(trace.domObserver.maxWorkMs, elapsed);
  });
  mutations.observe(document.body, { attributes: true, subtree: true, childList: true,
    attributeFilter: ['data-selected', 'data-agent-id', 'data-renderer-target'] });
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
  trace.dispose = () => { observer?.disconnect(); mutations.disconnect(); nodesById.clear(); selectedStates.clear(); document.removeEventListener('keydown', input, true); document.removeEventListener('click', input, true); };
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
