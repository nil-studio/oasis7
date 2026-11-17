function isRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function displayableString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

const PROMPT_RESULT_STATUSES = new Set(["accepted", "applied", "stale", "rejected", "blocked"]);

export function createViewerPromptFeedbackModule({ feedbackBadgeClass, isLocaleZh, localeText }) {
  function promptResultStatus(feedback) {
    const status = String(feedback?.response?.status || "").trim().toLowerCase();
    return PROMPT_RESULT_STATUSES.has(status) ? status : null;
  }

  function redactPromptControlResponse(response) {
    if (!isRecord(response) || String(response.value_visibility || "").trim().toLowerCase() !== "hidden") {
      return response;
    }
    const safeKeys = [
      "code",
      "message",
      "request_id",
      "operation",
      "preview",
      "status",
      "value_visibility",
      "reason_code",
      "next_step",
      "idempotent_replay",
    ];
    return safeKeys.reduce((safe, key) => {
      if (response[key] !== undefined && response[key] !== null) {
        safe[key] = response[key];
      }
      return safe;
    }, {});
  }

  function semanticPromptFeedbackCode(feedback) {
    const responseStatus = String(feedback?.response?.status || "").trim().toLowerCase();
    const enhanced = PROMPT_RESULT_STATUSES.has(responseStatus)
      || feedback?.response?.request_id != null
      || feedback?.response?.authority_epoch != null;
    if (!enhanced) {
      return null;
    }
    return displayableString(feedback?.response?.reason_code)
      || displayableString(feedback?.response?.code);
  }

  function promptFeedbackStatusLabel(status, locale) {
    const labels = {
      accepted: localeText(locale, "已接受", "Accepted"),
      applied: localeText(locale, "已应用", "Applied"),
      stale: localeText(locale, "版本已过期", "Stale"),
      rejected: localeText(locale, "已拒绝", "Rejected"),
      blocked: localeText(locale, "已阻塞", "Blocked"),
    };
    return labels[status] || status;
  }

  function promptFeedbackNextStep(response, locale) {
    const nextStep = displayableString(response?.next_step);
    const nextStepLabels = {
      no_action_required: localeText(locale, "无需进一步操作。", "No further action is required."),
      continue_runtime: localeText(locale, "继续观察当前运行时。", "Continue observing the current runtime."),
      refresh_version_and_retry: localeText(locale, "刷新当前版本，保留草稿后重新编辑。", "Refresh the current version, keep the draft, and re-edit before retrying."),
      reauthenticate_and_refresh_binding: localeText(locale, "重新认证并刷新当前绑定后再试。", "Re-authenticate and refresh the current binding before retrying."),
      refresh_authority_and_retry_with_new_request_id: localeText(locale, "刷新权限后使用新的请求编号重试。", "Refresh authority, then retry with a new request id."),
      correct_request_and_retry: localeText(locale, "修正请求内容后重试。", "Correct the request and retry."),
      switch_to_supported_mode: localeText(locale, "切换到支持提示词控制的运行模式。", "Switch to a runtime mode that supports prompt control."),
      refresh_and_retry: localeText(locale, "刷新当前状态后重试。", "Refresh the current state and retry."),
    };
    if (nextStep && nextStepLabels[nextStep]) {
      return nextStepLabels[nextStep];
    }
    if (nextStep) {
      return nextStep.replaceAll("_", " ");
    }
    const status = String(response?.status || "").trim().toLowerCase();
    const reasonCode = String(response?.reason_code || response?.code || "").trim().toLowerCase();
    if (reasonCode === "request_id_conflict") {
      return nextStepLabels.refresh_authority_and_retry_with_new_request_id;
    }
    if (reasonCode === "version_conflict") {
      return nextStepLabels.refresh_version_and_retry;
    }
    if (status === "stale") {
      return nextStepLabels.refresh_version_and_retry;
    }
    if (status === "blocked") {
      return nextStepLabels.reauthenticate_and_refresh_binding;
    }
    return null;
  }

  function humanizePromptField(field) {
    return String(field || "").trim().replaceAll("_", " ");
  }

  function summarizeAppliedFields(feedback) {
    const fields = Array.isArray(feedback?.response?.applied_fields)
      ? feedback.response.applied_fields.map(humanizePromptField).filter(Boolean)
      : [];
    return fields.length ? fields.join(", ") : null;
  }

  function describePromptResult(feedback, locale) {
    const resultStatus = promptResultStatus(feedback);
    if (!resultStatus) {
      return null;
    }
    const response = feedback.response || {};
    const version = Number(response.version);
    const versionLabel = Number.isFinite(version) ? `v${Math.max(0, Math.floor(version))}` : null;
    const operation = String(response.operation || feedback.action || "prompt").trim().toLowerCase();
    const replay = response.idempotent_replay === true
      ? (isLocaleZh(locale) ? "这是已完成收据的重放，没有产生第二次变更。" : "This is a replay of the completed receipt; no second mutation was made.")
      : null;
    const description = {
      label: promptFeedbackStatusLabel(resultStatus, locale),
      summary: null,
      detail: null,
      code: semanticPromptFeedbackCode(feedback),
      diagnostics: displayableString(response.message),
      badgeClass: feedbackBadgeClass(feedback),
    };
    if (resultStatus === "accepted") {
      description.summary = response.preview === true
        ? (isLocaleZh(locale) ? "提示词预览已接受，尚未应用变更。" : "Prompt preview accepted; no changes were applied.")
        : (isLocaleZh(locale) ? "提示词请求已接受。" : "Prompt request accepted.");
    } else if (resultStatus === "applied") {
      const action = operation === "rollback"
        ? (isLocaleZh(locale) ? "提示词回滚已在当前运行时应用" : "Prompt rollback applied to the current runtime")
        : (isLocaleZh(locale) ? "提示词已在当前运行时应用" : "Prompt applied to the current runtime");
      description.summary = versionLabel ? `${action} (${versionLabel})。` : `${action}。`;
    } else if (resultStatus === "stale") {
      description.summary = isLocaleZh(locale) ? "提示词草稿基于过期版本。" : "The prompt draft is based on a stale version.";
    } else if (resultStatus === "rejected") {
      description.summary = isLocaleZh(locale) ? "提示词请求被运行时拒绝。" : "The runtime rejected the prompt request.";
    } else {
      description.summary = isLocaleZh(locale) ? "提示词控制已阻塞。" : "Prompt control is blocked.";
    }
    const details = [];
    if (resultStatus === "applied" && Array.isArray(response.applied_fields) && response.applied_fields.length) {
      details.push(isLocaleZh(locale)
        ? `已应用字段：${summarizeAppliedFields(feedback)}。`
        : `Applied fields: ${summarizeAppliedFields(feedback)}.`);
    }
    if (resultStatus === "applied" && response.applied_scope) {
      details.push(isLocaleZh(locale)
        ? `作用范围：${response.applied_scope}；持久化：${response.persistence_scope || "none"}；同步：${response.sync_scope || "none"}。`
        : `Scope: ${response.applied_scope}; persistence: ${response.persistence_scope || "none"}; sync: ${response.sync_scope || "none"}.`);
    }
    const nextStep = promptFeedbackNextStep(response, locale);
    if (nextStep) {
      details.push(isLocaleZh(locale) ? `下一步：${nextStep}` : `Next step: ${nextStep}`);
    }
    if (replay) {
      details.push(replay);
    }
    description.detail = details.join(" ") || null;
    return description;
  }

  function describePromptVersionState(state, feedback = state?.lastPromptFeedback, locale = state?.uiLocale) {
    const currentVersion = Math.max(0, Math.floor(Number(state?.promptDraft?.currentVersion || 0)));
    const nextRollbackTargetVersion = Math.max(
      0,
      Math.floor(Number(state?.promptDraft?.rollbackTargetVersion || 0)),
    );
    const responseVersion = Number(feedback?.response?.version);
    const ackVersion = Number.isFinite(responseVersion) ? Math.max(0, Math.floor(responseVersion)) : currentVersion;
    const responseRollbackVersion = Number(feedback?.response?.rolled_back_to_version);
    const restoredFromVersion =
      feedback?.stage === "rollback_ack" && Number.isFinite(responseRollbackVersion)
        ? Math.max(0, Math.floor(responseRollbackVersion))
        : null;
    const summary = restoredFromVersion == null
      ? (isLocaleZh(locale)
          ? `当前生效 prompt 版本是 ${formatPromptVersionLabel(currentVersion)}。`
          : `Active prompt version is ${formatPromptVersionLabel(currentVersion)}.`)
      : (isLocaleZh(locale)
          ? `当前生效 prompt 版本是 ${formatPromptVersionLabel(currentVersion)}；内容恢复自 ${formatPromptVersionLabel(restoredFromVersion)}。`
          : `Active prompt version is ${formatPromptVersionLabel(currentVersion)}; content was restored from ${formatPromptVersionLabel(restoredFromVersion)}.`);
    const detail = restoredFromVersion == null
      ? (isLocaleZh(locale)
          ? `回滚输入框默认指向下一次目标 ${formatPromptVersionLabel(nextRollbackTargetVersion)}。`
          : `The rollback input defaults to the next target ${formatPromptVersionLabel(nextRollbackTargetVersion)}.`)
      : (isLocaleZh(locale)
          ? `这次回滚生成了新的保存版本 ${formatPromptVersionLabel(ackVersion)}。下面输入框现在指向下一次目标 ${formatPromptVersionLabel(nextRollbackTargetVersion)}，不是刚恢复的版本。`
          : `The rollback created a new saved version ${formatPromptVersionLabel(ackVersion)}. The input below now points to the next target ${formatPromptVersionLabel(nextRollbackTargetVersion)}, not the restored version.`);
    return {
      currentVersion,
      nextRollbackTargetVersion,
      ackVersion,
      restoredFromVersion,
      summary,
      detail,
    };
  }

  return {
    describePromptResult,
    describePromptVersionState,
    promptResultStatus,
    redactPromptControlResponse,
    semanticPromptFeedbackCode,
  };
}

export function formatPromptVersionLabel(value) {
  return `v${Math.max(0, Math.floor(Number(value || 0)))}`;
}
