import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { BrowserModelConnectionPanel } from "./browser_model_connection_panel.jsx";
import { createBrowserGameSession } from "./browser_game_session.js";
import { ModelConnectionPanel } from "./model_connection_panel.jsx";

export function BrowserByokSurface(props) {
  const read = (value) => typeof value === "function" ? value() : value;
  const tr = (cn, en) => String(read(props.locale) || "en").startsWith("zh") ? cn : en;
  const [driver, setDriver] = createSignal(null), [state, setState] = createSignal(null);
  const [error, setError] = createSignal(""), [confirm, setConfirm] = createSignal(false), [runConsent, setRunConsent] = createSignal(false);
  let contextKey, previousAccount, unsubscribe, alive = true;
  const config = () => read(props.config);
  const reason = (code) => {
    if (code === "executor_takeover_confirmation_required") return tr("当前已有其他执行方。核对任务后，勾选授权以明确切换到此浏览器。", "Another executor is bound. Review the task and explicitly confirm switching to this browser.");
    if (["operation_unknown", "operation_pending", "recovery_required", "participant_binding_required"].includes(code)) return tr("原件或绑定尚未确认。请只读核对；未知结果不能重发，也不能据最新快照判断未发生。", "An original operation or binding is unconfirmed. Use read-only lookup; do not resubmit or infer non-execution from a recent snapshot.");
    if (["task_fence_changed", "game_session_changed", "executor_not_bound", "task_unavailable"].includes(code)) return tr("任务、目标或执行权限已改变。已停止；重新核对实际任务与执行身份后再确认。", "Task, goal or executor authority changed. Stopped; review the actual task and executor again before confirming.");
    if (["page_paused", "stopped"].includes(code)) return tr("本页已暂停新决策。返回后先核对原件，再明确开始；已受理世界作业仍可能推进。", "New decisions are paused. Check originals before explicitly starting again; accepted world jobs may continue.");
    if (["legacy_run_active", "legacy_state_unknown"].includes(code)) return tr("先在下方核对并停止旧执行端运行；状态未知时不能开始浏览器运行。", "Review and stop the previous executor below. Browser execution cannot start while its state is unknown.");
    if (code === "agent_busy") return tr("同一 Agent 已在另一个页面运行。本页不会自动接管。", "This Agent is running in another page. This page will not take over automatically.");
    if (String(code).includes("budget") || code === "run_time_limit") return tr("本轮预算或期限已到，不自动扩大或重置。", "This run's budget or deadline is exhausted; it is not automatically increased or reset.");
    return tr("游戏客户端操作未完成。请检查本机存储、登录、合法任务及当前部署支持；原件未知仍保持未知。", "The Game Client operation did not complete. Check browser storage, sign-in, the authorized task and deployment support; unknown originals stay unknown.");
  };
  function refresh() {
    const value = driver()?.snapshot(); setState(value || null);
    if (value?.code) setError(reason(value.code));
  }
  createEffect(() => {
    const accountId = read(props.accountId), agentId = read(props.agentId), owner = read(props.ownerSigner), trusted = config();
    const key = `${accountId || ""}:${agentId || ""}:${owner?.identityId || owner?.publicKey || ""}:${trusted?.servicePublicKey || ""}`;
    if (key === contextKey) return;
    contextKey = key; unsubscribe?.(); setError(""); setConfirm(false); setRunConsent(false);
    const old = driver();
    if (old) {
      if (previousAccount !== accountId) void old.logout().catch(() => { if (alive) setError(reason("storage_write_failed")); });
      old.dispose();
    }
    previousAccount = accountId; setDriver(null); setState(null);
    if (!accountId || !agentId || !owner || !trusted) return;
    try {
      const captured = contextKey;
      const signer = { publicKeyHex: owner.publicKey,
        sign: async (bytes) => { if (captured !== contextKey) throw new Error("game_session_changed"); return owner.sign(bytes); } };
      const session = (props.createSession || createBrowserGameSession)({ config: trusted, accountId, agentId, ownerSigner: signer, isCurrent: () => alive && captured === contextKey });
      setDriver(session); unsubscribe = session.subscribe(refresh); refresh();
    } catch { setError(reason("game_configuration_invalid")); }
  });
  const modelReady = () => { state(); return driver()?.modelSession.snapshot().checked === true; };
  const act = async (action) => {
    setError(""); setRunConsent(false);
    try { await action(driver()); } catch (failure) { if (alive) setError(reason(failure.code)); }
    finally { if (alive) refresh(); }
  };
  onCleanup(() => { alive = false; contextKey = null; unsubscribe?.(); driver()?.dispose(); });
  return <section class="stack" style="overflow-wrap:anywhere;min-width:0" aria-label={tr("浏览器自带模型 Agent", "Browser BYOK Agent")}>
    <Show when={config()} fallback={<p role="status">{tr("此部署尚未启用浏览器 Game Client。不会收集本地模型 Key。", "This deployment has not enabled the browser Game Client. Model keys are not collected.")}</p>}>
      <Show when={driver()} fallback={<p role="status">{tr("先登录并选中自己的 Agent，才能核对游戏任务。", "Sign in and select your Agent before checking the game task.")}</p>}>
        <p>{tr("实际执行入口：当前浏览器。模型 Key 不授予游戏权限；下面的登记仅复用既有玩家/Agent/Task，不创建新的 Agent。", "Execution location: this browser. A model key grants no game authority. Registration below reuses the existing player, Agent and Task; it creates no Agent.")}</p>
        <Show when={state()?.fence}>{(fence) => <p role="status">{`world=${fence().world_id} · owner=${fence().owner_binding} · Agent=${fence().agent_id} · Task=${fence().task_id} · goal=${fence().active_goal_reference?.goal_ref || "—"} · executor=${fence().executor_binding?.executor_public_key || "—"}`}<br />{state()?.goal}</p>}</Show>
        <button type="button" disabled={["running", "preparing"].includes(state()?.phase)} onClick={() => { setConfirm(false); void act((session) => session.prepare({ inspectOnly: true })); }}>{tr("只读核对当前任务", "Inspect current task")}</button>
        <label><input type="checkbox" checked={confirm()} onChange={(event) => setConfirm(event.currentTarget.checked)} disabled={!state()?.fence || ["running", "preparing"].includes(state()?.phase)} />{tr("确认核对既有任务并授权此浏览器执行身份；若已有其他执行方，明确同意切换。", "Confirm the existing task and authorize this browser executor; explicitly switch if another executor is bound.")}</label>
        <button type="button" disabled={!state()?.fence || !confirm() || ["running", "preparing"].includes(state()?.phase)} onClick={() => { const confirmedFence = state()?.fence; setConfirm(false); void act((session) => session.prepare({ takeover: true, confirmedFence })); }}>{tr("核对任务并登记执行身份", "Check task and register executor")}</button>
        <BrowserModelConnectionPanel locale={props.locale} accountId={props.accountId} executionReady={() => state()?.ready === true} session={() => driver()?.modelSession} onStop={() => driver()?.stop()} />
        <label><input type="checkbox" checked={runConsent()} onChange={(event) => setRunConsent(event.currentTarget.checked)} disabled={!state()?.ready || state()?.phase === "running"} />{tr("确认上述实际 Agent、任务和目标；开始后供应商账户计费。检查与执行共享本站账号最多 8 次调用、131072 token；每次执行输出最多 512 token，本轮 120 秒。", "Confirm the actual Agent, Task and goal above. Starting charges your provider account. Checks and execution share an account limit of 8 calls and 131072 reserved tokens. Each execution call allows 512 output tokens; this run lasts 120 seconds.")}</label>
        <div class="model-connection-panel__actions">
          <button type="button" disabled={!runConsent() || !state()?.ready || !modelReady() || state()?.phase === "running"} onClick={() => void act((session) => session.start())}>{tr("确认开始 Agent", "Confirm Agent start")}</button>
          <button type="button" onClick={() => driver()?.stop()}>{tr("停止新决策", "Stop new decisions")}</button>
          <button type="button" disabled={state()?.phase === "running"} onClick={() => void act((session) => session.recover())}>{tr("只读核对原件", "Look up original operations")}</button>
          <button type="button" onClick={() => void act((session) => session.forget())}>{tr("忘记本机 Key", "Forget local key")}</button>
        </div>
        <p role="status">{tr("运行状态", "Execution state")}: {state()?.phase || "—"}</p>
      </Show>
    </Show>
    <Show when={error()}><p role="alert">{error()}</p></Show>
    <ModelConnectionPanel locale={props.locale} agentId={props.agentId} canControl={false} managementOnly={true} />
  </section>;
}
