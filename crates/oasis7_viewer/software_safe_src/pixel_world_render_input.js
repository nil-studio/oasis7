import { budgetPixelWorldDisplay } from "./pixel_world_display_budget.js";
import { $RAW } from "solid-js/store";

export const PIXEL_WORLD_RENDER_INPUT_VERSION = 1;
export const PIXEL_WORLD_RENDER_INPUT_MAX_BYTES = 256 * 1024;
const MAX_ITEMS = 500;
const MAX_NODES = 16384;
const raw = value => value?.[$RAW] || value;
const capacity = () => { throw new Error("pixel world display capacity exceeded"); };
const fields = names => Object.fromEntries(names.split(" ").map(key => [key, true]));
const position = fields("x_cm y_cm z_cm");
const relation = fields("kind status source_class freshness label");
const entity = {
  ...fields("id name label kind location_id"), pos: position,
  resources: true, power: fields("state"), relation, assignment: relation,
  profile: fields("radius_cm"), fragment_profile: { blocks: { blocks: true } },
};
const moduleVisual = { ...fields("entity_id module_id kind label"), anchor: true };
const gameplay = {
  ...fields("acceptedIntentDetail acceptedIntentId acceptedIntentScope acceptedIntentSummary acceptedIntentTarget blockerDetail blockerKind blockerLabel executionCauseDetail executionCauseKind executionState executionStateLabel executionSummary goalKind goalTitle lastWorldChange narrativeBlockerDetail narrativeNextStep nextStepHint objective progressDetail progressPercent receiptTargetAgentId"),
  availableActions: true, recentFeedback: true, recommendedAction: true,
  microDepotFacilities: true, micro_depot_facilities: true,
};

// Only display-owned subtrees reach the bridge. Never enumerate a snapshot or
// an Agent: genomes, decision history, auth and unknown fields stay in the host.
function projectOnce(input, markerLimit) {
  let nodes = 0;
  function copy(value, schema = true, depth = 0) {
    value = raw(value);
    if (++nodes > MAX_NODES) throw new RangeError("pixel world projection budget exceeded");
    if (depth > 16) capacity();
    if (value === undefined) return undefined;
    if (value === null || typeof value === "boolean") return value;
    if (typeof value === "number") { if (!Number.isFinite(value)) capacity(); return value; }
    if (typeof value === "string") { if (value.length > 8192) capacity(); return value; }
    if (Array.isArray(value)) {
      if (value.length > MAX_ITEMS) capacity();
      return value.map(item => copy(item, schema, depth + 1));
    }
    if (typeof value !== "object") capacity();
    const result = {};
    const keys = schema === true ? Object.keys(value) : Object.keys(schema);
    if (keys.length > MAX_ITEMS) capacity();
    for (const key of keys) {
      if (key === "__proto__" || key === "constructor" || key === "prototype") continue;
      let sourceItem = value[key];
      if ((key === "name" || key === "label") && typeof sourceItem === "string") {
        let bounded = "", scalars = 0;
        for (const scalar of sourceItem) { if (scalars++ === 128) break; bounded += scalar; }
        sourceItem = bounded;
      }
      const item = copy(sourceItem, schema === true ? true : schema[key], depth + 1);
      if (item !== undefined) result[key] = item;
    }
    return result;
  }
  const source = raw(input) || {};
  const snapshot = raw(source.snapshot) || {};
  const model = raw(snapshot.model) || {};
  const facilities = raw(source.gameplay)?.microDepotFacilities || raw(source.gameplay)?.micro_depot_facilities || [];
  const reserve = Array.isArray(raw(facilities)) ? raw(facilities).length : 0;
  const budget = budgetPixelWorldDisplay(source, Math.max(0,markerLimit-reserve));
  const pick = group => budget.selected.filter(item=>item.group===group);
  const lists = {agents:[],locations:[]};
  if (!source.worldView) {
    lists.agents = pick("agents").map(item=>copy(item.get(),entity));
    lists.locations = pick("locations").map(item=>copy(item.get(),entity));
  }
  const moduleVisuals = source.worldView ? {} : Object.fromEntries(pick("modules").map(item=>[item.id,copy(item.get(),moduleVisual)]));
  const provenance = fields("geometry_source freshness valid_at_source_cut");
  const identity = fields("world_id reorg_epoch event_seq");
  const relationEndpoint = fields("kind id");
  const relations = raw(source.worldView?.relations) || [];
  if (!Array.isArray(relations) || relations.length > 10000) capacity();
  const worldView = source.worldView ? {
    ...copy(source.worldView,fields("schema_version world_id scope_id source_cut request_generation completeness reorg_epoch")),
    ...source.worldView.world_bounds && {world_bounds:copy(source.worldView.world_bounds,fields("width_cm depth_cm height_cm"))},
    entities: budget.selected.filter(item=>["agents","locations","modules"].includes(item.group)).map(item=>copy(item.get(),{...fields("entity_id kind"),render_record:entity,provenance})),
    aggregates:pick("aggregates").map(item=>copy(item.get(),{...fields("aggregate_id label member_count"),position,bounds:fields("min_x_cm max_x_cm min_y_cm max_y_cm min_z_cm max_z_cm"),provenance})),
    events:pick("events").map(item=>copy(item.get(),{...fields("event_id source_class source_cut freshness label"),identity,feed_ref:identity,position})),
    relations: copy(relations.filter(item=>budget.retained.has(`${item.from?.kind}:${item.from?.id}`) && budget.retained.has(`${item.to?.kind}:${item.to?.id}`)).slice(0,MAX_ITEMS),{...fields("relation_id kind source_class status freshness valid_at_source_cut label route_count"),from:relationEndpoint,to:relationEndpoint}),
  } : null;
  const social = raw(model.social_edges) || {};
  const socialKeys = Object.keys(social).sort();
  if (socialKeys.length > 10000) capacity();
  const visibleOwner = owner => {
    owner = raw(owner);
    const kind = owner?.type === "Agent" ? "agent" : owner?.type === "Location" ? "location" : null;
    const id = kind === "agent" ? raw(owner?.data)?.agent_id : raw(owner?.data)?.location_id;
    return kind && budget.retained.has(`${kind}:${id}`);
  };
  const owner = {...fields("type"),data:fields("agent_id location_id")};
  const socialEdges = {};
  for (const id of socialKeys) {
    const edge = raw(social[id]);
    if (!visibleOwner(edge?.from) || !visibleOwner(edge?.to)) continue;
    if (Object.keys(socialEdges).length >= MAX_ITEMS) break;
    socialEdges[id] = copy(edge,{...fields("edge_id lifecycle expires_at_tick relation_kind schema_id weight_bps"),from:owner,to:owner});
  }
  const dto = {
    schema_version: PIXEL_WORLD_RENDER_INPUT_VERSION,
    ...copy(source, fields("locale selectedKind selectedId worldViewStale")),
    snapshot: {
      ...copy(snapshot, fields("time")),
      ...snapshot.config && { config: copy(snapshot.config, { space: fields("width_cm depth_cm height_cm") }) },
      model: {
        ...!source.worldView && model.module_visual_entities && { module_visual_entities: moduleVisuals },
        ...model.social_edges && { social_edges: socialEdges },
      },
      ...snapshot.player_gameplay && { player_gameplay: copy(snapshot.player_gameplay, { primary_intent: fields("schema_version source_class freshness control_state status intent_id agent_id world_id reorg_epoch logical_time event_seq updated_at") }) },
    },
    lists,
    display_budget: budget.metadata,
    gameplay: copy(source.gameplay || {}, gameplay),
    ...source.selected && { selected: copy(source.selected, entity) },
    ...source.presentation && { presentation: copy(source.presentation, fields("world_bounds_label marker_truth_note")) },
    ...worldView && { worldView },
    ...source.navigation && { navigation: copy(source.navigation) },
  };
  if (dto.lists.agents.length + dto.lists.locations.length > MAX_ITEMS) capacity();
  // Validate before crossing WASM; encoded byte count also bounds Unicode data.
  if (new TextEncoder().encode(JSON.stringify(dto)).byteLength > PIXEL_WORLD_RENDER_INPUT_MAX_BYTES) {
    throw new RangeError("pixel world projection budget exceeded");
  }
  return dto;
}

export function projectPixelWorldRenderInput(input, markerLimit = MAX_ITEMS) {
  for (let limit = markerLimit; limit >= 1; limit = Math.floor(limit * 0.75)) {
    try { return projectOnce(input, limit); }
    catch (error) {
      if (!(error instanceof RangeError) || error.message !== "pixel world projection budget exceeded") throw error;
    }
  }
  capacity();
}

export function encodePixelWorldRenderInput(input) {
  return JSON.stringify(projectPixelWorldRenderInput(input));
}
