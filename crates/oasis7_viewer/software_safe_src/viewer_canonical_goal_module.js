import { canonicalAgentChatView, currentCanonicalReorgEpoch } from "./viewer_canonical_owner_read_module.js";
// Pending transport receipts cannot publish a committed goal or enqueue provider work.
export function scheduleCanonicalGoalPending({ ack, feedback, state, clone, requestSnapshotSafe,
  sameAgentChatFeedback, failPendingAgentChatAck, sendJson, scheduleAgentChatAckTimeout, getSocket }) {
  feedback.stage = "pending";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.effect = "goal submitted; waiting for canonical commit";
  feedback.response = clone(ack);
  requestSnapshotSafe();
  if (feedback.canonicalRetryCount >= 29) {
    scheduleAgentChatAckTimeout(feedback);
    return null;
  }
  feedback.canonicalRetryCount += 1;
  return window.setTimeout(() => {
    if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || feedback.stage !== "pending") return;
    const request = feedback.canonicalRequest;
    const socket = getSocket();
    const canonical = canonicalAgentChatView(state);
    const authorityChanged = state.worldFeed?.worldId !== request.world_id || currentCanonicalReorgEpoch(state) !== request.reorg_epoch
      || state.worldFeed?.stale === true || canonical?.canonical_authority?.branch_id !== request.canonical_authority?.branch_id
      || canonical?.canonical_authority?.agent_identity_generation !== request.canonical_authority?.agent_identity_generation;
    if (authorityChanged || request.auth.player_id !== state.auth.playerId || request.auth.public_key !== state.auth.publicKey
      || request.agent_id !== state.auth.boundAgentId || !socket || socket.readyState !== WebSocket.OPEN) {
      failPendingAgentChatAck("canonical goal identity or connection changed"); return;
    }
    sendJson({ type: "agent_chat", request });
    scheduleAgentChatAckTimeout(feedback);
  }, 1500);
}

export function canonicalGoalMarkup(state, agentId, escapeHtml) {
  const goal = authenticatedCanonicalGoal(state, agentId);
  return goal ? `<p data-canonical-agent-goal>Current goal: ${escapeHtml(goal.message)}</p>` : "";
}

export function applyCommittedChatAck({ ack, feedback, state, clone, pushChatHistory }) {
  feedback.stage = "ack";
  feedback.ok = true;
  feedback.accepted = true;
  feedback.reason = null;
  feedback.effect = `chat accepted at tick ${Number(ack?.accepted_at_tick || state.logicalTime)}`;
  feedback.response = clone(ack);
  state.lastChatFeedback = feedback;
  pushChatHistory({
    id: `chat-ack-${feedback.id}`,
    source: "player",
    agentId: ack?.agent_id || feedback.agentId || null,
    message: feedback.pendingMessage || "",
    tick: Number(ack?.accepted_at_tick || state.logicalTime || 0),
    speaker: feedback.pendingPlayerId || state.auth.playerId || null,
    playerId: feedback.pendingPlayerId || state.auth.playerId || null,
    targetAgentId: ack?.agent_id || feedback.agentId || null,
    intentSeq: ack?.intent_seq || null,
  });
}

export function authenticatedCanonicalGoal(state, agentId) {
  const canonical = canonicalAgentChatView(state);
  if (!canonical?.goal || typeof canonical.goal.message !== "string"
    || !["accepted", "blocked"].includes(canonical.goal.status)
    || typeof canonical.goal.intent_id !== "string" || !canonical.goal.intent_id
    || canonical.current_intent_id !== canonical.goal.intent_id
    || state.worldFeed?.stale === true || canonical.world_id !== state.worldFeed?.worldId
    || canonical.reorg_epoch !== currentCanonicalReorgEpoch(state) || canonical.agent_id !== agentId || canonical.player_id !== state.auth.playerId
    || canonical.public_key !== state.auth.publicKey) return null;
  return canonical.goal;
}

export function canonicalAckMatches(ack, request) {
  return ack?.agent_id === request.agent_id && ack?.player_id === request.player_id
    && ack?.intent_seq === request.intent_seq && ack?.intent_tick === request.intent_tick
    && ack?.auth_nonce === request.auth?.nonce;
}
