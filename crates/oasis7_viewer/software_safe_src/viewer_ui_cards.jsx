import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge } from "./viewer_ui_primitives.jsx";
import { targetSyncProgressLines } from "./viewer_target_sync_progress.js";
import {
  claimField,
  compactValue,
  claimQuoteRows,
  claimQuoteMetricClass,
  claimTarget,
  claimStatusText,
  claimOwnedDetail,
  releaseClaimActionState,
  expansionBranchCards,
} from "./viewer_claim_formatters.js";
import { FirstDeliveryPreview } from "./first_delivery_preview.jsx";

function uiLocale() { return core.state.uiLocale; }
function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

function EntityListPendingState(props) {
  const locale = () => props.locale ?? uiLocale();
  const label = () => props.label ?? tr(locale(), "目标", "targets");
  const progress = () => props.progress ?? core.buildTargetSyncProgress();
  const progressLines = () => targetSyncProgressLines(progress(), locale(), tr);
  return (
    <div class="entity-list-pending" aria-live="polite" aria-busy="true">
      <div class="entity-list-pending__row">
        <span class="entity-list-pending__spinner" aria-hidden="true" />
        <span>
          {tr(
            locale(),
            `正在同步${label()}…`,
            `Syncing ${label()}…`,
          )}
        </span>
      </div>
      <Show when={progressLines().length > 0}>
        <div class="entity-list-pending__progress">
          <For each={progressLines()}>
            {(line) => <div>{line}</div>}
          </For>
        </div>
      </Show>
      <div class="entity-list-pending__skeleton" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
    </div>
  );
}
function JsonBlock(props) {
  return <pre class="json">{JSON.stringify(props.value, null, 2)}</pre>;
}
function DiagnosticDetails(props) {
  const locale = () => props.locale ?? uiLocale();
  const [isOpen, setIsOpen] = createSignal(false);
  const resolvedValue = () => (typeof props.value === "function" ? props.value() : props.value);
  return (
    <details class="diagnostic" onToggle={(event) => setIsOpen(event.currentTarget.open)}>
      <summary>{props.label ?? tr(locale(), "原始诊断", "Raw diagnostics")}</summary>
      <div class="stack flow-top">
        <Show when={props.note}>
          <div class="feedback-detail">{props.note}</div>
        </Show>
        <Show when={isOpen()}>
          <JsonBlock value={resolvedValue()} />
        </Show>
      </div>
    </details>
  );
}
function ClaimAgentChoiceCard(props) {
  const locale = () => props.locale ?? uiLocale();
  const claim = () => props.claim || {};
  const quote = () => claimField(claim(), "next_claim_quote", "nextClaimQuote", "quote") || {};
  const blockedReason = () => claimField(quote(), "blocked_reason", "blockedReason");
  const ownedClaims = () => {
    const owned = claimField(claim(), "owned_claims", "ownedClaims");
    return Array.isArray(owned) ? owned : [];
  };
  const releaseActionState = () => releaseClaimActionState(props.availableActions || []);
  return (
    <PanelSection
      title={tr(locale(), "Claim-Agent Choice", "Claim-Agent Choice")}
      eyebrow={tr(locale(), "占用 / 维护 / 释放", "Claim / Maintain / Release")}
      meta={tr(locale(), "只展示现有 claim 快照与已发布动作；这里不会新增转账或 claim 规则。", "Shows only the current claim snapshot and published actions; this adds no transfer UI or claim rules.")}
    >
      <div class="badge-row">
        <Badge class={blockedReason() ? "badge badge--warn" : "badge badge--good"}>
          {blockedReason()
            ? tr(locale(), "暂缓 claim", "Wait before claiming")
            : tr(locale(), "claim 条件可读", "Claim readable")}
        </Badge>
        <Badge>{`owned=${ownedClaims().length}`}</Badge>
      </div>
      <div class="feedback-summary">
        {blockedReason()
          ? tr(locale(), "下一次 claim 需要先等待、补资金或提升资格；原始原因已收在诊断明细。", "The next claim needs waiting, funding, or eligibility first; the raw reason is kept in diagnostic detail.")
          : tr(locale(), "当前 quote 没有发布阻塞原因；玩家可以把它当成“可比较但仍需按正式动作执行”的 claim 机会。", "The current quote publishes no blocker reason; treat it as a comparable claim opportunity that still needs a canonical action to execute.")}
      </div>
      <div class="summary-grid">
        <For each={claimQuoteRows(quote())}>
          {([label, value]) => (
            <MetricCard class={claimQuoteMetricClass(label)} label={label} value={compactValue(value)} />
          )}
        </For>
      </div>
      <Show when={blockedReason()}>
        <DiagnosticDetails
          locale={locale()}
          label={tr(locale(), "Claim 阻塞诊断", "Claim blocker diagnostics")}
          value={() => ({ blocked_reason: blockedReason(), quote: quote() })}
        />
      </Show>
      <Show when={ownedClaims().length > 0}>
        <div>
          <div class="panel__title panel__title--spaced">{tr(locale(), "已占用 Agent", "Owned Claims")}</div>
          <div class="event-list">
            <For each={ownedClaims()}>
              {(owned) => (
                <EventCard
                  title={claimTarget(owned)}
                  badge={claimField(owned, "release_ready", "releaseReady") || claimField(owned, "release_ready_in_epochs", "releaseReadyInEpochs") === 0 || claimField(owned, "status") === "release_ready" ? "release ready" : claimField(owned, "release_cooldown", "releaseCooldown") ? "cooldown" : "maintain"}
                  badgeClass={claimField(owned, "release_ready", "releaseReady") || claimField(owned, "release_ready_in_epochs", "releaseReadyInEpochs") === 0 || claimField(owned, "status") === "release_ready" ? "badge badge--accent" : "badge"}
                  meta={claimStatusText(owned)}
                >
                  <div class="feedback-summary">
                    {releaseActionState().available
                      ? tr(locale(), "Release 已作为正式可用动作发布；可以从可用动作列表执行。", "Release is published as a canonical available action; execute it from the available actions list.")
                      : releaseActionState().published
                        ? tr(locale(), "Release 动作已经发布但当前不可执行；先处理可用动作列表里的阻塞原因。", "Release is published but currently disabled; resolve the blocker shown in the available actions list first.")
                      : tr(locale(), "维护方式是保持控制权与 upkeep 健康；release 只作为状态指导，直到正式动作发布。", "Maintain by keeping control and upkeep healthy; release stays guidance-only until a canonical action is published.")}
                  </div>
                  <Show when={claimOwnedDetail(owned)}>
                    <div class="feedback-detail">{claimOwnedDetail(owned)}</div>
                  </Show>
                </EventCard>
              )}
            </For>
          </div>
        </div>
      </Show>
    </PanelSection>
  );
}

function ExpansionTradeoffCards(props) {
  const locale = () => props.locale ?? uiLocale();
  const cards = () => expansionBranchCards(props.gameplay, locale());
  const legacyOnly = () => cards().length === 1 && cards()[0].legacy;
  return (
    <PanelSection
      title={tr(locale(), "扩张取舍", "Expansion Tradeoffs")}
      eyebrow={legacyOnly()
        ? tr(locale(), "旧版 / 不完整", "Legacy / Incomplete")
        : tr(locale(), "运行时推荐", "Runtime Recommendations")}
      meta={props.gameplay?.branchHint || tr(locale(), "当前分支提示尚未发布。", "No branch premise is published yet.")}
    >
      <Show
        when={!legacyOnly()}
        fallback={(
          <div class="feedback-summary">
            {tr(locale(), "结构化分支推荐不可用；此处仅保留旧版提示，不会从动作文本合成取舍字段。", "Structured branch recommendations are unavailable; only the legacy hint is shown, and no tradeoff fields are synthesized from action text.")}
          </div>
        )}
      >
        <div class="action-grid">
          <For each={cards()}>
            {(card) => (
            <EventCard
              class="event-card event-card--action"
              title={card.routeLabel || tr(locale(), "未命名路线", "Unnamed route")}
              badge={card.action
                ? card.action.disabledReason
                  ? tr(locale(), "暂不可用", "unavailable")
                  : tr(locale(), "可执行", "actionable")
                : tr(locale(), "动作未发布", "action unpublished")}
              badgeClass={card.action && !card.action.disabledReason ? "badge badge--good" : "badge badge--warn"}
              meta={props.gameplay?.goalTitle || tr(locale(), "当前扩张目标", "Current expansion goal")}
            >
              <Show when={!card.complete}>
                <div class="badge-row"><Badge class="badge badge--warn">{tr(locale(), "推荐信息不完整", "Incomplete recommendation")}</Badge></div>
              </Show>
              <div class="feedback-detail">
                <div class="metric__label">{tr(locale(), "即时收益", "Immediate gain")}</div>
                {card.immediateGain || tr(locale(), "即时收益未发布", "Immediate gain unavailable")}
              </div>
              <For each={card.futureBeats}>{(futureBeat, index) => <div class="feedback-detail"><div class="metric__label">{tr(locale(), `后续变化 ${index() + 1}`, `Future beat ${index() + 1}`)}</div>{futureBeat}</div>}</For>
              <Show when={card.futureBeats.length === 0}><div class="feedback-detail"><div class="metric__label">{tr(locale(), "旧版后续变化", "Legacy future beat")}</div>{card.futureBeatChanged || tr(locale(), "后续变化未发布", "Future beat unavailable")}</div></Show>
              <div class="feedback-detail">
                <div class="metric__label">{tr(locale(), "风险或锁定", "Risk or lock-in")}</div>
                {card.riskOrLockin || tr(locale(), "风险或锁定未发布", "Risk or lock-in unavailable")}
              </div>
              <div class="feedback-detail">
                <div class="metric__label">{tr(locale(), "下次续玩钩子", "Next-session hook")}</div>
                {card.nextSessionHook || tr(locale(), "下次续玩钩子未发布", "Next-session hook unavailable")}
              </div>
              <Show when={card.firstDeliveryPreview}>
                <FirstDeliveryPreview preview={card.firstDeliveryPreview} locale={locale()} tr={tr} />
              </Show>
              <div class="feedback-summary">
                {card.action
                  ? card.action.disabledReason
                    ? `${card.action.label || card.action.actionId}: ${card.action.disabledReason}`
                    : card.action.label || card.action.actionId
                  : card.actionId || tr(locale(), "关联动作未发布", "Linked action unpublished")}
              </div>
            </EventCard>
            )}
          </For>
        </div>
      </Show>
    </PanelSection>
  );
}

function InlineHelpTip(props) {
  const locale = () => props.locale ?? uiLocale();
  const [isOpen, setIsOpen] = createSignal(false);
  let rootRef;

  onMount(() => {
    const handlePointerDown = (event) => {
      if (!rootRef?.contains(event.target)) {
        setIsOpen(false);
      }
    };
    const handleKeyDown = (event) => {
      if (event.key === "Escape") {
        setIsOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    });
  });

  return (
    <div ref={rootRef} class="inline-help-tip" data-open={isOpen() ? "true" : "false"}>
      <button
        type="button"
        class="inline-help-tip__button"
        aria-label={props.label ?? tr(locale(), "打开比例说明", "Open scale guidance")}
        aria-describedby={props.id}
        aria-expanded={isOpen() ? "true" : "false"}
        aria-controls={props.id}
        onClick={() => setIsOpen((value) => !value)}
      >
        ?
      </button>
      <div id={props.id} class="inline-help-tip__panel" aria-hidden={isOpen() ? "false" : "true"}>
        <div class="inline-help-tip__title">{props.title ?? tr(locale(), "比例说明", "Scale Guidance")}</div>
        <div class="inline-help-tip__body">
          <For each={props.lines ?? []}>
            {(line) => <div class="feedback-detail">{line}</div>}
          </For>
        </div>
      </div>
    </div>
  );
}
function FeedbackCard(props) {
  const feedbackStage = () => normalizedFeedbackStage(props.feedbackStage);
  return (
    <div
      class="feedback-card"
      data-feedback-stage={feedbackStage()}
      data-feedback-kind={props.feedback?.kind} data-prompt-value-visibility={props.feedback?.kind === "prompt" ? props.feedback?.response?.value_visibility : undefined}
      role={props.liveRegion ? "status" : undefined}
      aria-live={props.liveRegion ? "polite" : undefined}
    >
      <div class="badge-row">
        <Badge class={props.display.badgeClass}>{props.display.label}</Badge>
        <Show when={props.display.code}>
          <Badge>{`code=${props.display.code}`}</Badge>
        </Show>
      </div>
      <div class="feedback-summary">{props.display.summary}</div>
      <Show when={props.display.detail}>
        <div class="feedback-detail">{props.display.detail}</div>
      </Show>
      <Show when={props.feedback}>
        <DiagnosticDetails value={props.feedback} />
      </Show>
      <Show when={props.recoveryAction && props.display.code === "control_lost"}><div class="toolbar" data-control-loss-recovery="binding"><button type="button" data-testid="control-loss-recovery" onClick={props.recoveryAction}>{props.recoveryLabel}</button></div></Show>
    </div>
  );
}

function normalizedFeedbackStage(stage) {
  const value = String(stage || "").trim().toLowerCase();
  if (["ack", "sent", "queued", "completed", "blocked", "rejected", "error"].includes(value)) {
    return value;
  }
  return undefined;
}

function MetricCard(props) {
  return (
    <div class={props.class ?? "metric"}>
      <div class="metric__label">{props.label}</div>
      <div class="metric__value">{props.value}</div>
      <Show when={props.detail}>
        <div class="feedback-detail flow-top--tight">{props.detail}</div>
      </Show>
      <Show when={props.children}>
        <div class="badge-row badge-row--tight">
          {props.children}
        </div>
      </Show>
    </div>
  );
}

function EventCard(props) {
  return (
    <div
      class={props.class ?? "event-card"}
      data-action-state={props.actionState}
      data-testid={props.testId}
      role={props.role}
      aria-live={props.ariaLive}
    >
      <div class="event-card__title">
        <span>{props.title}</span>
        <Show when={props.badge}>
          <span class={props.badgeClass ?? "badge"}>{props.badge}</span>
        </Show>
      </div>
      <Show when={props.meta}>
        <div class="event-card__meta">{props.meta}</div>
      </Show>
      {props.children}
    </div>
  );
}

function PanelSection(props) {
  return (
    <div class={`panel panel--nested ${props.class ?? ""}`}>
      <div class="panel__header">
        <div class="stack stack--compact">
          <Show when={props.eyebrow}>
            <div class="panel__eyebrow">{props.eyebrow}</div>
          </Show>
          <div class="panel__title">{props.title}</div>
          <Show when={props.meta}>
            <div class="panel__meta-copy">{props.meta}</div>
          </Show>
        </div>
      </div>
      <div class="panel__body stack">{props.children}</div>
    </div>
  );
}

function CalloutCard(props) {
  return (
    <div
      class={`callout ${props.variant === "warn" ? "callout--warn" : ""} ${props.class ?? ""}`}
      data-callout-kind={props.kind ?? ""}
    >
      <div class="callout__header">
        <div class="callout__title">{props.title}</div>
        <Show when={props.badge}>
          <Badge class={props.badgeClass ?? "badge badge--warn"}>{props.badge}</Badge>
        </Show>
      </div>
      <div class="callout__body">{props.children}</div>
    </div>
  );
}

export {
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
};
