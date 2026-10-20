import { createSignal } from "solid-js";

function displayValue(value) {
  if (value === null || value === undefined || value === "") return "-";
  return String(value);
}

function classificationCopy(value, locale, tr) {
  switch (String(value || "")) {
    case "enough_to_advance":
      return tr(locale, "足以推进下一步", "Enough to advance");
    case "partial_progress":
      return tr(locale, "可获得部分进展", "Partial progress");
    default:
      return tr(locale, "电力投入不划算", "Poor power tradeoff");
  }
}

function targetCopy(value, locale, tr) {
  switch (String(value || "")) {
    case "factory_build_hardware":
      return tr(locale, "工厂硬件建造", "Factory hardware build");
    default:
      return tr(locale, "当前工业目标", "Current industrial target");
  }
}

function nextDecisionGuidance(value, locale, tr) {
  switch (String(value || "")) {
    case "enough_to_advance":
      return tr(
        locale,
        "这笔预估足以推进目标：把推荐量作为计划参考，再从支持的玩法动作继续；当前面板不会替你提交精炼。",
        "This quote can advance the target: keep the recommended amount as a planning reference, then continue through a supported gameplay action. This panel will not submit refining for you.",
      );
    case "partial_progress":
      return tr(
        locale,
        "这次只能缩小缺口：先比较补电、采矿或等待，再选择支持的玩法动作；当前面板只提供预估。",
        "This only reduces the gap: compare recharging, mining, or waiting before choosing a supported gameplay action. This panel only provides the estimate.",
      );
    default:
      return tr(
        locale,
        "这笔电力投入不划算：先补电、采矿或等待，调整计划后再请求一份新预估。",
        "This power tradeoff is poor: recharge, mine, or wait, then adjust the plan and request a new estimate.",
      );
  }
}

const QUOTE_ERROR_RECOVERY = {
  invalid_amount: ["请输入正整数克数，再请求只读预估。", "Enter a positive whole-number amount in grams, then request another quote."],
  connection_unavailable: ["世界连接已断开。连接恢复后重新请求预估。", "The world connection is offline. Request another quote after it reconnects."],
  player_agent_binding_required: ["请先领取自己的 Agent；已领取时等待 Agent 同步，再刷新玩法快照。", "Claim your Agent first; if already claimed, wait for Agent sync and refresh the gameplay snapshot."],
  player_session_required: ["玩家会话尚未就绪。请先完成登录并进入世界。", "The player session is not ready. Sign in and enter the world first."],
  session_sync_failed: ["玩家控制资格未完成同步。请刷新玩法快照，确认自己的 Agent 后重试。", "Player control has not finished syncing. Refresh the gameplay snapshot and confirm your Agent before retrying."],
  refine_quote_unavailable: ["当前世界尚不支持权威精炼预估。请查看已支持的生产方案；此请求未提交精炼或消耗资源。", "This world does not yet support an authoritative refining quote. Review the supported production plans; this request did not submit refining or spend resources."],
  refine_quote_rejected: ["当前材料、电力或精炼条件不满足预估要求。请检查资源和输入量后重新规划。", "The current materials, power, or refining prerequisites do not support this quote. Check resources and the amount before replanning."],
  auth_signature_invalid: ["请求认证失败。请恢复当前玩家会话后重试。", "Request authentication failed. Recover the current player session before retrying."],
  session_key_unknown: ["当前玩家会话已失效。请重新进入世界后重试。", "The current player session is no longer valid. Enter the world again before retrying."],
};
function quoteRequestErrorCopy(error, code, locale, tr) {
  if (!error && !code) return "";
  const recovery = Object.hasOwn(QUOTE_ERROR_RECOVERY, code) ? QUOTE_ERROR_RECOVERY[code] : null;
  if (recovery) return tr(locale, ...recovery);
  return tr(
    locale,
    "无法获取精炼预估。请检查连接、玩家会话和输入量后重试。",
    "Could not get the refining quote. Check the connection, player session, and amount, then retry.",
  );
}

function linkageCopy(value, locale, tr) {
  switch (String(value || "")) {
    case "enables_factory_build_hardware_goal":
      return tr(locale, "本次产出可满足工厂硬件目标", "This output satisfies the factory hardware target");
    case "reduces_factory_build_hardware_shortfall":
      return tr(locale, "本次产出会缩小工厂硬件缺口", "This output reduces the factory hardware gap");
    default:
      return tr(locale, "本次产出不会缩小当前工厂硬件缺口", "This output does not reduce the current factory hardware gap");
  }
}

function QuoteMetric(props) {
  return (
    <div class={props.class ? `metric ${props.class}` : "metric"}>
      <div class="metric__label">{props.label}</div>
      <div class="metric__value">{displayValue(props.value)}</div>
      {props.detail ? <div class="metric__detail">{props.detail}</div> : null}
    </div>
  );
}

export function RefineQuotePreflightCard(props) {
  const quote = () => props.quote || {};
  const locale = () => props.locale;
  const tr = props.tr;
  return (
    <section class="panel panel--nested" data-testid="refine-quote-preflight" data-quote-kind="preflight">
      <div class="panel__header">
        <div class="stack stack--compact">
          <div class="panel__eyebrow">{tr(locale(), "提交前估价", "Before You Commit")}</div>
          <div class="panel__title">{tr(locale(), "化合物精炼预估", "Compound Refining Quote")}</div>
          <div class="panel__meta-copy">
            {tr(
              locale(),
              "这是只读预估，不会提交精炼、扣除电力或生成回执。",
              "This is a read-only quote. It does not submit refining, spend electricity, or create a receipt.",
            )}
          </div>
        </div>
      </div>
      <div class="panel__body stack">
        <div class="badge-row">
          <span class="badge badge--accent">{tr(locale(), "预估", "quote")}</span>
          <span class="badge" data-target-id={displayValue(quote().target_id)}>{`${tr(locale(), "目标", "target")}: ${targetCopy(quote().target_id, locale(), tr)}`}</span>
          <span class="badge">{`${tr(locale(), "Agent", "Agent")}: ${displayValue(quote().owner_agent_id)}`}</span>
        </div>
        <div class="summary-grid">
          <QuoteMetric
            class="metric--target-gap-outcome"
            label={tr(locale(), "目标缺口", "Target gap")}
            value={`${displayValue(quote().target_gap_before)} → ${displayValue(quote().target_gap_after)}`}
          />
          <QuoteMetric label={tr(locale(), "精炼量", "Refine amount")} value={`${displayValue(quote().compound_mass_g)} g`} />
          <QuoteMetric label={tr(locale(), "电力成本", "Electricity cost")} value={quote().electricity_cost} />
          <QuoteMetric label={tr(locale(), "剩余电力", "Electricity remaining")} value={quote().electricity_after} />
          <QuoteMetric label={tr(locale(), "硬件产出", "Hardware output")} value={quote().hardware_output} />
          <QuoteMetric label={tr(locale(), "建议精炼量", "Recommended amount")} value={`${displayValue(quote().recommended_refine_amount)} g`} />
        </div>
        <div class="feedback-summary">
          {`${tr(locale(), "目标关联", "Target linkage")}: ${linkageCopy(quote().target_linkage, locale(), tr)}`}
        </div>
        <div class="feedback-summary">
          {`${tr(locale(), "价值判断", "Value assessment")}: ${classificationCopy(quote().value_classification, locale(), tr)}`}
        </div>
        <div class="feedback-summary" data-testid="refine-quote-next-decision">
          {`${tr(locale(), "下一步建议", "Next decision")}: ${nextDecisionGuidance(quote().value_classification, locale(), tr)}`}
        </div>
      </div>
    </section>
  );
}

export function RefineQuotePreflightPanel(props) {
  const [compoundMassG, setCompoundMassG] = createSignal("40");
  const [requesting, setRequesting] = createSignal(false);
  const [requestError, setRequestError] = createSignal("");
  const [requestStatus, setRequestStatus] = createSignal("");
  const locale = () => props.locale;
  const tr = props.tr;
  const remoteRequestState = () => props.requestState || {};
  const visibleError = () => quoteRequestErrorCopy(
    remoteRequestState().status === "error" ? remoteRequestState().error : requestError(),
    remoteRequestState().errorCode,
    locale(),
    tr,
  );
  const safeErrorCode = () => remoteRequestState().status === "error"
    ? (Object.hasOwn(QUOTE_ERROR_RECOVERY, remoteRequestState().errorCode) ? remoteRequestState().errorCode : "unknown_error") : null;
  const visibleStatus = () => remoteRequestState().status === "received"
    ? tr(locale(), "预估已返回，请查看报价结果。", "Quote received; review the estimate below.")
    : requestStatus();

  async function requestQuote(event) {
    event.preventDefault();
    setRequestError("");
    setRequestStatus("");
    setRequesting(true);
    try {
      const result = await props.requestRefineQuote(compoundMassG());
      if (!result?.ok) {
        setRequestError(result?.reason || tr(locale(), "无法请求预估，请稍后重试。", "Could not request a quote. Please try again."));
        return;
      }
      setRequestStatus(tr(locale(), "已请求只读预估，正在等待报价结果。", "Read-only quote requested; waiting for the quote result."));
    } catch (error) {
      setRequestError(`${tr(locale(), "请求预估失败", "Quote request failed")}: ${String(error)}`);
    } finally {
      setRequesting(false);
    }
  }

  return (
    <section id="viewer-refine-quote-panel" class="panel panel--nested" data-testid="refine-quote-panel" data-quote-kind="preflight">
      <div class="panel__header">
        <div class="stack stack--compact">
          <div class="panel__eyebrow">{tr(locale(), "提交前估价", "Before You Commit")}</div>
          <div class="panel__title">{tr(locale(), "化合物精炼预估", "Compound Refining Quote")}</div>
          <div class="panel__meta-copy">
            {tr(locale(), "请求预估不会提交精炼、扣除电力或生成回执。", "Requesting a quote does not submit refining, spend electricity, or create a receipt.")}
          </div>
        </div>
      </div>
      <div class="panel__body stack">
        <form class="stack stack--compact" onSubmit={requestQuote} data-testid="refine-quote-request-form">
          <label>
            <span>{tr(locale(), "精炼量（克）", "Refine amount (g)")}</span>
            <input
              aria-label={tr(locale(), "精炼量（克）", "Refine amount (g)")}
              type="number"
              min="1"
              step="1"
              inputmode="numeric"
              value={compoundMassG()}
              onInput={(event) => setCompoundMassG(event.currentTarget.value)}
            />
          </label>
          <button type="submit" class="button button--secondary" disabled={requesting()}>
            {requesting() ? tr(locale(), "正在请求预估…", "Requesting quote…") : tr(locale(), "请求预估", "Request quote")}
          </button>
        </form>
        {visibleError() ? <div class="feedback-summary feedback-summary--error" role="alert">{visibleError()}</div> : null}
        {safeErrorCode() ? <details class="feedback-summary"><summary>{tr(locale(), "安全诊断", "Safe diagnostics")}</summary><code>{safeErrorCode()}</code></details> : null}
        {visibleStatus() ? <div class="feedback-summary" role="status">{visibleStatus()}</div> : null}
        {props.quote ? <RefineQuotePreflightCard quote={props.quote} locale={locale()} tr={tr} /> : null}
      </div>
    </section>
  );
}
