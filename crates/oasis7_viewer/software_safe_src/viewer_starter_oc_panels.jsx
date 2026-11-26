import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge } from "./viewer_ui_primitives.jsx";
import { FirstChatUnlockPreview } from "./first_chat_unlock_preview.jsx";
import { keepFocusInDialog } from "./viewer_modal_focus.js";
import {
  gameplayActionBlockedReasonId,
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionButtonLabel,
  gameplayActionDetail,
  gameplayActionDisabledReason,
  gameplayActionDisplayLabel,
  gameplayActionPendingFor,
  gameplayActionTestId,
} from "./viewer_gameplay_action_helpers.js";
import {
  clearStarterOcClaimPending,
  completeStarterOcOnboarding,
  firstAgentChatAction,
  gameplayProgressionAction,
  rawStarterOcActionAvailable,
  scheduleStarterOcBackgroundConfirmation,
  shouldShowStarterOcRequiredGate,
  starterOcAction,
  starterOcClaimPendingForCurrentAgent,
  starterOcCreditVisibleForCurrentAgent,
  starterOcFeedbackNeedsLocalAdvance,
  starterOcSubmittedFeedback,
} from "./viewer_starter_oc_controller.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function StarterOcGuide(props) {
  const locale = () => props.locale;
  return (
    <div class="stack stack--compact">
      <div class="feedback-summary">
        {tr(
          locale(),
          "等待同步时不用空等：先了解下一步。第一笔 OC 是新手启动资金，入账后会解锁第一次 Agent 聊天和早期玩法操作。",
          "Do not idle through sync: learn the next step now. The first OC is starter budget; once credited, it unlocks the first Agent chat and early gameplay actions.",
        )}
      </div>
      <div class="summary-grid">
        <div class="metric">
          <div class="metric__label">{tr(locale(), "第一笔 OC", "First OC")}</div>
          <div class="metric__value">{tr(locale(), "新手启动资金", "Starter budget")}</div>
        </div>
        <div class="metric">
          <div class="metric__label">{tr(locale(), "用途", "Use")}</div>
          <div class="metric__value">{tr(locale(), "解锁 Agent 聊天", "Unlock Agent chat")}</div>
        </div>
        <div class="metric">
          <div class="metric__label">{tr(locale(), "玩法目标", "Play Goal")}</div>
          <div class="metric__value">{tr(locale(), "指挥 Agent 恢复产线", "Guide the Agent")}</div>
        </div>
      </div>
    </div>
  );
}

export function StarterOcOnboardingPanel(props) {
  const { FeedbackCard, renderGameplayAction } = props;
  const locale = () => props.locale;
  const gameplay = () => props.gameplay;
  const action = () => starterOcAction(gameplay());
  const waitingForFirstAgent = () => Boolean(props.waitingForFirstAgent);
  const hideActionButton = () => Boolean(props.hideActionButton);
  return (
    <div class="stack stack--compact">
      <StarterOcGuide locale={locale()} />
      <Show when={!hideActionButton()}>
        <Show
          when={action()}
          fallback={(
            <div class="feedback-detail">
              {waitingForFirstAgent()
                ? tr(
                  locale(),
                  "当前还在等第一个 Agent 写入 committed 快照；OC 按钮会在 Agent 同步后自动出现。",
                  "The first Agent is still waiting for the committed snapshot; the OC button appears automatically after the Agent syncs.",
                )
                : tr(
                  locale(),
                  "如果聊天提示 OC 不足，回到这里领取初始 OC。",
                  "If chat says OC is missing, return here to claim starter OC.",
                )}
            </div>
          )}
        >
          {(starterAction) => (
            <div class="toolbar">
              <button
                class={gameplayActionButtonClass(starterAction())}
                aria-busy={gameplayActionButtonBusyAttrs(starterAction())}
                disabled={gameplayActionButtonDisabled(starterAction(), gameplay(), locale())}
                onClick={() => renderGameplayAction(starterAction())}
              >
                {gameplayActionDisplayLabel(starterAction(), locale())}
              </button>
            </div>
          )}
        </Show>
      </Show>
      <Show when={hideActionButton() && !action()}>
        <div class="feedback-detail">
          {waitingForFirstAgent()
            ? tr(
              locale(),
              "当前还在等第一个 Agent 写入 committed 快照；OC 按钮会在 Agent 同步后自动出现。",
              "The first Agent is still waiting for the committed snapshot; the OC button appears automatically after the Agent syncs.",
            )
            : tr(
              locale(),
              "如果聊天提示 OC 不足，回到这里领取初始 OC。",
              "If chat says OC is missing, return here to claim starter OC.",
            )}
        </div>
      </Show>
    </div>
  );
}

export function StarterOcRequiredGate(props) {
  const { observeViewerStateRevision, FeedbackCard, renderGameplayAction } = props;
  observeViewerStateRevision();
  const locale = () => uiLocale();
  const [autoConfirmAttempts, setAutoConfirmAttempts] = createSignal(0);
  const [manualConfirmAttempts, setManualConfirmAttempts] = createSignal(0);
  const [lastConfirmMode, setLastConfirmMode] = createSignal("auto");
  const gameplay = () => core.buildGameplaySummary(locale());
  const action = () => starterOcAction(gameplay());
  const submittedFeedback = () => starterOcSubmittedFeedback();
  const pendingCredit = () => starterOcClaimPendingForCurrentAgent() || Boolean(submittedFeedback());
  const creditConfirmed = () => pendingCredit()
    && (starterOcCreditVisibleForCurrentAgent() || Boolean(firstAgentChatAction(gameplay())))
    && !rawStarterOcActionAvailable();
  const progressionAction = () => gameplayProgressionAction(gameplay());
  const snapshotRefreshAction = () => (gameplay()?.availableActions || []).find((action) => action.executeKind === "request_snapshot") || null;
  const gateOpen = () => shouldShowStarterOcRequiredGate(gameplay());
  const firstChatUnlockPreview = () => core.state.snapshot?.player_gameplay?.agent_claim?.first_chat_unlock_preview || null;
  const chatAction = () => firstAgentChatAction(gameplay());
  const confirmationAction = () => {
    const refreshAction = snapshotRefreshAction();
    const advanceAction = progressionAction();
    return starterOcFeedbackNeedsLocalAdvance(submittedFeedback())
      ? advanceAction || refreshAction
      : refreshAction || advanceAction;
  };
  const visibleConfirmAttempt = () => lastConfirmMode() === "manual"
    ? Math.max(manualConfirmAttempts(), 1)
    : Math.min(autoConfirmAttempts() + 1, 3);
  const confirmStatusLabel = () => {
    if (creditConfirmed()) {
      return tr(locale(), "已入账", "Credited");
    }
    if (lastConfirmMode() === "manual") {
      return gameplayActionPendingFor(snapshotRefreshAction())
        ? tr(locale(), "手动确认中", "Manual confirmation")
        : tr(locale(), "等待手动确认回执", "Waiting for manual confirmation");
    }
    return autoConfirmAttempts() >= 3
      ? tr(locale(), "等待手动确认", "Waiting for manual confirmation")
      : tr(locale(), "自动确认中", "Auto-confirming");
  };
  const confirmProgressLabel = () => {
    if (creditConfirmed()) {
      return tr(locale(), "完成", "Done");
    }
    if (lastConfirmMode() === "manual") {
      return tr(
        locale(),
        `手动第 ${visibleConfirmAttempt()} 次确认`,
        `Manual check ${visibleConfirmAttempt()}`,
      );
    }
    return tr(locale(), `第 ${visibleConfirmAttempt()} 次确认`, `Check ${visibleConfirmAttempt()} of 3`);
  };
  const confirmSummaryCopy = () => {
    if (creditConfirmed()) {
      return tr(
        locale(),
        "第一笔 OC 已经写入本地快照。现在可以开始第一次 Agent 聊天，后续早期玩法动作也会解锁。",
        "The first OC is now visible in the local snapshot. You can start the first Agent chat and continue early gameplay actions.",
      );
    }
    if (lastConfirmMode() === "manual") {
      return gameplayActionPendingFor(snapshotRefreshAction())
        ? tr(
          locale(),
          "已发起手动确认。本地世界正在刷新快照，确认这笔初始 OC 是否已经写入。",
          "Manual confirmation started. The local world is refreshing the snapshot to verify whether the starter OC is visible.",
        )
        : tr(
          locale(),
          "自动确认还没有看到入账结果；可以再次手动刷新确认，或等待下一次快照同步。",
          "Auto-confirmation has not seen the credit yet; retry manual refresh or wait for the next snapshot sync.",
        );
    }
    return autoConfirmAttempts() >= 3
      ? tr(
        locale(),
        "自动确认已经跑完 3 次，仍未看到入账结果。可以手动再确认一次，或等待运行时快照继续同步。",
        "Auto-confirmation has completed 3 checks without seeing the credit. Retry manual confirmation or wait for the runtime snapshot to keep syncing.",
      )
      : tr(
        locale(),
        "领取请求已经提交。系统正在自动推进并刷新本地世界，确认这笔初始 OC 写入可见快照。",
        "The claim was submitted. The system is automatically advancing and refreshing the local world to confirm the starter OC in the visible snapshot.",
      );
  };
  const primaryAction = () => {
    if (creditConfirmed()) {
      return chatAction();
    }
    return pendingCredit() ? confirmationAction() : action();
  };
  let primaryButtonRef;
  let scheduledAutoConfirmAttempt = -1;
  let autoConfirmTimer = null;
  let autoCompleteTimer = null;
  createEffect(() => {
    if (gateOpen()) {
      window.setTimeout(() => primaryButtonRef?.focus(), 0);
    }
  });
  createEffect(() => {
    if (creditConfirmed()) {
      if (autoCompleteTimer == null) {
        autoCompleteTimer = window.setTimeout(() => {
          completeStarterOcOnboarding();
          setAutoConfirmAttempts(0);
          setManualConfirmAttempts(0);
          setLastConfirmMode("auto");
          core.requestRender();
          autoCompleteTimer = null;
        }, 1200);
      }
      return;
    }
    if (autoCompleteTimer != null) {
      window.clearTimeout(autoCompleteTimer);
      autoCompleteTimer = null;
    }
    if (!pendingCredit() || creditConfirmed()) {
      scheduledAutoConfirmAttempt = -1;
      if (!pendingCredit()) {
        setAutoConfirmAttempts(0);
        setManualConfirmAttempts(0);
        setLastConfirmMode("auto");
      }
      return;
    }
    const nextAction = confirmationAction();
    const attempt = autoConfirmAttempts();
    if (!nextAction || attempt >= 3 || scheduledAutoConfirmAttempt === attempt) {
      return;
    }
    if (gameplayActionButtonDisabled(nextAction, gameplay(), locale())) {
      return;
    }
    scheduledAutoConfirmAttempt = attempt;
    autoConfirmTimer = window.setTimeout(() => {
      setLastConfirmMode("auto");
      renderGameplayAction(nextAction);
      setAutoConfirmAttempts((value) => value + 1);
    }, attempt === 0 ? 450 : 1600);
  });

  onCleanup(() => {
    if (autoConfirmTimer != null) {
      window.clearTimeout(autoConfirmTimer);
    }
    if (autoCompleteTimer != null) {
      window.clearTimeout(autoCompleteTimer);
    }
  });

  return (
    <Show when={gateOpen()}>
      {() => (
        <div
          class="auth-gate"
          role="dialog"
          aria-modal="true"
          aria-labelledby="starter-oc-gate-title"
          data-viewer-fixture-state="starter_oc_required_gate"
        >
          <div class="auth-gate__dialog">
            <div class="auth-gate__header">
              <div>
                <div class="panel__eyebrow">{tr(locale(), "新手必经步骤", "Required Onboarding Step")}</div>
                <h1 id="starter-oc-gate-title" class="auth-gate__title">
                  {creditConfirmed()
                    ? tr(locale(), "OC 已入账", "OC Credited")
                    : pendingCredit()
                    ? tr(locale(), "正在确认 OC 入账", "Confirming OC Credit")
                    : tr(locale(), "领取第一笔 OC", "Claim Your First OC")}
                </h1>
              </div>
              <Badge class={creditConfirmed() ? "badge badge--good" : pendingCredit() ? "badge badge--accent" : "badge badge--good"}>
                {creditConfirmed() ? "credited" : pendingCredit() ? "syncing" : "ready"}
              </Badge>
            </div>
            <Show
              when={!pendingCredit()}
              fallback={(
                <div class="stack stack--compact">
                  <div class="feedback-summary">
                    {confirmSummaryCopy()}
                  </div>
                  <div class="summary-grid">
                    <div class="metric">
                      <div class="metric__label">{tr(locale(), "状态", "Status")}</div>
                      <div class="metric__value">
                        {creditConfirmed()
                          ? tr(locale(), "已入账", "Credited")
                          : confirmStatusLabel()}
                      </div>
                    </div>
                    <div class="metric">
                      <div class="metric__label">{tr(locale(), "进度", "Progress")}</div>
                      <div class="metric__value">
                        {creditConfirmed()
                          ? tr(locale(), "完成", "Done")
                          : confirmProgressLabel()}
                      </div>
                    </div>
                    <div class="metric">
                      <div class="metric__label">{tr(locale(), "你可以做什么", "What To Do")}</div>
                      <div class="metric__value">
                        {creditConfirmed()
                          ? tr(locale(), "开始聊天", "Start chat")
                          : tr(locale(), "先看玩法说明", "Read the guide")}
                      </div>
                    </div>
                  </div>
                  <StarterOcGuide locale={locale()} />
                  <Show when={submittedFeedback()}>
                    {(feedback) => (
                      <FeedbackCard
                        feedback={feedback()}
                        feedbackStage={feedback().stage}
                        display={core.describeSemanticFeedback(feedback(), locale())}
                        liveRegion
                      />
                    )}
                  </Show>
                </div>
              )}
            >
              <Show
                when={firstChatUnlockPreview()}
                fallback={<StarterOcOnboardingPanel gameplay={gameplay()} locale={locale()} hideActionButton={true} />}
              >
                {(preview) => <FirstChatUnlockPreview preview={preview()} locale={locale()} tr={tr} />}
              </Show>
            </Show>
            <Show when={!firstChatUnlockPreview()}><div class="feedback-detail">
              {creditConfirmed()
                ? tr(locale(), "OC 会作为第一次 LLM/Agent chat 的启动预算；用它向 Agent 发第一条指令，推动产线恢复。", "OC is the starter budget for the first LLM/Agent chat. Use it to send the first command and move production forward.")
                : pendingCredit()
                ? tr(locale(), "不用空等：系统会自动推进确认。若本地世界暂时没有回执，下面的按钮可以手动补一次确认。", "No need to idle: confirmation runs automatically. If the local world has not responded yet, the button below can retry one confirmation.")
                : tr(locale(), "这是进入 Agent 聊天和早期玩法动作前必须完成的一步。领取后会进入入账确认。", "This step is required before Agent chat and early gameplay actions. Claiming it moves you to credit confirmation.")}
            </div></Show>
            <div class="toolbar">
              <Show when={primaryAction()}>
                {(nextAction) => (
                  <button
                    ref={primaryButtonRef}
                    data-testid="viewer-playthrough-action-claim-starter-oc"
                    class={gameplayActionButtonClass(nextAction())}
                    aria-busy={gameplayActionButtonBusyAttrs(nextAction())}
                    disabled={gameplayActionButtonDisabled(nextAction(), gameplay(), locale())}
                    onClick={() => {
                      if (creditConfirmed()) {
                        completeStarterOcOnboarding();
                        setAutoConfirmAttempts(0);
                        setManualConfirmAttempts(0);
                        setLastConfirmMode("auto");
                      }
                      if (pendingCredit()) {
                        setLastConfirmMode("manual");
                        setManualConfirmAttempts((value) => value + 1);
                      }
                      renderGameplayAction(nextAction());
                    }}
                  >
                    {gameplayActionDisplayLabel(
                      nextAction(),
                      locale(),
                      creditConfirmed()
                        ? tr(locale(), "开始第一次 Agent 聊天", "Start First Agent Chat")
                        : pendingCredit()
                        ? tr(locale(), "手动再确认一次", "Retry Confirmation")
                        : gameplayActionButtonLabel(nextAction(), locale()),
                    )}
                  </button>
                )}
              </Show>
              <Show when={creditConfirmed() && !primaryAction()}>
                <button
                  ref={primaryButtonRef}
                  onClick={() => {
                    completeStarterOcOnboarding();
                    setAutoConfirmAttempts(0);
                    setManualConfirmAttempts(0);
                    setLastConfirmMode("auto");
                    core.requestRender();
                  }}
                >
                  {tr(locale(), "继续", "Continue")}
                </button>
              </Show>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
}
