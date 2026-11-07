import { createSignal, onCleanup, untrack } from 'solid-js';
import { createWorldViewClient, worldViewPosition } from './pixel_world_world_view.js';
import { pixelWorldScope } from './pixel_world_scope.js';

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
  const client = createWorldViewClient({request:core.requestWorldView,onChange:setState});
  let scope = pixelWorldScope(core.state), lastSnapshot = null, lastPins = '', currentQuery = {};
  let cameraBounds = null, cameraKey = '', timer = null, command = 0, disposed = false;
  let following = null, selectedCoreKey = null, inFlight = null, dirty = false;
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
    cancelScheduled(); client.invalidate();
    timer = setTimeout(() => {timer = null; void read();},debounceMs);
  }
  function query(query = {}) {
    command += 1; cancelScheduled();client.clearChanges();
    currentQuery = {...query};
    if (inFlight) {client.invalidate();dirty=false;return inFlight.then(()=>read());}
    return read();
  }
  function select(ref) {
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
    state,navigation,pauseVersion,selection,scope:()=>scope,
    drilldown(aggregate) {
      if (pixelWorldScope(core.state) !== scope || !state().view?.aggregates?.some(value=>value.aggregate_id === aggregate?.aggregate_id)) return;
      const bounds=viewportBounds(aggregate.drilldown?.bounds);if (!bounds) return;
      navigate('stop');setNavigation({request_id:String(++command),mode:'fit_bounds',bounds,target:null});
      return query({bounds,detail_level:aggregate.drilldown.detail_level});
    },
    query,select,navigate,
    eventFeed(event) {
      if (pixelWorldScope(core.state) !== scope) return false;
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
      scope = pixelWorldScope(core.state);cancelScheduled();command += 1;
      dirty = false;lastSnapshot = null;lastPins = '';cameraBounds = null;cameraKey = '';currentQuery = {};following = null;selectedCoreKey = null;
      setSpatialSelection(null);setNavigation(null);client.reset(scope,core.state.worldId);
    },
    refresh(snapshot) {
      if (!snapshot || !enabled() || disposed) return;
      if (spatialSelection() && selectedCoreKey !== coreKey()) setSpatialSelection(null);
      const pinKey = JSON.stringify(pins());
      if (snapshot === lastSnapshot && pinKey === lastPins) return;
      lastSnapshot = snapshot;
      if (timer != null) return;
      void read();
    },
    async returnTarget() {
      const id = mainTarget();if (!id) return;
      const expectedScope = scope;
      const pending = query({search:id}), expectedCommand = command;
      const view = await pending;
      if (!view || expectedScope !== pixelWorldScope(core.state) || command !== expectedCommand) return;
      const target = view.entities.find(entity => entity.kind === 'agent' && entity.entity_id === id);
      if (!target || !worldViewPosition(target,view,false,true)) {setState(previous=>({...previous,notice:'target_unavailable'}));return;}
      select({kind:'agent',id});currentQuery = {};cameraKey = '';navigate('locate',{kind:'agent',id});
    },
    dispose() {disposed = true;cancelScheduled();client.reset(null);},
  };
  const poll = setInterval(() => {
    if (core.state.viewerProtocol?.capabilities?.includes('world_view_delta_v1') && typeof document !== 'undefined' && document.visibilityState === 'visible' && state().view) void read();
  },2000);
  onCleanup(()=>{clearInterval(poll);controller.dispose();});
  return controller;
}
