import { createSignal, onCleanup, untrack } from 'solid-js';
import { createWorldViewClient, worldViewPosition } from './pixel_world_world_view.js';
import { pixelWorldScope } from './pixel_world_scope.js';
import { worldViewAuthorityKey } from './pixel_world_world_view_delta.js';

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
  const client = createWorldViewClient({request:core.requestWorldView,onChange:next=>{setState(next);checkPending(next);}});
  let scope = pixelWorldScope(core.state), lastSnapshot = null, lastPins = '', currentQuery = {};
  let cameraBounds = null, cameraKey = '', timer = null, command = 0, disposed = false;
  let following = null, selectedCoreKey = null, inFlight = null, dirty = false;
  let presented = null, pending = null, pendingTimer = null;
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
  }
  function checkPending(next) {
    if (!pending) return;
    const view = next.view;
    if (disposed || pixelWorldScope(core.state) !== scope || pending.scope !== scope || pending.command !== command
      || pending.coreKey !== coreKey() || !enabled() || next.error || next.stale || !view || requestBinding(view) !== pending.binding
      || deleted(next,pending.ref)) {
      cancelPending('unavailable');return;
    }
    const selected = proved(pending.ref,view);
    if (selected) {
      cancelPending();selectedCoreKey = coreKey();setSpatialSelection(selected);
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
  function navigate(mode,target = null) {
    cancelPending();
    following = mode === 'follow' ? target : null;
    setNavigation({request_id:String(++command),mode,target});
  }
  async function read() {
    if (inFlight) { dirty = true; return inFlight; }
    if (disposed || !enabled() || pixelWorldScope(core.state) !== scope) return null;
    const pinned = pins(); lastPins = JSON.stringify(pinned);
    const query = {...currentQuery};delete query.pinned_entities;delete query.resume_cursor;delete query.cursor;
    if (core.state.viewerProtocol?.capabilities?.includes('world_view_pins_v1')) query.pinned_entities = pinned;
    const caps = core.state.viewerProtocol?.capabilities || [];
    if (caps.includes('world_view_aggregate_v1')) query.detail_level ||= 'auto'; else delete query.detail_level;
    if (caps.includes('world_view_event_anchor_v1')) query.include_events = true; else delete query.include_events;
    inFlight = client.query(query,{delta:caps.includes('world_view_delta_v1'),preserveChanges:true});
    try { return await inFlight; } finally {
      inFlight = null;
      if (dirty && !disposed) {dirty=false;void read();}
    }
  }
  function schedule() {
    cancelPending();
    cancelScheduled(); client.invalidate();
    timer = setTimeout(() => {timer = null; void read();},debounceMs);
  }
  function query(query = {}) {
    cancelPending();
    command += 1; cancelScheduled();client.clearChanges();
    currentQuery = {...query};
    if (inFlight) {client.invalidate();dirty=false;return inFlight.then(()=>read());}
    return read();
  }
  function select(ref) {
    cancelPending();
    if (pixelWorldScope(core.state) !== scope) return null;
    command += 1;
    const view = state().view;
    if (view) {
      if (ref?.kind === 'industry_aggregate' && view.aggregates?.some(value=>value.aggregate_id === ref.id)
        || ref?.kind === 'world_event' && view.events?.some(value=>value.event_id === ref.id)) {
        selectedCoreKey=coreKey();setSpatialSelection(ref);return ref;
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
      return selected;
    }
    setSpatialSelection(null);
    return core.applySelection(ref);
  }
  const controller = {
    readStatistics(expectedSourceCut = null) {
      if (disposed || pixelWorldScope(core.state) !== scope) return null;
      return client.statistics?.(expectedSourceCut) ?? null;
    },
    state,navigation,pauseVersion,selection,selectionStatus,scope:()=>scope,
    // Called by Host only after publishing this render DTO. Store identities,
    // never the previous scene's entity facts or coordinates.
    present(view, scene, presentedScope = scope) {
      presented = null;
      if (!view || !scene) { cancelPending();return; }
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
      setSelectionStatus('pending');
      pendingTimer = setTimeout(()=>cancelPending('unavailable'),30000);
      return null;
    },
    drilldown(aggregate) {
      if (pixelWorldScope(core.state) !== scope || !state().view?.aggregates?.some(value=>value.aggregate_id === aggregate?.aggregate_id)) return;
      const bounds=viewportBounds(aggregate.drilldown?.bounds);if (!bounds) return;
      navigate('stop');setNavigation({request_id:String(++command),mode:'fit_bounds',bounds,target:null});
      return query({bounds,detail_level:aggregate.drilldown.detail_level});
    },
    query,select,navigate,
    eventFeed(event) {
      if (pixelWorldScope(core.state) !== scope) return false;
      cancelPending();
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
    overview() {navigate('overview');},
    cameraChanged(camera) {
      if (disposed || !enabled()) return;
      const bounds = viewportBounds(camera?.viewport_bounds);
      if (!bounds) return;
      const key = JSON.stringify(bounds);
      if (key === cameraKey) return;
      cameraBounds = bounds; cameraKey = key;
      currentQuery = {...currentQuery,bounds:cameraBounds};
      schedule();
    },
    resetScope() {
      cancelPending();presented = null;
      scope = pixelWorldScope(core.state);cancelScheduled();command += 1;
      dirty = false;lastSnapshot = null;lastPins = '';cameraBounds = null;cameraKey = '';currentQuery = {};following = null;selectedCoreKey = null;
      setSpatialSelection(null);setNavigation(null);client.reset(scope,core.state.worldId);
    },
    refresh(snapshot) {
      checkPending(state());
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
      const id = mainTarget();if (!id) return;
      const expectedScope = scope;
      const pending = query({search:id}), expectedCommand = command;
      const view = await pending;
      if (!view || expectedScope !== pixelWorldScope(core.state) || command !== expectedCommand) return;
      const target = view.entities.find(entity => entity.kind === 'agent' && entity.entity_id === id);
      if (!target || !worldViewPosition(target,view,false,true)) {setState(previous=>({...previous,notice:'target_unavailable'}));return;}
      select({kind:'agent',id});currentQuery = {};cameraKey = '';navigate('locate',{kind:'agent',id});
    },
    dispose() {disposed = true;cancelPending();presented = null;cancelScheduled();client.reset(null);},
  };
  const poll = setInterval(() => {
    if (core.state.viewerProtocol?.capabilities?.includes('world_view_delta_v1') && typeof document !== 'undefined' && document.visibilityState === 'visible' && state().view) void read();
  },2000);
  onCleanup(()=>{clearInterval(poll);controller.dispose();});
  return controller;
}
