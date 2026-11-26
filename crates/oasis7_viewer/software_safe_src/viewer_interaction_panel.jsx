import { For, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { AgentChatOwnerControls } from "./agent_chat_owner_controls.jsx";
import { AgentContextLite } from "./agent_context_lite.jsx";
import { buildAgentContextDisplayModel } from "./viewer_agent_context_display_model.js";
import { InteractionAdvancedPanels } from "./viewer_interaction_advanced_panels.jsx";
import {
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionDisplayLabel,
} from "./viewer_gameplay_action_helpers.js";
import {
  shouldShowStarterOcRequiredGate,
  starterOcAction,
} from "./viewer_starter_oc_controller.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

function normalizedId(value) {
  return String(value || "").trim();
}

export function InteractionPanel(props) {
  const {
    observeViewerStateRevision,
    describeAgentSessionStatus,
    readAgentContextFixtureMetadata,
    readAgentContextFixtureGameplay,
    viewerVisualFixtureNameFromQuery,
    viewerTestApiEnabled,
    renderGameplayAction,
    EmptyEntityRecoveryCard,
    PanelSection,
    FeedbackCard,
    EventCard,
    DiagnosticDetails,
    chatEntryCardClass,
    chatEntryTitle,
    chatEntryMeta,
    chatEntryMessage,
  } = props;
  const revision = () => observeViewerStateRevision();
  const locale = () => uiLocale();
  const selectedAgentId = () => {
    revision();
    return core.selectedAgentId();
  };
  const agentId = () => {
    const id = normalizedId(selectedAgentId());
    if (!id || !core.isAgentVisibleToCurrentSession(id)) {
      return null;
    }
    return id;
  };
  const selectedTarget = () => {
    revision();
    return core.state.selectedKind && core.state.selectedId
      ? { kind: core.state.selectedKind, id: core.state.selectedId }
      : null;
  };
  const selectedTargetLabel = () => core.state.selectedObject?.name || core.state.selectedObject?.label || core.state.selectedId;
  const gameplaySummary = () => {
    revision();
    return core.buildGameplaySummary(locale());
  };
  const authSurface = () => {
    revision();
    return core.buildAuthSurfaceModel();
  };
  const promptCapability = () => authSurface().capabilities.prompt_control;
  const chatCapability = () => authSurface().capabilities.agent_chat;
  const mainTokenTransferCapability = () => authSurface().capabilities.main_token_transfer;
  const mainTokenTransferPolicy = () => core.hostedActionPolicy("main_token_transfer");
  const binding = () => {
    revision();
    return core.selectedAgentBindingInfo();
  };
  const selectedAgentLabel = () => {
    const selected = core.state.snapshot?.model?.agents?.[agentId()];
    return selected?.name || selected?.label || agentId();
  };
  const selectedAgentActivity = () => core.state.snapshot?.model?.agents?.[agentId()]?.activity;
  const selectedAgentIntent = () => core.state.snapshot?.player_gameplay?.primary_intent;
  const selectedAgentIntentConnectionStatus = () => {
    const status = core.state.connectionStatus;
    // A software-safe canvas fallback can report a renderer diagnostic through
    // the shared error channel while the runtime transport remains usable.
    if (status === "error" && String(core.state.lastError || "").startsWith("pixel_world_host:")) {
      return "connected";
    }
    return status;
  };
  const selectedAgentFreshness = () => {
    revision();
    const selected = core.state.snapshot?.model?.agents?.[agentId()];
    const intent = selectedAgentIntent();
    const intentMatches = normalizedId(intent?.agent_id) === normalizedId(agentId())
      && (!intent?.target_agent_id || normalizedId(intent.target_agent_id) === normalizedId(agentId()));
    return selected?.freshness || (intentMatches ? intent?.freshness : null) || null;
  };
  const selectedAgentContextFixtureMetadata = () => readAgentContextFixtureMetadata(core.state.snapshot, viewerVisualFixtureNameFromQuery(), viewerTestApiEnabled());
  const selectedAgentContextFixtureGameplay = () => readAgentContextFixtureGameplay(core.state.snapshot, agentId(), selectedAgentContextFixtureMetadata());
  const selectedAgentContextModel = () => buildAgentContextDisplayModel({
    snapshot: core.state.snapshot,
    selected: { kind: core.state.selectedKind, id: core.state.selectedId },
    agent: agentId() ? core.state.snapshot?.model?.agents?.[agentId()] : null,
    agentVisible: Boolean(agentId()),
    gameplay: selectedAgentContextFixtureGameplay(),
    intent: selectedAgentIntent(),
    freshness: selectedAgentFreshness(),
    connectionStatus: selectedAgentIntentConnectionStatus(),
    feedback: selectedAgentFeedback(),
    receipt: null,
    locale: locale(),
  });
  const selectedAgentStatus = () => describeAgentSessionStatus(agentId(), locale());
  const canControlSelectedAgent = () => selectedAgentStatus().isCurrentSessionAgent;
  const selectedAgentControlReason = () => selectedAgentStatus().detail;
  const promptFeedback = () => {
    revision();
    return core.snapshotSemanticFeedback(core.state.lastPromptFeedback);
  };
  const chatFeedback = () => {
    revision();
    return core.snapshotSemanticFeedback(core.state.lastChatFeedback);
  };
  const promptFeedbackDisplay = () => core.describeSemanticFeedback(promptFeedback(), locale());
  const chatFeedbackDisplay = () => core.describeSemanticFeedback(chatFeedback(), locale());
  const selectedAgentFeedback = () => {
    const selectedId = normalizedId(agentId());
    if (!selectedId) {
      return null;
    }
    return [chatFeedback(), promptFeedback()]
      .find((feedback) => normalizedId(feedback?.agentId || feedback?.agent_id) === selectedId) || null;
  };
  const promptVersionState = () => core.describePromptVersionState(promptFeedback(), locale());
  const chatHistory = () => {
    revision();
    return core.state.chatHistory
      .filter((entry) => entry.agentId === agentId() || entry.targetAgentId === agentId())
      .slice(0, 12);
  };
  const interactionEnabled = () => promptCapability().enabled;
  const promptControlsEnabled = () => interactionEnabled() && canControlSelectedAgent();
  const chatControlsEnabled = () => {
    revision();
    return chatCapability().enabled && canControlSelectedAgent() && !core.isAgentChatInFlight();
  };
  const commandStarterOcAction = () => starterOcAction(gameplaySummary());
  const starterOcGateOpen = () => shouldShowStarterOcRequiredGate(gameplaySummary());
  const promptOverridesVisible = () => {
    revision();
    return !!core.state.promptOverridesVisible;
  };
  const assetLaneStatusText = () =>
    mainTokenTransferCapability().enabled
      ? tr(locale(), "仅预览", "preview_only")
      : mainTokenTransferCapability().code || "blocked";
  const assetLaneDetail = () =>
    mainTokenTransferCapability().enabled
      ? tr(
          locale(),
          "契约表明这个通道具备 strong_auth 级 main_token_transfer 能力，但观察器这里仍然不会直接暴露转账表单。",
          "Contract marks main_token_transfer as strong_auth-capable on this lane, but viewer still exposes no transfer form here.",
        )
      : mainTokenTransferCapability().reason;
  const promptSettingsSummary = () =>
    promptOverridesVisible()
      ? tr(
          locale(),
          "高级提示词设置已展开；你可以继续做预览、应用、回滚，页面也会显示最近一次反馈。",
          "Advanced prompt settings are expanded; preview/apply/rollback and the latest prompt feedback are visible.",
        )
      : tr(
          locale(),
          "提示词覆盖默认收起，避免把操作员级编辑控件直接堆在主入口。显式展开后仍可做预览、应用、回滚，`__AW_TEST__.sendPromptControl(...)` 也保持可用。",
          "Prompt Overrides stay hidden by default so operator-level editing controls do not dominate the primary entry. Expanding them keeps preview/apply/rollback available, and `__AW_TEST__.sendPromptControl(...)` remains available.",
        );
  const promptSettingsButtonLabel = () =>
    promptOverridesVisible()
      ? tr(locale(), "收起提示词覆盖", "Hide Prompt Overrides")
      : tr(locale(), "显示提示词覆盖", "Show Prompt Overrides");
  const playerSessionReadyCopy = () =>
    tr(
      locale(),
      "当前 Agent 已绑定到你的本地玩家会话。可以直接发送第一条聊天指令；提示词和资产治理能力先收在后置区域。",
      "This Agent is bound to your local player session. Send the first chat command here; prompt and asset/governance controls stay in the deferred area.",
    );
  const commandBoundaryCopy = () => canControlSelectedAgent() ? (interactionEnabled() ? playerSessionReadyCopy() : promptCapability().reason) : selectedAgentControlReason();
  const promptControlDisabledReason = () => canControlSelectedAgent() ? promptCapability().reason : selectedAgentControlReason();
  const promptRecoveryRequired = () => {
    const feedback = promptFeedback();
    return feedback?.kind === "prompt" && feedback?.stage === "blocked"
      && (feedback?.response?.value_visibility === "hidden" || !!String(feedback?.response?.next_step || "").trim());
  };
  return (
    <Show
      when={agentId()}
      fallback={
        selectedTarget()?.kind === "location"
          ? (
            <div class="stack command-surface command-surface--non-agent" data-command-target-kind="location">
              <div class="badge-row command-surface__target-row" data-command-continuity="target">
                <Badge class="badge badge--accent">{tr(locale(), "当前核查目标", "Current Inspect Target")}</Badge>
                <Badge>{selectedTargetLabel()}</Badge>
                <Badge>{`location=${selectedTarget()?.id}`}</Badge>
              </div>
              <AgentContextLite model={selectedAgentContextModel()} locale={locale()} fixtureMetadata={selectedAgentContextFixtureMetadata()} />
            </div>
          )
          : (
            <Show
              when={selectedAgentId()}
              fallback={
                <Show
                  when={gameplaySummary()?.blockerKind === "runtime_snapshot_empty_entities"}
                  fallback={<EmptyState>{tr(locale(), "先选中一个行动体，才能解锁提示词和聊天控制。", "Select an agent to unlock prompt/chat controls.")}</EmptyState>}
                >
                  <EmptyEntityRecoveryCard locale={locale()} gameplay={gameplaySummary} />
                </Show>
              }
            >
              <EmptyState>
                {tr(
                  locale(),
                  "当前账号还没有可操作的 Agent。请先认领你的第一个 Agent，或等待绑定同步完成。",
                  "This account has no controllable Agent yet. Claim your first Agent, or wait for binding sync to complete.",
                )}
              </EmptyState>
            </Show>
          )
      }
    >
      <div class="stack command-surface" data-command-agent={agentId()} data-command-chat-history={String(chatHistory().length)}>
      <div class="badge-row command-surface__target-row" data-command-continuity="target">
        <Badge class="badge badge--accent command-surface__target-secondary">{tr(locale(), "当前交互目标", "Current Target")}</Badge>
        <Badge>{selectedAgentLabel()}</Badge>
        <Badge class="command-surface__target-secondary">{`agent=${agentId()}`}</Badge>
        <Badge class={`${selectedAgentStatus().badgeClass} command-surface__target-secondary`}>{selectedAgentStatus().badge}</Badge>
        <Badge class={`${chatControlsEnabled() ? "badge badge--good" : "badge badge--warn"} command-surface__target-secondary`}>
          {chatControlsEnabled() ? tr(locale(), "聊天可用", "Chat Ready") : tr(locale(), "聊天受限", "Chat Limited")}
        </Badge>
        <div class="command-surface__continuity-summary" data-command-continuity="summary" role="group" aria-label={tr(locale(), "指挥连续性摘要", "Command continuity summary")}>
          <span><span class="metric__label">{tr(locale(), "状态", "Status")}</span> {selectedAgentContextModel().state?.label || tr(locale(), "不可用", "Unavailable")}</span>
          <span><span class="metric__label">{tr(locale(), "新鲜度", "Freshness")}</span> {selectedAgentContextModel().freshness?.label || tr(locale(), "不可用", "Unavailable")}</span>
          <span class="command-surface__continuity-objective"><span class="metric__label">{tr(locale(), "目标", "Objective")}</span> {selectedAgentContextModel().objective?.state === "published" && selectedAgentContextModel().objective.value ? selectedAgentContextModel().objective.value : tr(locale(), "目标不可用", "Objective unavailable")}</span>
        </div>
      </div>
      <AgentContextLite model={selectedAgentContextModel()} locale={locale()} fixtureMetadata={selectedAgentContextFixtureMetadata()} />
      <Show
        when={interactionEnabled() && canControlSelectedAgent()}
        fallback={
          <EmptyState class="command-surface__auth-boundary">
            {commandBoundaryCopy()}
          </EmptyState>
        }
      >
        <div class="badge-row command-surface__auth-boundary">
          <Badge class="badge badge--good">{authSurface().currentTier}</Badge>
          <Badge>{`player=${core.state.auth.playerId}`}</Badge>
          <Badge>{`source=${authSurface().source}`}</Badge>
        </div>
        <EmptyState class="command-surface__auth-boundary">{playerSessionReadyCopy()}</EmptyState>
      </Show>
      <details class="diagnostic command-surface__capability-details">
        <summary>{tr(locale(), "能力诊断", "Capability Diagnostics")}</summary>
        <div class="badge-row command-surface__capability-row command-surface__diagnostic-strip">
        <Badge class="badge badge--diagnostic">{`boundPlayer=${binding()?.playerId || "-"}`}</Badge>
        <Badge class="badge badge--diagnostic">{`boundKey=${binding()?.publicKey ? `${binding().publicKey.slice(0, 10)}…` : "-"}`}</Badge>
        <Badge class={promptControlsEnabled() ? "badge badge--good" : "badge badge--warn"}>
          {`prompt=${promptControlsEnabled() ? "enabled" : promptCapability().code || "agent_not_bound"}`}
        </Badge>
        <Badge class={chatControlsEnabled() ? "badge badge--good" : "badge badge--warn"}>
          {`chat=${chatControlsEnabled() ? "enabled" : chatCapability().code || "agent_not_bound"}`}
        </Badge>
        <Badge class={mainTokenTransferCapability().enabled ? "badge badge--good" : "badge badge--warn"}>
          {`mainToken=${assetLaneStatusText()}`}
        </Badge>
        </div>
      </details>
      <Show when={!starterOcGateOpen() && canControlSelectedAgent() && commandStarterOcAction()}>
        {(action) => (
          <div class="toolbar">
            <button
              class={gameplayActionButtonClass(action())}
              aria-busy={gameplayActionButtonBusyAttrs(action())}
              disabled={gameplayActionButtonDisabled(action(), gameplaySummary(), locale())}
              onClick={() => renderGameplayAction(action())}
            >
              {gameplayActionDisplayLabel(action(), locale())}
            </button>
          </div>
        )}
      </Show>
      <PanelSection
        class="command-surface__chat-panel"
        title={tr(locale(), "行动体聊天", "Agent Chat")}
        eyebrow={tr(locale(), "指挥面板", "Command Surface")}
        meta={tr(locale(), "向当前目标发消息并读回复。", "Message the current target and read replies.")}
      >
        <div class="field">
          <AgentChatOwnerControls core={core} agentId={agentId()} locale={locale()} />
          <label for="agent-chat-message">{tr(locale(), "消息", "Message")}</label>
              <textarea
                id="agent-chat-message"
                rows="4"
                placeholder={tr(locale(), "给当前选中的行动体发一条消息", "Send a message to the selected agent")}
                disabled={!chatControlsEnabled()}
                value={core.state.chatDraft.message}
                onInput={(event) => {
              core.state.chatDraft.message = String(event.currentTarget.value || "");
              core.state.chatDraft.dirty = true;
            }}
          />
        </div>
        <div class="toolbar">
            <button
              data-chat-send="1"
              disabled={!chatControlsEnabled()}
              onClick={() => core.sendAgentChat(agentId(), core.state.chatDraft.message)}
            >
            {tr(locale(), "发送聊天", "Send Chat")}
          </button>
        </div>
        <Show when={chatFeedback()} fallback={<EmptyState>{tr(locale(), "还没有聊天反馈。", "No chat feedback yet.")}</EmptyState>}>
          {(feedback) => <FeedbackCard feedback={feedback()} display={chatFeedbackDisplay()} recoveryAction={() => void core.refreshPromptControlBinding()} recoveryLabel={tr(locale(), "刷新权限与 Agent 绑定", "Refresh authority and Agent binding")} />}
        </Show>
        <div>
          <div class="panel__title panel__title--spaced">{tr(locale(), "消息流", "Message Flow")}</div>
          <div class="event-list">
            <Show when={chatHistory().length > 0} fallback={<EmptyState>{tr(locale(), "这个行动体还没有聊天历史。", "No chat history for this agent yet.")}</EmptyState>}>
              <For each={chatHistory()}>
                {(entry) => (
                  <EventCard
                    class={chatEntryCardClass(entry)}
                    title={chatEntryTitle(entry, locale())}
                    badge={`tick=${Number(entry.tick || 0)}`}
                    meta={chatEntryMeta(entry, locale())}
                  >
                    <div class="feedback-summary">{chatEntryMessage(entry, locale())}</div>
                    <DiagnosticDetails value={entry} />
                  </EventCard>
                )}
              </For>
            </Show>
          </div>
        </div>
      </PanelSection>
      <InteractionAdvancedPanels
        locale={locale}
        PanelSection={PanelSection}
        FeedbackCard={FeedbackCard}
        promptVersionState={promptVersionState}
        promptOverridesVisible={promptOverridesVisible}
        promptSettingsSummary={promptSettingsSummary}
        canControlSelectedAgent={canControlSelectedAgent}
        promptSettingsButtonLabel={promptSettingsButtonLabel}
        promptControlsEnabled={promptControlsEnabled}
        promptControlDisabledReason={promptControlDisabledReason}
        authSurface={authSurface}
        promptFeedback={promptFeedback}
        promptFeedbackDisplay={promptFeedbackDisplay}
        promptRecoveryRequired={promptRecoveryRequired}
        mainTokenTransferCapability={mainTokenTransferCapability}
        assetLaneStatusText={assetLaneStatusText}
        mainTokenTransferPolicy={mainTokenTransferPolicy}
        assetLaneDetail={assetLaneDetail}
      />
      </div>
    </Show>
  );
}
