import { describe, expect, it } from "vitest";
import { buildViewerEntityLists, resourceSummary } from "./viewer_entity_list_renderer.js";

describe("viewer entity list renderer", () => {
  it("filters only visible agents, sorts targets, and formats resource summaries", () => {
    const lists = buildViewerEntityLists({
      selectedSearch: "alpha",
      entityCollections: () => ({
        agents: [
          { id: "agent-1", location_id: "alpha-base", resources: { ore: 3 } },
          { id: "agent-2", location_id: "hidden-base", resources: { ore: 9 } },
        ],
        locations: [
          { id: "alpha<dock", name: "Alpha <Dock>", resources: { water: 2 } },
        ],
      }),
      isAgentVisibleToCurrentSession: (agentId) => agentId === "agent-1",
    });

    expect(lists.agents.map((agent) => agent.id)).toEqual(["agent-1"]);
    expect(lists.locations.map((location) => location.id)).toEqual(["alpha<dock"]);
    expect(resourceSummary(lists.agents[0].resources)).toBe("ore:3");
  });
});
