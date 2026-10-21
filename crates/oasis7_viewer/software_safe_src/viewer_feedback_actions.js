function actionField(action, snakeKey, camelKey) {
  return action?.[snakeKey] ?? action?.[camelKey] ?? null;
}

function executeKindForAction(actionId, protocolAction) {
  if (protocolAction === "request_snapshot" || protocolAction === "world.request_snapshot") return "request_snapshot";
  if (protocolAction === "live_control.step") return "step";
  if (protocolAction === "live_control.play") return "play";
  if (protocolAction === "agent_chat") return "agent_chat";
  if (protocolAction === "prompt_control.apply") return "reprioritize";
  if (protocolAction !== "gameplay_action.submit") return "unsupported";
  if (actionId === "claim_first_agent") return "claim_first_agent";
  if (actionId === "claim_starter_oc") return "claim_starter_oc";
  return "gameplay_action";
}

export function normalizeViewerAvailableActionFields(action) {
  const actionId = actionField(action, "action_id", "actionId");
  const protocolAction = actionField(action, "protocol_action", "protocolAction");
  const targetAgentId = actionField(action, "target_agent_id", "targetAgentId");
  const disabledReason = actionField(action, "disabled_reason", "disabledReason");
  return {
    actionId,
    label: action?.label || null,
    protocolAction,
    targetAgentId,
    disabledReason,
  };
}

export function normalizeViewerAvailableActions({
  gameplay,
  locale,
  localeText,
  agentExists,
  emptyEntityBlocker,
  firstAgentClaimSyncPending,
}) {
  const rawActions = Array.isArray(gameplay?.available_actions)
    ? gameplay.available_actions
    : Array.isArray(gameplay?.availableActions)
      ? gameplay.availableActions
      : [];

  return rawActions.map((action) => {
    const {
      actionId,
      label,
      protocolAction,
      targetAgentId,
      disabledReason,
    } = normalizeViewerAvailableActionFields(action);
    const starterOcMissingAgentReason = actionId === "claim_starter_oc" && !agentExists(targetAgentId)
      ? localeText(
        locale,
        "第一个 Agent 认领已提交，正在等待 committed 快照创建 Agent；请先推进或刷新一次。",
        "First Agent claim submitted; waiting for the committed snapshot to create the Agent. Advance or refresh once first.",
      )
      : null;
    const shouldKeepRuntimeDisabledReason =
      protocolAction === "request_snapshot"
      || protocolAction === "world.request_snapshot"
      || (firstAgentClaimSyncPending && protocolAction === "live_control.step")
      || (firstAgentClaimSyncPending && protocolAction === "live_control.play")
      || actionId === "claim_first_agent"
      || actionId === "claim_starter_oc";

    return {
      actionId,
      label,
      protocolAction,
      targetAgentId,
      disabledReason: shouldKeepRuntimeDisabledReason
        ? disabledReason || starterOcMissingAgentReason || null
        : disabledReason || emptyEntityBlocker?.disabledReason || null,
      executeKind: executeKindForAction(actionId, protocolAction),
    };
  });
}

export function normalizeGameplayActionRequest(action) {
  if (!action || typeof action !== "object") {
    return null;
  }
  const normalized = {
    ...action,
    protocol_action: action.protocol_action || action.protocolAction || null,
    action_id: action.action_id || action.actionId || null,
    target_agent_id: action.target_agent_id || action.targetAgentId || null,
    actor_agent_id: action.actor_agent_id || action.actorAgentId || null,
    disabled_reason: action.disabled_reason || action.disabledReason || null,
  };
  return normalized;
}
