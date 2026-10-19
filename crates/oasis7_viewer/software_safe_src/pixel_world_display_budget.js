import { $RAW } from "solid-js/store";
const raw = value => value?.[$RAW] || value;
const MAX_SOURCE = 10000;
const fail = () => { throw Error("pixel world display capacity exceeded"); };
const key = (kind, id) => `${kind}:${id}`;
const compare = (a,b) => a < b ? -1 : a > b ? 1 : 0;

// Read identity/anchor fields only while choosing a bounded display subset.
// Authorization, viewport queries and complete Targets lists remain upstream.
export function budgetPixelWorldDisplay(input, limit = 500) {
  input = raw(input) || {};
  const view = raw(input.worldView);
  const model = raw(raw(input.snapshot)?.model) || {};
  const candidates = new Map();
  const counts = {agents:0,locations:0,modules:0,aggregates:0,events:0};
  function add(kind, id, get, group) {
    if (candidates.size >= MAX_SOURCE) fail();
    id = String(id ?? "");
    const identity = key(kind,id);
    if (candidates.has(identity)) fail();
    candidates.set(identity,{kind,id,key:identity,get,group});
    counts[group]++;
  }
  function list(values, kind, group, idField = "id") {
    values = raw(values) || [];
    if (!Array.isArray(values) || values.length > MAX_SOURCE) fail();
    for (const item of values) { const value = raw(item); add(kind, value?.[idField], ()=>value, group); }
  }
  if (view) {
    const entities = raw(view.entities) || [];
    if (!Array.isArray(entities) || entities.length > MAX_SOURCE) fail();
    for (const item of entities) {
      const value = raw(item), kind = value?.kind;
      if (kind === "agent" || kind === "location" || kind === "factory") add(kind,value.entity_id,()=>value,kind === "agent" ? "agents" : kind === "location" ? "locations" : "modules");
    }
    list(view.aggregates,"aggregate","aggregates","aggregate_id");
    list(view.events,"event","events","event_id");
  } else {
    list(input.lists?.agents,"agent","agents");
    list(input.lists?.locations,"location","locations");
    const modules = raw(model.module_visual_entities) || {};
    const ids = Object.keys(modules);
    if (ids.length > MAX_SOURCE) fail();
    for (const id of ids) add("module_visual",id,()=>raw(modules[id]),"modules");
  }
  const required = new Set();
  const gameplay = raw(input.gameplay) || {};
  const wanted = [key(input.selectedKind,input.selectedId), key(input.navigation?.target?.kind,input.navigation?.target?.id),
    ...[gameplay.recommendedAction?.targetAgentId,gameplay.receiptTargetAgentId,gameplay.recentFeedback?.agentId,
      raw(raw(input.snapshot)?.player_gameplay)?.primary_intent?.agent_id].filter(Boolean).map(id=>key("agent",id))];
  function closure(item, result = new Set()) {
    if (!item || result.has(item.key)) return result;
    result.add(item.key);
    const value = item.get();
    const record = view && (item.kind === "agent" || item.kind === "location" || item.kind === "factory") ? raw(value.render_record) : value;
    const location = record?.location_id || raw(record?.anchor)?.data?.location_id;
    const agent = raw(record?.anchor)?.data?.agent_id;
    if (location) closure(candidates.get(key("location",location)),result);
    if (agent) closure(candidates.get(key("agent",agent)),result);
    return result;
  }
  for (const id of wanted) for (const dependent of closure(candidates.get(id))) required.add(dependent);
  if (required.size > limit) fail();
  const retained = new Set(required);
  const ordered = [...candidates.values()].sort((a,b)=>compare(a.key,b.key));
  for (const item of ordered) {
    const needed = [...closure(item)].filter(id=>!retained.has(id));
    if (retained.size + needed.length <= limit) for (const id of needed) retained.add(id);
  }
  const selected = ordered.filter(item=>retained.has(item.key));
  return {selected,retained, metadata:{source_counts:counts,projected_markers:selected.length,
    omitted_markers:candidates.size-selected.length,required_entities:[...required].sort(),
    reason:candidates.size>selected.length ? "marker_budget" : null}};
}
