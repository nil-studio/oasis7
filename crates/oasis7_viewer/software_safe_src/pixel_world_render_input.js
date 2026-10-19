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
export function projectPixelWorldRenderInput(input) {
  let nodes = 0;
  function copy(value, schema = true, depth = 0) {
    value = raw(value);
    if (++nodes > MAX_NODES || depth > 16) capacity();
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
      const item = copy(value[key], schema === true ? true : schema[key], depth + 1);
      if (item !== undefined) result[key] = item;
    }
    return result;
  }
  function entityList(value) { return copy(value || [], entity); }
  function moduleVisuals(value) {
    value = raw(value);
    const keys = Object.keys(value);
    if (keys.length > MAX_ITEMS) capacity();
    return Object.fromEntries(keys.map(id => [id, copy(value[id], moduleVisual)]));
  }
  const source = raw(input) || {};
  const snapshot = raw(source.snapshot) || {};
  const model = raw(snapshot.model) || {};
  const dto = {
    schema_version: PIXEL_WORLD_RENDER_INPUT_VERSION,
    ...copy(source, fields("locale selectedKind selectedId worldViewStale")),
    snapshot: {
      ...copy(snapshot, fields("time")),
      ...snapshot.config && { config: copy(snapshot.config, { space: fields("width_cm depth_cm height_cm") }) },
      model: {
        ...model.module_visual_entities && { module_visual_entities: moduleVisuals(model.module_visual_entities) },
        ...model.social_edges && { social_edges: copy(model.social_edges) },
      },
      ...snapshot.player_gameplay && { player_gameplay: copy(snapshot.player_gameplay, { primary_intent: fields("schema_version source_class freshness control_state status intent_id agent_id world_id reorg_epoch logical_time event_seq updated_at") }) },
    },
    lists: source.worldView ? {agents:[],locations:[]} : { agents: entityList(source.lists?.agents), locations: entityList(source.lists?.locations) },
    gameplay: copy(source.gameplay || {}, gameplay),
    ...source.selected && { selected: copy(source.selected, entity) },
    ...source.presentation && { presentation: copy(source.presentation, fields("world_bounds_label marker_truth_note")) },
    ...source.worldView && { worldView: copy(source.worldView) },
    ...source.navigation && { navigation: copy(source.navigation) },
  };
  if (dto.lists.agents.length + dto.lists.locations.length > MAX_ITEMS) capacity();
  // Validate before crossing WASM; encoded byte count also bounds Unicode data.
  if (new TextEncoder().encode(JSON.stringify(dto)).byteLength > PIXEL_WORLD_RENDER_INPUT_MAX_BYTES) capacity();
  return dto;
}

export function encodePixelWorldRenderInput(input) {
  return JSON.stringify(projectPixelWorldRenderInput(input));
}
