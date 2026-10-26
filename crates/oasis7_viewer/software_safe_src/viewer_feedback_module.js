import { normalizeRecipeSubstitution } from "./recipe_substitution.js";
import { feedbackForScope } from "./viewer_feedback_scope.js";
import { projectGameplayReceiptFeedback, firstClaimAwaitingOutcome, gameplayExecutionStateLabel } from "./viewer_gameplay_receipt_feedback.js";
import { normalizeIndustrialStarterProfile } from "./industrial_starter_outcome.js";
import { normalizeViewerAvailableActions } from "./viewer_feedback_actions.js";
import { buildGameplayEconomicSurface } from "./viewer_feedback_gameplay_economics.js";
import { buildValidationUnlockPreviewDisplayModel } from "./viewer_validation_unlock_preview_display_model.js";
import { buildWaitResolutionQuoteDisplayModel } from "./viewer_wait_resolution_quote_display_model.js";
import { normalizeFirstDeliveryPreview } from "./first_delivery_preview_display_model.js";
import { normalizeFactoryProductionFailureDisposition } from "./viewer_factory_failure_disposition_display_model.js";
import { createViewerPromptFeedbackModule } from "./viewer_prompt_feedback_module.js";
function isRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}
function displayableStrings(value) {
  return Array.isArray(value)
    ? value.filter((entry) => typeof entry === "string").map((entry) => entry.trim()).filter(Boolean)
    : [];
}
function displayableString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}
export function createViewerFeedbackModule({
  clone,
  feedbackBadgeClass,
  hostedActionPolicy,
  isAgentVisibleToCurrentSession,
  isLocaleZh,
  localeText,
  state,
}) {
  const promptFeedback = createViewerPromptFeedbackModule({ feedbackBadgeClass, isLocaleZh, localeText });
  function snapshotControlFeedback(feedback) {
    if (!feedback) return null;
    return {
      id: feedback.id,
      action: feedback.action,
      accepted: feedback.accepted,
      stage: feedback.stage,
      reason: feedback.reason,
      hint: feedback.hint,
      effect: feedback.effect,
      deltaLogicalTime: feedback.deltaLogicalTime || 0,
      deltaEventSeq: feedback.deltaEventSeq || 0,
      deltaTraceCount: feedback.deltaTraceCount || 0,
    };
  }
  function snapshotSemanticFeedback(feedback) {
    feedback = feedbackForScope(feedback,state);
    if (!feedback) return null;
    const response = promptFeedback.redactPromptControlResponse(feedback.response);
    return {
      id: feedback.id,
      kind: feedback.kind,
      action: feedback.action,
      agentId: feedback.agentId || null,
      requestId: feedback.requestId || response?.request_id || null,
      requestAuthNonce: feedback.requestAuthNonce ?? null,
      accepted: feedback.accepted,
      stage: feedback.stage,
      ok: feedback.ok,
      reason: feedback.reason || null,
      effect: feedback.effect || null,
      response: clone(response) || null,
    };
  }
  function semanticFeedbackCode(feedback) {
    if (feedback?.kind === "prompt") {
      const resultCode = promptFeedback.semanticPromptFeedbackCode(feedback);
      if (resultCode) {
        return resultCode;
      }
    }
    if (feedback?.stage !== "error") {
      return null;
    }
    const responseCode = String(feedback?.response?.reason_code || feedback?.response?.code || "").trim();
    if (responseCode) {
      return responseCode;
    }
    const effectCode = String(feedback?.effect || "").trim();
    return effectCode || null;
  }
  function semanticFeedbackMessage(feedback) {
    const responseMessage = String(feedback?.response?.message || "").trim();
    if (responseMessage) {
      return responseMessage;
    }
    const reason = String(feedback?.reason || "").trim();
    return reason || null;
  }
  function formatPromptVersionLabel(value) {
    return `v${Math.max(0, Math.floor(Number(value || 0)))}`;
  }
  function describeSemanticFeedback(feedback, locale = state.uiLocale) {
    if (!feedback) {
      return null;
    }
    const code = semanticFeedbackCode(feedback);
    const diagnostics = semanticFeedbackMessage(feedback);
    const rejectionSummary = (fallbackZh, fallbackEn) => {
      const fallback = isLocaleZh(locale) ? fallbackZh : fallbackEn;
      if (diagnostics && code) {
        return `${fallback} ${code}: ${diagnostics}`;
      }
      if (diagnostics) {
        return `${fallback} ${diagnostics}`;
      }
      if (code) {
        return `${fallback} ${code}`;
      }
      return fallback;
    };
    const rejectionDetail = (fallbackZh, fallbackEn) =>
      diagnostics || code || (isLocaleZh(locale) ? fallbackZh : fallbackEn);
    const description = {
      label: feedback.stage || "idle",
      summary: feedback.effect || diagnostics || (isLocaleZh(locale) ? "反馈已更新。" : "Feedback updated."),
      detail: null,
      code,
      diagnostics,
      badgeClass: feedbackBadgeClass(feedback),
    };
    if (feedback.stage === "error") {
      if (code === "control_lost") {
        description.label = isLocaleZh(locale) ? "控制权已丢失" : "Control lost";
        description.summary = isLocaleZh(locale)
          ? "当前 Agent 控制权已失效。"
          : "Control of this Agent is no longer available.";
        description.detail = isLocaleZh(locale)
          ? "请重新认证并刷新当前绑定后再试。"
          : "Re-authenticate and refresh the current binding before retrying.";
        return description;
      }
      if (code === "llm_init_failed") {
        description.label = isLocaleZh(locale) ? "LLM 不可用" : "LLM unavailable";
        description.summary = isLocaleZh(locale)
          ? "当前栈没有可用的 LLM 配置，因此无法开始聊天。"
          : "Chat cannot start because this stack has no usable LLM configuration.";
        description.detail =
          isLocaleZh(locale)
            ? "请把 model、base URL 和 API key 写入当前 config.toml 或 OASIS7_LLM_* 环境变量，然后重启 launcher 栈。"
            : "Add model, base URL, and API key to the active config.toml or OASIS7_LLM_* env, then restart the launcher stack.";
        return description;
      }
      if (code === "target_version_not_found") {
        description.label = isLocaleZh(locale) ? "找不到回滚目标" : "Rollback target missing";
        description.summary = isLocaleZh(locale)
          ? "当前 Agent 没有这个可回滚版本。"
          : "The selected rollback version is not available for this agent.";
        description.detail = isLocaleZh(locale)
          ? "请先刷新 prompt 状态，或改选一个真实存在的保存版本后再重试。"
          : "Refresh prompt state or choose an existing saved version before retrying.";
        return description;
      }
      if (code === "rollback_noop") {
        description.label = isLocaleZh(locale) ? "回滚无变化" : "Rollback noop";
        description.summary = isLocaleZh(locale)
          ? "这个回滚目标不会改变当前 prompt。"
          : "That rollback target would not change the current prompt.";
        description.detail = isLocaleZh(locale)
          ? "只有在你确实要恢复不同 prompt 内容时，才需要选择更旧的版本。"
          : "Pick an older version only when you need to restore different prompt content.";
        return description;
      }
      if (feedback.kind === "prompt") {
        description.label = isLocaleZh(locale) ? "Prompt 失败" : "Prompt failed";
        description.summary = rejectionSummary("Prompt 控制没有完成。", "Prompt control did not complete.");
        description.detail = rejectionDetail(
          "后端没有返回更详细的拒绝原因；可展开诊断查看原始载荷。",
          "The backend did not return a more specific rejection reason; open diagnostics for the raw payload.",
        );
        return description;
      }
      if (feedback.kind === "chat") {
        description.label = isLocaleZh(locale) ? "聊天失败" : "Chat failed";
        description.summary = rejectionSummary("Agent 聊天没有完成。", "Agent chat did not complete.");
        description.detail = rejectionDetail(
          "后端没有返回更详细的拒绝原因；可展开诊断查看原始载荷。",
          "The backend did not return a more specific rejection reason; open diagnostics for the raw payload.",
        );
        return description;
      }
      if (feedback.kind === "gameplay_action") {
        description.label = isLocaleZh(locale) ? "玩法动作失败" : "Gameplay action failed";
        description.summary = rejectionSummary("正式玩法动作没有完成。", "The gameplay action did not complete.");
        description.detail = rejectionDetail(
          "runtime 没有返回更详细的拒绝原因；可展开诊断查看原始载荷。",
          "The runtime did not return a more specific rejection reason; open diagnostics for the raw payload.",
        );
        return description;
      }
      description.label = code || "Request failed";
      description.summary = diagnostics || (isLocaleZh(locale) ? "请求失败。" : "The request failed.");
      description.detail = isLocaleZh(locale)
        ? "展开诊断可查看后端原始载荷。"
        : "Open diagnostics for the raw backend payload.";
      return description;
    }
    if (feedback.kind === "prompt") {
      const promptResultDescription = promptFeedback.describePromptResult(feedback, locale);
      if (promptResultDescription) {
        return promptResultDescription;
      }
      const version = Number(feedback?.response?.version || 0);
      const appliedFields = Array.isArray(feedback?.response?.applied_fields)
        ? feedback.response.applied_fields.map((field) => String(field || "").trim().replaceAll("_", " ")).filter(Boolean).join(", ") || null
        : null;
      if (feedback.stage === "preview_ack") {
        description.label = isLocaleZh(locale) ? "预览已就绪" : "Preview ready";
        description.summary = isLocaleZh(locale)
          ? `Prompt 预览已基于 ${formatPromptVersionLabel(version)} 准备完成。`
          : `Prompt preview is ready from ${formatPromptVersionLabel(version)}.`;
        description.detail = isLocaleZh(locale)
          ? "应用前请先检查返回的摘要或 prompt 字段。"
          : "Review the returned digest or prompt fields before applying.";
        return description;
      }
      if (feedback.stage === "apply_ack") {
        description.label = isLocaleZh(locale) ? "Prompt 已保存" : "Prompt saved";
        description.summary = isLocaleZh(locale)
          ? `Prompt 改动已保存为 ${formatPromptVersionLabel(version)}。`
          : `Prompt changes are now saved as ${formatPromptVersionLabel(version)}.`;
        description.detail = appliedFields
          ? (isLocaleZh(locale) ? `已应用字段：${appliedFields}。` : `Applied fields: ${appliedFields}.`)
          : (isLocaleZh(locale) ? "Prompt 改动已被接受并持久化。" : "Prompt changes were accepted and persisted.");
        return description;
      }
      if (feedback.stage === "rollback_ack") {
        const restoredVersion = Number(feedback?.response?.rolled_back_to_version || 0);
        description.label = isLocaleZh(locale) ? "回滚已应用" : "Rollback applied";
        description.summary =
          isLocaleZh(locale)
            ? `当前生效 prompt 已保存为 ${formatPromptVersionLabel(version)}，其内容恢复自 ${formatPromptVersionLabel(restoredVersion)}。`
            : `Active prompt is now saved as ${formatPromptVersionLabel(version)} after restoring content from ${formatPromptVersionLabel(restoredVersion)}.`;
        description.detail =
          isLocaleZh(locale)
            ? "回滚会生成一个新的保存版本；下面输入框指向的是下一次回滚目标，不是刚刚恢复出来的版本。"
            : "Rollback creates a new saved version; the rollback input below points to the next target, not the version that was just restored.";
        return description;
      }
      description.label = isLocaleZh(locale) ? "Prompt 进行中" : "Prompt in progress";
      description.summary = feedback.effect || (isLocaleZh(locale) ? "Prompt 请求正在处理。" : "Prompt request is in flight.");
      description.detail = isLocaleZh(locale)
        ? "请等待 ack/error 返回后再发起下一次 prompt 操作。"
        : "Wait for ack/error before issuing another prompt action.";
      return description;
    }

    if (feedback.kind === "chat") {
      if (feedback.stage === "ack") {
        const acceptedAtTick = Number(feedback?.response?.accepted_at_tick || 0);
        description.label = isLocaleZh(locale) ? "聊天已受理" : "Chat accepted";
        description.summary = isLocaleZh(locale)
          ? `消息已在 tick ${acceptedAtTick} 进入 runtime 队列。`
          : `Message entered the runtime queue at tick ${acceptedAtTick}.`;
        description.detail = isLocaleZh(locale)
          ? "请查看 Message Flow，确认玩家出站消息和后续 Agent 回应。"
          : "Watch Message Flow for the outbound player message and any inbound agent reply.";
        return description;
      }
      description.label = isLocaleZh(locale) ? "聊天进行中" : "Chat in progress";
      description.summary = feedback.effect || (isLocaleZh(locale) ? "聊天请求正在处理。" : "Chat request is in flight.");
      description.detail = isLocaleZh(locale)
        ? "请等待 ack/error 返回后再发送下一条消息。"
        : "Wait for ack/error before sending another message.";
      return description;
    }
    if (feedback.kind === "gameplay_action") {
      if (feedback.stage === "ack") {
        const acceptedAtTick = Number(feedback?.response?.accepted_at_tick || 0);
        const message = String(feedback?.response?.message || "");
        const submittedToChain = /\bsubmitted\b.*\bchain runtime\b/i.test(message);
        description.label = isLocaleZh(locale) ? "玩法动作已受理" : "Gameplay action accepted";
        description.summary = isLocaleZh(locale)
          ? `动作已在 tick ${acceptedAtTick} 进入 runtime 队列。`
          : `The action entered the runtime queue at tick ${acceptedAtTick}.`;
        description.detail = submittedToChain
          ? (isLocaleZh(locale)
            ? `${message}。正在等待 committed world sync；同步完成后 Agent 会出现在世界里。`
            : `${message}. Waiting for committed world sync; the Agent will appear after the synced snapshot lands.`)
          : message
            || (isLocaleZh(locale)
              ? "请继续观察 gameplay feedback 或刷新后的快照。"
              : "Watch gameplay feedback or the refreshed snapshot for the next world-state change.");
        return description;
      }
      description.label = isLocaleZh(locale) ? "玩法动作进行中" : "Gameplay action in progress";
      description.summary = feedback.effect || (isLocaleZh(locale) ? "玩法动作请求正在处理。" : "Gameplay action request is in flight.");
      description.detail = isLocaleZh(locale)
        ? "请等待 ack/error 或新的 gameplay 快照反馈。"
        : "Wait for ack/error or a new gameplay snapshot update.";
      return description;
    }
    return description;
  }
  function describePromptVersionState(feedback = state.lastPromptFeedback, locale = state.uiLocale) {
    const currentVersion = Math.max(0, Math.floor(Number(state.promptDraft.currentVersion || 0)));
    const nextRollbackTargetVersion = Math.max(
      0,
      Math.floor(Number(state.promptDraft.rollbackTargetVersion || 0)),
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

  function buildGameplaySummary(locale = state.uiLocale) {
    const lastGameplayActionFeedback = feedbackForScope(state.lastGameplayActionFeedback,state);
    const gameplay = state.snapshot?.player_gameplay;
    if (!gameplay || typeof gameplay !== "object") {
      return null;
    }
    const modelAgents = state.snapshot?.model?.agents || {};
    const agents = Object.keys(modelAgents)
      .filter((agentId) => isAgentVisibleToCurrentSession?.(agentId) !== false);
    const locations = Object.keys(state.snapshot?.model?.locations || {});
    const missingAgents = agents.length === 0;
    const missingLocations = locations.length === 0;
    const boundOwnerSync = state.auth?.available && state.auth?.registrationStatus === "registered"
      && Boolean(state.auth?.boundAgentId) && state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1");
    let emptyEntityBlocker = missingAgents || missingLocations
      ? (() => {
        const missingLabel = missingAgents && missingLocations
          ? localeText(locale, "agents 与 locations", "agents and locations")
          : missingAgents
            ? "agents"
            : "locations";
        return {
          blockerKind: "runtime_snapshot_empty_entities",
          blockerDetail: localeText(
            locale,
            missingAgents && missingLocations
              ? "当前 gameplay 快照没有 Agent / 地点；如果这是新用户空世界，请先认领第一个 Agent。"
              : `当前 gameplay 快照缺少 ${missingLabel}；如果这是新用户空世界，请先认领第一个 Agent。`,
            missingAgents && missingLocations
              ? "The current gameplay snapshot has no agents/locations; if this is a new-user empty world, claim the first Agent first."
              : `The current gameplay snapshot is missing ${missingLabel}; if this is a new-user empty world, claim the first Agent first.`,
          ),
          nextStepHint: localeText(
            locale,
            "如果页面显示“认领第一个 Agent”，先提交认领；只有认领入口缺失时才刷新快照或检查 runtime bootstrap。",
            "If the page shows Claim First Agent, submit that claim first; only refresh the snapshot or inspect runtime bootstrap when the claim entry is missing.",
          ),
          disabledReason: localeText(
            locale,
            `当前快照缺少 ${missingLabel}；先完成第一个 Agent 认领。`,
            `Current snapshot is missing ${missingLabel}; claim the first Agent first.`,
          ),
        };
      })()
      : null;
    if (emptyEntityBlocker && boundOwnerSync) {
      emptyEntityBlocker = {
        blockerKind: "agent_owner_sync_pending",
        blockerDetail: localeText(locale, "已绑定的 Agent 正在同步，请勿重复领取。", "Your bound Agent is syncing; do not claim another Agent."),
        nextStepHint: localeText(locale, "等待 Agent 同步，或刷新玩法快照核对当前归属。", "Wait for Agent sync, or refresh the gameplay snapshot to verify current ownership."),
        disabledReason: localeText(locale, "等待已绑定 Agent 同步。", "Waiting for the bound Agent to sync."),
      };
    }
    const progressRaw = Number(gameplay.progress_percent);
    const progressPercent = Number.isFinite(progressRaw)
      ? Math.max(0, Math.min(100, Math.floor(progressRaw)))
      : null;
    const acceptedIntentId = gameplay.accepted_intent_id || null;
    const intentSummary = gameplay.intent_summary || null;
    const intentScope = gameplay.intent_scope || null;
    const intentTarget = gameplay.intent_target || null;
    const statusReason = gameplay.status_reason || null;
    const lastWorldChange = gameplay.last_world_change || null;
    const resumeAnchor = gameplay.resume_anchor || null;
    const resumeNextStep = gameplay.resume_next_step || null;
    const agentExists = (agentId) => Boolean(String(agentId || "").trim() && modelAgents[String(agentId || "").trim()]);
    const pendingClaim = firstClaimAwaitingOutcome(lastGameplayActionFeedback) ? lastGameplayActionFeedback : firstClaimAwaitingOutcome(gameplay.recent_feedback) ? gameplay.recent_feedback : null;
    const firstAgentClaimSyncPending = emptyEntityBlocker && pendingClaim;
    if (firstAgentClaimSyncPending) emptyEntityBlocker = { ...emptyEntityBlocker,
      nextStepHint: localeText(locale, "等待并刷新原请求的权威结果；请勿重复提交认领。", "Refresh the original request's authoritative result; do not submit the claim again."),
    };
    let availableActions = normalizeViewerAvailableActions({
      gameplay,
      locale,
      localeText,
      agentExists,
      emptyEntityBlocker,
      firstAgentClaimSyncPending,
    });
    // A registered binding is only a pending display hint; it grants no command authority.
    // The private signed owner read and backend ownership checks remain authoritative.
    if (boundOwnerSync) availableActions = availableActions.filter(action => action.actionId !== "claim_first_agent");
    const {localGameplayFeedback,recentFeedback} = projectGameplayReceiptFeedback(gameplay,lastGameplayActionFeedback,intentTarget);
    const runtimeBlockerKind = gameplay.blocker_kind || null;
    const runtimeBlockerDetail = gameplay.blocker_detail || null;
    const runtimeAlreadyPublishedEmptyEntityBlocker = !boundOwnerSync
      && runtimeBlockerKind === "runtime_snapshot_empty_entities";
    const recentStage = String(recentFeedback?.stage || "").trim().toLowerCase();
    const pendingGameplayFeedback =
      ["accepted", "submitted", "queued", "ack", "registering", "signing", "sent", "received", "pending", "committed", "unknown"].includes(recentStage)
      && Boolean(String(recentFeedback?.action || "").trim());
    const pendingEmptyWorldClaimSync = Boolean(emptyEntityBlocker && pendingGameplayFeedback);
    const resolvedStageStatus = pendingEmptyWorldClaimSync
      ? gameplay.stage_status || "accepted"
      : emptyEntityBlocker ? "blocked" : gameplay.stage_status || null;
    const resolvedBlockerKind = runtimeAlreadyPublishedEmptyEntityBlocker
      ? runtimeBlockerKind
      : emptyEntityBlocker
        ? emptyEntityBlocker.blockerKind
        : runtimeBlockerKind;
    const resolvedBlockerDetail = runtimeAlreadyPublishedEmptyEntityBlocker
      ? runtimeBlockerDetail || emptyEntityBlocker?.blockerDetail || null
      : emptyEntityBlocker
        ? emptyEntityBlocker.blockerDetail
        : runtimeBlockerDetail;
    const executionState = firstAgentClaimSyncPending && ["received", "pending", "unknown"].includes(pendingClaim.stage) ? pendingClaim.stage : pendingEmptyWorldClaimSync
      ? "accepted"
      : emptyEntityBlocker
      ? "blocked"
      : gameplay.execution_state
      || (() => {
        if (["accepted", "submitted", "queued", "ack"].includes(recentStage)) {
          return "accepted";
        }
        if (recentStage === "rejected") {
          return "rejected";
        }
        if (["blocked", "completed_no_progress"].includes(recentStage)) {
          return "blocked";
        }
        if (recentStage === "completed_advanced") {
          return "completed";
        }
        if (resolvedStageStatus === "blocked") {
          return "blocked";
        }
        if (resolvedStageStatus === "branch_ready") {
          return "completed";
        }
        return "executing";
      })();
    const executionStateLabel = gameplayExecutionStateLabel(executionState, locale, localeText);
    const executionStateMachine = [
      ...["received", "pending", "unknown"].map(id => ({ id, label: gameplayExecutionStateLabel(id, locale, localeText) })),
      { id: "accepted", label: localeText(locale, "已接受", "Accepted") },
      { id: "executing", label: localeText(locale, "执行中", "Executing") },
      { id: "blocked", label: localeText(locale, "已阻塞", "Blocked") },
      { id: "completed", label: localeText(locale, "已完成", "Completed") },
      { id: "rejected", label: localeText(locale, "已拒绝", "Rejected") },
    ];
    const executionCauseKind = ["received", "pending", "unknown"].includes(executionState) ? "outcome_unconfirmed" : pendingEmptyWorldClaimSync
      ? "queued_for_execution"
      : emptyEntityBlocker
      ? "world_constraint"
      : gameplay.causality_kind
      || (() => {
        if (executionState === "accepted") return "queued_for_execution";
        if (executionState === "rejected") return "request_rejected";
        if (executionState === "blocked") return "world_constraint";
        if (executionState === "completed") return "goal_progressed";
        return null;
      })();
    const executionCauseLabel = (() => {
      switch (executionCauseKind) {
        case "outcome_unconfirmed":
          return localeText(locale, "等待权威结果", "Awaiting Authoritative Result");
        case "queued_for_execution":
          return localeText(locale, "等待执行", "Queued for Execution");
        case "world_constraint":
          return localeText(locale, "世界约束", "World Constraint");
        case "agent_override":
          return localeText(locale, "Agent 改走了别的允许路径", "Agent Chose Differently");
        case "goal_progressed":
          return localeText(locale, "世界已推进", "World Progressed");
        case "request_rejected":
          return localeText(locale, "请求被拒绝", "Request Rejected");
        default:
          return null;
      }
    })();
    const executionCauseDetail = pendingEmptyWorldClaimSync
      ? recentFeedback?.hint || recentFeedback?.effect || resolvedBlockerDetail || null
      : emptyEntityBlocker
      ? resolvedBlockerDetail || emptyEntityBlocker.blockerDetail || null
      : gameplay.causality_detail
      || (() => {
        if (executionState === "blocked") {
          return resolvedBlockerDetail || recentFeedback?.reason || null;
        }
        if (executionState === "accepted") {
          return recentFeedback?.hint || recentFeedback?.effect || null;
        }
        if (executionState === "completed") {
          return recentFeedback?.effect || gameplay.progress_detail || null;
        }
        if (executionState === "rejected") {
          return recentFeedback?.reason || null;
        }
        return null;
      })();
    const executionSummary = (() => {
      if (executionCauseKind === "agent_override") {
        return localeText(
          locale,
          "本次目标已推动世界继续前进，但执行它的 Agent 最终采用了另一条被允许的计划。",
          "This goal still advanced the world, but the acting agent finished it through a different allowed plan.",
        );
      }
      switch (executionState) {
        case "received":
        case "pending":
        case "unknown":
          return localeText(locale, "请求结果尚未确认；请回读原请求的权威结果，不要重复提交。", "The original request outcome is unconfirmed; read its authoritative result before submitting again.");
        case "accepted":
          return localeText(
            locale,
            "最新一条目标相关指令已经入队，正在等待 committed world delta 或后续回执。",
            "The latest goal-affecting command is queued and waiting for committed world delta or follow-up feedback.",
          );
        case "blocked":
          return localeText(
            locale,
            "当前目标没有继续推进，主要原因已经被归入可修复的 blocker taxonomy。",
            "The current goal is not moving forward; the primary reason is now grouped into a repairable blocker taxonomy.",
          );
        case "completed":
          return localeText(
            locale,
            "当前目标最近一次执行已经产生世界级结果，可以决定是继续放大、恢复，还是切到下一条主线。",
            "The current goal's latest execution already produced a world-level result; you can now amplify it, recover it, or pivot to the next line.",
          );
        case "rejected":
          return localeText(
            locale,
            "最新请求在执行前被拒绝，需要先修正请求本身或权限/模式前提。",
            "The latest request was rejected before execution; fix the request itself or its permission/mode prerequisites first.",
          );
        default:
          return localeText(
            locale,
            "当前目标正在执行中，先盯住状态机、主因果和下一步，再决定是否继续推进。",
            "The current goal is executing; read the state machine, primary causality, and next step before pushing again.",
          );
      }
    })();
    const blockerLabel = (() => {
      switch (resolvedBlockerKind) {
        case "material_shortage":
          return localeText(locale, "缺料", "Missing Material");
        case "power_shortage":
          return localeText(locale, "缺电", "Missing Power");
        case "governance_gate":
          return localeText(locale, "治理限制", "Governance Restriction");
        case "no_progress":
          return localeText(locale, "没有前进", "No Forward Progress");
        case "llm_required":
          return localeText(locale, "缺少玩法能力", "Missing Gameplay Capability");
        case "runtime_sync_unavailable":
          return localeText(locale, "运行时同步不可用", "Runtime Sync Unavailable");
        case "execution_world_not_ready":
          return localeText(locale, "执行世界未就绪", "Execution World Not Ready");
        case "runtime_snapshot_empty_entities":
          return localeText(locale, "认领第一个 Agent", "Claim the first Agent");
        case "industrial_termination":
          return localeText(locale, "工业作业已终止", "Industrial job terminated");
        case "industrial_terminal":
          return localeText(locale, "工业产出终端受阻", "Industrial output terminal blocked");
        case "product_validation":
        case "product_validation_rejected":
          return localeText(locale, "产品验证失败", "Product validation failed");
        default:
          return resolvedBlockerKind ? localeText(locale, "当前阻塞", "Current blocker") : null;
      }
    })();
    const factoryProductionFailureDisposition = normalizeFactoryProductionFailureDisposition(
      gameplay.factory_production_failure_disposition ?? gameplay.factoryProductionFailureDisposition,
      locale,
      localeText, availableActions,
    );
    const narrativeNextStep = pendingEmptyWorldClaimSync
      ? localeText(
        locale,
        "认领动作已提交到本地世界，正在等待 committed world sync；同步完成后 Agent 会出现在世界里。",
        "The claim has been submitted to the local world and is waiting for committed world sync; the Agent will appear after the synced snapshot lands.",
      )
      : emptyEntityBlocker
      ? emptyEntityBlocker.nextStepHint
      : gameplay.next_step_hint || resumeNextStep || null;
    const recoveryCueText = [
      resolvedBlockerKind,
      narrativeNextStep,
      resumeNextStep,
      statusReason,
      recentFeedback?.reason,
      recentFeedback?.hint,
    ]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    const isRecoveryChoiceState =
      Boolean(resolvedBlockerKind)
      || executionState === "blocked"
      || (executionState !== "completed" && /\b(blocked|blocker|recover|recovery|repair|restore|replenish|refresh|snapshot|advance|confirm|prove|resume)\b/.test(recoveryCueText));
    const wantsSnapshotProof = emptyEntityBlocker || /\b(refresh|snapshot|fresh state|world state)\b/.test(recoveryCueText);
    const wantsAdvanceProof = /\b(advance|step|apply|confirm|prove|verify|check)\b/.test(recoveryCueText);
    const wantsResumeProof = /\b(resume|recover|restore|replenish|repair)\b/.test(recoveryCueText);
    const starterOcClaimAvailable = availableActions.some((action) => (
      action.executeKind === "claim_starter_oc"
      && !action.disabledReason
    ));
    const starterOcBlocksChat = starterOcClaimAvailable && availableActions.some((action) => (
      action.executeKind === "agent_chat"
      && String(action.disabledReason || "").toLowerCase().includes("starter oc")
    ));
    const genericRecommendedAction = availableActions
      .filter((action) => !action.disabledReason)
      .sort((left, right) => {
        const priority = (action) => {
          if (starterOcBlocksChat && action.executeKind === "claim_starter_oc") return -1;
          if (isRecoveryChoiceState) {
            if (emptyEntityBlocker && action.executeKind === "claim_first_agent") return -1;
            return {
              request_snapshot: wantsSnapshotProof ? 0 : 2,
              step: wantsAdvanceProof ? 0 : 1,
              play: wantsResumeProof ? 1 : 2,
              claim_first_agent: 1,
              claim_starter_oc: 1,
              gameplay_action: 4,
              agent_chat: 5,
            }[action.executeKind] ?? 6;
          }
          return {
            claim_first_agent: 0,
            claim_starter_oc: 0,
            gameplay_action: 0,
            step: 1,
            play: 2,
            request_snapshot: 3,
            agent_chat: 4,
          }[action.executeKind] ?? 5;
        };
        return priority(left) - priority(right);
      })[0] || null;
    const factoryFailureRecoveryAction = factoryProductionFailureDisposition?.recoveryAction;
    const recommendedAction = factoryProductionFailureDisposition
      ? factoryFailureRecoveryAction?.executeKind === "none" ? null : availableActions.find((action) => (
        action.actionId === factoryProductionFailureDisposition.recoveryActionId
        && action.executeKind === factoryFailureRecoveryAction?.executeKind && !action.disabledReason
      )) || null
      : genericRecommendedAction;
    const recoveryActionDetail = (action, economicSurface) => {
      if (!action) return null;
      if (action.disabledReason) return action.disabledReason;
      if (!isRecoveryChoiceState) {
        return localeText(
          locale,
          "可以直接从正式网页入口执行。",
          "Playable directly from the formal Web entry.",
        );
      }
      if (action.executeKind === "request_snapshot") {
        return localeText(
          locale,
          "刷新快照，先确认 blocker 是否仍存在，再决定是否提交新的玩法动作。",
          "Refresh the snapshot to confirm whether the blocker is still present before submitting another gameplay action.",
        );
      }
      if (action.executeKind === "step") {
        return localeText(
          locale,
          "推进一个 committed step，用它执行或验证恢复，再回看 blocker 和世界反馈。",
          "Advance one committed step to apply or prove recovery, then re-check the blocker and world feedback.",
        );
      }
      if (action.executeKind === "play") {
        return localeText(
          locale,
          "在恢复前提已经就绪后恢复实时推进，并观察回执是否重新产生世界变化。",
          "Resume live play after recovery prerequisites are ready, then watch whether feedback produces world change again.",
        );
      }
      if (economicSurface?.repairAction) {
        return localeText(
          locale,
          `修复路径：${economicSurface.repairAction}`,
          `Recovery path: ${economicSurface.repairAction}`,
        );
      }
      return narrativeNextStep
        || localeText(
          locale,
          "先完成恢复或证明动作，再继续提交新的玩法动作。",
          "Finish the recovery or proof action before submitting more gameplay actions.",
        );
    };
    const acceptedIntentSummary = intentSummary
      || acceptedIntentId
      || (pendingEmptyWorldClaimSync ? recentFeedback?.effect || recentFeedback?.action : null)
      || localeText(
        locale,
        "还没有一条被正式接受的玩家意图",
        "No player-facing accepted intent yet",
      );
    const acceptedIntentDetail = (() => {
      if (lastWorldChange) {
        return lastWorldChange;
      }
      if (statusReason) {
        return statusReason;
      }
      if (recentFeedback?.hint) {
        return recentFeedback.hint;
      }
      if (pendingEmptyWorldClaimSync) {
        return localeText(
          locale,
          "系统已经收到认领请求，正在等待链上 committed 快照把新 Agent 同步到 viewer。",
          "The system has accepted the claim and is waiting for the committed chain snapshot to sync the new Agent into the viewer.",
        );
      }
      return localeText(
        locale,
        "先提交一个玩法动作，再看系统如何确认、推进或阻塞它。",
        "Submit one gameplay action first, then read how the system confirms, advances, or blocks it.",
      );
    })();
    const narrativeBlockerDetail = pendingEmptyWorldClaimSync
      ? recentFeedback?.hint || resolvedBlockerDetail || statusReason || null
      : resolvedBlockerDetail || statusReason || recentFeedback?.reason || null;
    const economicSurface = buildGameplayEconomicSurface({
      locale,
      localeText,
      gameplay,
      availableActions,
      recommendedAction,
      recentFeedback,
      blockerLabel,
      narrativeNextStep,
      lastWorldChange,
    });
    const enrichedAvailableActions = availableActions.map((action) => ({
      ...action,
      playerDetail: recoveryActionDetail(action, economicSurface),
    }));
    const enrichedRecommendedAction = recommendedAction
      ? enrichedAvailableActions.find((action) => action.actionId === recommendedAction.actionId && action.executeKind === recommendedAction.executeKind)
        || {
          ...recommendedAction,
          playerDetail: recoveryActionDetail(recommendedAction, economicSurface),
        }
      : null;
    const controlProofConsequence = [
      executionCauseLabel,
      executionCauseDetail,
    ].filter(Boolean).join(": ") || executionSummary || lastWorldChange || null;
    const controlProofRecovery = enrichedRecommendedAction?.label
      || enrichedRecommendedAction?.actionId
      || economicSurface?.repairAction
      || blockerLabel
      || null;
    const primaryIntent = gameplay.primary_intent || {};
    const receipt = primaryIntent.agency_read_model?.causal_receipt;
    const matchingReceipt = receipt?.intent_id === primaryIntent.intent_id && primaryIntent.intent_id
      && primaryIntent.agent_id && isAgentVisibleToCurrentSession(primaryIntent.agent_id)
      && Number.isSafeInteger(receipt?.action_id) && receipt.action_id >= 0
      && displayableString(receipt.receipt_id) && displayableString(receipt.commit_id);
    const controlProofSummary = matchingReceipt && receipt.disposition === "applied"
      && Array.isArray(receipt.domain_event_refs) && receipt.domain_event_refs.some((ref) => Number.isSafeInteger(ref) && ref >= 0)
      ? localeText(locale, "权威回执已提交世界效果；Agent 预测仍需与实际结果核对。", "An authoritative receipt committed a world effect; compare the Agent prediction with the actual result.")
      : matchingReceipt && receipt.disposition === "not_applied"
        ? localeText(locale, "权威回执记录世界效果未生效；请读取原因与下一步。", "An authoritative receipt records no applied world effect; read the reason and next step.")
        : localeText(locale, "详细因果回执尚不可用；接受意图或 Agent 预测不证明世界效果。", "Detailed causal receipt unavailable; accepted intent or Agent prediction does not prove a world effect.");
    const controlProof = {
      intentId: displayableString(gameplay.primary_intent?.intent_id),
      agentId: displayableString(gameplay.primary_intent?.agent_id),
      primaryNextStep: displayableString(primaryIntent.next_step),
      agency: gameplay.primary_intent?.intent_id && gameplay.primary_intent?.agent_id && isAgentVisibleToCurrentSession(gameplay.primary_intent.agent_id)
        ? clone(gameplay.primary_intent.agency_read_model || null)
        : null,
      intent: displayableString(primaryIntent.message) || acceptedIntentSummary,
      consequence: controlProofConsequence,
      recovery: controlProofRecovery,
      nextMove: narrativeNextStep,
      summary: controlProofSummary,
      state: executionState,
    };
    const availabilityLabel = (value) => (
      value === true
        ? "available"
        : value === false
          ? "unavailable"
          : "unverified"
    );
    const agencyMoves = {
      interrupt: availabilityLabel(gameplay.can_interrupt),
      reprioritize: availabilityLabel(gameplay.can_reprioritize),
      correction: gameplay.replacement_intent_summary
        || gameplay.reprioritize_hint
        || gameplay.escalation_hint
        || null,
      handoff: gameplay.handoff_result
        || gameplay.override_reason
        || null,
      summary: localeText(
        locale,
        "P1 玩家动词：不要只等 AI 继续，优先暴露打断、重排、纠偏和新旧意图交接。",
        "P1 player verbs: do not only wait for AI to continue; expose interrupt, reprioritize, correction, and handoff.",
      ),
    };
    const sameLoopRepeatCount = Number(gameplay.same_loop_repeat_count);
    const normalizedRepeatCount = Number.isFinite(sameLoopRepeatCount)
      ? Math.max(0, Math.floor(sameLoopRepeatCount))
      : null;
    const grindOnlyFlag = gameplay.grind_only_flag === true;
    const leverageClass = gameplay.leverage_class || gameplay.player_leverage_class || null;
    const progressionProof = {
      firstWinGoal: gameplay.first_win_goal_id
        || gameplay.first_win_definition
        || null,
      playerAction: gameplay.player_action || null,
      worldChange: gameplay.world_change_due_to_player || null,
      leverageVerdict: gameplay.player_leverage_verdict
        || gameplay.player_leverage_score
        || null,
      leverageClass,
      antiGrind: leverageClass
        ? `${leverageClass}${normalizedRepeatCount == null ? "" : ` · repeat=${normalizedRepeatCount}`}${grindOnlyFlag ? " · grind_only" : ""}`
        : grindOnlyFlag
          ? localeText(locale, "grind_only 风险已触发", "grind_only risk is active")
          : localeText(locale, "等待 leverage_class / anti-grind truth", "Waiting for leverage_class / anti-grind truth"),
      summary: localeText(
        locale,
        "P1 首个胜利：证明玩家动作带来可恢复、可复用或可谈判的新 leverage，而不是只增加产量。",
        "P1 first win: prove the player action creates recoverable, reusable, or negotiable leverage, not just more output.",
      ),
    };
    const dependencyStatus = gameplay.major_power_dependency_status || "unverified";
    const fallbackTradeoffPreview = (
      Array.isArray(gameplay.fallback_tradeoff_preview)
        ? gameplay.fallback_tradeoff_preview
        : Array.isArray(gameplay.fallbackTradeoffPreview)
          ? gameplay.fallbackTradeoffPreview
          : []
    )
      .filter(isRecord)
      .map((option) => ({
        valueClass: option.value_class || option.valueClass || null,
        available: option.available === true,
        cost: displayableString(option.cost) || null,
        progressKept: displayableString(option.progress_kept ?? option.progressKept) || null,
        opportunityCost: displayableString(option.opportunity_cost ?? option.opportunityCost) || null,
        reason: displayableString(option.reason) || null,
        recommended: option.recommended === true,
      }));
    if (factoryProductionFailureDisposition) {
      fallbackTradeoffPreview.length = 0;
    }
    const waitResolutionQuote = factoryProductionFailureDisposition
      ? null
      : buildWaitResolutionQuoteDisplayModel(
        gameplay.wait_resolution_quote ?? gameplay.waitResolutionQuote,
        locale,
        localeText,
      );
    if (waitResolutionQuote) {
      const safeWaitIndex = fallbackTradeoffPreview.findIndex((option) => option.valueClass === "safe_wait");
      fallbackTradeoffPreview.splice(safeWaitIndex < 0 ? fallbackTradeoffPreview.length : safeWaitIndex, safeWaitIndex < 0 ? 0 : 1, waitResolutionQuote.fallbackTradeoffOption);
    }
    const noSafeFallbackReason = displayableString(
      gameplay.no_safe_fallback_reason ?? gameplay.noSafeFallbackReason,
    );
    const requiredNextDecisionActionId = displayableString(
      gameplay.required_next_decision_action_id ?? gameplay.requiredNextDecisionActionId,
    );
    const requiredNextDecisionClass = displayableString(
      gameplay.required_next_decision_class ?? gameplay.requiredNextDecisionClass,
    );
    const noSafeFallbackHandoff = !factoryProductionFailureDisposition && (noSafeFallbackReason
      || requiredNextDecisionActionId
      || requiredNextDecisionClass
      ? {
        reason: noSafeFallbackReason,
        requiredNextDecisionActionId,
        requiredNextDecisionClass,
      }
      : null);
    // Legacy presentation flags/cost strings have no canonical action evidence.
    // They cannot recover executable options after reconnect or version upgrade.
    const recoveryOptionComparisons = [];
    const capabilityRecovery = Array.isArray(gameplay.gameplay_capability?.recovery)
      ? gameplay.gameplay_capability.recovery.filter(isRecord).map((option) => ({
        kind: displayableString(option.kind) || "unknown",
        status: displayableString(option.status) || "unknown",
        reasonCode: displayableString(option.reason_code) || "evidence_unavailable",
        summary: `${({repair: "Repair", rebuild: "Rebuild", pivot: "Pivot", current_recipe_run: "Run the current recipe"})[option.kind] || "Recovery"}: ${displayableString(option.status) || "unknown"} · ${({
          unsupported_recovery_action: "This recovery action is not supported.",
          spectator_has_no_personal_capability: "Select an owned Agent to inspect personal capability.",
          personal_owner_unavailable: "The current owner could not be resolved.",
          current_line_evidence_unavailable: "Current line evidence is unavailable.",
          current_execution_authority_unavailable: "Current execution authority could not be verified.",
          canonical_recipe_quote_unavailable: "A current recipe quote is unavailable.",
          canonical_recipe_admission_rejected: "Current rules or resources block this recipe run.",
          canonical_recipe_quote_available: "A current quote supports this run; submission will recheck it.",
        })[option.reason_code] || "Current evidence is unavailable."}`,

      }))
      : [];
    const matureWorldContinuation = {
      dependencyStatus,
      recoveryOptions: capabilityRecovery.length > 0
        ? capabilityRecovery.map((option) => option.summary).join(" / ")
        : localeText(locale, "恢复可行性未知：缺少当前权威证据", "Recovery feasibility unknown: current canonical evidence unavailable"),
      recoveryOptionComparisons,
      recoveryAssessments: capabilityRecovery.map(({kind, status, reasonCode}) => ({kind, status, reasonCode})),
      recoveryPath: gameplay.recovery_path_detail
        || gameplay.recovery_path_kind
        || narrativeNextStep
        || null,
      summary: dependencyStatus === "forced"
        ? localeText(
          locale,
          "P2 阻塞：继续路径被强制绑定到 major power，需要提供独立 repair/rebuild/pivot。",
          "P2 blocker: continuation is forced into major power dependency; expose independent repair/rebuild/pivot.",
        )
        : localeText(
          locale,
          "P2 成熟世界承接：小玩家需要不依附大组织也能修复、重建或转向。",
          "P2 mature-world continuation: small players need repair, rebuild, or pivot paths without forced major-power dependency.",
        ),
    };
    const enabledGameplayActions = enrichedAvailableActions
      .filter((action) => !action.disabledReason && action.executeKind === "gameplay_action");
    const attractionCaused = gameplay.player_action && gameplay.world_change_due_to_player
      ? `${gameplay.player_action} -> ${gameplay.world_change_due_to_player}`
      : gameplay.player_action
        ? `${gameplay.player_action} -> ${localeText(locale, "等待玩家导致的世界变化", "waiting for player-caused world change")}`
        : localeText(locale, "等待玩家导致的世界变化", "waiting for player-caused world change");
    const attractionNewOption = leverageClass
      || gameplay.player_leverage_verdict
      || gameplay.first_win_goal_id
      || gameplay.branch_hint
      || enabledGameplayActions.map((action) => action.label || action.actionId).filter(Boolean).join(" / ")
      || localeText(locale, "等待新选择", "waiting for new option");
    const attractionWhyContinue = gameplay.branch_hint
      || narrativeNextStep
      || gameplay.resume_next_step
      || localeText(locale, "等待下一分支", "waiting for next branch");
    const attractionWaitingCostParts = [
      resolvedBlockerDetail || blockerLabel || statusReason || recentFeedback?.reason || null,
      normalizedRepeatCount != null ? `repeat=${normalizedRepeatCount}` : null,
      grindOnlyFlag ? "grind_only" : null,
    ].filter(Boolean);
    const attractionWaitingCost = attractionWaitingCostParts.length > 0
      ? attractionWaitingCostParts.join(" · ")
      : localeText(locale, "等待 / 未验证：尚未发布等待成本", "waiting/unverified: no waiting cost published");
    const attractionRecovery = gameplay.recovery_path_detail
      || gameplay.recovery_path_kind
      || (capabilityRecovery.length > 0 ? capabilityRecovery.map((option) => option.summary).join(" / ") : null)
      || localeText(locale, "等待恢复路径", "waiting for recovery path");
    const hasPlayerCausedWorldChange = Boolean(gameplay.player_action && gameplay.world_change_due_to_player);
    const hasNewOption = attractionNewOption !== localeText(locale, "等待新选择", "waiting for new option");
    const hasWhyContinue = attractionWhyContinue !== localeText(locale, "等待下一分支", "waiting for next branch");
    const hasAvailableRecovery = capabilityRecovery.some((option) => option.status === "available");
    const hasRecoveryPath = Boolean(gameplay.recovery_path_detail || gameplay.recovery_path_kind || hasAvailableRecovery);
    const missingAttractionSignals = [
      !hasPlayerCausedWorldChange,
      !hasNewOption,
      !hasWhyContinue,
      !hasRecoveryPath,
    ].filter(Boolean).length;
    const attractionWeak =
      grindOnlyFlag
      || (normalizedRepeatCount != null && normalizedRepeatCount >= 3)
      || (progressPercent != null && progressPercent >= 80 && missingAttractionSignals >= 2)
      || missingAttractionSignals >= 3;
    const attractionVerdict = attractionWeak
      ? "progression_pass_but_attraction_weak"
      : hasPlayerCausedWorldChange && hasNewOption
        ? "attraction_evidence_present"
        : "attraction_watch";
    const attractionProof = {
      verdict: attractionVerdict,
      whatICaused: attractionCaused,
      newOption: attractionNewOption,
      whyContinue: attractionWhyContinue,
      waitingCost: attractionWaitingCost,
      recovery: attractionRecovery,
      summary: attractionWeak
        ? localeText(
          locale,
          "前 10/30 分钟吸引力预警：进度可以通过，但玩家造成的变化、新选择或恢复理由不足。",
          "First 10/30-minute attraction warning: progression can pass while attraction is weak because player-caused change, new option, or recovery reason is missing.",
        )
        : localeText(
          locale,
          "前 10/30 分钟吸引力证据：玩家能看到自己造成了什么、解锁了什么、为什么继续、等待代价和恢复路径。",
          "First 10/30-minute attraction proof: the player can see what they caused, what opened up, why to continue, the waiting cost, and the recovery path.",
        ),
    };
    const replayPlayerIntent = gameplay.player_action || null;
    const replayWorldResult = gameplay.world_change_due_to_player || null;
    const shareReplaySnippet = replayPlayerIntent && replayWorldResult
      ? [
        replayPlayerIntent,
        executionCauseLabel || executionStateLabel || executionState,
        replayWorldResult,
      ].filter(Boolean).join(" -> ")
      : null;
    const shareReplay = {
      playerIntent: replayPlayerIntent,
      agentExecution: executionCauseLabel || executionStateLabel || executionState || null,
      worldResult: replayWorldResult,
      nextBranch: gameplay.branch_hint || narrativeNextStep || null,
      snippet: shareReplaySnippet,
      summary: localeText(
        locale,
        "P2 分享单位：玩家意图、AI/世界执行、世界结果和下一分支必须能组成一段可复盘短故事。",
        "P2 share unit: player intent, AI/world execution, world result, and next branch should form a replayable short story.",
      ),
    };
    const branchRecommendations = (
      Array.isArray(gameplay.branch_recommendations)
        ? gameplay.branch_recommendations
        : Array.isArray(gameplay.branchRecommendations)
          ? gameplay.branchRecommendations
          : []
    ).map((recommendation) => {
      const rawPreview = recommendation.first_delivery_preview || recommendation.firstDeliveryPreview;
      return {
        actionId: recommendation.action_id || recommendation.actionId || null,
        routeLabel: recommendation.route_label || recommendation.routeLabel || null,
        immediateGain: recommendation.immediate_gain || recommendation.immediateGain || null, futureBeatChanged: recommendation.future_beat_changed || recommendation.futureBeatChanged || null,
        futureBeats: Array.isArray(recommendation.future_beats) ? recommendation.future_beats : Array.isArray(recommendation.futureBeats) ? recommendation.futureBeats : [],
        riskOrLockin: recommendation.risk_or_lockin || recommendation.riskOrLockin || null,
        nextSessionHook: recommendation.next_session_hook || recommendation.nextSessionHook || null,
        firstDeliveryPreview: normalizeFirstDeliveryPreview(rawPreview),
      };
    });
    const rawMicroDepotFacilities = (
      Array.isArray(gameplay.micro_depot_facilities)
        ? gameplay.micro_depot_facilities
        : Array.isArray(gameplay.microDepotFacilities)
          ? gameplay.microDepotFacilities
          : []
    );
    const microDepotFacilities = rawMicroDepotFacilities
      .filter(isRecord)
      .map((facility) => {
        const rawInventory = facility.available_units_by_kind ?? facility.availableUnitsByKind;
        const inventory = isRecord(rawInventory) ? clone(rawInventory) : {};
        return {
          facilityId: facility.facility_id || facility.facilityId || null,
          ownerClaimId: facility.owner_claim_id || facility.ownerClaimId || null,
          status: facility.status || null,
          locationId: facility.location_id || facility.locationId || null,
          serviceRadiusCm: facility.service_radius_cm ?? facility.serviceRadiusCm ?? null,
          inventoryRevision: facility.inventory_revision ?? facility.inventoryRevision ?? null,
          availableUnitsByKind: isRecord(inventory) ? inventory : {},
          throughputEpoch: facility.throughput_epoch ?? facility.throughputEpoch ?? null,
          throughputRemainingUnits: facility.throughput_remaining_units ?? facility.throughputRemainingUnits ?? null,
          throughputLimitUnitsPerEpoch: facility.throughput_limit_units_per_epoch ?? facility.throughputLimitUnitsPerEpoch ?? null,
          supportedResourceKinds: displayableStrings(
            facility.supported_resource_kinds ?? facility.supportedResourceKinds,
          ),
          moduleId: facility.module_id || facility.moduleId || null,
          moduleVersion: facility.module_version || facility.moduleVersion || null,
          wasmHash: facility.wasm_hash || facility.wasmHash || null,
          upkeepPaid: facility.upkeep_paid ?? facility.upkeepPaid ?? null,
          lastReceiptId: facility.last_receipt_id || facility.lastReceiptId || null,
          lastProposalHash: facility.last_proposal_hash || facility.lastProposalHash || null,
          availableActions: displayableStrings(facility.available_actions ?? facility.availableActions),
        };
      });
    const validationUnlockPreview = buildValidationUnlockPreviewDisplayModel(
      gameplay.validation_unlock_preview ?? gameplay.validationUnlockPreview,
      locale,
      isLocaleZh,
    );

    return {
      stageId: gameplay.stage_id || null,
      stageStatus: resolvedStageStatus,
      acceptedIntentId,
      acceptedIntentSummary,
      acceptedIntentScope: intentScope,
      acceptedIntentTarget: intentTarget,
      receiptTargetAgentId: localGameplayFeedback ? localGameplayFeedback.agentId : intentTarget,
      acceptedIntentDetail,
      statusReason,
      lastWorldChange,
      resumeAnchor,
      resumeNextStep,
      executionState,
      executionStateLabel,
      executionStateMachine,
      executionSummary,
      executionCauseKind,
      executionCauseLabel,
      executionCauseDetail,
      goalId: gameplay.goal_id || null,
      goalKind: gameplay.goal_kind || null,
      goalTitle: gameplay.goal_title || null,
      objective: gameplay.objective || null,
      progressDetail: gameplay.progress_detail || null,
      progressPercent,
      blockerKind: resolvedBlockerKind,
      blockerLabel,
      blockerDetail: resolvedBlockerDetail,
      blockerSupplementalDetail: emptyEntityBlocker && runtimeBlockerDetail && !runtimeAlreadyPublishedEmptyEntityBlocker
        ? runtimeBlockerDetail
        : null,
      nextStepHint: firstAgentClaimSyncPending ? emptyEntityBlocker.nextStepHint : runtimeAlreadyPublishedEmptyEntityBlocker
        ? gameplay.next_step_hint || emptyEntityBlocker?.nextStepHint || resumeNextStep || null
        : emptyEntityBlocker
          ? emptyEntityBlocker.nextStepHint
          : gameplay.next_step_hint || resumeNextStep || null,
      branchHint: gameplay.branch_hint || null,
      branchRecommendations,
      microDepotFacilities,
      validationUnlockPreview,
      factoryProductionFailureDisposition,
      industrialStarterProfile: normalizeIndustrialStarterProfile(gameplay.starter_industrial_feasibility),
      recipeSubstitution: normalizeRecipeSubstitution(gameplay.recipe_substitution),
      narrativeBlockerDetail,
      narrativeNextStep,
      economicSurface,
      controlProof,
      attractionProof,
      agencyMoves,
      progressionProof,
      fallbackTradeoffPreview,
      waitResolutionQuote,
      noSafeFallbackHandoff,
      matureWorldContinuation,
      shareReplay,
      entityCounts: {
        agents: agents.length,
        locations: locations.length,
      },
      availableActions: enrichedAvailableActions,
      recommendedAction: enrichedRecommendedAction,
      recentFeedback,
      agentClaim: clone(gameplay.agent_claim),
      assetGovernanceHandoff: isLocaleZh(locale)
        ? "资产 / 治理动作仍在单独 lane 处理；viewer 这里不会直接暴露主代币转账表单。"
        : "Asset/governance actions remain a separate lane. viewer exposes no main token transfer form here.",
    };
  }

  return {
    buildGameplaySummary,
    describePromptVersionState,
    describeSemanticFeedback,
    snapshotControlFeedback,
    snapshotSemanticFeedback,
  };
}
