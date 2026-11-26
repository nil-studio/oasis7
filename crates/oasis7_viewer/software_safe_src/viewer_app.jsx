import {
  CalloutCard,
  ClaimAgentChoiceCard,
  DiagnosticDetails,
  EntityListPendingState,
  EventCard,
  ExpansionTradeoffCards,
  FeedbackCard,
  InlineHelpTip,
  JsonBlock,
  MetricCard,
  PanelSection,
  normalizedFeedbackStage,
} from "./viewer_ui_cards.jsx";
import { TargetsPanel as TargetsPanelView } from "./viewer_targets_panel.jsx";
import {
  EmptyEntityRecoveryCard as EmptyEntityRecoveryCardView,
  HostedLoginForm,
  HostedLoginGate as HostedLoginGateView,
  ViewerEntryMenu as ViewerEntryMenuView,
  shouldShowHostedLoginGate,
} from "./viewer_hosted_login_gate.jsx";
import { WorldStageHero as WorldStageHeroView } from "./viewer_world_stage_hero.jsx";
import {
  StarterOcGuide,
  StarterOcOnboardingPanel as StarterOcOnboardingPanelView,
  StarterOcRequiredGate as StarterOcRequiredGateView,
} from "./viewer_starter_oc_panels.jsx";
import {
  AgentClaimPanel as AgentClaimPanelView,
  AgentClaimSessionBoundaryCard as AgentClaimSessionBoundaryCardView,
} from "./viewer_agent_claim_panel.jsx";
import { InteractionPanel as InteractionPanelView } from "./viewer_interaction_panel.jsx";
import { InteractionAdvancedPanels } from "./viewer_interaction_advanced_panels.jsx";
import { DetailsPanel } from "./viewer_details_panel.jsx";
import {
  claimField,
  compactValue,
  claimMoney,
  claimQuoteRows,
  claimQuoteMetricClass,
  claimTarget,
  claimStatusText,
  claimOwnedDetail,
  releaseClaimActionState,
  expansionBranchCards,
} from "./viewer_claim_formatters.js";
import {
  starterOcAction,
  touchStarterOcOnboardingState,
  markStarterOcClaimPending,
  scheduleStarterOcBackgroundConfirmation,
  clearStarterOcClaimPending,
  completeStarterOcOnboarding,
  __markStarterOcOnboardingCompleteForTest,
  starterOcClaimPendingForCurrentAgent,
  starterOcOnboardingCompletedForCurrentAgent,
  starterOcCreditVisibleForCurrentAgent,
  rawStarterOcActionAvailable,
  starterOcSubmittedFeedback,
  starterOcFeedbackNeedsLocalAdvance,
  shouldShowStarterOcRequiredGate,
  visibleGameplayActionsForPanels,
  gameplayProgressionAction,
  firstAgentChatAction,
} from "./viewer_starter_oc_controller.js";
export { __markStarterOcOnboardingCompleteForTest };
import {
  gameplayStatusBadgeClass,
  gameplayStageToneClass,
  gameplayStageLabel,
  goalExecutionBadgeClass,
  gameplayActionKey,
  gameplayActionBlockedReasonId,
  gameplayActionFeedbackMatches,
  clearGameplayActionPending,
  markGameplayActionPending,
  gameplayActionFeedbackInFlight,
  gameplayActionPendingFor,
  isPendingFirstAgentClaimSync,
  gameplayActionControlBoundaryReason,
  gameplayActionDisabledReason,
  gameplayActionButtonLabel,
  gameplayActionBusyLabel,
  gameplayActionDisplayLabel,
  gameplayActionButtonClass,
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonDisabled,
  gameplayActionTestId,
  gameplayActionDetail,
} from "./viewer_gameplay_action_helpers.js";
import { targetSyncProgressLines } from "./viewer_target_sync_progress.js";
import { LiveControlStatus } from "./viewer_live_control_status.jsx";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { RecipeSubstitutionPanel } from "./recipe_substitution_panel.jsx";
import { viewerRuntimeConfig } from "./viewer_runtime_config_module.js"; import { authSigningIdentity } from "./viewer_auth_session_module.js";
import { BrowserCanonicalEntry } from "./browser_canonical_entry.jsx";
import { AgentChatOwnerControls } from "./agent_chat_owner_controls.jsx";
import { IndustrialStarterOutcomeCard } from "./industrial_starter_outcome_card.jsx";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { TargetListWindow, createTargetListProjection } from "./target_list_window.jsx";
import { render as mount } from "solid-js/web";
import * as core from "./legacy_core.js";
import { FirstChatUnlockPreview } from "./first_chat_unlock_preview.jsx";
import { PixelWorldHost } from "./pixel_world_host.jsx"; import { WorldFeedSurface } from "./world_feed_integration.jsx";
import { MobileJumpRail, SecondaryViewerNavigation, focusViewerAnchor, installViewerRouteController } from "./viewer_navigation.jsx";
import { DirectorEntryCard, DirectorSurface } from "./director_surface.jsx"; import { createViewerDirectorSession } from "./director_viewer_integration.js";
import { MicroDepotFacilitiesPanel } from "./micro_depot_facilities_panel.jsx";
import { RecoveryOptionComparisonPanel } from "./recovery_option_comparison_panel.jsx"; import { FallbackTradeoffPanel } from "./fallback_tradeoff_panel.jsx"; import { WaitResolutionQuoteCard } from "./wait_resolution_quote_card.jsx";
import { FragmentRefillPreviewGameplayPanel, GovernanceVoteQuoteGameplayPanel, MarketQuoteDecisionGameplayPanel, PowerSaleQuoteGameplayPanel, PowerSurvivalQuoteGameplayPanel, ProductValidationQuoteGameplayPanel, RefineQuoteGameplayPanel, ScheduleRecipeQuoteGameplayPanel, TransferMaterialQuoteGameplayPanel, WarDeclarationQuoteGameplayPanel } from "./gameplay_quote_panels.jsx";
import { ReprioritizeActionForm } from "./reprioritize_action_form.jsx";
import { createViewerAgentClaimDisplayModel } from "./viewer_agent_claim_display_model.js";
import { AgentClaimChoiceCard } from "./agent_claim_choice_card.jsx";
import { FirstDeliveryPreview } from "./first_delivery_preview.jsx";
import {
  HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  isHostedPublicJoinDeploymentMode,
} from "./software_safe_constants.js";
import { AgentActivitySurface } from "./agent_activity_surface.jsx"; import { HostedTestLoginOptIn } from "./viewer_hosted_test_login_view.jsx";
import { AgentIntentSurface } from "./agent_intent_surface.jsx";
import { FactoryProductionFailureDispositionCard } from "./factory_production_failure_disposition_card.jsx";
import { ControlProofPanel } from "./control_proof_panel.jsx";
import { PasskeyAccountSurface, hostedAccountGateCopy } from "./viewer_passkey_account_surface.jsx";
import { keepFocusInDialog } from "./viewer_modal_focus.js";
import { AgentContextLite } from "./agent_context_lite.jsx";
import { buildAgentContextDisplayModel } from "./viewer_agent_context_display_model.js";
import { pixelWorldBlockerPresentation, pixelWorldConnectionPresentation } from "./pixel_world_presentation.js";
import { pixelWorldReadableModuleLabel, pixelWorldSelectedEntityLabel } from "./pixel_world_identity.js";
const [viewerStateRevision, setViewerStateRevision] = createSignal(0);
function observeViewerStateRevision() {
  viewerStateRevision();
}
function uiLocale() { return core.state.uiLocale; }
function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}
function localeCode(locale) {
  return core.isLocaleZh(locale) ? "zh" : "en";
}
function buildViewerEntryUrls(locale) {
  const softwareSafeUrl = new URL(window.location.href);
  softwareSafeUrl.searchParams.set("locale", localeCode(locale));
  softwareSafeUrl.searchParams.delete("language");
  return {
    softwareSafeUrl: softwareSafeUrl.toString(),
  };
}
const {
  agentBindingForId,
  agentClaimUsesCurrentBoundAgent,
  buildAgentClaimAction,
  buildAgentClaimTargets,
  describeAgentSessionStatus,
  hasAgentClaimSessionBoundary,
  hasExecutableAgentClaim,
  normalizedId,
  publishedClaimChoiceCandidates,
  slot1ClaimChoiceQuote,
} = createViewerAgentClaimDisplayModel({ state: core.state, tr });

function HostedLoginGate() {
  return <HostedLoginGateView observeViewerStateRevision={observeViewerStateRevision} />;
}
function EmptyEntityRecoveryCard(props) {
  return <EmptyEntityRecoveryCardView {...props} CalloutCard={CalloutCard} />;
}
function ViewerEntryMenu() {
  return <ViewerEntryMenuView observeViewerStateRevision={observeViewerStateRevision} buildViewerEntryUrls={buildViewerEntryUrls} />;
}
function StarterOcOnboardingPanel(props) {
  return <StarterOcOnboardingPanelView {...props} FeedbackCard={FeedbackCard} renderGameplayAction={renderGameplayAction} />;
}
function StarterOcRequiredGate() {
  return <StarterOcRequiredGateView observeViewerStateRevision={observeViewerStateRevision} FeedbackCard={FeedbackCard} renderGameplayAction={renderGameplayAction} />;
}
function renderGameplayAction(action) {
  if (action.executeKind === "agent_chat") {
    core.applySelection({ kind: "agent", id: action.targetAgentId });
    return;
  }
  markGameplayActionPending(action, gameplayActionButtonLabel(action, uiLocale()));
  if (action.actionId === "claim_starter_oc") {
    markStarterOcClaimPending(action);
  }
  const result = core.sendGameplayAction(action);
  if (result && result.ok === false) {
    clearGameplayActionPending(action);
  } else if (
    result
    && result.ok === true
    && !result.feedback
    && action.executeKind !== "request_snapshot"
    && !(starterOcClaimPendingForCurrentAgent() && ["step", "play"].includes(action.executeKind))
  ) {
    clearGameplayActionPending(action);
  }
  if (action.actionId === "claim_starter_oc" && result && result.ok === false) {
    clearStarterOcClaimPending();
  } else if (action.actionId === "claim_starter_oc") {
    scheduleStarterOcBackgroundConfirmation();
    core.requestRender();
  }
  return result;
}

function AgentClaimSessionBoundaryCard(props) {
  return <AgentClaimSessionBoundaryCardView {...props} CalloutCard={CalloutCard} />;
}
function AgentClaimPanel(props) {
  return <AgentClaimPanelView {...props} CalloutCard={CalloutCard} renderGameplayAction={renderGameplayAction} />;
}
function gameplayProgressLabel(progressPercent, locale) {
  return progressPercent == null
    ? tr(locale, "进度待发布", "Progress Pending")
    : tr(locale, `进度 ${progressPercent}%`, `Progress ${progressPercent}%`);
}

function chatEntryTitle(entry, locale) {
  const target = entry.targetAgentId || entry.agentId || "agent";
  if (entry.source === "error") {
    return `${target} ${tr(locale, "回复失败", "reply failed")}`;
  }
  if (entry.source === "player") {
    return `${tr(locale, "玩家", "Player")} -> ${target}`;
  }
  return `${entry.agentId || target} ${tr(locale, "回应", "Reply")}`;
}

function chatEntryCardClass(entry) {
  if (entry.source === "error") return "event-card event-card--chat-error";
  if (entry.source === "player") return "event-card event-card--chat-player";
  return "event-card event-card--chat-agent";
}

function chatEntryMeta(entry, locale) {
  if (entry.source === "error") {
    const code = entry.code ? ` · code=${entry.code}` : "";
    return `${entry.speaker || "runtime"}${code} · tick=${Number(entry.tick || 0)}`;
  }
  const speaker = entry.source === "player"
    ? entry.playerId || entry.speaker || tr(locale, "玩家", "Player")
    : entry.speaker || entry.agentId || "agent";
  const location = entry.locationId || tr(locale, "未知位置", "unknown location");
  return `${speaker} · ${location}`;
}

function chatEntryMessage(entry, locale) {
  const message = String(entry.message || "").trim();
  if (entry.source === "error") {
    const prefix = tr(locale, "Agent 回复没有完成", "Agent reply did not complete");
    return message ? `${prefix}: ${message}` : prefix;
  }
  return message || tr(locale, "这条消息没有可读正文。", "This message has no readable text.");
}

function connectionStatusLabel(status, locale) {
  return pixelWorldConnectionPresentation(status, locale).label;
}

function renderResourceSummary(resources) {
  return core.resourceSummary(resources);
}

function WorldStageHero() {
  return (
    <WorldStageHeroView
      observeViewerStateRevision={observeViewerStateRevision}
      CalloutCard={CalloutCard}
      EmptyEntityRecoveryCard={EmptyEntityRecoveryCard}
      InlineHelpTip={InlineHelpTip}
      ViewerEntryMenu={ViewerEntryMenu}
      connectionStatusLabel={connectionStatusLabel}
      gameplayProgressLabel={gameplayProgressLabel}
      renderGameplayAction={renderGameplayAction}
    />
  );
}
function TargetsPanel() {
  return (
    <TargetsPanelView
      observeViewerStateRevision={observeViewerStateRevision}
      StarterOcOnboardingPanel={StarterOcOnboardingPanel}
      renderGameplayAction={renderGameplayAction}
      describeAgentSessionStatus={describeAgentSessionStatus}
      renderResourceSummary={renderResourceSummary}
    />
  );
}

function WorldSummaryPanel(props = {}) {
  observeViewerStateRevision();
  const locale = () => uiLocale();
  const state = core.state;
  const gameplaySummary = () => core.buildGameplaySummary(locale());
  const starterOcGateOpen = () => shouldShowStarterOcRequiredGate(gameplaySummary());
  const gameplayActionFeedback = () => core.snapshotSemanticFeedback(state.lastGameplayActionFeedback);
  const promptFeedback = () => core.snapshotSemanticFeedback(state.lastPromptFeedback);
  const chatFeedback = () => core.snapshotSemanticFeedback(state.lastChatFeedback);
  const gameplayActionFeedbackDisplay = () => core.describeSemanticFeedback(gameplayActionFeedback(), locale());
  const promptFeedbackDisplay = () => core.describeSemanticFeedback(promptFeedback(), locale());
  const chatFeedbackDisplay = () => core.describeSemanticFeedback(chatFeedback(), locale());
  const authSurface = () => core.buildAuthSurfaceModel();
  const hostedActionMatrixView = () => core.buildHostedActionMatrixView();
  const hostedRecoveryHint = () => core.buildHostedRecoveryHint(locale());
  const tierBadgeClass = (status) =>
    status === "active"
      || status === "active_legacy_preview"
      || status === "active_hosted_issue"
      || status === "active_hosted_session"
      || status === "preview_backend_reauth_available"
      ? "badge badge--good"
      : status === "issued_pending_register" || status === "upgrade_after_player_session" || status === "preview_only"
        ? "badge badge--accent"
      : status === "superseded"
        ? "badge"
        : "badge badge--warn";
  const showRebindNotice = () =>
    Boolean(state.auth.pendingRequestedAgentId)
      && (state.auth.pendingForceRebind
        || state.auth.runtimeStatus === "rebind_retrying"
        || state.auth.runtimeStatus === "rebind_registering");
  const showPlayerSessionSurface = () =>
    !!hostedRecoveryHint()
      || (
        !state.auth.available
        && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
      )
      || showRebindNotice();
  const diagnosticsSummaryBadges = () => [
    `auth=${state.auth.available ? state.auth.registrationStatus || "ready" : "missing"}`,
    `events=${state.recentEvents.length}`,
  ];

  return (
    <div data-viewer-overlay="world-summary">
      <Show when={!starterOcGateOpen() && starterOcAction(gameplaySummary())}>
        <CalloutCard
          title={tr(locale(), "领取第一笔 OC", "Claim Your First OC")}
          badge="ready"
          badgeClass="badge badge--good"
        >
          <StarterOcOnboardingPanel gameplay={gameplaySummary()} locale={locale()} />
        </CalloutCard>
      </Show>
      <details class="gameplay-details-surface" id="viewer-gameplay-details">
      <summary class="gameplay-details-surface__summary">
        <div class="diagnostic-surface__title">
          <span>{tr(locale(), "玩法明细", "Gameplay Details")}</span>
          <span class="diagnostic-surface__meta">
            {tr(locale(), "世界棋盘上方已保留目标、下一步和回执；这里展开看完整状态机与经济明细。", "The world board already carries objective, next move, and receipt; expand here for the full state machine and economy details.")}
          </span>
        </div>
        <Badge>{diagnosticsSummaryBadges().join(" · ")}</Badge>
      </summary>
      <div class="stack flow-top">
        <PanelSection
          title={tr(locale(), "正式玩法摘要", "Formal Gameplay Summary")}
          eyebrow={tr(locale(), "玩家主路径", "Player Path")}
          meta={tr(locale(), "先看目标、阻塞和下一步，再决定是否进入指挥面板。", "Read the goal, blocker, and next step first, then decide whether to enter the command surface.")}
        >
        <Show
          when={gameplaySummary()}
          fallback={<EmptyState>{tr(locale(), "等待首条规范玩法快照…", "Waiting for the first canonical gameplay snapshot…")}</EmptyState>}
        >
          {(gameplay) => (
            <>
              <div class="badge-row">
                <Badge class={gameplayStatusBadgeClass(gameplay().stageStatus)}>
                  {gameplayStageLabel(gameplay().stageStatus, locale())}
                </Badge>
                <Badge class="badge badge--accent">
                  {gameplayProgressLabel(gameplay().progressPercent, locale())}
                </Badge>
              </div>
              <RecipeSubstitutionPanel model={gameplay().recipeSubstitution} locale={locale()} />
              <FactoryProductionFailureDispositionCard
                disposition={gameplay().factoryProductionFailureDisposition}
                locale={locale()}
                localeText={tr}
                onAction={renderGameplayAction}
              />
              <ControlProofPanel proof={gameplay().controlProof} locale={locale()} tr={tr} fixture={viewerVisualFixtureNameFromQuery() === "control_proof_applied"} />
              <PanelSection
                title={tr(locale(), "吸引力证明", "Attraction Proof")}
                eyebrow={tr(locale(), "TASK-GAME-076: 0-30 分钟", "TASK-GAME-076: 0-30 Minutes")}
                meta={tr(locale(), "只从 canonical player_gameplay 派生首局吸引力证据；缺失项会显示为等待或未验证。", "Derives first-session attraction evidence only from canonical player_gameplay; missing signals stay waiting or unverified.")}
              >
                <div class="badge-row">
                  <Badge>{gameplay().attractionProof?.verdict || "unverified"}</Badge>
                </div>
                <div class="feedback-summary">
                  {gameplay().attractionProof?.summary
                    || tr(locale(), "等待吸引力证据发布。", "Waiting for attraction proof.")}
                </div>
                <div class="summary-grid">
                  <MetricCard
                    label={tr(locale(), "我造成了什么", "What I caused")}
                    value={gameplay().attractionProof?.whatICaused || tr(locale(), "等待玩家导致的世界变化", "waiting for player-caused world change")}
                  />
                  <MetricCard
                    label={tr(locale(), "新选择", "New option")}
                    value={gameplay().attractionProof?.newOption || tr(locale(), "等待新选择", "waiting for new option")}
                  />
                  <MetricCard
                    label={tr(locale(), "为什么继续", "Why continue")}
                    value={gameplay().attractionProof?.whyContinue || tr(locale(), "等待下一分支", "waiting for next branch")}
                  />
                  <MetricCard
                    label={tr(locale(), "等待代价", "Waiting cost")}
                    value={gameplay().attractionProof?.waitingCost || tr(locale(), "等待 / 未验证", "waiting/unverified")}
                  />
                  <MetricCard
                    label={tr(locale(), "恢复", "Recovery")}
                    value={gameplay().attractionProof?.recovery || tr(locale(), "等待恢复路径", "waiting for recovery path")}
                  />
                </div>
              </PanelSection>
              <PanelSection
                title={tr(locale(), "玩家能动性动词", "Agency Moves")}
                eyebrow={tr(locale(), "P1: 打断 / 重排 / 纠偏", "P1: Interrupt / Reprioritize / Correct")}
                meta={tr(locale(), "只展示已由玩法快照发布或可从现有状态推导的能动性入口，不在 viewer 里伪造新动作。", "Shows only agency entries published by the gameplay snapshot or derived from current state; the viewer does not invent new actions.")}
              >
                <div class="feedback-summary">
                  {gameplay().agencyMoves?.summary
                    || tr(locale(), "等待玩家能动性动词发布。", "Waiting for player agency moves.")}
                </div>
                <div class="summary-grid">
                  <MetricCard
                    label={tr(locale(), "打断", "Interrupt")}
                    value={gameplay().agencyMoves?.interrupt || tr(locale(), "未验证", "unverified")}
                  />
                  <MetricCard
                    label={tr(locale(), "重排", "Reprioritize")}
                    value={gameplay().agencyMoves?.reprioritize || tr(locale(), "未验证", "unverified")}
                  />
                  <MetricCard
                    label={tr(locale(), "纠偏", "Correction")}
                    value={gameplay().agencyMoves?.correction || tr(locale(), "等待替代意图", "waiting for replacement intent")}
                  />
                  <MetricCard
                    label={tr(locale(), "交接结果", "Handoff")}
                    value={gameplay().agencyMoves?.handoff || tr(locale(), "等待新旧意图交接", "waiting for handoff")}
                  />
                </div>
              </PanelSection>
              <PanelSection
                title={tr(locale(), "首胜与反刷", "First Win & Anti-Grind")}
                eyebrow={tr(locale(), "P1: 小玩家第一场工业胜利", "P1: Small-Player First Industrial Win")}
                meta={tr(locale(), "把玩家动作、世界变化和 leverage 类型放在一起，避免首胜只变成产量数字。", "Pairs player action, world change, and leverage class so the first win is not reduced to output volume.")}
              >
                <div class="feedback-summary">
                  {gameplay().progressionProof?.summary
                    || tr(locale(), "等待首胜与反刷证据发布。", "Waiting for first-win and anti-grind evidence.")}
                </div>
                <div class="summary-grid">
                  <MetricCard
                    label={tr(locale(), "首胜目标", "First Win")}
                    value={gameplay().progressionProof?.firstWinGoal || tr(locale(), "待发布", "not published")}
                  />
                  <MetricCard
                    label={tr(locale(), "玩家动作", "Player Action")}
                    value={gameplay().progressionProof?.playerAction || tr(locale(), "待提交", "not submitted")}
                  />
                  <MetricCard
                    label={tr(locale(), "世界变化", "World Change")}
                    value={gameplay().progressionProof?.worldChange || tr(locale(), "待回执", "waiting for receipt")}
                  />
                  <MetricCard
                    label={tr(locale(), "反刷 leverage", "Anti-Grind Leverage")}
                    value={gameplay().progressionProof?.antiGrind || tr(locale(), "待验证", "unverified")}
                    detail={gameplay().progressionProof?.leverageVerdict}
                  />
                </div>
              </PanelSection>
              <PanelSection
                title={tr(locale(), "成熟世界承接", "Mature-World Continuation")}
                eyebrow={tr(locale(), "P2: 修复 / 重建 / 转向", "P2: Repair / Rebuild / Pivot")}
                meta={tr(locale(), "呈现世界变复杂之后，小玩家是否仍有独立承接路径和可复盘短故事。", "Shows whether small players retain independent continuation paths and replayable story evidence after the world becomes complex.")}
              >
                <div class="feedback-summary">
                  {gameplay().matureWorldContinuation?.summary
                    || tr(locale(), "等待成熟世界承接证据发布。", "Waiting for mature-world continuation evidence.")}
                </div>
                <div class="summary-grid">
                  <MetricCard
                    label={tr(locale(), "依赖状态", "Dependency")}
                    value={gameplay().matureWorldContinuation?.dependencyStatus || tr(locale(), "未验证", "unverified")}
                  />
                  <MetricCard
                    label={tr(locale(), "恢复路径", "Recovery Path")}
                    value={gameplay().matureWorldContinuation?.recoveryPath || tr(locale(), "等待运行时指引", "waiting for runtime guidance")}
                  />
                  <MetricCard
                    label={tr(locale(), "分享回放", "Share Replay")}
                    value={gameplay().shareReplay?.snippet || tr(locale(), "等待可复盘片段", "waiting for replayable snippet")}
                    detail={gameplay().shareReplay?.summary}
                  />
                </div>
                <RecoveryOptionComparisonPanel
                  continuation={gameplay().matureWorldContinuation}
                  locale={locale()}
                  tr={tr}
                />
              </PanelSection>
              <EventCard
                title={tr(locale(), "已接受意图", "Accepted Intent")}
                badge={gameplay().acceptedIntentScope || gameplay().executionStateLabel || "-"}
                badgeClass={goalExecutionBadgeClass(gameplay().executionState)}
                meta={
                  gameplay().acceptedIntentTarget
                    ? tr(locale(), `当前作用对象 ${gameplay().acceptedIntentTarget}`, `Current target ${gameplay().acceptedIntentTarget}`)
                    : tr(locale(), "当前主意图", "Current primary intent")
                }
              >
                <div class="feedback-summary">{gameplay().acceptedIntentSummary}</div>
                <div class="feedback-detail">{gameplay().acceptedIntentDetail}</div>
                <Show when={gameplay().resumeAnchor}>
                  <div class="badge-row">
                    <Badge>{tr(locale(), "续玩锚点", "Resume Anchor")}</Badge>
                  </div>
                  <div class="feedback-detail">{gameplay().resumeAnchor}</div>
                </Show>
              </EventCard>
              <EventCard
                title={tr(locale(), "目标执行状态", "Goal Execution")}
                badge={gameplay().executionStateLabel || gameplay().executionState || "-"}
                badgeClass={goalExecutionBadgeClass(gameplay().executionState)}
                meta={tr(locale(), "统一状态机：Accepted -> Executing -> Blocked / Completed / Rejected", "Unified state machine: Accepted -> Executing -> Blocked / Completed / Rejected")}
              >
                <div class="badge-row">
                  <For each={gameplay().executionStateMachine || []}>
                    {(item) => (
                      <Badge class={gameplay().executionState === item.id ? goalExecutionBadgeClass(item.id) : "badge"}>
                        {item.label}
                      </Badge>
                    )}
                  </For>
                </div>
                <div class="feedback-summary">
                  {gameplay().executionSummary
                    || tr(locale(), "等待目标执行状态更新。", "Waiting for goal execution state updates.")}
                </div>
                <Show when={gameplay().executionCauseLabel}>
                  <div class="badge-row">
                    <Badge>{gameplay().executionCauseLabel}</Badge>
                  </div>
                </Show>
                <Show when={gameplay().executionCauseDetail}>
                  <div class="feedback-detail">{gameplay().executionCauseDetail}</div>
                </Show>
              </EventCard>
              <EventCard
                title={gameplay().goalTitle || tr(locale(), "当前目标", "Current Goal")}
                badge={gameplay().progressPercent == null ? "n/a" : `${gameplay().progressPercent}%`}
                badgeClass="badge badge--accent"
                meta={gameplay().objective || tr(locale(), "当前还没有目标说明。", "No objective text yet.")}
              >
                <Show when={gameplay().progressDetail}>
                  <div class="feedback-detail">{gameplay().progressDetail}</div>
                </Show>
                <Show when={gameplay().blockerKind || gameplay().narrativeBlockerDetail}>
                  <div class="badge-row badge-row--spaced">
                    <Badge class="badge badge--warn">
                      {pixelWorldBlockerPresentation(gameplay().blockerKind, locale()).label}
                    </Badge>
                  </div>
                  <div class="feedback-detail">
                    {gameplay().narrativeBlockerDetail || tr(locale(), "当前玩法被阻塞，需要显式恢复。", "Gameplay is blocked and needs explicit recovery.")}
                  </div>
                </Show>
                <Show when={gameplay().blockerSupplementalDetail}>
                  <div class="feedback-detail">{gameplay().blockerSupplementalDetail}</div>
                </Show>
                <div class="badge-row badge-row--spaced">
                  <Badge class="badge badge--accent">{tr(locale(), "下一步", "Next Step")}</Badge>
                </div>
                <div class="feedback-summary">
                  {gameplay().narrativeNextStep || tr(locale(), "等待下一次运行时指引更新。", "Wait for the next runtime guidance update.")}
                </div>
                <Show when={gameplay().branchHint}>
                  <div class="feedback-detail">{gameplay().branchHint}</div>
                </Show>
                <Show when={gameplay().entityCounts}>
                  <div class="badge-row">
                    <Badge>{`agents=${gameplay().entityCounts.agents}`}</Badge>
                    <Badge>{`locations=${gameplay().entityCounts.locations}`}</Badge>
                  </div>
                </Show>
              </EventCard>
              <WaitResolutionQuoteCard quote={gameplay().waitResolutionQuote} locale={locale()} tr={tr} />
              <FallbackTradeoffPanel options={gameplay().fallbackTradeoffPreview} noSafeFallbackHandoff={gameplay().noSafeFallbackHandoff} locale={locale()} tr={tr} />
              <Show when={gameplay().validationUnlockPreview}>
                {(preview) => (
                  <EventCard
                    title={tr(locale(), "产品验证预览", "Product Validation Preview")}
                    badge={preview().stageStatusLabel || "unknown"}
                    badgeClass={preview().stageStatus === "available" ? "badge badge--good" : "badge badge--warn"}
                    meta={preview().localizedValueSummary || tr(locale(), "验证结果未声明新的能力；请根据现有角色和阶段决定下一步。", "The validation result declares no new capability; use the existing role and stage to choose the next move.")}
                  >
                    <div class="feedback-summary" data-testid="validation-unlock-preview">
                      {`${preview().productId || tr(locale(), "未知产品", "Unknown product")} · ${preview().roleLabel || tr(locale(), "未知", "unknown")} · ${preview().tradable ? tr(locale(), "可交易", "tradable") : tr(locale(), "不可交易", "not tradable")}`}
                    </div>
                    <div class="feedback-detail">
                      {`${tr(locale(), "阶段", "Stage")}: ${preview().currentStageLabel || tr(locale(), "未知", "unknown")} / ${preview().requiredStageLabel || tr(locale(), "未知", "unknown")}`}
                    </div>
                    <Show when={preview().localizedNextStepHint}>
                      <div class="feedback-detail">{preview().localizedNextStepHint}</div>
                    </Show>
                  </EventCard>
                )}
              </Show>
              <PanelSection
                title={tr(locale(), "能力经济可读性", "Capability Economics")}
                eyebrow={tr(locale(), "下一步会带来什么", "What The Next Move Changes")}
                meta={tr(locale(), "把当前玩法拆成投入、产出、新用途、操作指引和下一步效果，帮助玩家判断现在该补资源、推进一步，还是换目标。", "Break the current loop into input, output, new use, action guidance, and next effect so the player can choose whether to refill resources, advance one step, or switch targets.")}
              >
                <div class="summary-grid">
                  <MetricCard
                    label={tr(locale(), "投入", "Input")}
                    value={gameplay().economicSurface?.input || tr(locale(), "待发布", "not published")}
                  />
                  <MetricCard
                    label={tr(locale(), "产出", "Output")}
                    value={gameplay().economicSurface?.output || tr(locale(), "待发布", "not published")}
                  />
                  <MetricCard
                    label={tr(locale(), "新用途", "New Use")}
                    value={gameplay().economicSurface?.unlockedValue || tr(locale(), "待发布", "not published")}
                  />
                  <MetricCard
                    label={tr(locale(), "下一步操作指引", "Next Action Guidance")}
                    value={gameplay().economicSurface?.repairAction || tr(locale(), "待发布", "not published")}
                    detail={gameplay().economicSurface?.blockerLabel
                      ? tr(locale(), `当前阻塞归类: ${gameplay().economicSurface.blockerLabel}`, `Current blocker class: ${gameplay().economicSurface.blockerLabel}`)
                      : null}
                  />
                  <MetricCard
                    label={tr(locale(), "下一步价值", "Next Value")}
                    value={gameplay().economicSurface?.nextValue || tr(locale(), "待发布", "not published")}
                  />
                </div>
              </PanelSection>
              <IndustrialStarterOutcomeCard profile={gameplay().industrialStarterProfile} locale={locale} />
              <MicroDepotFacilitiesPanel
                facilities={gameplay().microDepotFacilities}
                locale={locale}
                tr={tr}
              />
              <RefineQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <PowerSaleQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <PowerSurvivalQuoteGameplayPanel core={core} locale={locale()} tr={tr} /><ScheduleRecipeQuoteGameplayPanel core={core} locale={locale()} tr={tr} /><TransferMaterialQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <FragmentRefillPreviewGameplayPanel core={core} locale={locale()} tr={tr} />
              <WarDeclarationQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <GovernanceVoteQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <MarketQuoteDecisionGameplayPanel core={core} locale={locale()} tr={tr} />
              <Show when={gameplay().agentClaim}>
                <ClaimAgentChoiceCard
                  locale={locale()}
                  claim={gameplay().agentClaim}
                  availableActions={gameplay().availableActions}
                />
              </Show>
              <Show when={expansionBranchCards(gameplay(), locale()).length > 0}>
                <ExpansionTradeoffCards gameplay={gameplay()} locale={locale()} />
              </Show>
              <Show when={gameplay().recentFeedback}>
                {(feedback) => (
                  <EventCard
                    title={tr(locale(), "最近玩法反馈", "Recent Gameplay Feedback")}
                    badge={feedback().stage || "-"}
                    badgeClass={
                      feedback().stage === "blocked" ? "badge badge--warn" : "badge badge--good"
                    }
                    meta={
                      feedback().action
                        ? tr(locale(), `来自动作 ${feedback().action}`, `From action ${feedback().action}`)
                        : tr(locale(), "最近一条玩法回执", "Most recent gameplay feedback")
                    }
                  >
                    <div class="feedback-summary">
                      {feedback().effect
                        || feedback().reason
                        || tr(locale(), "最新回执已更新，但还没有新的世界级后果。", "The latest feedback is in, but there is no new world-level consequence yet.")}
                    </div>
                    <Show when={feedback().reason}>
                      <div class="feedback-detail">{feedback().reason}</div>
                    </Show>
                    <Show when={feedback().hint}>
                      <div class="feedback-detail">{feedback().hint}</div>
                    </Show>
                  </EventCard>
                )}
              </Show>
              <Show when={!starterOcGateOpen() && gameplayActionFeedback()}>
                {(feedback) => (
                  <FeedbackCard
                    feedback={feedback()}
                    feedbackStage={feedback().stage}
                    display={gameplayActionFeedbackDisplay()}
                    liveRegion
                  />
                )}
              </Show>
              <ProductValidationQuoteGameplayPanel core={core} locale={locale()} tr={tr} />
              <Show when={!shouldShowStarterOcRequiredGate(gameplay()) && (gameplay().recommendedAction?.actionId === "claim_starter_oc" || starterOcAction(gameplay()))}>
                <CalloutCard
                  title={tr(locale(), "领取第一笔 OC", "Claim Your First OC")}
                  badge={starterOcAction(gameplay()) ? "ready" : "next"}
                  badgeClass={starterOcAction(gameplay()) ? "badge badge--good" : "badge badge--accent"}
                >
                  <StarterOcOnboardingPanel gameplay={gameplay()} locale={locale()} />
                </CalloutCard>
              </Show>
              <Show when={hasExecutableAgentClaim(core.state.snapshot, gameplay().agentClaim)}>
                <AgentClaimPanel gameplay={gameplay()} locale={locale()} />
              </Show>
              <Show when={hasAgentClaimSessionBoundary(gameplay().agentClaim)}>
                <AgentClaimSessionBoundaryCard gameplay={gameplay()} locale={locale()} />
              </Show>
              <div>
                <div class="panel__title panel__title--spaced">{tr(locale(), "可用玩法动作", "Available Gameplay Actions")}</div>
                <div class="action-grid">
                  <Show
                    when={visibleGameplayActionsForPanels(gameplay()).length > 0}
                    fallback={<EmptyState>{tr(locale(), "当前还没有发布规范玩法动作。", "No canonical gameplay actions published yet.")}</EmptyState>}
                  >
                    <For each={visibleGameplayActionsForPanels(gameplay())}>
                      {(action) => {
                        const disabledReason = () => gameplayActionDisabledReason(action, gameplay(), locale());
                        const actionState = () => (disabledReason() ? "blocked" : "ready");
                        const blockedReasonId = gameplayActionBlockedReasonId(action);
                        return (
                        <EventCard
                          class="event-card event-card--action"
                          actionState={actionState()}
                          title={action.label || action.actionId || "unknown_action"}
                          badge={gameplay().recommendedAction?.actionId === action.actionId
                            ? tr(locale(), "recommended", "recommended")
                            : disabledReason() ? tr(locale(), "受阻", "Blocked") : "ready"}
                          badgeClass={gameplay().recommendedAction?.actionId === action.actionId
                            ? "badge badge--accent"
                            : disabledReason() ? "badge badge--warn" : "badge badge--good"}
                          meta={
                            action.targetAgentId
                              ? tr(locale(), `作用对象 ${action.targetAgentId}`, `Acts on ${action.targetAgentId}`)
                              : tr(locale(), "世界级动作", "World-level action")
                          }
                        >
                          <Show
                            when={disabledReason()}
                            fallback={<div class="feedback-detail">{gameplayActionDetail(action, gameplay(), locale())}</div>}
                          >
                            <div class="feedback-detail" id={blockedReasonId}>{disabledReason()}</div>
                            <Show when={gameplay().nextStepHint}>
                              <div class="feedback-detail">{gameplay().nextStepHint}</div>
                            </Show>
                            <div class="feedback-summary">
                              <a href="#viewer-details-panel">
                                {tr(locale(), "重试前先查看下一步或玩法详情。", "Review Next Move or Gameplay Details before retrying.")}
                              </a>
                            </div>
                          </Show>
                          <Show
                            when={action.executeKind === "request_snapshot" || action.executeKind === "step" || action.executeKind === "play" || action.executeKind === "pause" || action.executeKind === "gameplay_action" || action.executeKind === "claim_first_agent" || action.executeKind === "claim_starter_oc"}
                          >
                            <div class="toolbar">
                              <button
                                data-testid={gameplayActionTestId(action)}
                                aria-label={action.label || action.actionId || undefined}
                                class={gameplayActionButtonClass(action)}
                                aria-busy={gameplayActionButtonBusyAttrs(action)}
                                disabled={gameplayActionButtonDisabled(action, gameplay(), locale())}
                                aria-describedby={disabledReason() ? blockedReasonId : undefined}
                                onClick={() => renderGameplayAction(action)}
                              >
                                {gameplayActionDisplayLabel(action, locale())}
                              </button>
                            </div>
                          </Show>
                          <Show when={action.executeKind === "reprioritize" && !disabledReason()}>
                            <ReprioritizeActionForm action={action} locale={locale()} tr={tr} observeState={observeViewerStateRevision} />
                          </Show>
                          <Show when={action.executeKind === "agent_chat"}>
                            <div class="toolbar">
                              <button
                                data-testid={gameplayActionTestId(action)}
                                aria-label={action.label || action.actionId || undefined}
                                class={gameplayActionButtonClass(action)}
                                aria-busy={gameplayActionButtonBusyAttrs(action)}
                                disabled={gameplayActionButtonDisabled(action, gameplay(), locale())}
                                aria-describedby={disabledReason() ? blockedReasonId : undefined}
                                onClick={() => renderGameplayAction(action)}
                              >
                                {gameplayActionDisplayLabel(action, locale())}
                              </button>
                            </div>
                          </Show>
                        </EventCard>
                        );
                      }}
                    </For>
                  </Show>
                </div>
              </div>
              <CalloutCard
                title={tr(locale(), "未在此页暴露的动作", "Actions Not Exposed On This Page")}
                badge="handoff"
                badgeClass="badge badge--warn"
              >
                <div class="feedback-summary">{gameplay().assetGovernanceHandoff}</div>
                <div class="feedback-detail">
                  {tr(
                    locale(),
                    "资产 / 治理相关能力请走单独 lane；这张主入口页面只保留正式玩法所需的最小动作面。",
                    "Asset and governance actions stay on their dedicated lane; this primary entry only keeps the minimum surface needed for formal gameplay.",
                  )}
                </div>
              </CalloutCard>
            </>
          )}
        </Show>
      </PanelSection>
      <Show when={showPlayerSessionSurface()}>
        <PanelSection
          title={tr(locale(), "进入会话", "Player Access")}
          eyebrow={tr(locale(), "只在需要时出现", "Only When Needed")}
          meta={tr(locale(), "只有当玩家会话缺失、重绑中或需要恢复时，这里才会打断主玩法路径。", "This only interrupts the main path when the player session is missing, rebinding, or needs recovery.")}
        >
          <div class="badge-row">
            <Badge class={state.auth.available ? "badge badge--good" : "badge badge--warn"}>
              {`auth=${state.auth.available ? state.auth.registrationStatus || "ready" : "missing"}`}
            </Badge>
            <Badge class="badge badge--accent">{`tier=${authSurface().currentTier}`}</Badge>
            <Badge>{`player=${state.auth.playerId || "-"}`}</Badge>
            <Badge>{`boundAgent=${state.auth.boundAgentId || "-"}`}</Badge>
          </div>
          <EmptyState>
            {hostedRecoveryHint()?.detail || state.auth.rebindNotice || authSurface().currentTierReason}
          </EmptyState>
          <Show when={hostedRecoveryHint()}>
            {(hint) => <EmptyState>{hint().detail}</EmptyState>}
          </Show>
          <Show
            when={
              !state.auth.available
              && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
            }
          >
            <HostedLoginForm locale={locale()} />
          </Show>
          <Show when={state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE}>
            <div class="toolbar">
              <button
                data-auth-action="logout"
                onClick={() => {
                  void core.logoutHostedPlayerSession();
                }}
              >
                {tr(locale(), "释放玩家会话", "Release Player Session")}
              </button>
            </div>
          </Show>
        </PanelSection>
      </Show>
      <details id="viewer-diagnostics-panel" class="panel diagnostic-surface" data-viewer-surface="diagnostics">
        <summary class="panel__header diagnostic-surface__summary">
          <div class="diagnostic-surface__title">
            <div class="panel__title">{tr(locale(), "运行诊断", "Runtime Diagnostics")}</div>
            <div class="diagnostic-surface__meta">
              {tr(
                locale(),
                "执行通道、认证/会话、托管矩阵与最近事件都收在这里，避免它们继续抢占主玩法首屏。",
                "Execution lanes, auth/session truth, hosted matrix, and recent events live here so they no longer dominate the primary gameplay viewport.",
              )}
            </div>
          </div>
          <div class="badge-row">
            <For each={diagnosticsSummaryBadges()}>
              {(label) => <Badge class="badge badge--diagnostic">{label}</Badge>}
            </For>
          </div>
        </summary>
        <div class="panel__body stack">
          <DirectorEntryCard controller={props.directorController} locale={locale} onRequest={props.onRequestDirector} />
          <div class="badge-row">
            <Badge>{`ws=${state.wsUrl || "-"}`}</Badge>
            <Badge>{`entryReason=${state.viewerReason || "-"}`}</Badge>
            <Badge>{`renderer=${state.renderer || "n/a"}`}</Badge>
            <Badge>{`controlProfile=${state.controlProfile}`}</Badge>
          </div>
          <PanelSection title={tr(locale(), "执行通道", "Execution Lanes")}>
            <Show
              when={core.selectedAgentExecutionDebugContext()}
              fallback={
                <EmptyState>
                  {tr(
                    locale(),
                    "先选中一个行动体，才能查看当前执行通道元数据。",
                    "Select an agent to inspect the current execution-lane metadata.",
                  )}
                </EmptyState>
              }
            >
              {(debug) => (
                <>
                  <div class="badge-row">
                    <Badge class="badge badge--accent">selected agent lane</Badge>
                    <Badge>{`provider=${debug().provider_mode || "-"}`}</Badge>
                    <Badge>{`mode=${debug().execution_mode || "-"}`}</Badge>
                    <Badge>{`env=${debug().environment_class || "-"}`}</Badge>
                  </div>
                  <div class="badge-row">
                    <Badge>{`obs=${debug().observation_schema_version || "-"}`}</Badge>
                    <Badge>{`act=${debug().action_schema_version || "-"}`}</Badge>
                    <Badge>{`agentProfile=${debug().agent_profile || "-"}`}</Badge>
                    <Badge>{`providerFallback=${debug().fallback_reason || "-"}`}</Badge>
                  </div>
                  <EmptyState class="flow-lift--tight">
                    {tr(
                      locale(),
                      "上面的通道徽标表示 phase-1 期望执行契约；下面的提供方检查徽标表示 runtime_live 基于 /v1/provider/info 和 /v1/provider/health 的真实探测结果。",
                      "Lane badges show the expected phase-1 execution contract. Provider check badges below show the actual runtime_live probe against /v1/provider/info and /v1/provider/health.",
                    )}
                  </EmptyState>
                  <div class="badge-row">
                    <Badge class="badge badge--accent">provider check</Badge>
                    <Badge>{`status=${debug().provider_check_status || "-"}`}</Badge>
                    <Badge>{`source=${debug().provider_check_source || "-"}`}</Badge>
                    <Badge>{`fallback=${debug().provider_check_fallback_reason || "-"}`}</Badge>
                  </div>
                  <Show
                    when={
                      debug().provider_check_error
                      || debug().provider_reported_capabilities?.length
                      || debug().provider_reported_supported_action_sets?.length
                    }
                  >
                    <div class="badge-row">
                      <Badge>{`actualCaps=${(debug().provider_reported_capabilities || []).join(",") || "-"}`}</Badge>
                      <Badge>
                        {`actualActions=${(debug().provider_reported_supported_action_sets || []).join(",") || "-"}`}
                      </Badge>
                      <Badge>{`checkError=${debug().provider_check_error || "-"}`}</Badge>
                    </div>
                  </Show>
                  <JsonBlock value={debug()} />
                </>
              )}
            </Show>
          </PanelSection>
          <div class="badge-row">
            <Badge class={state.auth.available ? "badge badge--good" : "badge badge--warn"}>
              {`auth=${state.auth.available ? state.auth.registrationStatus || "ready" : "missing"}`}
            </Badge>
            <Badge class="badge badge--accent">{`tier=${authSurface().currentTier}`}</Badge>
            <Badge>{`source=${authSurface().source}`}</Badge>
            <Badge>{`deploymentHint=${authSurface().deploymentHint}`}</Badge>
            <Badge>{`player=${state.auth.playerId || "-"}`}</Badge>
            <Badge>{`pubkey=${state.auth.publicKey ? `${state.auth.publicKey.slice(0, 10)}…` : "-"}`}</Badge>
            <Badge>{`epoch=${state.auth.sessionEpoch == null ? "-" : state.auth.sessionEpoch}`}</Badge>
            <Badge>{`runtime=${state.auth.runtimeStatus || "-"}`}</Badge>
            <Badge>{`boundAgent=${state.auth.boundAgentId || "-"}`}</Badge>
            <Badge>{`requestedAgent=${state.auth.pendingRequestedAgentId || "-"}`}</Badge>
            <Badge>{state.auth.pendingForceRebind ? "rebind=forcing" : "rebind=idle"}</Badge>
          </div>
          <div class="toolbar">
            <Show when={state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE}>
              <button
                data-auth-action="logout"
                onClick={() => {
                  void core.logoutHostedPlayerSession();
                }}
              >
                {tr(locale(), "释放玩家会话", "Release Player Session")}
              </button>
            </Show>
          </div>
          <Show when={hostedRecoveryHint()}>
            {(hint) => <EmptyState>{hint().detail}</EmptyState>}
          </Show>
          <Show
            when={
              !state.auth.available
              && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
            }
          >
            <HostedLoginForm
              locale={locale()}
              channelId="diag-hosted-login-channel"
              handleId="diag-hosted-login-handle"
              codeId="diag-hosted-login-code"
            />
          </Show>
          <Show when={state.auth.recoveryErrorCode || state.auth.recoveryErrorMessage}>
            <div class="badge-row">
              <Badge class="badge badge--warn">{`recoveryError=${state.auth.recoveryErrorCode || "-"}`}</Badge>
              <Badge>{state.auth.recoveryErrorMessage || "-"}</Badge>
            </div>
          </Show>
          <Show when={showRebindNotice()}>
            <div class="badge-row">
              <Badge class="badge badge--accent">rebind</Badge>
              <Badge>{`target=${state.auth.pendingRequestedAgentId || "-"}`}</Badge>
              <Badge>{state.auth.pendingForceRebind ? "mode=force_rebind" : "mode=awaiting_retry"}</Badge>
            </div>
            <EmptyState>
              {tr(
                locale(),
                "玩家会话正在切换到请求的行动体；注册成功后，当前动作会继续执行。",
                "Player session is switching to the requested agent and the current action will continue after registration succeeds.",
              )}
            </EmptyState>
          </Show>
          <Show when={state.hostedAdmission}>
            {(admission) => (
              <div class="badge-row">
                <Badge>{`activeSlots=${admission().active_player_sessions}/${admission().max_player_sessions}`}</Badge>
                <Badge>
                  {`effectiveSlots=${
                    admission().effective_player_sessions == null
                      ? "-"
                      : `${admission().effective_player_sessions}/${admission().max_player_sessions}`
                  }`}
                </Badge>
                <Badge>{`runtimeBound=${admission().runtime_bound_player_sessions ?? "-"}`}</Badge>
                <Badge>{`runtimeOnly=${admission().runtime_only_player_sessions ?? "-"}`}</Badge>
                <Badge>{`runtimeProbe=${admission().runtime_probe_status || "-"}`}</Badge>
                <Badge>{`issueBudget=${admission().remaining_issue_budget}`}</Badge>
                <Badge>{`leaseTTL=${admission().slot_lease_ttl_ms}`}</Badge>
                <Badge>{`issued=${admission().issued_players_total}`}</Badge>
                <Badge>{`released=${admission().released_players_total}`}</Badge>
              </div>
            )}
          </Show>
          <Show when={state.hostedAdmission?.runtime_probe_error}>
            <div class="badge-row">
              <Badge class="badge badge--warn">{`runtimeProbeError=${state.hostedAdmission.runtime_probe_error}`}</Badge>
            </div>
          </Show>
            <PanelSection title={tr(locale(), "会话阶梯", "Session Ladder")}>
            <EmptyState>{authSurface().currentTierReason}</EmptyState>
            <div class="event-list">
              <For each={authSurface().tiers}>
                {(tier) => (
                  <EventCard title={tier.label} badge={tier.status} badgeClass={tierBadgeClass(tier.status)} meta={tier.reason} />
                )}
              </For>
            </div>
            <div class="badge-row">
              <Badge class={authSurface().capabilities.prompt_control.enabled ? "badge badge--good" : "badge badge--warn"}>
                {`prompt=${
                  authSurface().capabilities.prompt_control.enabled
                    ? "enabled"
                    : authSurface().capabilities.prompt_control.code
                }`}
              </Badge>
              <Badge class={authSurface().capabilities.agent_chat.enabled ? "badge badge--good" : "badge badge--warn"}>
                {`chat=${
                  authSurface().capabilities.agent_chat.enabled
                    ? "enabled"
                    : authSurface().capabilities.agent_chat.code
                }`}
              </Badge>
              <Badge class="badge badge--warn">
                {`mainToken=${authSurface().capabilities.main_token_transfer.code}`}
              </Badge>
            </div>
            <EmptyState>{authSurface().reconnect}</EmptyState>
          </PanelSection>
          <Show when={hostedActionMatrixView().length > 0}>
            <PanelSection title={tr(locale(), "托管动作矩阵", "Hosted Action Matrix")}>
              <EmptyState>
                {tr(
                  locale(),
                  "这里是启动器导出的托管公开加入真值面。质检应直接读取这些动作编号，而不是只靠按钮状态推断。",
                  "This is the hosted public-join truth surface exported by the launcher. QA should read these action ids directly instead of inferring from button state alone.",
                )}
              </EmptyState>
              <div class="event-list">
                <For each={hostedActionMatrixView()}>
                  {(item) => (
                    <EventCard
                      title={item.actionId}
                      badge={item.enabled ? "enabled" : item.code || "blocked"}
                      badgeClass={item.enabled ? "badge badge--good" : "badge badge--warn"}
                      meta={`required_auth=${item.requiredAuth} · availability=${item.availability}`}
                    >
                      <EmptyState>{item.reason || "-"}</EmptyState>
                      <Show when={item.capabilityReason && item.capabilityReason !== item.reason}>
                        <EmptyState>{`viewer=${item.capabilityReason}`}</EmptyState>
                      </Show>
                    </EventCard>
                  )}
                </For>
              </div>
            </PanelSection>
          </Show>
          <div class="summary-grid">
            <MetricCard label={tr(locale(), "提示词反馈", "Prompt Feedback")} value={promptFeedback()?.stage || "idle"}>
              <Show when={promptFeedbackDisplay()}>
                <Badge class={promptFeedbackDisplay().badgeClass}>
                  {promptFeedbackDisplay().label}
                </Badge>
              </Show>
            </MetricCard>
            <MetricCard label={tr(locale(), "聊天反馈", "Chat Feedback")} value={chatFeedback()?.stage || "idle"}>
              <Show when={chatFeedbackDisplay()}>
                <Badge class={chatFeedbackDisplay().badgeClass}>
                  {chatFeedbackDisplay().label}
                </Badge>
              </Show>
            </MetricCard>
          </div>
          <div>
            <div class="panel__title panel__title--spaced">{tr(locale(), "最近事件", "Recent Events")}</div>
            <div class="event-list">
              <Show when={state.recentEvents.length > 0} fallback={<EmptyState>{tr(locale(), "等待实时事件…", "Waiting for live events…")}</EmptyState>}>
                <For each={state.recentEvents}>
                  {(event) => (
                    <EventCard
                      title={core.summarizeEventTitle(event)}
                      badge={`#${Number(event.id || 0)}`}
                      meta={`time=${Number(event.time || 0)}`}
                    >
                      <JsonBlock value={event.kind} />
                    </EventCard>
                  )}
                </For>
              </Show>
            </div>
          </div>
        </div>
      </details>
      </div>
      </details>
    </div>
  );
}
function InteractionPanel() {
  return (
    <InteractionPanelView
      observeViewerStateRevision={observeViewerStateRevision}
      describeAgentSessionStatus={describeAgentSessionStatus}
      readAgentContextFixtureMetadata={readAgentContextFixtureMetadata}
      readAgentContextFixtureGameplay={readAgentContextFixtureGameplay}
      viewerVisualFixtureNameFromQuery={viewerVisualFixtureNameFromQuery}
      viewerTestApiEnabled={viewerTestApiEnabled}
      renderGameplayAction={renderGameplayAction}
      EmptyEntityRecoveryCard={EmptyEntityRecoveryCard}
      PanelSection={PanelSection}
      FeedbackCard={FeedbackCard}
      EventCard={EventCard}
      DiagnosticDetails={DiagnosticDetails}
      chatEntryCardClass={chatEntryCardClass}
      chatEntryTitle={chatEntryTitle}
      chatEntryMeta={chatEntryMeta}
      chatEntryMessage={chatEntryMessage}
    />
  );
}
function AppShell(props = {}) {
  observeViewerStateRevision();
  const locale = () => uiLocale();
  const directorSession = createViewerDirectorSession({ core, fetchImpl: window.fetch?.bind(window) });
  createEffect(() => { observeViewerStateRevision(); directorSession.observeRuntime(); });
  const diagnosticsVisualFixture = () => viewerVisualFixtureNameFromQuery() === "gameplay_diagnostics_expanded";
  const starterOcGateOpen = () => shouldShowStarterOcRequiredGate(core.buildGameplaySummary(locale()));
  onMount(() => onCleanup(installViewerRouteController()));
  return (
    <div class="viewer-shell" data-viewer-shell="player-fullscreen">
      <MobileJumpRail locale={locale} tr={tr} data-viewer-overlay="navigation" />
      <SecondaryViewerNavigation locale={locale} tr={tr} />
      <LiveControlStatus state={core.state} locale={locale} tr={tr} />
      <HostedLoginGate />
      <StarterOcRequiredGate />
      <section
        class="panel panel--targets"
        id="viewer-targets-panel"
        data-viewer-route-panel="targets" data-viewer-overlay="targets"
        tabIndex="-1"
        data-viewer-surface="targets"
        aria-hidden={starterOcGateOpen() ? "true" : undefined}
        inert={starterOcGateOpen() ? true : undefined}
      >
        <div class="panel__header panel__header--stack">
          <div class="panel__eyebrow">{tr(locale(), "导航", "Navigate")}</div>
          <div class="panel__title">{tr(locale(), "目标", "Targets")}</div>
          <div class="panel__meta-copy">{tr(locale(), "先在 Targets 中锁定对象，再进入世界舞台或指挥面板。", "Lock onto a target in Targets first, then move into the stage or command surface.")}</div>
          <a class="panel__route-close" href="#viewer-stage-panel" onClick={focusViewerAnchor}>
            {tr(locale(), "返回世界", "Back to World")}
          </a>
        </div>
        <div class="panel__body">
          <TargetsPanel />
        </div>
      </section>
      <section
        class="panel panel--stage"
        id="viewer-stage-panel"
        tabIndex="-1" data-viewer-map-layer="base"
        data-viewer-surface="stage"
        aria-hidden={starterOcGateOpen() ? "true" : undefined}
        inert={starterOcGateOpen() ? true : undefined}
      >
        <div class="panel__body panel__body--stage">
          <div class="stack">
            <Show when={diagnosticsVisualFixture()}>
              <WorldSummaryPanel directorController={directorSession.controller} onRequestDirector={directorSession.request} />
            </Show>
            <WorldStageHero />
            <Show when={props.pixelWorldReady !== false}>
              <PixelWorldHost locale={locale()} />
            </Show>
            <Show when={!diagnosticsVisualFixture()}>
              <WorldSummaryPanel directorController={directorSession.controller} onRequestDirector={directorSession.request} />
            </Show>
            <WorldFeedSurface core={core} locale={locale} tr={tr} observeState={observeViewerStateRevision} onReloadSnapshot={() => core.reloadWorldFeedFromAuthoritativeSnapshot()} />
          </div>
        </div>
      </section>
      <section
        class="panel panel--details"
        id="viewer-details-panel"
        data-viewer-route-panel="command" data-viewer-overlay="command"
        tabIndex="-1"
        data-viewer-surface="command"
        aria-hidden={starterOcGateOpen() ? "true" : undefined}
        inert={starterOcGateOpen() ? true : undefined}
      >
        <div class="panel__header panel__header--stack command-route-chrome">
          <div class="panel__eyebrow">{tr(locale(), "指挥与核查", "Command and Inspect")}</div>
          <div class="panel__title">{tr(locale(), "交互与明细", "Interact and Inspect")}</div>
          <div class="panel__meta-copy">
            {tr(locale(), "只有锁定目标后才进入 Command。聊天优先，提示词与对象核查继续后置。", "Enter Command only after locking a target. Chat comes first; prompt controls and raw inspection stay behind it.")}
          </div>
          <a class="panel__route-close" href="#viewer-stage-panel" onClick={focusViewerAnchor}>
            {tr(locale(), "返回世界", "Back to World")}
          </a>
        </div>
        <div class="panel__body">
          <DetailsPanel observeViewerStateRevision={observeViewerStateRevision} InteractionPanel={InteractionPanel} EmptyEntityRecoveryCard={EmptyEntityRecoveryCard} DiagnosticDetails={DiagnosticDetails} />
        </div>
      </section>
      <DirectorSurface controller={directorSession.controller} core={core} locale={locale} />
    </div>
  );
}
export { AppShell };

let visualTestAdapter = null;
export function configureViewerVisualTest(adapter) { visualTestAdapter = adapter; }
function viewerVisualFixtureNameFromQuery() { return visualTestAdapter?.fixtureName() || null; }
function viewerTestApiEnabled() { return visualTestAdapter?.enabled() === true; }
function readAgentContextFixtureMetadata(...args) { return visualTestAdapter?.readMetadata(...args) || null; }
function readAgentContextFixtureGameplay(...args) { return visualTestAdapter?.readGameplay(...args) || null; }

export function mountViewerApp(root = document.getElementById("app")) {
  if (!root) {
    throw new Error("viewer root #app is missing");
  }

  const initialized = core.initializeSoftwareSafeCore();
  // Keep the shell interactive; construct the world scope guard after bootstrap and fixtures.
  const [pixelWorldReady, setPixelWorldReady] = createSignal(false);
  let dispose = mount(() => <AppShell pixelWorldReady={pixelWorldReady()} />, root);
  core.setRenderHook(() => setViewerStateRevision((revision) => revision + 1));
  let disposed = false;
  void Promise.resolve(initialized).then(() => {
    if (disposed) return;
    const fixtureName = visualTestAdapter?.install() || null;
    if (fixtureName) root.setAttribute("data-viewer-visual-fixture", fixtureName);
    else root.removeAttribute("data-viewer-visual-fixture");
    if (visualTestAdapter) window.__OASIS7_VIEWER_FIXTURE_READY__ = true;
    setPixelWorldReady(true);
    core.requestRender();
  }).catch(() => {});

  return () => {
    disposed = true;
    core.setRenderHook(null);
    dispose();
    root.textContent = "";
  };
}
