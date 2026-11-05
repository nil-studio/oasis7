import { describe, expect, it } from "vitest";
import { createLiveControlStateController } from "./viewer_live_control_state.js";

function fixture() {
  const state = { lastControlFeedback: null };
  const pending = new Map();
  const controller = createLiveControlStateController(state, pending);
  controller.reset();
  const hello = (capabilities = ["live_control_state_v1"]) => controller.hello({ version: 2, control_profile: "live", capabilities });
  hello();
  const observe = (revision, paused, request_id = null, epoch = "ab".repeat(32)) => controller.receive({
    scope: "viewer_new_decisions", epoch, revision, paused, ...(request_id == null ? {} : { request_id }),
  });
  const request = (id, action) => {
    const feedback = { requestId: id, action, stage: "queued" };
    pending.set(id, feedback); controller.sent(feedback); state.lastControlFeedback = feedback;
    return feedback;
  };
  return { state, pending, controller, observe, request, hello };
}

describe("Viewer control observation", () => {
  it("a late confirmation completes only its own request and cannot roll back current state", () => {
    const f = fixture();
    f.observe(1, false);
    const pause = f.request(11, "pause");
    const play = f.request(12, "play");
    f.observe(3, false, 12);
    f.observe(2, true, 11);
    expect(pause.stage).toBe("completed_applied");
    expect(play.stage).toBe("completed_applied");
    expect(f.state.lastControlFeedback.requestId).toBe(12);
    expect(f.state.liveControlState).toMatchObject({ verified: true, revision: 3, paused: false });
    expect(f.pending.size).toBe(0);
  });

  it("an old confirmation does not finish the newer pending request", () => {
    const f = fixture();
    f.observe(1, false);
    f.request(11, "pause");
    f.request(12, "play");
    f.observe(2, true, 11);
    expect(f.state.liveControlPendingRequestId).toBe(12);
    expect(f.state.lastControlFeedback).toMatchObject({ requestId: 12, stage: "queued" });
    expect(f.pending.has(12)).toBe(true);
    f.observe(3, false, 12);
    expect(f.state.liveControlPendingRequestId).toBe(null);
  });

  it("request mode mismatch cannot confirm an unrelated control", () => {
    const f = fixture();
    f.observe(1, false);
    f.request(11, "pause");
    f.observe(2, false, 11);
    expect(f.pending.get(11).stage).toBe("queued");
    expect(f.state.liveControlPendingRequestId).toBe(11);
  });

  it("reset invalidates connection observations and in-flight controls", () => {
    const f = fixture();
    f.observe(1, false);
    const pause = f.request(11, "pause");
    f.controller.reset();
    expect(pause.stage).toBe("unknown");
    expect(f.pending.size).toBe(0);
    expect(f.state.liveControlState.verified).toBe(false);
    f.observe(2, true, 11);
    expect(f.state.liveControlState.verified).toBe(false);
    f.hello();
    f.observe(1, true, null, "cd".repeat(32));
    expect(f.state.liveControlState).toMatchObject({ verified: true, epoch: "cd".repeat(32), paused: true });
  });

  it("same revision conflicts remain unconfirmed until a higher observation", () => {
    const f = fixture();
    f.observe(2, false);
    f.observe(2, true);
    f.observe(2, false);
    expect(f.state.liveControlState.verified).toBe(false);
    f.observe(3, true);
    expect(f.state.liveControlState).toMatchObject({ verified: true, revision: 3, paused: true });
    f.observe(0, false);
    expect(f.state.liveControlState.verified).toBe(false);
  });

  it("a blocked original completes waiting without manufacturing a new policy", () => {
    const f = fixture();
    f.observe(1, true);
    f.request(11, "play");
    f.controller.blocked({ request_id: 11, status: "blocked" });
    expect(f.state.liveControlPendingRequestId).toBe(null);
    expect(f.state.liveControlState).toMatchObject({ verified: true, revision: 1, paused: true });
  });
});
