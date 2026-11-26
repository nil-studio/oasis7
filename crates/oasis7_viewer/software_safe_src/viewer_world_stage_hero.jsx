import { For, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import {
  gameplayActionBlockedReasonId,
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionDetail,
  gameplayActionDisabledReason,
  gameplayActionDisplayLabel,
  gameplayActionTestId,
  gameplayStageLabel,
  gameplayStageToneClass,
  gameplayStatusBadgeClass,
} from "./viewer_gameplay_action_helpers.js";
import { visibleGameplayActionsForPanels } from "./viewer_starter_oc_controller.js";
import { pixelWorldBlockerPresentation, pixelWorldConnectionPresentation } from "./pixel_world_presentation.js";
import { focusViewerAnchor } from "./viewer_navigation.jsx";
import { LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE } from "./software_safe_constants.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function WorldStageHero(props) {
  const {
    observeViewerStateRevision,
    CalloutCard,
    EmptyEntityRecoveryCard,
    InlineHelpTip,
    ViewerEntryMenu,
    connectionStatusLabel,
    gameplayProgressLabel,
    renderGameplayAction,
  } = props;
  observeViewerStateRevision();
  const locale = () => uiLocale();
  const gameplaySummary = () => core.buildGameplaySummary(locale());
  const authSurface = () => core.buildAuthSurfaceModel();
  const presentationScale = () => core.buildWorldScaleSurface(locale()).presentationScale;
  const selectedLabel = () =>
    core.state.selectedKind && core.state.selectedId
      ? `${core.state.selectedKind}:${core.state.selectedId}`
      : null;
  const identityKindLabel = () => {
    const source = String(authSurface().source || core.state.auth.source || "").trim();
    if (!core.state.auth.available) {
      return tr(locale(), "访客 / 未登录", "Guest / Not Signed In");
    }
    if (source === "hosted_browser_storage" || source === "hosted_player_session_issue") {
      return tr(locale(), "邮箱登录身份", "Hosted Account Identity");
    }
    if (source === "local_test_api_ephemeral" || source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      return tr(locale(), "本地测试身份", "Local Test Identity");
    }
    return authSurface().currentTier || tr(locale(), "玩家身份", "Player Identity");
  };
  const publicKeyShort = () =>
    core.state.auth.publicKey
      ? `${String(core.state.auth.publicKey).slice(0, 12)}...`
      : "-";
  const identityDetail = () => {
    if (!core.state.auth.available) {
      return tr(
        locale(),
        "当前还没有玩家 session；本地测试动作会按需生成临时玩家 key，托管公开模式才需要邮箱登录。",
        "No player session is active yet. Local test actions generate an ephemeral player key on demand; only hosted public join requires email sign-in.",
      );
    }
    return [
      `player=${core.state.auth.playerId || "-"}`,
      `pubkey=${publicKeyShort()}`,
      `session=${core.state.auth.registrationStatus || core.state.auth.runtimeStatus || "-"}`,
      `agent=${core.state.auth.boundAgentId || "-"}`,
    ].join(" · ");
  };
  const identityMeta = () => {
    const source = authSurface().source || core.state.auth.source || "-";
    if (!core.state.auth.available) {
      return `source=${source}`;
    }
    const loginNote = String(source) === "hosted_browser_storage"
      || String(source) === "hosted_player_session_issue"
      ? tr(locale(), "已通过托管账号会话", "hosted account session")
      : tr(locale(), "不是邮箱登录账号", "not an email login account");
    return `source=${source} · ${loginNote}`;
  };
  const selectionHint = () =>
    core.state.selectedKind && core.state.selectedId
      ? tr(locale(), "指挥面板会围绕这个对象展开。", "The command surface now follows this target.")
      : tr(locale(), "先从 Targets 锁定一个行动体或地点，再进入指挥面板。", "Lock onto an agent or location in Targets before entering the command surface.");
  const stageLabel = () => gameplayStageLabel(gameplaySummary()?.stageStatus, locale());
  const nextStepCopy = () =>
    gameplaySummary()?.narrativeNextStep
    || tr(locale(), "先读世界状态，再决定是否推进、恢复或对目标发消息。", "Read the world first, then decide whether to advance, resume, or message the target.");
  const acceptedIntentTitle = () =>
    gameplaySummary()?.acceptedIntentSummary
    || tr(locale(), "先提交一条明确意图", "Commit one clear intent first");
  const acceptedIntentDetail = () =>
    gameplaySummary()?.acceptedIntentTarget
      ? tr(
        locale(),
        `当前意图正围绕 ${gameplaySummary().acceptedIntentTarget} 展开。`,
        `The current intent is centered on ${gameplaySummary().acceptedIntentTarget}.`,
      )
      : selectionHint();
  const recommendedAction = () => gameplaySummary()?.recommendedAction || null;

  return (
    <div class="stage-hero stage-hero--compact" data-stage-state={gameplaySummary()?.blockerKind || "ready"}>
      <div class="stage-hero__topline">
        <div class="stack stack--hero">
          <div class="stage-hero__eyebrow-row">
            <div class="stage-hero__eyebrow">{tr(locale(), "工业世界指挥桌", "Industrial World Command Desk")}</div>
            <InlineHelpTip
              locale={locale()}
              id="viewer-stage-scale-tip"
              label={tr(locale(), "打开表现层比例说明", "Open presentation scale guidance")}
              title={tr(locale(), "表现层说明", "Presentation Notes")}
              lines={[
                presentationScale().markerTruthNote,
                presentationScale().zoomTruthNote,
                presentationScale().softwareSafeNote,
              ]}
            />
          </div>
          <div class="stage-hero__title">
            {gameplaySummary()?.goalTitle || tr(locale(), "进入世界，先看局势，再做动作", "Read the world first, then act.")}
          </div>
          <Show when={gameplaySummary()?.personalSubjectDetail}><div class="stage-hero__lede">{gameplaySummary().personalSubjectDetail}</div></Show>
          <div class="stage-hero__lede">
            {gameplaySummary()?.nextStepHint
              || gameplaySummary()?.objective
              || tr(
                locale(),
                "这张入口页优先保留世界、目标和关键动作；高级诊断与治理能力按需展开。",
                "This entry keeps the world, objective, and primary actions in front. Advanced diagnostics and governance stay on demand.",
            )}
          </div>
        </div>
        <ViewerEntryMenu />
      </div>
      <Show when={selectedLabel()}>
        {(selected) => (
          <div class="badge-row stage-hero__selection">
            <Badge class="badge badge--accent">{tr(locale(), "当前选择", "Current Selection")}</Badge>
            <Badge>{selected()}</Badge>
          </div>
        )}
      </Show>
      <div class="hero-focus-grid hero-focus-grid--compact">
        <div class="hero-focus-card">
          <div class="hero-focus-card__label">{tr(locale(), "局势", "Situation")}</div>
          <div class={gameplayStageToneClass(gameplaySummary()?.stageStatus)}>
            {stageLabel()}
          </div>
          <div class="hero-focus-card__detail">{gameplayProgressLabel(gameplaySummary()?.progressPercent, locale())}</div>
        </div>
        <div class="hero-focus-card">
          <div class="hero-focus-card__label">{tr(locale(), "已接受意图", "Accepted Intent")}</div>
          <div class="hero-focus-card__value hero-focus-card__value--body">{acceptedIntentTitle()}</div>
          <div class="hero-focus-card__detail">{acceptedIntentDetail()}</div>
        </div>
        <div
          class="hero-focus-card hero-focus-card--next-step"
          data-testid="viewer-next-step-card"
        >
          <div class="hero-focus-card__label">{tr(locale(), "下一步", "Next Step")}</div>
          <div class="hero-focus-card__value hero-focus-card__value--body">{nextStepCopy()}</div>
        </div>
        <div class="hero-focus-card" data-testid="viewer-identity-card">
          <div class="hero-focus-card__label">{tr(locale(), "当前身份", "Current Identity")}</div>
          <div class="hero-focus-card__value hero-focus-card__value--body">{identityKindLabel()}</div>
          <div class="hero-focus-card__detail">{identityDetail()}</div>
          <div class="hero-focus-card__detail">{identityMeta()}</div>
        </div>
      </div>
      <Show when={recommendedAction()}>
        {(action) => (
          <div class="stage-hero__secondary-action" data-hero-secondary-action="true">
            <CalloutCard
              title={tr(locale(), "推荐动作", "Recommended Action")}
              badge={action().executeKind || "ready"}
              badgeClass="badge badge--good"
            >
              <div class="feedback-summary">
                {action().label || action().actionId || tr(locale(), "当前存在一条更合适的推进动作。", "One action is currently the best next move.")}
              </div>
              <div
                class="feedback-detail"
                id={gameplayActionDisabledReason(action(), gameplaySummary(), locale()) ? gameplayActionBlockedReasonId(action()) : undefined}
              >
                {gameplayActionDisabledReason(action(), gameplaySummary(), locale())
                  || gameplayActionDetail(action(), gameplaySummary(), locale())}
              </div>
              <div class="toolbar" aria-label={tr(locale(), "下一步动作", "Next Move action")}>
                <button
                  type="button"
                  data-testid={gameplayActionTestId(action(), "recommended")}
                  class={gameplayActionButtonClass(action())}
                  aria-busy={gameplayActionButtonBusyAttrs(action())}
                  disabled={gameplayActionButtonDisabled(action(), gameplaySummary(), locale())}
                  aria-describedby={gameplayActionDisabledReason(action(), gameplaySummary(), locale()) ? gameplayActionBlockedReasonId(action()) : undefined}
                  onClick={() => renderGameplayAction(action())}
                >
                  {gameplayActionDisplayLabel(action(), locale())}
                </button>
              </div>
            </CalloutCard>
          </div>
        )}
      </Show>
      <Show when={gameplaySummary()?.blockerKind === "runtime_snapshot_empty_entities"}>
        <EmptyEntityRecoveryCard
          locale={locale()}
          gameplay={gameplaySummary}
          title={tr(locale(), "恢复世界快照", "Recover World Snapshot")}
        />
      </Show>
      <div class="stage-hero__mobile-shortcuts" aria-label={tr(locale(), "移动端快速入口", "Mobile quick actions")}>
        <a class="mobile-rail__link" href="#viewer-targets-panel" onClick={focusViewerAnchor}>{tr(locale(), "选择目标", "Select Target")}</a>
        <a class="mobile-rail__link" href="#viewer-details-panel" onClick={focusViewerAnchor}>{tr(locale(), "进入指挥", "Command")}</a>
      </div>
      <Show when={core.state.connectionStatus !== "connected"}>
        <CalloutCard
          title={tr(locale(), "世界连接需要注意", "World Connection Needs Attention")}
          badge={connectionStatusLabel(core.state.connectionStatus, locale())}
          badgeClass={core.connectionBadgeClass()}
          variant="warn"
        >
          <div class="feedback-summary">
            {tr(
              locale(),
              "首屏优先展示世界与目标；只有连接异常时，才把连接状态抬到这里提示你。",
              "This entry keeps the world and target first, and only elevates connection status when it needs attention.",
            )}
          </div>
        </CalloutCard>
      </Show>
    </div>
  );
}
