import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { BrowserModelConnectionPanel } from "./browser_model_connection_panel.jsx";
import { createBrowserGameSession } from "./browser_game_session.js";
import { ModelConnectionPanel } from "./model_connection_panel.jsx";
import { createBrowserOwnerEntry } from "./browser_owner_entry.js";
import { loadBrowserGameProtocol } from "./browser_game_protocol.js";

export function BrowserByokSurface(props) {
  const read = (value) => typeof value === "function" ? value() : value;
  const tr = (cn, en) => String(read(props.locale) || "en").startsWith("zh") ? cn : en;
  const [driver, setDriver] = createSignal(null), [state, setState] = createSignal(null);
  const [error, setError] = createSignal(""), [confirm, setConfirm] = createSignal(false), [runConsent, setRunConsent] = createSignal(false);
  const [entry, setEntry] = createSignal(null), [entryState, setEntryState] = createSignal(null);
  const [entryBusy, setEntryBusy] = createSignal(false), [goal, setGoal] = createSignal("");
  const [entryConsent, setEntryConsent] = createSignal(false), [projectionPending, setProjectionPending] = createSignal(false);
  let contextKey, entryKey, entryIdentityKey, entryController, previousAccount, unsubscribe, alive = true, entryGeneration = 0;
  const config = () => read(props.config);
  const goalBytes = () => new TextEncoder().encode(goal().trim()).byteLength;
  const entryLabel = () => ({ needs_participant: tr("账号绑定待核对", "Account binding needs verification"), needs_task: tr("需要创建任务", "Task needs preparation"), needs_goal: tr("需要发布目标", "Goal needs publication"), ready: tr("账号任务已就绪", "Account task ready"), pending: tr("原件结果待确认", "Original result pending"), owner_mismatch: tr("玩家身份与原绑定不符", "Owner identity mismatch"), stopped: tr("入口准备已停止，请先只读核对原件", "Entry preparation stopped; inspect originals before continuing") })[entryState()?.state] || "—";
  const reason = (code) => {
    if (["participant_registration_rejected", "owner_operation_rejected"].includes(code)) return tr("世界拒绝了该登记或任务原件。请核对原账号玩家身份和任务权限；不会切换身份或替换原 Agent。", "The world rejected this registration or task original. Check the account's original owner identity and task authority; no identity or Agent is replaced.");
    if (code === "owner_goal_invalid") return tr("请输入不超过 8192 UTF-8 字节的任务目标。", "Enter a task goal of at most 8192 UTF-8 bytes.");
    if (["owner_mismatch", "participant_owner_mismatch"].includes(code)) return tr("本机玩家身份与账号原绑定不符。已停止；请恢复原身份后重试，不会替换原 Agent。", "This browser owner identity differs from the account binding. Restore the original identity; the existing Agent is not replaced.");
    if (code === "executor_takeover_confirmation_required") return tr("当前已有其他执行方。核对任务后，勾选授权以明确切换到此浏览器。", "Another executor is bound. Review the task and explicitly confirm switching to this browser.");
    if (["operation_unknown", "operation_pending", "recovery_required", "participant_binding_required", "owner_operation_pending", "owner_response_unknown", "participant_registration_pending", "participant_response_unknown", "owner_original_conflict", "participant_original_conflict"].includes(code)) return tr("原件或绑定尚未确认。请只读核对；未知结果不能重发，也不能据最新快照判断未发生。", "An original operation or binding is unconfirmed. Use read-only lookup; do not resubmit or infer non-execution from a recent snapshot.");
    if (["task_fence_changed", "game_session_changed", "executor_not_bound", "task_unavailable"].includes(code)) return tr("任务、目标或执行权限已改变。已停止；重新核对实际任务与执行身份后再确认。", "Task, goal or executor authority changed. Stopped; review the actual task and executor again before confirming.");
    if (["page_paused", "stopped", "participant_stopped"].includes(code)) return tr("本页已暂停新决策。返回后先核对原件，再明确开始；已受理世界作业仍可能推进。", "New decisions are paused. Check originals before explicitly starting again; accepted world jobs may continue.");
    if (code === "legacy_migration_required") return tr("服务端尚未确认旧托管执行已关闭。请部署方完成迁移后再继续；旧原件结果未知仍不代表未发生。", "The server has not confirmed that legacy hosted execution is closed. Ask the deployment operator to complete migration; an unknown original result still does not mean it never happened.");
    if (["legacy_run_active", "legacy_state_unknown"].includes(code)) return tr("先在下方核对并停止旧执行端运行；状态未知时不能开始浏览器运行。", "Review and stop the previous executor below. Browser execution cannot start while its state is unknown.");
    if (code === "agent_busy") return tr("同一 Agent 已在另一个页面运行。本页不会自动接管。", "This Agent is running in another page. This page will not take over automatically.");
    if (String(code).includes("budget") || code === "run_time_limit") return tr("本轮预算或期限已到，不自动扩大或重置。", "This run's budget or deadline is exhausted; it is not automatically increased or reset.");
    return tr("游戏客户端操作未完成。请检查本机存储、登录、合法任务及当前部署支持；原件未知仍保持未知。", "The Game Client operation did not complete. Check browser storage, sign-in, the authorized task and deployment support; unknown originals stay unknown.");
  };
  function refresh() {
    const value = driver()?.snapshot(); setState(value || null);
    if (value?.code) setError(reason(value.code));
  }
  const ownerKey = (owner) => owner?.identityId || owner?.publicKey || "";
  const trustKey = (trusted) => JSON.stringify(trusted || null);
  const installedSigner = (owner, captured) => ({ publicKeyHex: owner.publicKey,
    sign: async (bytes) => { if (!alive || captured !== entryKey) throw new Error("game_session_changed"); const signature = await owner.sign(bytes); if (!alive || captured !== entryKey) throw new Error("game_session_changed"); return signature; } });
  const acceptEntry = (value, captured) => {
    if (!alive || captured !== entryKey) return;
    setEntryState(value);
    if (value?.state === "ready" && value.agentId) setProjectionPending(props.onCanonicalAgent?.(value.agentId) === false);
  };
  function openEntry({ accountId, owner, trusted }, resetGoal = false) {
    entryKey = `${entryIdentityKey}:${++entryGeneration}`;
    entryController?.abort(); entry()?.dispose?.(); setEntry(null); setEntryState(null);
    setEntryConsent(false); if (resetGoal) setGoal(""); setProjectionPending(false); setError("");
    if (!accountId || !owner || !trusted) { setEntryBusy(false); return; }
    const captured = entryKey, controller = new AbortController(); entryController = controller; setEntryBusy(true);
    void (async () => {
      try {
        const codec = await (props.protocolLoader || loadBrowserGameProtocol)();
        if (!alive || captured !== entryKey) return;
        const ownerEntry = (props.createOwnerEntry || createBrowserOwnerEntry)({ config: trusted, codec, accountId,
          ownerSigner: installedSigner(owner, captured), signal: controller.signal, isCurrent: () => alive && captured === entryKey });
        setEntry(ownerEntry);
        acceptEntry(await ownerEntry.inspect(), captured);
      } catch (failure) { if (alive && captured === entryKey) setError(reason(failure.code)); }
      finally { if (alive && captured === entryKey) setEntryBusy(false); }
    })();
  }
  function stopEntry() {
    entryKey = `stopped:${++entryGeneration}`;
    entryController?.abort(); entry()?.dispose?.(); setEntry(null); setEntryBusy(false);
    setEntryConsent(false); setConfirm(false); setRunConsent(false); setProjectionPending(false);
    driver()?.stop(); setEntryState({ state: "stopped" });
  }
  const resumeEntryReadOnly = () => openEntry({ accountId: read(props.accountId), owner: read(props.ownerSigner), trusted: config() });
  createEffect(() => {
    const accountId = read(props.accountId), owner = read(props.ownerSigner), trusted = config();
    const key = `${accountId || ""}:${ownerKey(owner)}:${trustKey(trusted)}`;
    if (key === entryIdentityKey) return;
    entryIdentityKey = key; openEntry({ accountId, owner, trusted }, true);
  });
  createEffect(() => {
    const accountId = read(props.accountId), selectedAgent = read(props.agentId), owner = read(props.ownerSigner), trusted = config();
    const canonicalAgent = entryState()?.state === "ready" ? entryState().agentId : null;
    // A Viewer target change invalidates the prior executor confirmation even
    // while the signed canonical participant remains this account's target.
    const key = `${entryKey}:${canonicalAgent || ""}:${selectedAgent || ""}`;
    if (key === contextKey) return;
    contextKey = key; unsubscribe?.(); setConfirm(false); setRunConsent(false);
    const old = driver();
    if (old) {
      if (previousAccount !== accountId) { const loggedOutContext = key; void old.logout().catch(() => { if (alive && contextKey === loggedOutContext) setError(reason("storage_write_failed")); }); }
      old.dispose();
    }
    previousAccount = accountId; setDriver(null); setState(null);
    if (!accountId || !canonicalAgent || !owner || !trusted) return;
    try {
      const captured = contextKey, capturedEntry = entryKey;
      const signer = installedSigner(owner, capturedEntry);
      const session = (props.createSession || createBrowserGameSession)({ config: trusted, accountId, agentId: canonicalAgent, ownerSigner: signer, isCurrent: () => alive && captured === contextKey && capturedEntry === entryKey });
      setDriver(session); unsubscribe = session.subscribe(refresh); refresh();
    } catch { setError(reason("game_configuration_invalid")); }
  });
  const prepareEntry = async () => {
    if (!entryConsent() || entryBusy() || !entry() || goalBytes() > 8192) return;
    const captured = entryKey, ownerEntry = entry(); setEntryBusy(true); setEntryConsent(false); setError("");
    try { acceptEntry(await ownerEntry.prepare({ goal: goal().trim(), consent: true }), captured); }
    catch (failure) { if (alive && captured === entryKey) setError(reason(failure.code)); }
    finally { if (alive && captured === entryKey) setEntryBusy(false); }
  };
  const modelReady = () => { state(); return driver()?.modelSession.snapshot().checked === true; };
  const act = async (action) => {
    const captured = contextKey, current = driver(); setError(""); setRunConsent(false);
    try { await action(current); } catch (failure) { if (alive && captured === contextKey) setError(reason(failure.code)); }
    finally { if (alive && captured === contextKey) refresh(); }
  };
  onCleanup(() => { alive = false; contextKey = null; entryKey = null; entryController?.abort(); entry()?.dispose?.(); unsubscribe?.(); driver()?.dispose(); });
  return <section class="stack" style="overflow-wrap:anywhere;min-width:0" aria-label={tr("浏览器自带模型 Agent", "Browser BYOK Agent")}>
    <Show when={config()} fallback={<p role="status">{tr("此部署尚未启用浏览器 Game Client。不会收集本地模型 Key。", "This deployment has not enabled the browser Game Client. Model keys are not collected.")}</p>}>
      <Show when={read(props.accountId) && read(props.ownerSigner)} fallback={<p role="status">{tr("先登录并恢复本机玩家身份，才能准备游戏任务。", "Sign in and restore your browser owner identity before preparing a game task.")}</p>}>
        <p>{tr("先准备本账号的合法游戏入口，再设置模型 Key。首次登记才创建玩家和 Agent；已有绑定由世界核对并复用，身份不符时停止。不会自动登记执行方或启动模型。", "Prepare this account's authorized game entry before adding a model key. First registration creates the player and Agent; the world reuses an existing binding and rejects a different owner. This does not register an executor or start a model.")}</p>
        <p role="status">{tr("账号入口状态", "Account entry state")}: {entryBusy() ? tr("只读核对或处理原件中", "Checking entry or original operation") : entryLabel()}</p>
        <button type="button" onClick={stopEntry}>{tr("停止入口准备", "Stop entry preparation")}</button>
        <Show when={entryState()?.state === "stopped"}><p role="status">{tr("已取消后续入口写入。已经发送的原件仍可能生效；重新打开后仅只读核对，不自动重发或继续准备。", "Further entry writes are cancelled. Already submitted originals may still commit. Reopen with read-only inspection; preparation is never automatically resumed or resubmitted.")}</p><button type="button" onClick={resumeEntryReadOnly}>{tr("重新打开并只读核对入口原件", "Reopen and inspect entry originals")}</button></Show>
        <Show when={entryState()?.agentId}><p>{tr("实际绑定 Agent", "Canonical Agent")}: {entryState()?.agentId} · Task={entryState()?.taskId || "—"}</p></Show>
        <Show when={projectionPending()}><p role="status">{tr("世界已确认实际 Agent，Viewer 投影仍待同步；以下操作使用上方正式绑定。", "The world confirmed the canonical Agent; the Viewer projection is still syncing. Operations below use the confirmed binding above.")}</p></Show>
        <Show when={entry() && entryState()?.state !== "ready"}>
          <label>{tr("任务目标", "Task goal")}<textarea aria-label={tr("任务目标", "Task goal")} value={goal()} maxlength="8192" disabled={entryBusy()} onInput={(event) => { setGoal(event.currentTarget.value); setEntryConsent(false); }} /></label>
          <p role="status">{tr("目标按 UTF-8 字节计量", "Goal size in UTF-8 bytes")}: {goalBytes()} / 8192</p>
          <label><input type="checkbox" checked={entryConsent()} disabled={entryBusy() || ["owner_mismatch", "pending"].includes(entryState()?.state)} onChange={(event) => setEntryConsent(event.currentTarget.checked)} />{tr("确认使用当前本机玩家身份登记或核对本账号原绑定，仅为缺失任务或目标使用上述目标；已有任务目标保持原状，原绑定不符时停止，不替换 Agent。", "Confirm registration or verification of this account's binding with the current browser owner identity and use this goal only for a missing Task or goal. Preserve an existing task goal. Stop on an owner mismatch; never replace the Agent.")}</label>
          <button type="button" disabled={entryBusy() || !entryConsent() || !goal().trim() || goalBytes() > 8192 || ["owner_mismatch", "pending"].includes(entryState()?.state)} onClick={() => void prepareEntry()}>{tr("准备玩家、Agent 与任务", "Prepare player, Agent and Task")}</button>
          <button type="button" disabled={entryBusy()} onClick={() => { const captured = entryKey; setEntryBusy(true); void entry().inspect().then((value) => acceptEntry(value, captured)).catch((failure) => { if (alive && captured === entryKey) setError(reason(failure.code)); }).finally(() => { if (alive && captured === entryKey) setEntryBusy(false); }); }}>{tr("只读核对账号入口原件", "Look up account entry originals")}</button>
        </Show>
      </Show>
      <Show when={driver()}>
        <p>{tr("实际执行入口：当前浏览器。模型 Key 不授予游戏权限；下面的执行身份登记仅复用上方已确认的玩家/Agent/Task，不创建新的 Agent。", "Execution location: this browser. A model key grants no game authority. Executor registration below reuses the confirmed player, Agent and Task above; it creates no Agent.")}</p>
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
