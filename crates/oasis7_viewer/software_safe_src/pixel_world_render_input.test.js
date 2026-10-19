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
    expect(() => projectPixelWorldRenderInput({ lists: { agents: Array.from({ length: 10001 }, (_, i) => ({ id: String(i) })) } })).toThrow(/capacity/);
    expect(() => encodePixelWorldRenderInput({ gameplay: { objective: "x".repeat(300000) } })).toThrow(/capacity/);
  });
  it("bounds module maps before reading values and avoids unused full lists with a world view", () => {
    const modules = Object.fromEntries(Array.from({length: 10001}, (_, i) => [String(i), null]));
    Object.defineProperty(modules, "0", {enumerable: true, get() { throw Error("unbounded value read"); }});
    expect(() => projectPixelWorldRenderInput({snapshot:{model:{module_visual_entities:modules}}})).toThrow(/capacity/);
    const lists = {};
    Object.defineProperty(lists, "agents", {get() { throw Error("unused source list read"); }});
    expect(projectPixelWorldRenderInput({lists, worldView:{entities:[]}}).lists).toEqual({agents:[],locations:[]});
  });
  it("clips ten thousand authorized actors stably while retaining semantic targets and location dependencies", () => {
    const agents = Array.from({length:9999}, (_, i) => ({id:`A${String(i).padStart(5,"0")}`,name:`Actor ${i}`,pos:{x_cm:i,y_cm:1,z_cm:0}}));
    agents[9998].location_id = "Z"; agents[9998].name = "😀".repeat(200);
    const source = {snapshot:{model:{},history:"x".repeat(5*1024*1024)},lists:{agents,locations:[{id:"Z",pos:{x_cm:99,y_cm:88,z_cm:0}}]},selectedKind:"agent",selectedId:"A09998",gameplay:{recommendedAction:{targetAgentId:"A09997"},receiptTargetAgentId:"A09996"}};
    const dto = projectPixelWorldRenderInput(source);
    expect(dto.lists.agents.length+dto.lists.locations.length).toBeLessThanOrEqual(500);
    for (const id of ["A09998","A09997","A09996"]) expect(dto.lists.agents.some(a=>a.id===id)).toBe(true);
    expect(dto.lists.locations[0].id).toBe("Z");
    expect(dto.display_budget.source_counts.agents).toBe(9999);
    expect(dto.display_budget.omitted_markers).toBe(9500);
    expect([...dto.lists.agents.find(a=>a.id==="A09998").name]).toHaveLength(128);
    expect(dto.snapshot.history).toBeUndefined();
    expect(source.lists.agents).toHaveLength(9999);
    source.lists.agents = [...agents].reverse();
    expect(projectPixelWorldRenderInput(source)).toEqual(dto);
  });
  it("reduces optional markers when rich resources exhaust the node budget", () => {
    const resources = Object.fromEntries(Array.from({length:30},(_,i)=>[`R${i}`,i]));
    const agents = Array.from({length:500},(_,i)=>({id:`A${i}`,resources,pos:{x_cm:i,y_cm:0,z_cm:0}}));
    const dto = projectPixelWorldRenderInput({lists:{agents,locations:[]},selectedKind:"agent",selectedId:"A499"});
    expect(dto.lists.agents.length).toBeLessThan(500);
    expect(dto.lists.agents.some(a=>a.id==="A499")).toBe(true);
    expect(dto.display_budget.omitted_markers).toBeGreaterThan(0);
  });
  it("clips authorized world-view factories while preserving navigation and freshness", () => {
    const entities = Array.from({length:1000},(_,i)=>({kind:"factory",entity_id:`F${String(i).padStart(4,"0")}`,render_record:{name:"Factory",pos:{x_cm:i,y_cm:0,z_cm:0}},provenance:{geometry_source:"authoritative",freshness:"current",valid_at_source_cut:"C"}}));
    const input = {worldView:{schema_version:1,source_cut:"C",entities,aggregates:[],events:[],relations:[]},navigation:{mode:"locate",target:{kind:"factory",id:"F0999"}}};
    const dto = projectPixelWorldRenderInput(input);
    expect(dto.worldView.entities).toHaveLength(500);
    expect(dto.worldView.entities.some(e=>e.entity_id==="F0999")).toBe(true);
    expect(dto.display_budget.required_entities).toContain("factory:F0999");
    expect(dto.worldView.entities[0].provenance.freshness).toBe("current");
    input.worldView.entities = entities.filter(e=>e.entity_id!=="F0999");
    expect(projectPixelWorldRenderInput(input).worldView.entities.some(e=>e.entity_id==="F0999")).toBe(false);
    input.worldView.relations = Array(10001).fill({});
    expect(()=>projectPixelWorldRenderInput(input)).toThrow(/capacity/);
  });
  it("keeps visibility removals and authoritative intent envelopes in the projection", () => {
    const input = { snapshot: { player_gameplay: { primary_intent: { schema_version: 2, world_id: "W", status: "accepted" } }, model: { social_edges: {} } }, lists: { agents: [{ id: "A" }] }, gameplay: { blockerKind: "blocked", recentFeedback: { stage: "committed", agentId: "A" } } };
    expect(projectPixelWorldRenderInput(input).snapshot.player_gameplay.primary_intent.status).toBe("accepted");
    input.lists.agents = [];
    expect(projectPixelWorldRenderInput(input).lists.agents).toEqual([]);
    expect(projectPixelWorldRenderInput(input).gameplay.recentFeedback.stage).toBe("committed");
  });
});
