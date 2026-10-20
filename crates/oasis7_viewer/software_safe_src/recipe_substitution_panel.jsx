import { For, Show } from "solid-js";
export function RecipeSubstitutionPanel(props) {
  const tr = (zh, en) => props.locale === "zh" ? zh : en;
  const missing = () => tr("未发布", "Unpublished");
  const n = value => value ?? missing();
  const kind = value => ({same_material: tr("同料换来源", "Alternate source"), profile_substitute: tr("显式异料替代", "Profile substitute"), new_recipe: tr("更换配方", "New recipe"), unknown: tr("兼容性未知", "Unknown compatibility")})[value];
  const nextAction = value => value === "explicit_confirmation" ? tr("明确确认所选候选", "Explicitly confirm the selected candidate") : (value === "resolve_blocker" || value === "restore_authority_and_preview_again") ? tr("补充权威证据或资源后复查", "Resolve authority evidence or resource blockers and recheck") : missing();
  const status = value => ({available: tr("存在合法候选", "Legal candidates available"), no_legal_substitute: tr("没有合法替代", "No legal substitute"), unknown: tr("替代条件未知", "Substitution conditions unknown")})[value];
  return <Show when={props.model}>{model => <section class="panel-section" aria-label={tr("配方替代", "Recipe substitution")}>
    <h3>{tr("配方替代", "Recipe substitution")}</h3>
    <p>{tr("预览与决定不扣料、不转换，也不增加工业进度或奖励。", "Previews and decisions consume no materials, perform no conversion, and add no industrial progress or reward.")}</p>
    <For each={model().roots}>{root => <p>{root.recipe} · {tr("工业进度", "Work")} {n(root.work)}</p>}</For>
    <For each={model().previews}>{preview => <article>
      <h4>{preview.slot} · {status(preview.status)}</h4>
      <Show when={preview.status === "unknown"}><p>{tr("补齐权威证据后重新预览；未知兼容性不能作为安全选项。", "Restore authority evidence and preview again; unknown compatibility provides no safe option.")}</p></Show>
      <For each={preview.candidates}>{candidate => <div>
        <strong>{kind(candidate.kind)} · {candidate.kind === "new_recipe" ? candidate.target : `${candidate.source || missing()} → ${candidate.target || missing()}`}</strong>
        <Show when={candidate.kind === "new_recipe"}><p>{tr("新配方投入 / 产出 / 用电", "New recipe inputs / outputs / power")}: {candidate.recipeInputs?.map(item => `${item.kind} × ${item.amount}`).join(" · ")} / {candidate.recipeOutputs?.map(item => `${item.kind} × ${item.amount}`).join(" · ")} / {n(candidate.recipePower)}</p></Show>
        <Show when={candidate.kind !== "new_recipe"}><p>{tr("投入 / 可执行产出", "Input / executable output")}: {n(candidate.sourceRequired)} {candidate.sourceUnit} / {n(candidate.targetQuantity)} {candidate.targetUnit}</p>
        <p>{tr("比例 / 取整", "Ratio / rounding")}: {n(candidate.ratioNumerator)} / {n(candidate.ratioDenominator)} · {candidate.rounding === "exact" ? tr("精确", "Exact") : candidate.rounding === "ceil" ? tr("向上取整", "Round up") : missing()}</p>
        <p>{tr("损耗 / 残余", "Loss / residual")}: {n(candidate.loss)} / {n(candidate.residualNumerator)} ÷ {n(candidate.residualDenominator)}</p>
        </Show>
        <p>{tr("用电 / 终端容量", "Power / terminal capacity")}: {n(candidate.power)} / {n(candidate.terminal)}</p>
        <p>{tr("物流 / 缓冲 / 价值依据", "Logistics / buffer / value basis")}: {candidate.logistics || missing()} / {candidate.buffer || missing()} / {candidate.valueClass || missing()}</p>
        <p>{tr("声明估值：已持有 / 已消费 / 预期产出", "Declared value: held / consumed / expected output")}: {n(candidate.valuation?.held)} / {n(candidate.valuation?.consumed)} / {n(candidate.valuation?.expected)} {candidate.valuation?.unit || missing()}</p>
        <p>{tr("已声明风险", "Declared risks")}: {candidate.risks?.join(" · ") || missing()}</p>
        <p>{candidate.causalEquivalent ? tr("全产出因果保持；决定本身不增加进度。", "Output causality preserved; the decision adds no work.") : tr("因果改变；需要明确处置与新候选，进度从零开始。", "Causality changes; explicit disposition and a new candidate starting at zero work are required.")}</p>
      </div>}</For>
      <p>{tr("下一动作 / 复查时刻", "Next action / recheck time")}: {nextAction(preview.nextAction)} / {n(preview.nextRecheck)}</p>
      <Show when={preview.blocker}><p>{tr("当前条件阻塞，等待补充权威证据或资源后复查。", "Conditions are blocked; recheck after authority evidence or resources become available.")}</p></Show>
    </article>}</For>
    <For each={model().decisions}>{decision => <article>
      <p>{tr("已确认决定", "Confirmed decision")}: {decision.id}</p>
      <p>{decision.scope === "decision_only" ? tr("仅记录选择；未授权转换。", "Choice recorded only; conversion is not authorized.") : decision.scope === "conversion_authorized" ? tr("已授权精确转换；以独立结算回执核对实际效果。", "Exact conversion authorized; verify actual effects using its separate settlement receipt.") : tr("授权范围未知；不能执行转换。", "Authorization scope unknown; conversion is not permitted.")}</p>
      <Show when={decision.settlement} fallback={<p>{tr("尚无转换结算回执。", "No conversion settlement receipt.")}</p>}>
        <p>{tr("实际转换已结算", "Actual conversion settled")}: {decision.settlement} · {tr("消耗 / 损耗", "Consumed / loss")}: {n(decision.consumed)} / {n(decision.loss)}</p>
      </Show>
    </article>}</For>
    <Show when={model().truncated}><p>{tr("结果超过读取上限；请按准确工作身份继续查询。", "Read limit reached; continue using an exact work identity.")}</p></Show>
  </section>}</Show>;
}
