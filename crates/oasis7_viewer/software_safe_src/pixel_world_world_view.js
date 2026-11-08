import { measureRetainedJson } from './pixel_world_retention.js';
import { applyWorldViewDelta, worldViewAuthorityKey, worldViewQueryKey, validateWorldViewEvolution, freezeWorldView } from "./pixel_world_world_view_delta.js";
import { pixelWorldScope } from "./pixel_world_scope.js";
// A read-only client. Request generation fences callbacks; it is not a world version.
export function worldViewScopeKey(state, deployment = '') {
  return JSON.stringify([deployment, pixelWorldScope(state)]);
}
const binding = view => JSON.stringify([view.world_id, view.scope_id, view.source_cut, view.world_bounds, worldViewAuthorityKey(view)]);
export function createWorldViewClient({ request, onChange = () => {} }) {
  let generation = 0, scope = null, expectedWorld = null, baselineQuery = null, retainedQuery = null;
  let state = { view: null, loading: false, stale: false, error: null, changes: [] };
  const publish = patch => { state = { ...state, ...patch }; onChange(state); };
  return {
    state: () => state,
    statistics() {
      const started = performance.now(), query = retainedQuery?.generation === generation ? retainedQuery : null;
      const published = measureRetainedJson([state.view, state.changes]);
      const pagingRoots = query ? [query.first(), query.last(), ...query.maps().flatMap(map => [...map.values()])] : [];
      const paging = measureRetainedJson(pagingRoots);
      return { localGeneration: generation, scopeBound: scope !== null, loading: state.loading,
        published: { ...published, entities: state.view?.entities?.length || 0, changes: state.changes.length },
        paging: { ...paging, active: Boolean(query), maps: query?.maps().map(map => map.size) || [0,0,0,0], cursors: query?.seen.size || 0 },
        observationMs: performance.now() - started, semantics: 'logical JSON bytes and unique objects per component; shared references may overlap; not heap/GPU/total memory' };
    },
    invalidate() { generation += 1; retainedQuery = null; },
    clearChanges() { publish({changes:[]}); },
    reset(nextScope, worldId = null) { generation += 1; retainedQuery = null; scope = nextScope; expectedWorld = worldId; publish({ view: null, loading: false, stale: false, error: null, changes: [] }); },
    async query(query = {}, { delta = false, preserveChanges = false } = {}) {
      const current = ++generation, currentScope = scope;
      const active = () => current === generation && currentScope === scope;
      const requestGeneration = String(current);
      const pageSize = Math.min(200, query.page_size || 200);
      publish({ loading: true, error: null });
      const queryKey = worldViewQueryKey(query);
      let resume = delta && baselineQuery === queryKey && !state.stale && !state.view?.source_frozen ? state.view?.resume_cursor : null;
      if (!resume && !preserveChanges) publish({changes:[]});
      let lastPublished = null, recoveries = 0, restart = false, first = null, cursor, entities = new Map(), relations = new Map(), aggregates = new Map(), events = new Map(), seen = new Set(), pages = 0;
      retainedQuery = { generation: current, maps: () => [entities, relations, aggregates, events], seen, first: () => first, last: () => lastPublished };
      const deadline = Date.now() + 30000;
      try {
        do {
          restart = false;
          if (++pages > 20 || Date.now() > deadline) throw Error('World view request budget exceeded');
          const page = await request({ ...query, page_size: pageSize, request_generation: requestGeneration, ...(cursor ? { cursor } : {}), ...(resume ? {resume_cursor:resume} : {}) });
          if (!active()) return null;
          if (page?.delivery === 'reset') { if (++recoveries > 1) throw Error('World view baseline unavailable'); resume = null; cursor = null; restart = true; baselineQuery = null; publish({view:null,stale:false,changes:[]}); continue; }
          if (page?.delivery === 'delta' || page?.delivery === 'unchanged') {
            try {
              if (!resume || page.request_generation !== requestGeneration) throw Error('World view unexpected delta');
              const view = applyWorldViewDelta(state.view,page);
              publish({view,loading:false,stale:false,error:null,changes:page.removals?.length ? page.removals : state.changes}); return view;
            } catch { if (++recoveries > 1) throw Error('World view continuity unavailable'); resume = null; cursor = null; restart = true; baselineQuery = null; publish({view:null,stale:false,changes:[]}); continue; }
          }
          if (!first && state.view && worldViewAuthorityKey(state.view) !== worldViewAuthorityKey(page)) publish({changes:[]});
          if (page?.unavailable_reasons?.length && page.completeness === 'unknown') throw Error('World view scope unavailable');
          if (expectedWorld && page?.world_id !== expectedWorld) throw Error('World view world binding mismatch');
          if (!page || !Array.isArray(page.entities) || !page.world_id || !page.source_cut || page.request_generation !== requestGeneration) throw Error('World view response binding mismatch');
          if (new TextEncoder().encode(JSON.stringify(page)).byteLength > 2 * 1024 * 1024) throw Error('World view page byte budget exceeded');
          if (page.entities.length + (page.relations?.length || 0) + (page.aggregates?.length || 0) + (page.events?.length || 0) > pageSize || (first && binding(first) !== binding(page))) throw Error('World view page binding mismatch');
          first ||= page;
          if (page.source_frozen) {
            first = { ...first, source_frozen: true };
            entities = new Map([...entities].map(([key, entity]) => [key, entity.provenance?.freshness === 'current' ? { ...entity, provenance: { ...entity.provenance, freshness: 'last_known', reason: 'frozen_pagination_cut' } } : entity]));
            relations = new Map([...relations].map(([key, relation]) => [key, relation.freshness === 'current' ? {...relation, freshness:'last_known'} : relation]));
          }
          for (let entity of page.entities) {
            if (first.source_frozen && entity.provenance?.freshness === 'current') entity = { ...entity, provenance: { ...entity.provenance, freshness: 'last_known', reason: 'frozen_pagination_cut' } };
            if (!entity.entity_id || !entity.kind) throw Error('World view entity identity missing');
            const key = `${entity.kind}:${entity.entity_id}`;
            if (entities.has(key) && JSON.stringify(entities.get(key)) !== JSON.stringify(entity)) throw Error('World view conflicting entity');
            entities.set(key, entity);
          }
          if (page.relations != null && !Array.isArray(page.relations)) throw Error('World view relations unavailable');
          for (let relation of page.relations || []) {
            if (!relation.relation_id) throw Error('World view relation identity missing');
            if (first.source_frozen && relation.freshness === 'current') relation = {...relation, freshness:'last_known'};
            if (relations.has(relation.relation_id) && JSON.stringify(relations.get(relation.relation_id)) !== JSON.stringify(relation)) throw Error('World view conflicting relation');
            relations.set(relation.relation_id, relation);
          }
          for (const [field,map,id] of [['aggregates',aggregates,'aggregate_id'],['events',events,'event_id']]) {
            if (page[field] != null && !Array.isArray(page[field])) throw Error('World view records invalid');
            for (const value of page[field] || []) {
              if (!value[id] || (map.has(value[id]) && JSON.stringify(map.get(value[id])) !== JSON.stringify(value))) throw Error('World view identity conflict');
              map.set(value[id],value);
            }
          }
          if (entities.size + relations.size + aggregates.size + events.size > 1000) throw Error('World view object budget exceeded');
          cursor = page.next_cursor || null;
          if (cursor && seen.has(cursor)) throw Error('World view cursor cycle');
          if (cursor) seen.add(cursor);
          const nextView = validateWorldViewEvolution(freezeWorldView({ ...first, entities: [...entities.values()], relations: [...relations.values()], aggregates:[...aggregates.values()],events:[...events.values()], next_cursor: cursor, resume_cursor:page.resume_cursor, update_sequence:page.update_sequence, completeness: cursor ? 'partial' : page.completeness }));
          if (new TextEncoder().encode(JSON.stringify(nextView)).byteLength > 2 * 1024 * 1024) {
            if (!lastPublished) throw Error('World view cumulative byte budget exceeded');
            publish({view:{...lastPublished,completeness:'partial',budget_limited:true,resume_cursor:undefined},loading:false});return state.view;
          }
          lastPublished = nextView;
          publish({ view:nextView, stale:false });
          if (cursor && entities.size + relations.size + aggregates.size + events.size + pageSize > 1000) {
            publish({view:{...state.view,completeness:'partial',budget_limited:true},loading:false});
            return state.view;
          }
        } while (cursor || restart);
        if (active()) { baselineQuery = state.view?.completeness === 'complete' && !state.view?.budget_limited ? queryKey : null; publish({ loading: false }); return state.view; }
        return null;
      } catch (error) {
        if (active()) publish({ loading: false, stale: Boolean(state.view), error: String(error?.message || error) });
        return null;
      } finally { if (retainedQuery?.generation === current) retainedQuery = null; }
    },
  };
}

// Only a current, verified authoritative point can move the camera or sustain follow.
export function worldViewPosition(entity, view, stale = false, allowDerived = false) {
  const p = entity?.provenance;
  if (stale || p?.freshness !== 'current' || p?.valid_at_source_cut !== view?.source_cut || !(p?.geometry_source === 'authoritative' || (allowDerived && p?.geometry_source === 'location_derived'))) return null;
  const point = entity.position;
  if (!point) return null;
  if (!['x_cm','y_cm','z_cm'].every(key => typeof point[key] === 'number' || (typeof point[key] === 'string' && /^-?(0|[1-9]\d*)$/.test(point[key])))) return null;
  const converted = Object.fromEntries(['x_cm','y_cm','z_cm'].map(k => [k, Number(point[k])]));
  return Object.values(converted).every(Number.isSafeInteger) ? converted : null;
}

export function createWorldViewTransport({ send, timeoutMs = 10000, getScope = () => null, getConnection = () => null }) {
  const pending = new Map(); let sequence = 0;
  return {
    request(query) {
      const id = `${query.request_generation}:${++sequence}`;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(Error('World view request timed out')); }, timeoutMs);
        pending.set(id, { resolve, reject, timer, scope: getScope(), connection: getConnection() });
        try { send({ type: 'request_world_view', query: { ...query, request_generation: id } }); }
        catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
      }).then(view => ({ ...view, request_generation: query.request_generation }));
    },
    handle(message) {
      if (message?.type !== 'world_view' && message?.type !== 'world_view_error') return false;
      const payload = message.view || message.error;
      const item = pending.get(payload?.request_generation);
      if (item) {
        clearTimeout(item.timer); pending.delete(payload.request_generation);
        if (item.scope !== getScope() || item.connection !== getConnection()) item.reject(Error('World view scope changed'));
        else if (message.type === 'world_view') item.resolve(payload); else item.reject(Error(payload.message || payload.code || 'World view unavailable'));
      }
      return true;
    },
    clear() { for (const item of pending.values()) { clearTimeout(item.timer); item.reject(Error('World view scope disconnected')); } pending.clear(); },
  };
}

// Text can retain disclosed unavailable routes; drawing remains a Rust authority check.
export function worldViewRelations(view) {
  const entities = new Set((view?.entities || []).map(entity => `${entity.kind}:${entity.entity_id}`));
  const aggregates = new Set((view?.aggregates || []).map(value=>value.aggregate_id));
  return (view?.relations || []).filter(relation => {
    if (!relation.relation_id || relation.valid_at_source_cut !== view.source_cut) return false;
    const runtime = relation.kind === 'logistics_route' && relation.source_class === 'runtime_projection';
    const industrial = relation.kind === 'industry_route' && relation.source_class === 'display_aggregation'
      && typeof relation.route_count === 'string' && /^(0|[1-9]\d*)$/.test(relation.route_count);
    const endpoint = ref => ref?.kind === 'aggregate'
      ? industrial && aggregates.has(ref.id) : entities.has(`${ref?.kind}:${ref?.id}`);
    return (runtime || industrial) && endpoint(relation.from) && endpoint(relation.to);
  });
}
