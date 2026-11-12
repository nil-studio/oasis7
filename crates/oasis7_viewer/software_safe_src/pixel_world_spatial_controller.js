import { createSignal, onCleanup, untrack } from 'solid-js';
import { createWorldViewClient, worldViewPosition } from './pixel_world_world_view.js';
import { pixelWorldScope } from './pixel_world_scope.js';
import { worldViewAuthorityKey } from './pixel_world_world_view_delta.js';

// Page-local only: a remounted controller must not reuse an earlier intent ID.
let selectionIntentGeneration = 0;
let navigationIntentGeneration = 0;
const nextNavigationId = () => String(++navigationIntentGeneration);
const refKey = ref => `${ref.kind}:${ref.id}`;
function viewportBounds(value) {
  if (!value || !['min_x_cm','max_x_cm','min_y_cm','max_y_cm'].every(key => typeof value[key] === 'string' && /^-?(0|[1-9]\d*)$/.test(value[key]))) return null;
  if (BigInt(value.min_x_cm) > BigInt(value.max_x_cm) || BigInt(value.min_y_cm) > BigInt(value.max_y_cm)) return null;
  return Object.fromEntries(['min_x_cm','max_x_cm','min_y_cm','max_y_cm'].map(key => [key,value[key]]));
}

// Owns spatial read state and local navigation, never business operation identity.
export function createPixelWorldSpatialController({core, enabled, getRenderState, getLocale, debounceMs = 180}) {
  const [state,setState] = createSignal({view:null,loading:false,stale:false,error:null});
  const [navigation,setNavigation] = createSignal(null);
  const [pauseVersion,setPauseVersion] = createSignal(0);
  const [spatialSelection,setSpatialSelection] = createSignal(null);
  const [selectionStatus,setSelectionStatus] = createSignal(null);
  const [selectionFeedback,setSelectionFeedback] = createSignal(null);
  const [overviewStatus,setOverviewStatus] = createSignal(null);
  const [queryStatus,setQueryStatus] = createSignal(null);
  let explicitQuery = null;
  function finishQuery(intent, value = null, status = null) {
    if (!intent) return;
    clearTimeout(intent.timer);intent.resolve(value);
    if (explicitQuery === intent) {explicitQuery=null;setQueryStatus(status);}
  }
  function cancelQuery() {
    if (!explicitQuery) return;
    client.invalidate();finishQuery(explicitQuery);
  }
  let feedbackScopeGeneration = 1, feedbackAuthority = null;
  const client = createWorldViewClient({request:core.requestWorldView,onChange:next=>{setState(next);checkPending(next);checkOverview(next);}});
  let scope = pixelWorldScope(core.state), lastSnapshot = null, lastPins = '', currentQuery = {};
  let cameraBounds = null, cameraKey = '', timer = null, command = 0, disposed = false;
  let following = null, selectedCoreKey = null, inFlight = null, dirty = false;
  let presented = null, pending = null, pendingTimer = null;
  let pendingOverview = null, overviewTimer = null, queryToken = 0, readingQueryToken = -1, readStartViews = new WeakSet();
  function cancelOverview(status = null, stop = false) {
    const retire = stop && (pendingOverview || navigation()?.overview_query);
    clearTimeout(overviewTimer);overviewTimer = null;pendingOverview = null;setOverviewStatus(status);
    if (retire) {command += 1;setNavigation({request_id:nextNavigationId(),mode:'stop',target:null});}
    queueMicrotask(()=>{if(dirty&&!inFlight&&!pendingOverview?.applying&&!disposed&&timer===null){dirty=false;void read();}});
  }
  function overviewBinding(view) {
    if (![view?.world_id,view?.scope_id,view?.source_cut,view?.request_generation].every(v=>typeof v==='string'&&v.length>0)) return null;
    return [JSON.parse(worldViewAuthorityKey(view)),view.source_cut,view.request_generation];
  }
  function checkOverview(next) {
    if (!pendingOverview) return;
    const intent = pendingOverview, view = next.view;
    if (disposed || pixelWorldScope(core.state)!==scope || intent.scope!==scope || intent.command!==command
      || intent.queryToken!==queryToken || intent.coreKey!==coreKey() || !enabled() || next.error || next.stale) {
      cancelOverview('unavailable',true);return;
    }
    // A queued read or the retained old complete view is not a page of this read.
    if (readingQueryToken!==intent.queryToken || (next.loading && (view==null || readStartViews.has(view)))) {
      if (!next.loading) cancelOverview('unavailable',true);
      return;
    }
    const binding = overviewBinding(view);
    if (!binding || view.source_frozen || !['partial','complete'].includes(view.completeness)) {
      cancelOverview('unavailable',true);return;
    }
    const key = JSON.stringify(binding);
    if (intent.binding && intent.binding!==key) {cancelOverview('unavailable',true);return;}
    intent.binding = key;
    if (intent.applying) return;
    if (view.completeness==='complete') {
      const request_id = intent.requestId;
      intent.applying = true;
      setOverviewStatus('applying');
      setNavigation({request_id,mode:'overview',target:null,overview_query:{version:1,binding}});
    } else if (!next.loading || view.budget_limited) cancelOverview('unavailable',true);
  }
  const requestBinding = view => JSON.stringify([worldViewAuthorityKey(view),view?.source_cut,view?.request_generation]);
  const canonicalRef = ref => ref?.kind === 'module_visual' ? {kind:'factory',id:ref.id} : ref;
  function proved(ref, view) {
    ref = canonicalRef(ref);
    if (ref?.kind === 'world_event') return view?.events?.some(v=>v.event_id === ref.id) ? ref : null;
    if (ref?.kind === 'industry_aggregate') return view?.aggregates?.some(v=>v.aggregate_id === ref.id) ? ref : null;
    return view?.entities?.some(v=>v.kind === ref?.kind && v.entity_id === ref.id) ? ref : null;
  }
  const deleted = (next, ref) => next.changes?.some(change=>change.world_deleted
    && change.id === ref.id && change.kind === ref.kind);
  function cancelPending(status = null) {
    clearTimeout(pendingTimer);pendingTimer = null;pending = null;setSelectionStatus(status);
    setSelectionFeedback(previous => status && previous ? Object.freeze({...previous,phase:status}) : null);
  }
  function beginFeedback(ref, phase) {
    setSelectionFeedback(Object.freeze({intent_generation:++selectionIntentGeneration,
      ref:Object.freeze({kind:ref.kind,id:ref.id}),world_id:core.state.worldId ?? null,
      scope_generation:feedbackScopeGeneration,phase}));
  }
  function checkPending(next) {
    const authority = next.view ? worldViewAuthorityKey(next.view) : null;
    if (authority !== feedbackAuthority) {
      if (feedbackAuthority !== null) feedbackScopeGeneration += 1;
      feedbackAuthority = authority;setSelectionFeedback(null);
    }
    if (!pending) {
      if (selectionFeedback()?.phase === 'selected' && (disposed || pixelWorldScope(core.state) !== scope
        || selectedCoreKey !== coreKey() || next.error || next.stale || (next.view && !proved(selectionFeedback().ref,next.view)))) setSelectionFeedback(null);
      return;
    }
    const view = next.view;
    if (disposed || pixelWorldScope(core.state) !== scope || pending.scope !== scope || pending.command !== command
      || pending.coreKey !== coreKey() || !enabled() || next.error || next.stale || !view || requestBinding(view) !== pending.binding
      || deleted(next,pending.ref)) {
      cancelPending('unavailable');return;
    }
    const selected = proved(pending.ref,view);
    if (selected) {
      const feedback = selectionFeedback();
      cancelPending();selectedCoreKey = coreKey();setSpatialSelection(selected);
      if (feedback) setSelectionFeedback(Object.freeze({...feedback,phase:'selected'}));
    } else if (!next.loading || view.completeness === 'complete') cancelPending('unavailable');
  }
  const coreKey = () => `${core.state.selectedKind}:${core.state.selectedId}`;
  client.reset(scope,core.state.worldId);
  const mainTarget = () => core.buildGameplaySummary(getLocale())?.acceptedIntentTarget
    || getRenderState()?.commercial_surface?.next_action?.target_agent_id || null;
  const selection = () => {
    if (untrack(() => pixelWorldScope(core.state)) !== scope) return null;
    const selected = spatialSelection();
    if (selected?.kind === 'world_event') return state().view?.events?.some(value=>value.event_id === selected.id) ? selected : null;
    if (selected?.kind === 'industry_aggregate') return state().view?.aggregates?.some(value=>value.aggregate_id === selected.id) ? selected : null;
    if (selected) return state().view?.entities?.some(entity => entity.kind === selected.kind && entity.entity_id === selected.id) ? selected : null;
    return untrack(() => core.state.selectedKind && core.state.selectedId
      ? {kind:core.state.selectedKind,id:core.state.selectedId} : null);
  };
  function pins() {
    const result = [], seen = new Set();
    for (const ref of [selection(), mainTarget() ? {kind:'agent',id:mainTarget()} : null, following]) {
      if (!ref || !['agent','location','factory'].includes(ref.kind) || !ref.id || seen.has(refKey(ref))) continue;
      seen.add(refKey(ref)); result.push({kind:ref.kind,id:ref.id});
    }
    return result.slice(0,3);
  }
  function cancelScheduled() { if (timer != null) clearTimeout(timer); timer = null; }
  function navigate(mode,target = null,preserveQuery = false) {
    if (!preserveQuery) cancelQuery();
    cancelPending();
    cancelOverview();
    following = mode === 'follow' ? target : null;
    command += 1;setNavigation({request_id:nextNavigationId(),mode,target});
  }
  async function read() {
    if (inFlight) { dirty = true; return inFlight; }
    if (pendingOverview?.applying) {dirty=true;return null;}
    if (disposed || !enabled() || pixelWorldScope(core.state) !== scope) return null;
    const pinned = pins(); lastPins = JSON.stringify(pinned);
    const intent=explicitQuery;
    if (intent) {setQueryStatus('reading');currentQuery={...intent.query};}
    const query = {...(intent?.query || currentQuery)};delete query.pinned_entities;delete query.resume_cursor;delete query.cursor;
    if (core.state.viewerProtocol?.capabilities?.includes('world_view_pins_v1')) query.pinned_entities = pinned;
    const caps = core.state.viewerProtocol?.capabilities || [];
    if (caps.includes('world_view_aggregate_v1')) query.detail_level ||= 'auto'; else delete query.detail_level;
    if (caps.includes('world_view_event_anchor_v1')) query.include_events = true; else delete query.include_events;
    readingQueryToken = queryToken;readStartViews = new WeakSet();
    if (state().view) readStartViews.add(state().view);
    inFlight = client.query(query,{delta:caps.includes('world_view_delta_v1'),preserveChanges:true});
    let result=null;
    try { result=await inFlight;return result; } finally {
      inFlight = null;
      if (intent && explicitQuery===intent) finishQuery(intent,result,state().error?'unavailable':null);
      if ((explicitQuery || dirty) && !disposed) {dirty=false;void read();}
    }
  }
  function schedule() {
    if (explicitQuery) {dirty=true;return;}
    cancelPending();
    cancelScheduled(); client.invalidate();
    timer = setTimeout(() => {timer = null; void read();},debounceMs);
  }
  function query(query = {}) {
    if (disposed) return Promise.resolve(null);
    if (!enabled() || pixelWorldScope(core.state)!==scope) {cancelQuery();setQueryStatus('unavailable');return Promise.resolve(null);}
    cancelQuery();
    cancelPending();
    cancelOverview(null,true);queryToken += 1;
    command += 1; cancelScheduled();client.clearChanges();
    let resolve;const result=new Promise(done=>resolve=done);
    const intent={query:{...query},resolve,timer:null};explicitQuery=intent;setQueryStatus('pending');
    intent.timer=setTimeout(()=>{if(explicitQuery===intent){client.invalidate();finishQuery(intent,null,'unavailable');}},30000);
    if (inFlight) {client.invalidate();dirty=false;} else void read();
    return result;
  }
  function select(ref) {
    cancelQuery();
    cancelPending();
    cancelOverview(null,true);
    if (pixelWorldScope(core.state) !== scope) return null;
    command += 1;
    const view = state().view;
    if (view) {
      if (ref?.kind === 'industry_aggregate' && view.aggregates?.some(value=>value.aggregate_id === ref.id)
        || ref?.kind === 'world_event' && view.events?.some(value=>value.event_id === ref.id)) {
        selectedCoreKey=coreKey();setSpatialSelection(ref);beginFeedback(ref,'selected');return ref;
      }
      const entity = view.entities?.find(entity => entity.entity_id === ref?.id
        && (entity.kind === ref.kind || (entity.kind === 'factory' && ref.kind === 'module_visual')));
      if (!entity) return null;
      const selected = {kind:entity.kind,id:entity.entity_id};
      selectedCoreKey = coreKey();
      setSpatialSelection(selected);
      const model = core.state.snapshot?.model;
      const collection = entity.kind === 'agent' ? model?.agents : entity.kind === 'location' ? model?.locations : null;
      if (collection?.[entity.entity_id]) {
        selectedCoreKey = refKey(selected);
        core.applySelection(selected);
      }
      beginFeedback(selected,'selected');return selected;
    }
    setSpatialSelection(null);
    const applied = core.applySelection(ref);
    if (applied?.kind && typeof applied.id === 'string') {
      selectedCoreKey = coreKey();beginFeedback(canonicalRef(applied),'selected');
    }
    return applied;
  }
  const controller = {
    readStatistics(expectedSourceCut = null, measure = true) {
      if (disposed || pixelWorldScope(core.state) !== scope) return null;
      return client.statistics?.(expectedSourceCut, measure) ?? null;
    },
    state,navigation,pauseVersion,selection,selectionStatus,selectionFeedback,overviewStatus,queryStatus,scope:()=>scope,
    // Called by Host only after publishing this render DTO. Store identities,
    // never the previous scene's entity facts or coordinates.
    present(view, scene, presentedScope = scope) {
      presented = null;
      if (!view || !scene) {
        cancelPending();
        if (pendingOverview) cancelOverview('unavailable',true);
        return;
      }
      if (pixelWorldScope(core.state) !== scope || presentedScope !== scope
        || worldViewAuthorityKey(view) !== worldViewAuthorityKey(state().view)
        || !view.source_cut || scene.display_source_cut !== view.source_cut) return;
      const authorized = new Set([
        ...(view.entities || []).map(value=>`${value.kind}:${value.entity_id}`),
        ...(view.aggregates || []).map(value=>`industry_aggregate:${value.aggregate_id}`),
        ...(view.events || []).map(value=>`world_event:${value.event_id}`),
      ]);
      const refs = new Set();
      for (const [field,kind] of [['agents','agent'],['locations','location'],['module_visual_entities','factory'],['industry_aggregates','industry_aggregate'],['world_events','world_event']]) {
        for (const value of scene[field] || []) if (authorized.has(`${kind}:${value.id}`)) refs.add(`${kind}:${value.id}`);
      }
      if (refs.size <= 1000) presented = {scope,authority:worldViewAuthorityKey(view),cut:view.source_cut,request:view.request_generation,refs};
    },
    selectPresented(ref) {
      if (disposed) return null;
      cancelOverview(null,true);
      const next = state(), view = next.view;
      if (!view) {
        if (!enabled()) return select(ref); // Preserve the legacy snapshot path only.
        cancelPending('unavailable');return null;
      }
      if (!enabled()) {cancelPending();return null;}
      ref = canonicalRef(ref);
      if (!ref || typeof ref.id !== 'string' || !['agent','location','factory','industry_aggregate','world_event'].includes(ref.kind)) return null;
      if (proved(ref,view)) return select(ref);
      cancelPending();command += 1;
      if (disposed || pixelWorldScope(core.state) !== scope || !next.loading || next.error || next.stale
        || deleted(next,ref) || view.completeness !== 'partial' || !presented || presented.scope !== scope
        || presented.authority !== worldViewAuthorityKey(view) || presented.cut !== view.source_cut || !presented.refs.has(refKey(ref))) return null;
      pending = {ref:{kind:ref.kind,id:ref.id},scope,command,coreKey:coreKey(),binding:requestBinding(view)};
      setSelectionStatus('pending');beginFeedback(ref,'pending');
      pendingTimer = setTimeout(()=>cancelPending('unavailable'),30000);
      return null;
    },
    drilldown(aggregate) {
      cancelOverview(null,true);
      if (pixelWorldScope(core.state) !== scope || !state().view?.aggregates?.some(value=>value.aggregate_id === aggregate?.aggregate_id)) return;
      const bounds=viewportBounds(aggregate.drilldown?.bounds);if (!bounds) return;
      navigate('stop');command += 1;setNavigation({request_id:nextNavigationId(),mode:'fit_bounds',bounds,target:null});
      return query({bounds,detail_level:aggregate.drilldown.detail_level});
    },
    query,select,navigate,
    eventFeed(event) {
      if (pixelWorldScope(core.state) !== scope) return false;
      cancelPending();
      cancelOverview(null,true);
      const view=state().view, feed=core.state.worldFeed;
      const authorized=view?.events?.find(value=>value.event_id === event?.event_id);
      const identity=authorized?.feed_ref;
      const match=authorized && identity && identity.world_id === feed?.worldId
        && String(identity.reorg_epoch) === String(feed?.reorgEpoch)
        && feed.events?.some(value=>String(value.event_seq) === String(identity.event_seq));
      if (!match) {setState(previous=>({...previous,notice:'event_feed_unavailable'}));return false;}
      const row=[...document.querySelectorAll('[data-world-feed-event]')].find(value=>value.dataset.worldFeedEvent === String(identity.event_seq));
      if (!row) {setState(previous=>({...previous,notice:'event_feed_unavailable'}));return false;}
      const exploration=document.querySelector('.pixel-world-exploration-disclosure');if (exploration) exploration.open=false;
      const disclosure=row.closest('details');if (disclosure) disclosure.open=true;
      row.setAttribute('tabindex','-1');row.focus();row.scrollIntoView?.({block:'nearest'});return true;
    },
    locate(ref) {if (select(ref)) {client.clearChanges();navigate('locate',ref);}},
    follow(ref) {navigate(ref ? 'follow' : 'stop',ref);},
    pauseFollow() {
      client.clearChanges();
      if (currentQuery.detail_level) currentQuery = {...currentQuery,detail_level:'auto'};
      navigate('stop');setPauseVersion(value=>value+1);
    },
    overview() {
      if (!enabled()) {navigate('overview');return;}
      client.clearChanges();
      navigate('stop',null,true);
      pendingOverview = {scope,command,queryToken,coreKey:coreKey(),requestId:nextNavigationId(),binding:null};
      setOverviewStatus('pending');overviewTimer = setTimeout(()=>cancelOverview('unavailable',true),30000);
      checkOverview(state());
    },
    navigationResult(result) {
      if (!pendingOverview?.applying || result?.request_id!==pendingOverview.requestId) return;
      checkOverview(state());
      if (!pendingOverview) return;
      if (result.status==='applied' && Number.isSafeInteger(result.render_version) && result.render_version>=0) cancelOverview();
      else cancelOverview('unavailable',true);
    },
    cameraChanged(camera) {
      if (disposed || !enabled()) return;
      const bounds = viewportBounds(camera?.viewport_bounds);
      if (!bounds) return;
      const key = JSON.stringify(bounds);
      if (key === cameraKey) return;
      cameraBounds = bounds; cameraKey = key;
      // Manual wheel/drag already calls pauseFollow via the real canvas input
      // listener. A renderer viewport report is not a new user command.
      if (pendingOverview) return;
      currentQuery = {...currentQuery,bounds:cameraBounds};
      queryToken += 1;
      schedule();
    },
    resetScope() {
      cancelQuery();
      cancelPending();presented = null;
      cancelOverview();queryToken += 1;readStartViews = new WeakSet();
      scope = pixelWorldScope(core.state);feedbackScopeGeneration += 1;feedbackAuthority = null;cancelScheduled();command += 1;
      dirty = false;lastSnapshot = null;lastPins = '';cameraBounds = null;cameraKey = '';currentQuery = {};following = null;selectedCoreKey = null;
      setSpatialSelection(null);setNavigation(null);client.reset(scope,core.state.worldId);
    },
    refresh(snapshot) {
      checkPending(state());
      checkOverview(state());
      if (!snapshot || !enabled() || disposed) return;
      if (spatialSelection() && selectedCoreKey !== coreKey()) setSpatialSelection(null);
      const pinKey = JSON.stringify(pins());
      if (snapshot === lastSnapshot && pinKey === lastPins) return;
      lastSnapshot = snapshot;
      if (timer != null) return;
      void read();
    },
    async returnTarget() {
      cancelPending();
      cancelOverview(null,true);
      const id = mainTarget();if (!id) return;
      const expectedScope = scope;
      const pending = query({search:id}), expectedCommand = command;
      const view = await pending;
      if (!view || expectedScope !== pixelWorldScope(core.state) || command !== expectedCommand) return;
      const target = view.entities.find(entity => entity.kind === 'agent' && entity.entity_id === id);
      if (!target || !worldViewPosition(target,view,false,true)) {setState(previous=>({...previous,notice:'target_unavailable'}));return;}
      select({kind:'agent',id});currentQuery = {};cameraKey = '';navigate('locate',{kind:'agent',id});
    },
    dispose() {disposed = true;cancelQuery();cancelPending();cancelOverview();presented = null;cancelScheduled();client.reset(null);},
  };
  const poll = setInterval(() => {
    if (core.state.viewerProtocol?.capabilities?.includes('world_view_delta_v1') && typeof document !== 'undefined' && document.visibilityState === 'visible' && state().view) void read();
  },2000);
  onCleanup(()=>{clearInterval(poll);controller.dispose();});
  return controller;
}
