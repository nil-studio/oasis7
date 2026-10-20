function isRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function displayableString(value) {
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }
  return null;
}

function finiteNumber(value) {
  if (value == null || value === "") {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizedCode(value) {
  return displayableString(value)?.toLowerCase() || null;
}

function normalizedActionToken(value) {
  return normalizedCode(value)?.replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "") || null;
}

function actionField(action, snakeKey, camelKey) {
  return action?.[snakeKey] ?? action?.[camelKey] ?? null;
}

function recoveryActionForDisposition(value, availableActions, locale, localeText) {
  const actions = Array.isArray(availableActions) ? availableActions : [];
  const recipeToken = normalizedActionToken(value.recipe_id ?? value.recipeId);
  const recipeSuffix = recipeToken?.startsWith("recipe_")
    ? recipeToken.slice("recipe_".length)
    : recipeToken;
  const executeKind = (action) => {
    const rawKind = normalizedCode(actionField(action, "execute_kind", "executeKind"));
    if (rawKind) return rawKind;
    const protocol = normalizedActionToken(actionField(action, "protocol_action", "protocolAction"));
    const actionId = normalizedActionToken(actionField(action, "action_id", "actionId"));
    if (protocol === "request_snapshot" || protocol === "world_request_snapshot") return "request_snapshot";
    if (protocol === "live_control_step") return "step";
    if (protocol === "live_control_play") return "play";
    if (protocol === "prompt_control_apply") return "reprioritize";
    if (protocol === "gameplay_action_submit" && actionId) return "gameplay_action";
    return null;
  };
  const isExecutable = (action) => ["gameplay_action", "request_snapshot", "step", "play", "reprioritize"]
    .includes(executeKind(action));
  const isEnabled = (action) => isExecutable(action) && !displayableString(
    actionField(action, "disabled_reason", "disabledReason"),
  );
  const matchingScheduleAction = actions.find((action) => {
    if (!isEnabled(action)) return false;
    const actionId = normalizedActionToken(actionField(action, "action_id", "actionId"));
    return actionId?.startsWith("schedule_recipe_")
      && recipeSuffix
      && (actionId.endsWith(`_${recipeSuffix}`) || actionId.endsWith(`_${recipeToken}`));
  });
  const snapshotAction = actions.find((action) => {
    const actionId = normalizedActionToken(actionField(action, "action_id", "actionId"));
    const protocolAction = normalizedActionToken(actionField(action, "protocol_action", "protocolAction"));
    return isEnabled(action) && (actionId === "request_snapshot" || protocolAction === "request_snapshot");
  });
  const otherRecoveryAction = actions.find((action) => {
    if (!isEnabled(action) || matchingScheduleAction === action || snapshotAction === action) return false;
    const token = [
      actionField(action, "action_id", "actionId"),
      actionField(action, "protocol_action", "protocolAction"),
      executeKind(action),
    ].map(normalizedActionToken).filter(Boolean).join("_");
    return executeKind(action) === "reprioritize"
      || /repair|rebuild|reroute|replenish|refill|requote|quote|wait/.test(token);
  });
  const selected = matchingScheduleAction || snapshotAction || otherRecoveryAction || {
    actionId: "no_safe_path",
    label: localeText(locale, "暂无安全恢复路径", "No safe recovery path"),
    protocolAction: null,
    targetAgentId: null,
    disabledReason: localeText(
      locale,
      "当前 committed 快照没有可执行的排程、修复、补给或刷新动作；等待下一次复查。",
      "The committed snapshot has no enabled schedule, repair, replenishment, reprioritize, or snapshot action; wait for the next recheck.",
    ),
    executeKind: "none",
  };
  const actionId = displayableString(actionField(selected, "action_id", "actionId")) || "no_safe_path";
  const label = displayableString(selected.label)
    || localeText(locale, "暂无安全恢复路径", "No safe recovery path");
  return {
    actionId,
    label,
    protocolAction: displayableString(actionField(selected, "protocol_action", "protocolAction")),
    targetAgentId: displayableString(actionField(selected, "target_agent_id", "targetAgentId")),
    disabledReason: displayableString(actionField(selected, "disabled_reason", "disabledReason")),
    executeKind: executeKind(selected) || "none",
  };
}

function localizedCodeLabel(code, locale, localeText, labels, fallbackZh, fallbackEn) {
  const label = labels[code];
  return label
    ? localeText(locale, label[0], label[1])
    : localeText(locale, fallbackZh, fallbackEn);
}

function normalizeInputs(value) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter(isRecord)
    .map((stack) => {
      const kind = displayableString(stack.kind ?? stack.material_kind ?? stack.materialKind);
      if (!kind) {
        return null;
      }
      return {
        kind,
        amount: finiteNumber(stack.amount),
      };
    })
    .filter(Boolean);
}

const BLOCKER_LABELS = {
  industrial_termination: ["工业作业已终止", "Industrial job terminated"],
  product_validation: ["产品验证失败", "Product validation failed"],
  product_validation_rejected: ["产品验证失败", "Product validation failed"],
};

const DISPOSITION_LABELS = {
  consumed_lost: ["投入已消费且损失", "Inputs consumed and lost"],
};

export function normalizeFactoryProductionFailureDisposition(
  value,
  locale,
  localeText,
  availableActions = [],
) {
  if (!isRecord(value)) {
    return null;
  }

  const blockerCode = normalizedCode(value.blocker_kind ?? value.blockerKind);
  const dispositionCode = normalizedCode(value.disposition_kind ?? value.dispositionKind);
  const recoveryAction = recoveryActionForDisposition(value, availableActions, locale, localeText);
  const nextRecheck = finiteNumber(value.next_recheck ?? value.nextRecheck);

  return {
    actionId: displayableString(value.action_id ?? value.actionId),
    requesterAgentId: displayableString(value.requester_agent_id ?? value.requesterAgentId),
    factoryId: displayableString(value.factory_id ?? value.factoryId),
    recipeId: displayableString(value.recipe_id ?? value.recipeId),
    blockerKind: localizedCodeLabel(
      blockerCode,
      locale,
      localeText,
      BLOCKER_LABELS,
      "生产受阻",
      "Production blocked",
    ),
    blockerDetail: displayableString(value.blocker_detail ?? value.blockerDetail),
    dispositionKind: localizedCodeLabel(
      dispositionCode,
      locale,
      localeText,
      DISPOSITION_LABELS,
      "处置已记录",
      "Disposition recorded",
    ),
    consumedInputs: normalizeInputs(value.consumed_inputs ?? value.consumedInputs),
    lostInputs: normalizeInputs(value.lost_inputs ?? value.lostInputs),
    consumedPower: finiteNumber(value.consumed_power ?? value.consumedPower),
    lostPower: finiteNumber(value.lost_power ?? value.lostPower),
    nextAction: recoveryAction.label,
    recoveryAction,
    recoveryActionId: recoveryAction.actionId,
    recoveryActionLabel: recoveryAction.label,
    recoveryActionDisabledReason: recoveryAction.disabledReason,
    nextRecheck,
    nextRecheckBoundary: nextRecheck == null
      ? localeText(locale, "下一次 committed 快照", "next committed snapshot")
      : localeText(locale, `世界时刻 ${nextRecheck}`, `world tick ${nextRecheck}`),
  };
}
