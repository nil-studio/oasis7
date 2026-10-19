import { describe, expect, it } from "vitest";
import { createStore, $RAW } from "solid-js/store";
import { projectPixelWorldRenderInput, encodePixelWorldRenderInput } from "./pixel_world_render_input.js";

describe("bounded renderer input", () => {
  it("does not traverse unrelated snapshot history or private agent data", () => {
    const agent = { id: "A", pos: { x_cm: 4, y_cm: 5, z_cm: 0 } };
    Object.defineProperty(agent, "cognition", { enumerable: true, get() { throw Error("private tree traversed"); } });
    const snapshot = { time: 9, config: { space: { width_cm: 100 } }, model: {} };
    Object.defineProperty(snapshot, "history", { enumerable: true, get() { throw Error("history traversed"); } });
    const dto = projectPixelWorldRenderInput({ snapshot, lists: { agents: [agent], locations: [] }, gameplay: {} });
    expect(dto.snapshot).toEqual({ time: 9, config: { space: { width_cm: 100 } }, model: {} });
    expect(dto.lists.agents).toEqual([{ id: "A", pos: { x_cm: 4, y_cm: 5, z_cm: 0 } }]);
  });
  it("crosses the boundary with detached ordinary objects and preserves source mutations", () => {
    const [input, setInput] = createStore({ snapshot: { time: 1, model: {} }, lists: { agents: [{ id: "001", pos: { x_cm: 2, y_cm: 3, z_cm: 0 } }] }, gameplay: {} });
    const dto = projectPixelWorldRenderInput(input);
    expect(dto.lists.agents[0][$RAW]).toBeUndefined();
    setInput("lists", "agents", 0, "pos", "x_cm", 12);
    expect(dto.lists.agents[0].pos.x_cm).toBe(2);
    expect(projectPixelWorldRenderInput(input).lists.agents[0].pos.x_cm).toBe(12);
    expect(JSON.parse(encodePixelWorldRenderInput(input)).schema_version).toBe(1);
  });
  it("rejects oversized displays explicitly instead of allocating unbounded input", () => {
    expect(() => projectPixelWorldRenderInput({ lists: { agents: Array.from({ length: 501 }, (_, i) => ({ id: String(i) })) } })).toThrow(/capacity/);
    expect(() => encodePixelWorldRenderInput({ gameplay: { objective: "x".repeat(300000) } })).toThrow(/capacity/);
  });
  it("bounds module maps before reading values and avoids unused full lists with a world view", () => {
    const modules = Object.fromEntries(Array.from({length: 501}, (_, i) => [String(i), null]));
    Object.defineProperty(modules, "0", {enumerable: true, get() { throw Error("unbounded value read"); }});
    expect(() => projectPixelWorldRenderInput({snapshot:{model:{module_visual_entities:modules}}})).toThrow(/capacity/);
    const lists = {};
    Object.defineProperty(lists, "agents", {get() { throw Error("unused source list read"); }});
    expect(projectPixelWorldRenderInput({lists, worldView:{entities:[]}}).lists).toEqual({agents:[],locations:[]});
  });
  it("keeps visibility removals and authoritative intent envelopes in the projection", () => {
    const input = { snapshot: { player_gameplay: { primary_intent: { schema_version: 2, world_id: "W", status: "accepted" } }, model: { social_edges: {} } }, lists: { agents: [{ id: "A" }] }, gameplay: { blockerKind: "blocked", recentFeedback: { stage: "committed", agentId: "A" } } };
    expect(projectPixelWorldRenderInput(input).snapshot.player_gameplay.primary_intent.status).toBe("accepted");
    input.lists.agents = [];
    expect(projectPixelWorldRenderInput(input).lists.agents).toEqual([]);
    expect(projectPixelWorldRenderInput(input).gameplay.recentFeedback.stage).toBe("committed");
  });
});
