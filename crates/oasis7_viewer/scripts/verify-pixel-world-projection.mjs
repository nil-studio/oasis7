import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { projectPixelWorldRenderInput, PIXEL_WORLD_RENDER_INPUT_MAX_BYTES } from "../software_safe_src/pixel_world_render_input.js";

const MARKERS = ["agents", "locations", "module_visual_entities", "micro_depot_facilities", "industry_aggregates", "world_events"];
const byteLength = value => Buffer.byteLength(JSON.stringify(value), "utf8");

// Exercise the real JS wire format against the generated Rust/WASM consumer.
// No browser, WebGL context or bridge mount is needed for pure derivation.
export async function verifyPixelWorldProjection({ bindingsPath, wasmPath }) {
  const bindings = await import(pathToFileURL(bindingsPath).href);
  const runtime = bindings.createPixelWorldModule();
  const compiled = await WebAssembly.compile(await readFile(wasmPath));
  await runtime.default({ module_or_path: compiled });
  const reports = [];
  for (const [count, richLabels] of [[1000, false], [10000, false], [350, true]]) {
    const agents = Array.from({ length: count }, (_, index) => ({
      id: `A${String(index).padStart(5, "0")}`,
      name: richLabels ? "😀".repeat(128) : `Actor ${index}`,
      pos: { x_cm: index, y_cm: 1, z_cm: 0 },
    }));
    const selected = agents.at(-1);
    const source = {
      locale: "en",
      snapshot: { time: 7, config: { space: { width_cm: 100000, depth_cm: 100000, height_cm: 1000 } }, model: {}, history: "x".repeat(5 * 1024 * 1024) },
      lists: { agents, locations: [] }, selected, selectedKind: "agent", selectedId: selected.id,
      gameplay: { recommendedAction: { targetAgentId: agents.at(-2).id }, receiptTargetAgentId: agents.at(-3).id },
    };
    const input = projectPixelWorldRenderInput(source);
    const inputBytes = byteLength(input);
    assert.ok(inputBytes <= PIXEL_WORLD_RENDER_INPUT_MAX_BYTES, "projected input exceeds wire budget");
    assert.equal(input.snapshot.history, undefined, "history crossed the display boundary");
    const output = runtime.build_pixel_world_render_state_json(JSON.stringify(input));
    assert.equal(output.fatal, undefined, `actual WASM rejected projected input (${count} actors): ${JSON.stringify(output.fatal)}`);
    assert.equal(output.goal_highlight,null,"guidance without a published goal must not become a goal highlight");
    assert.ok(!output.visual_hotspots.some(hotspot=>hotspot.kind==="goal"),"missing goal facts produced a spatial goal");
    const outputBytes = byteLength(output);
    assert.ok(outputBytes <= PIXEL_WORLD_RENDER_INPUT_MAX_BYTES, "actual WASM output exceeds wire budget");
    const markers = MARKERS.reduce((total, field) => total + (output[field]?.length || 0), 0);
    assert.ok(markers <= 500, "actual WASM output exceeds marker budget");
    for (const id of [selected.id, agents.at(-2).id, agents.at(-3).id]) {
      assert.ok(output.agents.some(agent => agent.id === id), `semantic target ${id} was clipped`);
    }
    assert.equal(output.display_budget.source_counts.agents, count, "source count was lost across the actual wire ABI");
    assert.equal(output.commercial_surface.world_read.agents, count, "visible source count must describe the authorized world");
    if (richLabels) assert.ok(output.display_budget.output_omitted_markers > 0, "rich-label fixture must exercise output byte clipping");
    assert.equal(source.lists.agents.length, count, "projection mutated the source world");
    reports.push({ sourceActors: count, richLabels, inputBytes, outputBytes, markers });
  }
  for (const richLabels of [false,true]) {
    const entities=Array.from({length:1000},(_,index)=>({kind:"agent",entity_id:`W${String(index).padStart(4,"0")}`,
      render_record:{name:richLabels?"界".repeat(128):`Actor ${index}`,pos:{x_cm:index,y_cm:0,z_cm:0}},
      provenance:{geometry_source:"authoritative",freshness:"current",valid_at_source_cut:"C"}}));
    const input=projectPixelWorldRenderInput({worldView:{schema_version:1,world_id:"W",scope_id:"S",source_cut:"C",entities,aggregates:[],events:[],relations:[]},
      snapshot:{model:{},config:{space:{width_cm:100000,depth_cm:100000,height_cm:1000}}},selectedKind:"agent",selectedId:"W0999"});
    const output=runtime.build_pixel_world_render_state_json(JSON.stringify(input));
    assert.equal(output.fatal,undefined,"actual WASM rejected world-view profile");
    assert.equal(output.agents.length,1000);
    assert.equal(output.display_profile,"world_view_v1");
    assert.equal(output.display_source_cut,"C");
    assert.equal(output.goal_highlight,null);
    assert.ok(!output.visual_hotspots.some(hotspot=>hotspot.kind==="goal"));
    const actualGoal={...input,gameplay:{...input.gameplay,goalKind:"RecoverCapability"}};
    assert.ok(runtime.build_pixel_world_render_state_json(JSON.stringify(actualGoal)).visual_hotspots.some(hotspot=>hotspot.kind==="goal"),"published canonical goal was suppressed");
    assert.ok(byteLength(input)<=2*1024*1024 && byteLength(output)<=2*1024*1024);
    if(richLabels)assert.ok(byteLength(input)>256*1024,"Unicode fixture must cross the legacy byte boundary");
    const invalid={...input,display_profile:"world_view_v2"};
    assert.ok(runtime.build_pixel_world_render_state_json(JSON.stringify(invalid)).fatal,"unknown profile must fail across the real ABI");
    if(!richLabels){
      const legacy={...input,worldView:{...input.worldView,entities:[...input.worldView.entities.slice(0,499),input.worldView.entities.at(-1)]}};
      delete legacy.display_profile;
      const oldOutput=runtime.build_pixel_world_render_state_json(JSON.stringify(legacy));
      assert.equal(oldOutput.fatal,undefined,"older V1 world-view input must remain compatible");
      assert.ok(oldOutput.agents.length<=500 && byteLength(oldOutput)<=256*1024);
    }
    reports.push({profile:"world_view_v1",richLabels,inputBytes:byteLength(input),outputBytes:byteLength(output),markers:output.agents.length});
  }
  const expandedBadge=projectPixelWorldRenderInput({selectedKind:"agent",selectedId:"A",lists:{agents:[{id:"A",kind:"x".repeat(8192),pos:{x_cm:1,y_cm:1,z_cm:0}}]}});
  assert.ok(runtime.build_pixel_world_render_state_json(JSON.stringify(expandedBadge)).fatal,
    "input-valid kind must not emit an oversized derived status badge");
  return reports;
}
