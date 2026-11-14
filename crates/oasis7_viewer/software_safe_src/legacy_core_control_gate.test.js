import { beforeEach, describe, expect, it, vi } from "vitest";

describe("legacy core live control gates", () => {
  beforeEach(() => {
    vi.resetModules();
    window.history.replaceState({}, "", "/software_safe.html?test_api=1&locale=en");
    document.body.innerHTML = "";
  });

  it("reads only existing world-view transport counters without initialization or a request",async()=>{
    const core=await import('./legacy_core.js');
    expect(core.readWorldViewTransportStatistics()).toMatchObject({pendingRequests:0});
    expect(Object.keys(core.readWorldViewTransportStatistics()).sort()).toEqual(['pendingRequests','semantics']);
  });

  it("blocks top-level live controls from camelCase gameplay actions", async () => {
    const core = await import("./legacy_core.js");
    await core.initializeSoftwareSafeCore();
    core.injectSnapshot({
      time: 1,
      model: {
        agents: {},
        locations: {},
      },
      player_gameplay: {
        availableActions: [
          {
            actionId: "advance_step",
            label: "Advance one committed step",
            protocolAction: "live_control.step",
            disabledReason: "wait for committed snapshot",
          },
        ],
        next_step_hint: "Wait for committed snapshot before stepping.",
      },
    });
    core.state.controlProfile = "live";

    expect(core.sendControl("step", { count: 1 })).toEqual(
      expect.objectContaining({
        accepted: false,
        stage: "blocked",
        reason: "wait for committed snapshot",
        hint: "Wait for committed snapshot before stepping.",
      }),
    );
  });
});
