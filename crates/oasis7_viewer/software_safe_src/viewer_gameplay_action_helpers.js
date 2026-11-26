import * as core from "./legacy_core.js";

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

function normalizedId(value) {
  return String(value || "").trim();
}

export function gameplayStatusBadgeClass(status) {
  return status === "blocked"
    ? "badge badge--warn"
    : status === "branch_ready"
      ? "badge badge--good"
      : "badge badge--accent";
}

export function gameplayStageToneClass(status) {
  return status === "blocked"
    ? "hero-focus-card__value hero-focus-card__value--warn"
    : status === "branch_ready"
      ? "hero-focus-card__value hero-focus-card__value--good"
      : "hero-focus-card__value hero-focus-card__value--accent";
}

export function gameplayStageLabel(status, locale) {
  if (status === "blocked") {
    return tr(locale, "当前受阻", "Blocked Now");
  }
  if (status === "branch_ready") {
    return tr(locale, "可以推进", "Ready to Act");
  }
  if (status === "active") {
    return tr(locale, "正在推进", "In Motion");
  }
  if (status === "completed") {
    return tr(locale, "阶段完成", "Stage Complete");
  }
  return status || tr(locale, "等待同步", "Waiting for Sync");
}

export function goalExecutionBadgeClass(state) {
  return state === "blocked" || state === "rejected"
    ? "badge badge--warn"
    : state === "completed"
      ? "badge badge--good"
      : "badge badge--accent";
}

const PENDING_GAMEPLAY_FEEDBACK_STAGES = new Set(["accepted", "submitted", "queued", "ack", "registering", "signing", "sent", "received", "pending", "committed", "unknown"]);
const GAMEPLAY_ACTION_BUSY_STAGES = new Set(["queued", "registering", "signing", "sent", "received", "pending", "unknown"]);
const GAMEPLAY_ACTION_PENDING_MIN_MS = 900;
let gameplayActionPendingClearTimer = null;

export function gameplayActionKey(action) {
  if (!action) {
    return "";
  }
  const actionId = normalizedId(action.actionId || action.action_id || action.protocolAction || action.protocol_action || action.executeKind);
  const targetAgentId = normalizedId(action.targetAgentId || action.target_agent_id || action.actorAgentId || action.actor_agent_id);
  return `${actionId}::${targetAgentId}`;
}

export function gameplayActionBlockedReasonId(action) {
  const key = gameplayActionKey(action).replace(/[^a-zA-Z0-9_-]+/g, "-");
  return `gameplay-action-${key || "unknown"}-blocked-reason`;
}

export function gameplayActionFeedbackMatches(action, feedback = core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback)) {
  if (!action || !feedback || feedback.kind !== "gameplay_action") {
    return false;
  }
  const actionId = normalizedId(action.actionId || action.action_id || action.protocolAction || action.protocol_action || action.executeKind);
  const feedbackAction = normalizedId(feedback.action);
  if (!actionId || !feedbackAction || !feedbackAction.includes(actionId)) {
    return false;
  }
  const targetAgentId = normalizedId(action.targetAgentId || action.target_agent_id || action.actorAgentId || action.actor_agent_id);
  const feedbackAgentId = normalizedId(feedback.agentId || feedback.targetAgentId);
  return !targetAgentId || !feedbackAgentId || targetAgentId === feedbackAgentId;
}

export function clearGameplayActionPending(action = null) {
  if (gameplayActionPendingClearTimer != null) {
    window.clearTimeout(gameplayActionPendingClearTimer);
    gameplayActionPendingClearTimer = null;
  }
  if (action && core.state.gameplayActionPending.actionKey !== gameplayActionKey(action)) {
    return;
  }
  core.state.gameplayActionPending.actionKey = null;
  core.state.gameplayActionPending.label = null;
  core.state.gameplayActionPending.startedAtUnixMs = null;
  core.requestRender();
}

export function markGameplayActionPending(action, label) {
  const key = gameplayActionKey(action);
  if (!key) {
    return;
  }
  if (gameplayActionPendingClearTimer != null) {
    window.clearTimeout(gameplayActionPendingClearTimer);
  }
  core.state.gameplayActionPending.actionKey = key;
  core.state.gameplayActionPending.label = label || normalizedId(action.label || action.actionId || action.executeKind);
  core.state.gameplayActionPending.startedAtUnixMs = Date.now();
  gameplayActionPendingClearTimer = window.setTimeout(() => {
    gameplayActionPendingClearTimer = null;
    if (!gameplayActionFeedbackMatches(action) || !GAMEPLAY_ACTION_BUSY_STAGES.has(normalizedId(core.state.lastGameplayActionFeedback?.stage).toLowerCase())) {
      clearGameplayActionPending(action);
    }
  }, GAMEPLAY_ACTION_PENDING_MIN_MS);
  core.requestRender();
}

export function gameplayActionFeedbackInFlight(action) {
  const feedback = core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback);
  return gameplayActionFeedbackMatches(action, feedback)
    && GAMEPLAY_ACTION_BUSY_STAGES.has(normalizedId(feedback.stage).toLowerCase());
}

export function gameplayActionPendingFor(action) {
  const key = gameplayActionKey(action);
  return Boolean(key && core.state.gameplayActionPending.actionKey === key) || gameplayActionFeedbackInFlight(action);
}

export function isPendingFirstAgentClaimSync(action, gameplay) {
  if (action?.actionId !== "claim_first_agent") {
    return false;
  }
  if (gameplay?.blockerKind !== "runtime_snapshot_empty_entities") {
    return false;
  }
  const feedback = gameplay?.recentFeedback || core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback);
  const feedbackAction = String(feedback?.action || "").trim();
  const feedbackStage = String(feedback?.stage || "").trim().toLowerCase();
  return feedbackAction.includes("claim_first_agent") && PENDING_GAMEPLAY_FEEDBACK_STAGES.has(feedbackStage);
}

export function gameplayActionControlBoundaryReason(action, locale) {
  const actionId = normalizedId(action?.actionId || action?.action_id);
  const protocolAction = normalizedId(action?.protocolAction || action?.protocol_action);
  const targetAgentId = normalizedId(action?.targetAgentId || action?.target_agent_id);
  if (!targetAgentId || actionId === "claim_first_agent") {
    return null;
  }
  if (protocolAction === "request_snapshot" || protocolAction === "live_control.step" || protocolAction === "live_control.play" || protocolAction === "live_control.pause") {
    return null;
  }
  const boundAgentId = normalizedId(core.state.auth.boundAgentId);
  if (!boundAgentId) {
    return tr(
      locale,
      "当前账号尚未绑定 Agent；这个动作来自共享世界快照，不能作为当前账号动作执行。",
      "The current account has no bound Agent; this action came from the shared world snapshot and cannot be executed as the current account.",
    );
  }
  if (targetAgentId !== boundAgentId) {
    return tr(
      locale,
      `这个动作目标是 ${targetAgentId}，但当前账号绑定的是 ${boundAgentId}。`,
      `This action targets ${targetAgentId}, but the current account is bound to ${boundAgentId}.`,
    );
  }
  return null;
}

export function gameplayActionDisabledReason(action, gameplay, locale) {
  if (action?.disabledReason) {
    return action.disabledReason;
  }
  if (isPendingFirstAgentClaimSync(action, gameplay)) {
    return tr(
      locale,
      "认领已提交，正在等待链上 committed 快照同步。",
      "Claim submitted; waiting for the committed chain snapshot to sync.",
    );
  }
  const controlBoundaryReason = gameplayActionControlBoundaryReason(action, locale);
  if (controlBoundaryReason) {
    return controlBoundaryReason;
  }
  return null;
}

export function gameplayActionButtonLabel(action, locale) {
  if (action.actionId === "claim_first_agent") {
    return tr(locale, "认领第一个 Agent", "Claim First Agent");
  }
  if (action.actionId === "claim_starter_oc") {
    return tr(locale, "领取初始 OC", "Claim Starter OC");
  }
  if (action.executeKind === "claim_agent") {
    return tr(locale, "认领 Agent", "Claim Agent");
  }
  if (action.executeKind === "request_snapshot") {
    return tr(locale, "刷新快照", "Refresh Snapshot");
  }
  if (action.executeKind === "step") {
    return tr(locale, "推进一步", "Advance One Step");
  }
  if (action.executeKind === "play") {
    return tr(locale, "恢复实时推进", "Resume Live Play");
  }
  if (action.executeKind === "pause") {
    return tr(locale, "暂停实时推进", "Pause Live Play");
  }
  if (action.executeKind === "agent_chat") {
    return tr(locale, "切到聊天面板", "Use Chat Panel");
  }
  return tr(locale, "提交玩法动作", "Submit Gameplay Action");
}

export function gameplayActionBusyLabel(action, locale) {
  const stage = core.state.lastGameplayActionFeedback?.stage;
  if (gameplayActionFeedbackMatches(action) && stage === "unknown") return tr(locale, "结果待确认，请刷新", "Result unknown; refresh");
  if (gameplayActionFeedbackMatches(action) && ["received", "pending"].includes(stage)) return tr(locale, "等待权威结果...", "Waiting for result...");
  if (action?.executeKind === "request_snapshot") {
    return tr(locale, "刷新中...", "Refreshing...");
  }
  if (action?.executeKind === "step") {
    return tr(locale, "推进中...", "Advancing...");
  }
  if (action?.executeKind === "play") {
    return tr(locale, "恢复中...", "Resuming...");
  }
  if (action?.executeKind === "pause") {
    return tr(locale, "暂停中...", "Pausing...");
  }
  if (action?.actionId === "claim_starter_oc") {
    return tr(locale, "确认中...", "Confirming...");
  }
  if (action?.actionId === "claim_first_agent" || action?.executeKind === "claim_agent") {
    return tr(locale, "提交中...", "Submitting...");
  }
  return tr(locale, "处理中...", "Working...");
}

export function gameplayActionDisplayLabel(action, locale, fallback = null) {
  if (gameplayActionPendingFor(action)) {
    return gameplayActionBusyLabel(action, locale);
  }
  return fallback ?? gameplayActionButtonLabel(action, locale);
}

export function gameplayActionButtonClass(action) {
  return gameplayActionPendingFor(action) ? "is-loading" : "";
}

export function gameplayActionButtonBusyAttrs(action) {
  return gameplayActionPendingFor(action) ? "true" : "false";
}

export function gameplayActionButtonDisabled(action, gameplay, locale) {
  return Boolean(gameplayActionDisabledReason(action, gameplay, locale) || gameplayActionPendingFor(action));
}

export function gameplayActionTestId(action, role = "available") {
  if (role === "recommended") {
    return "viewer-playthrough-action-recommended";
  }
  if (action?.executeKind === "request_snapshot") {
    return "viewer-available-action-request-snapshot";
  }
  if (action?.executeKind === "step") {
    return "viewer-available-action-step";
  }
  if (action?.executeKind === "play") {
    return "viewer-available-action-play";
  }
  if (action?.executeKind === "pause") {
    return "viewer-available-action-pause";
  }
  const raw = action?.actionId || action?.protocolAction || action?.executeKind || "unknown";
  const safe = String(raw).trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "unknown";
  return `viewer-playthrough-action-${safe}`;
}

export function gameplayActionDetail(action, gameplay, locale) {
  if (action?.actionId === "claim_first_agent") {
    return action?.disabledReason
      || tr(locale, "新用户空世界入口：提交后会创建并绑定第一个 starter Agent。", "New-user empty-world entry: submitting creates the first starter Agent.");
  }
  if (action?.actionId === "claim_starter_oc") {
    return action?.disabledReason
      || tr(locale, "领取一次性初始 OC，解锁第一次 LLM/Agent chat。", "Claim one-time starter OC to unlock the first LLM/Agent chat.");
  }
  return action?.playerDetail
    || action?.disabledReason
    || gameplay?.economicSurface?.repairAction
    || gameplay?.narrativeNextStep
    || tr(locale, "可以直接从正式网页入口执行。", "Playable directly from the formal Web entry.");
}
