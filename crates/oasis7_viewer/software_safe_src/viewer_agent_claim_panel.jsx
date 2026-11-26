import { createSignal, For, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge } from "./viewer_ui_primitives.jsx";
import { AgentClaimChoiceCard } from "./agent_claim_choice_card.jsx";
import { createViewerAgentClaimDisplayModel } from "./viewer_agent_claim_display_model.js";
import {
  gameplayActionBlockedReasonId,
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionDetail,
  gameplayActionDisabledReason,
  gameplayActionDisplayLabel,
  gameplayActionPendingFor,
  gameplayActionTestId,
} from "./viewer_gameplay_action_helpers.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

const {
  buildAgentClaimAction,
  buildAgentClaimTargets,
  hasAgentClaimSessionBoundary,
  hasExecutableAgentClaim,
  publishedClaimChoiceCandidates,
  slot1ClaimChoiceQuote,
} = createViewerAgentClaimDisplayModel({ state: core.state, tr });

export function AgentClaimSessionBoundaryCard(props) {
  const { CalloutCard } = props;
  const locale = () => props.locale ?? uiLocale();
  const agentClaim = () => props.gameplay?.agentClaim || null;
  return (
    <CalloutCard
      title={tr(locale(), "当前账号尚未绑定 Agent", "Current Account Has No Bound Agent")}
      badge="observe"
      badgeClass="badge badge--accent"
    >
      <div class="feedback-summary">
        {tr(
          locale(),
          "这个世界已经有 Agent，但当前账号还没有可作为 claimer 的绑定 Agent。",
          "This world already has Agents, but the current account has no bound Agent that can act as the claimer.",
        )}
      </div>
      <div class="feedback-detail">
        {tr(
          locale(),
          "可以先观察世界对象；认领入口必须来自当前会话绑定和 canonical slot-1 quote，不能从世界里的第一个 Agent 推断。",
          "You can observe world objects first; claim entry must come from the current session binding and canonical slot-1 quote, not from the first Agent in the world.",
        )}
      </div>
      <div class="badge-row">
        <Badge>{`boundAgent=${core.state.auth.boundAgentId || "-"}`}</Badge>
        <Badge>{`claimer=${agentClaim()?.claimer_agent_id || "-"}`}</Badge>
        <Badge>{`owned=${agentClaim()?.owned_claim_count ?? 0}/${agentClaim()?.claim_cap ?? "-"}`}</Badge>
      </div>
      <AgentClaimChoiceCard locale={locale} tr={tr} quote={() => agentClaim()?.next_claim_quote || agentClaim()?.nextClaimQuote} publishedCandidates={() => publishedClaimChoiceCandidates(core.state.snapshot, agentClaim())} choiceQuote={() => slot1ClaimChoiceQuote(agentClaim())} />
    </CalloutCard>
  );
}
export function AgentClaimPanel(props) {
  const { CalloutCard, renderGameplayAction } = props;
  const locale = () => props.locale ?? uiLocale();
  const [selectedTargetId, setSelectedTargetId] = createSignal("");
  const agentClaim = () => props.gameplay?.agentClaim || null;
  const quote = () => agentClaim()?.next_claim_quote || null;
  const targets = () => buildAgentClaimTargets(core.state.snapshot, agentClaim());
  const selectedTarget = () => {
    const current = selectedTargetId();
    if (current && targets().some((target) => target.id === current)) {
      return current;
    }
    return targets()[0]?.id || "";
  };
  const claimAction = () => buildAgentClaimAction(agentClaim(), selectedTarget());
  const disabledReason = () => {
    if (!agentClaim()) {
      return tr(locale(), "当前快照没有发布 Agent claim 数据。", "The current snapshot has no Agent claim data.");
    }
    if (!selectedTarget()) {
      return tr(locale(), "当前没有可认领的 Agent。", "There is no claimable agent right now.");
    }
    return claimAction()?.disabledReason || null;
  };

  return (
    <CalloutCard
      title={tr(locale(), "认领 Agent", "Agent Claim")}
      badge={quote() ? `slot=${quote().slot_index}` : "claim"}
      badgeClass={disabledReason() ? "badge badge--warn" : "badge badge--good"}
    >
      <div class="feedback-summary">
        {agentClaim()?.objective
          || tr(locale(), "选择一个未被占用的 Agent，并用当前玩家会话提交认领。", "Pick an unclaimed agent and submit the claim with the current player session.")}
      </div>
      <div class="feedback-detail">
        {agentClaim()?.progress_detail
          || tr(locale(), "首次 slot-1 认领可以使用专用 starter claim 额度补足前置费用。", "The first slot-1 claim can use the dedicated starter claim allowance for upfront costs.")}
      </div>
      <div class="badge-row">
        <Badge>{`claimer=${agentClaim()?.claimer_agent_id || "-"}`}</Badge>
        <Badge>{`owned=${agentClaim()?.owned_claim_count ?? 0}/${agentClaim()?.claim_cap ?? "-"}`}</Badge>
        <Badge>{`eligible=${quote()?.eligible_claim_balance ?? agentClaim()?.slot_1_eligible_claim_balance ?? "-"}`}</Badge>
        <Badge>{`upfront=${quote()?.total_upfront_amount ?? "-"}`}</Badge>
      </div>
      <AgentClaimChoiceCard locale={locale} tr={tr} quote={quote} publishedCandidates={() => publishedClaimChoiceCandidates(core.state.snapshot, agentClaim())} choiceQuote={() => slot1ClaimChoiceQuote(agentClaim())} />
      <div class="control-grid">
        <div class="field">
          <label for="agent-claim-target">
            {tr(locale(), "目标 Agent", "Target Agent")}
          </label>
          <select
            id="agent-claim-target"
            value={selectedTarget()}
            onInput={(event) => setSelectedTargetId(event.currentTarget.value)}
          >
            <For each={targets()}>
              {(target) => (
                <option value={target.id}>
                  {`${target.name}${target.isClaimer ? ` (${tr(locale(), "当前绑定", "current binding")})` : ""}`}
                </option>
              )}
            </For>
          </select>
        </div>
      </div>
      <div class="toolbar">
        <button
          class={gameplayActionButtonClass(claimAction())}
          aria-busy={gameplayActionButtonBusyAttrs(claimAction())}
          disabled={Boolean(disabledReason()) || gameplayActionPendingFor(claimAction())}
          onClick={() => {
            const action = claimAction();
            if (action) {
              renderGameplayAction(action);
            }
          }}
        >
          {gameplayActionDisplayLabel(claimAction(), locale(), tr(locale(), "认领 Agent", "Claim Agent"))}
        </button>
      </div>
    </CalloutCard>
  );
}
