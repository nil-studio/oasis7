import { alternativeStatusLabel, alternativePosition, alternativeScopeLabel, alternativeRiskLabel, alternativeAuthorityLabel, alternativeBlockerLabel, alternativeOutcomeLabel } from "./safe_alternative_display.js";
import { safeAlternativeStyles } from "./safe_alternative_styles.js";
import { For, Show, createSignal } from "solid-js";
export function SafeAlternativePanel(props) {
  const [goal, setGoal] = createSignal("");
  const [waypoints, setWaypoints] = createSignal("");
  const [arrival, setArrival] = createSignal("");
  const [error, setError] = createSignal("");
  const ui = () => props.core.state.safeAlternative;
  const run = async (fn) => { try { setError(""); await fn(); } catch (e) { setError(String(e)); } };
  const op = (operation, data) => props.core.safeAlternative.command({ operation, data });
  const expiry = () => Number(props.core.state.logicalTime) + 100;
  const propose = () => {
    const points = JSON.parse(waypoints());
    if (!Array.isArray(points) || !points.length || points.length > 8) throw new Error("Enter 1 to 8 waypoint coordinates");
    return op("propose", { request_id: crypto.randomUUID(), original: {
      goal: goal(), waypoints: points, arrival_tick: arrival().trim() ? Number(arrival()) : null }, valid_until_tick: expiry() });
  };
  return <details class="safe-alternative-panel">
    <style>{safeAlternativeStyles}</style>
    <summary>Safe movement alternatives / 安全移动替代</summary>
    <p>Waypoint routes and arrival timing are unsupported. Compare a direct movement alternative before independently confirming and executing. Preview and confirmation have no world effect.</p>
    <label>Original goal / 原目标<input value={goal()} onInput={e => setGoal(e.currentTarget.value)} /></label>
    <label>Waypoint coordinates / 途经点坐标<textarea placeholder={'[{"x_cm":0,"y_cm":0,"z_cm":0},{"x_cm":1,"y_cm":0,"z_cm":0}]'} value={waypoints()} onInput={e => setWaypoints(e.currentTarget.value)} /></label>
    <label>Requested arrival tick / 原到达时间（可选）<input type="number" value={arrival()} onInput={e => setArrival(e.currentTarget.value)} /></label>
    <button onClick={() => run(propose)} disabled={ui()?.status === "unknown"}>Compare alternatives / 比较替代</button>
    <button onClick={() => run(() => props.core.safeAlternative.recover())}>Recover original command / 恢复原命令</button>
    <button onClick={() => run(() => props.core.safeAlternative.recover(true))}>Retry exact original / 重发原签名命令</button>
    <p role="status">{alternativeStatusLabel(ui()?.status)} {error() || ui()?.error}</p>
    <Show when={ui()?.record}>{record => <>
      <p>Original goal: {record().original?.goal}</p>
      <p>Requested waypoints: {(record().original?.waypoints || []).map(alternativePosition).join(" → ")}</p>
      <p>Requested arrival tick: {record().original?.arrival_tick ?? "No timing constraint"}</p>
      <p>State: {alternativeStatusLabel(record().state)}</p>
      <Show when={record().blocker}><p role="status">{alternativeBlockerLabel(record().blocker)}</p></Show>
      <For each={record().candidates || []}>{candidate => <article>
        <h4>{candidate.goal}</h4><p>Target: {alternativePosition(candidate.to)}</p>
        <p>Scope: {alternativeScopeLabel(candidate.scope)}</p><p>Cost upper bound: {candidate.cost_upper_bound}</p>
        <p>Risk: {alternativeRiskLabel(candidate.risk)}</p><p>Consequences: {candidate.consequences}</p>
        <p>Withdrawal: {candidate.retractability}</p><p>Reason: {candidate.recommendation}</p>
        <p>Authority: {alternativeAuthorityLabel(candidate.required_authority)}</p>
        <button disabled={record().state !== "awaiting_confirmation" || ui()?.status === "unknown"}
          onClick={() => run(() => op("confirm", { request_id: record().request_id, generation: record().generation,
            candidate_id: candidate.candidate_id, candidate_digest: candidate.candidate_digest,
            confirmation_id: crypto.randomUUID(), valid_until_tick: record().valid_until_tick }))}>Confirm this exact candidate / 确认此候选</button>
      </article>}</For>
      <button disabled={record().authorization?.state !== "available" || ui()?.status === "unknown"}
        onClick={() => run(() => op("execute", { request_id: record().request_id,
          authorization_id: record().authorization.authorization_id, execution_id: crypto.randomUUID() }))}>Execute confirmed choice / 执行已确认选择</button>
      <button disabled={ui()?.status === "unknown"} onClick={() => run(() => op("reevaluate", { request_id: record().request_id, valid_until_tick: expiry() }))}>Reevaluate explicitly / 明确重新评估</button>
      <button disabled={ui()?.status === "unknown"} onClick={() => run(() => op("read", { request_id: record().request_id }))}>Read current state / 重读状态</button>
      <button disabled={ui()?.status === "unknown"} onClick={() => run(() => op("cancel", { request_id: record().request_id }))}>Decline or cancel / 拒绝或取消</button>
      <Show when={record().outcome}><p>Actual outcome: {alternativeOutcomeLabel(record().outcome)}</p></Show>
    </>}</Show>
  </details>;
}
