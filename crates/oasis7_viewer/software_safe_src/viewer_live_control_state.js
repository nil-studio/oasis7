// A current-connection observation, never a world action or progress receipt.
export const LIVE_CONTROL_STATE_CAPABILITY = "live_control_state_v1";
export function createLiveControlStateController(state, pending) {
  let generation = 0;
  let supported = false;
  let epoch = null;
  let revision = 0;
  let paused = null;
  let conflictedRevision = null;
  function publish(verified) {
    state.liveControlState = { supported, verified, epoch, revision, paused: verified ? paused : null };
  }
  return {
    reset() {
      generation += 1;
      supported = false; epoch = null; revision = 0; paused = null; conflictedRevision = null;
      state.liveControlPendingRequestId = null;
      for (const [id, feedback] of pending) {
        if (feedback.liveControlGeneration == null) continue;
        feedback.stage = "unknown";
        feedback.reason = "connection changed before Viewer control confirmation";
        if (state.lastControlFeedback?.requestId === id) state.lastControlFeedback = feedback;
        pending.delete(id);
      }
      publish(false);
    },
    hello(message) {
      supported = message.control_profile === "live" && Number(message.version ?? 2) >= 2
        && Array.isArray(message.capabilities) && message.capabilities.includes(LIVE_CONTROL_STATE_CAPABILITY);
      publish(false);
    },
    sent(feedback) {
      if (!["play", "pause"].includes(feedback.action)) return;
      feedback.liveControlGeneration = generation;
      if (supported) state.liveControlPendingRequestId = feedback.requestId;
    },
    blocked(ack) {
      const feedback = pending.get(ack?.request_id);
      if (ack?.status !== "blocked" || feedback?.liveControlGeneration !== generation
        || !["play", "pause"].includes(feedback?.action)) return;
      if (state.liveControlPendingRequestId === ack.request_id) state.liveControlPendingRequestId = null;
    },
    receive(observation) {
      if (!supported) return;
      if (!observation || observation.scope !== "viewer_new_decisions"
        || !/^[0-9a-f]{64}$/.test(observation.epoch ?? "")
        || !Number.isSafeInteger(observation.revision) || observation.revision < 1
        || typeof observation.paused !== "boolean"
        || (observation.request_id != null && (!Number.isSafeInteger(observation.request_id) || observation.request_id < 0))) {
        publish(false); return;
      }
      if (epoch != null && observation.epoch !== epoch) return;
      if (epoch == null) epoch = observation.epoch;
      if (observation.revision === revision && observation.paused !== paused) {
        conflictedRevision = revision; publish(false); return;
      }
      if (conflictedRevision != null && observation.revision <= conflictedRevision) return;
      if (observation.revision >= revision) {
        revision = observation.revision; paused = observation.paused; conflictedRevision = null;
        publish(true);
      }
      const id = observation.request_id;
      const feedback = pending.get(id);
      if (!feedback || feedback.liveControlGeneration !== generation
        || !["play", "pause"].includes(feedback.action)
        || observation.paused !== (feedback.action === "pause")) return;
      feedback.stage = "completed_applied";
      feedback.reason = null;
      feedback.effect = observation.paused ? "Viewer new decisions paused" : "Viewer new decisions enabled";
      if (state.lastControlFeedback?.requestId === id) state.lastControlFeedback = feedback;
      pending.delete(id);
      if (state.liveControlPendingRequestId === id) state.liveControlPendingRequestId = null;
    },
  };
}
