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
  return reports;
}
