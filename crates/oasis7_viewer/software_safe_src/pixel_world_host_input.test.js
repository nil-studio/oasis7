import { afterEach, expect, it, vi } from "vitest";
import * as core from "./legacy_core.js";
import { buildPixelWorldRenderInput } from "./pixel_world_host_input.js";

afterEach(()=>vi.restoreAllMocks());
it("keeps Targets search separate from authorized world input and removes revoked actors",()=>{
  const agents = [{id:"A"},{id:"B"},{id:"private"}];
  vi.spyOn(core,"entityCollections").mockReturnValue({agents,locations:[{id:"L"}]});
  let allowed = new Set(["A","B"]);
  vi.spyOn(core,"isAgentVisibleToCurrentSession").mockImplementation(id=>allowed.has(id));
  vi.spyOn(core,"buildWorldScaleSurface").mockReturnValue({physicalTruth:{},presentationScale:{}});
  vi.spyOn(core,"buildGameplaySummary").mockReturnValue({});
  core.state.selectedSearch = "no-match";
  expect(buildPixelWorldRenderInput().lists.agents.map(a=>a.id)).toEqual(["A","B"]);
  allowed = new Set(["B"]);
  expect(buildPixelWorldRenderInput().lists.agents.map(a=>a.id)).toEqual(["B"]);
  agents.splice(1,1);
  expect(buildPixelWorldRenderInput().lists.agents).toEqual([]);
  core.state.selectedSearch = "";
});
