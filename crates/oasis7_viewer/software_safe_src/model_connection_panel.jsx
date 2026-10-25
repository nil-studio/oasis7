import { modelConnectionStyles } from "./model_connection_styles.js";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { acceptsModelCredentials, createViewerModelConnectionClient } from "./viewer_model_connection_client.js";
const COPY = {
 executor_busy: ["同一执行设备正在处理原运行，请先核对状态。", "This device is handling an existing run. Check its status first."],
 deployment_unavailable: ["当前部署尚未启用模型连接，暂不接收凭证。", "Model connections are not enabled on this deployment; credentials are not accepted."],
 authentication_required: ["请先登录当前玩家账号。", "Sign in to the current player account."],
 authentication_failed: ["模型凭证认证失败，请重新输入或到供应商检查。", "Model authentication failed. Re-enter the key or check the provider account."],
 quota_unavailable: ["供应商额度不可用，请到供应商账户处理。", "Provider quota is unavailable. Check the provider account."],
 budget_exhausted: ["本次模型调用限制已用尽，新推理已受限。", "This session's model budget is exhausted; new inference is blocked."],
 rate_limited: ["供应商限流；仅可在原期限和预算内重试。", "Provider rate limit reached. Retry only within the original deadline and budget."],
 model_unavailable: ["当前模型不可用，请选择已支持配置。", "This model is unavailable. Choose a supported configuration."],
 capability_unsupported: ["模型工具能力尚未就绪，不能开始游戏执行。", "Model tool support is not ready; game execution cannot start."],
 transport_failed: ["请求结果尚未确认。请刷新状态；不要假定检查或停止已经完成。", "The request result is unconfirmed. Refresh status; do not assume probing or stopping completed."],
 session_expired: ["连接会话已过期，请重新连接并确认开始。", "The connection expired. Reconnect and explicitly confirm any new run."],
 version_conflict: ["连接已被其他页面修改，请核对刷新后的状态再操作。", "Another page changed this connection. Review the refreshed state before continuing."],
 executor_unavailable: ["本部署尚无可用游戏执行器。可检查模型连接，但不能开始行动。", "No game executor is available on this deployment. Model checks are available; actions cannot start."],
 untrusted_executor: ["尚未建立可信执行端，不能提交凭证。", "A trusted executor has not been established. Credentials cannot be submitted."],
 invalid_request: ["连接请求无效，请刷新并重新确认。", "The connection request is invalid. Refresh and confirm again."],
 connection_capacity_exceeded: ["连接存储容量已满；已有连接仍可管理，本次没有调用模型。", "Connection storage capacity is full. Existing connections remain manageable; no model call was made."],
 list_changed: ["连接列表在读取时发生变化，请重新刷新；已知连接仍可核对和管理。", "The connection list changed while reading. Refresh again; known connections can still be checked and managed."],
 invalid_response: ["模型响应格式无效，请核对协议兼容性。", "The model response format is invalid. Check protocol compatibility."],
 response_limit: ["模型响应超过安全大小上限，请核对输出配置。", "The model response exceeded the safe size limit. Check output settings."],
 request_failed: ["连接服务暂不可用，请刷新状态或稍后重试。", "The connection service is unavailable. Refresh status or try again later."],
};
const RUN_REASON = {
 completed: ["本轮执行上限已达到；尚未证明目标完成", "Run limit reached; goal completion is unverified"],
 budget_exhausted: ["调用预算已用尽", "Call budget exhausted"],
 pending_original: ["原操作结果待确认", "Original operation result pending"],
 rejected_original: ["原操作已拒绝", "Original operation rejected"],
 expired_original: ["原操作已过期", "Original operation expired"],
 control_changed: ["授权或运行状态已改变", "Authority or run state changed"],
 context_expired: ["决策上下文已过期", "Decision context expired"],
 model_failed: ["模型调用未完成", "Model call did not complete"],
 input_limit: ["模型输入达到上限", "Model input reached its limit"],
 model_timeout: ["模型调用超时", "Model call timed out"],
 invalid_model_output: ["模型工具输出不受支持", "Unsupported model tool output"],
 aborted: ["执行器已结束本轮", "Executor ended this run"],
 host_failure: ["执行端暂不可用", "Executor unavailable"],
};
const MODEL_FAILURE = {
 input_limit: ["模型输入达到上限。请核对上下文容量与原任务状态，再明确确认后续执行。", "Model input reached its limit. Review context capacity and the original task state before explicitly confirming further execution."],
 output_limit: ["模型输出达到上限。截断输出不能作为可执行动作；请核对原操作状态和输出配置后再明确确认。", "Model output reached its limit. Truncated output is not an executable action; review original-operation status and output settings before explicitly confirming further execution."],
 model_identity_mismatch: ["返回模型与所选配置不符。请核对供应商模型配置。", "The returned model differs from the selected configuration. Check the provider model settings."],
 unexpected_streaming: ["供应商返回了不支持的流式响应。请核对兼容协议配置。", "The provider returned an unsupported streaming response. Check protocol compatibility settings."],
 unsupported_finish_reason: ["模型结束方式不受支持。请核对模型协议兼容性。", "The model completion type is unsupported. Check model protocol compatibility."],
 unsupported_tool: ["模型请求了不支持的工具。请核对可用游戏工具与模型配置。", "The model requested an unsupported tool. Review available game tools and model settings."],
 invalid_tool_identity: ["模型工具调用标识无效。请核对协议兼容性与原操作状态。", "The model tool-call identity is invalid. Check protocol compatibility and original-operation status."],
 unsupported_content: ["模型返回内容格式不受支持。请核对模型协议兼容性。", "The model returned an unsupported content format. Check model protocol compatibility."],
 capability_unsupported: ["模型能力不满足执行要求。请核对工具协议与模型配置。", "Model capabilities do not meet execution requirements. Check tool protocol and model settings."],
 other: ["模型调用失败。请核对连接与原操作状态。", "The model call failed. Check the connection and original-operation status."],
};
const STATE = {
 checking: ["检查中", "Checking"], ready: ["模型可用", "Model ready"],
 blocked: ["模型连接受限", "Model connection blocked"], revoked: ["已断开并忘记", "Disconnected and forgotten"], expired: ["会话已过期", "Session expired"],
 running: ["执行器正在运行", "Executor running"], stopped: ["已停止新决策", "New decisions stopped"],
 idle: ["尚未开始执行", "Execution not started"], unavailable: ["游戏执行器不可用", "Game executor unavailable"],
};
export function ModelConnectionPanel(props) {
 const client = props.client || createViewerModelConnectionClient();
 const read = (value) => typeof value === "function" ? value() : value;
 const zh = () => String(read(props.locale) || "en").startsWith("zh");
 const tr = (cn, en) => zh() ? cn : en;
 const message = (code) => (COPY[code === "game_executor_unavailable" ? "executor_unavailable" : code] || MODEL_FAILURE[code] || COPY.request_failed)[zh() ? 0 : 1];
 const modelFailure = (connection) => {
  const execution = connection.execution;
  if (["revoked", "expired"].includes(connection.state) || execution?.phase !== "finished" || !["model_failed", "input_limit"].includes(execution.reason)) return null;
  return MODEL_FAILURE[execution.model_failure]?.[zh() ? 0 : 1] || null;
 };
 const stateLabel = (state) => (STATE[state] || ["状态未知", "Status unknown"])[zh() ? 0 : 1];
 const [capabilities, setCapabilities] = createSignal(null);
 const [connections, setConnections] = createSignal([]);
 const [preset, setPreset] = createSignal("");
 const [selectedId, setSelectedId] = createSignal("");
 const [consent, setConsent] = createSignal(false);
 const [probeConsent, setProbeConsent] = createSignal(false);
 const [runConsent, setRunConsent] = createSignal(false);
 const [busy, setBusy] = createSignal(false);
 const [error, setError] = createSignal("");
 const [now, setNow] = createSignal(Date.now());
 let keyInput, previousAgent, disposed = false, refreshGeneration = 0, renewing = false;
 const managementOnly = () => read(props.managementOnly) === true;
 const manageable = (cap) => managementOnly() ? cap?.ok === true && cap.enabled === true && cap.authenticated === true : acceptsModelCredentials(cap);
 const available = () => manageable(capabilities());
 const agentId = () => read(props.agentId) || "";
 const canControl = () => read(props.canControl) === true && !!agentId();
 const selected = () => connections().find((connection) => connection.id === selectedId());
 const expired = (connection) => !connection || ["expired", "revoked"].includes(connection.state) || !connection.expires_at_ms || !connection.absolute_expires_at_ms || now() >= Math.min(connection.expires_at_ms, connection.absolute_expires_at_ms);
 const apply = (connection) => {
  if (disposed) return;
  refreshGeneration++;
  setConnections((previous) => { const current = previous.find((value) => value.id === connection.id); if (current && current.version > connection.version) return previous; return current ? previous.map((value) => value.id === connection.id ? connection : value) : [...previous, connection]; });
 };
 function discardUnavailable(connection, failure) {
  if (failure.status !== 404 && failure.status !== 401 && !["authentication_required", "authentication_failed"].includes(failure.code)) return;
  refreshGeneration++;
  setConnections(previous => previous.filter(value => value.id !== connection.id));
  if (selectedId() === connection.id) setSelectedId("");
  if (failure.status === 401 || ["authentication_required", "authentication_failed"].includes(failure.code)) setCapabilities(null);
 }
 async function refresh() {
  const generation = ++refreshGeneration;
  let capabilityRead = false;
  try {
   const cap = await client.capabilities();
   if (disposed || generation !== refreshGeneration) return;
   setCapabilities(cap);
   capabilityRead = true;
   if (!manageable(cap)) { setConnections([]); if (keyInput) keyInput.value = ""; return; }
   const items = await client.list();
   if (disposed || generation !== refreshGeneration) return;
   setConnections(items);
   if (!items.some((item) => item.id === selectedId())) setSelectedId(items[0]?.id || "");
  } catch (failure) {
   if (!disposed && generation === refreshGeneration) {
    setError(message(failure.code));
    if (!capabilityRead) setCapabilities(null);
    if (keyInput) keyInput.value = "";
    // A failed history page must not hide a known live connection. Read its
    // current authority/version independently without claiming list completeness.
    const current = selected();
    if (capabilityRead && current) {
     try { const value = await client.read(current); if (!disposed && generation === refreshGeneration) apply(value); }
     catch (readFailure) { if (!disposed && generation === refreshGeneration) discardUnavailable(current, readFailure); }
    }
   }
  }
 }
 async function act(action) {
  if (busy()) return;
  setBusy(true); setError("");
  try { await action(); } catch (failure) { if (!disposed) { setError(message(failure.code)); setProbeConsent(false); setRunConsent(false); await refresh(); } }
  finally { if (!disposed) setBusy(false); }
 }
 function create(event) {
  event.preventDefault();
  if (managementOnly() || busy() || !available() || !canControl() || !preset() || !consent()) return;
  let raw = keyInput.value;
  keyInput.value = "";
  void act(async () => { try { const connection = await client.create({ preset_id: preset(), raw_key: raw, consent: true }); apply(connection); setSelectedId(connection.id); setConsent(false); setProbeConsent(false); setRunConsent(false); } finally { raw = ""; } });
 }
 onMount(() => {
  void refresh();
  const clock = setInterval(() => setNow(Date.now()), 1000);
  const renew = setInterval(async () => {
   setNow(Date.now());
   if (managementOnly() || disposed || renewing || busy() || !available() || document.visibilityState !== "visible") return;
   renewing = true;
   try {
    for (const connection of connections()) {
     if (expired(connection)) continue;
     const interval = capabilities()?.limits?.renew_interval_ms;
     if (!Number.isFinite(interval) || interval < 1000) continue;
     // Server deadlines remain authoritative; an expired local view never renews.
     if (connection.expires_at_ms - Date.now() > capabilities().limits.lease_ms - interval) continue;
     const leaseGeneration = refreshGeneration;
     try {
      const current = await client.lease(connection);
      if (!disposed && leaseGeneration === refreshGeneration) apply(current);
     } catch (failure) {
      if (disposed || leaseGeneration !== refreshGeneration) continue;
      setError(message(failure.code));
      // Read the same connection after a lost reply or a completed worker's CAS
      // change. Reconciliation never activates a run or changes its budget.
      const generation = refreshGeneration;
      try {
       const current = await client.read(connection);
       if (!disposed && generation === refreshGeneration) apply(current);
      } catch (readFailure) { if (!disposed && generation === refreshGeneration) { setError(message(readFailure.code)); discardUnavailable(connection, readFailure); } }
     }
    }
   } finally { renewing = false; }
  }, 1000);
  const visible = () => { setNow(Date.now()); if (document.visibilityState === "visible") void refresh(); };
  document.addEventListener("visibilitychange", visible);
  onCleanup(() => { disposed = true; refreshGeneration++; clearInterval(clock); clearInterval(renew); document.removeEventListener("visibilitychange", visible); if (keyInput) keyInput.value = ""; });
 });
 createEffect(() => { selectedId(); setProbeConsent(false); setRunConsent(false); });
 createEffect(() => {
  const id = agentId();
  if (id !== previousAgent || !canControl() || !available()) {
   if (keyInput) keyInput.value = "";
   setConsent(false); setProbeConsent(false); setRunConsent(false);
  }
  previousAgent = id;
 });
 return <details class="model-connection-panel" data-testid="model-connection-panel">
  <style>{modelConnectionStyles}</style>
  <summary>{managementOnly() ? tr("旧托管模型连接管理", "Legacy hosted connection management") : tr("实验模型连接 · 第一方执行方", "Experimental model connection · first-party executor")}</summary>
  <p>{tr("连接模型不会认领 Agent、授予游戏权限或自动开始运行。", "Connecting a model does not claim an Agent, grant game authority, or start a run.")}</p>
  <Show when={error()}><p role="alert">{error()}</p></Show>
  <Show when={available()} fallback={<p role="status">{capabilities() ? message(capabilities().reason || (capabilities().authenticated ? "untrusted_executor" : "authentication_required")) : tr("正在核对部署与登录状态；暂不接收凭证。", "Checking deployment and sign-in; credentials are not accepted yet.")}</p>}>
   <Show when={managementOnly()}><p>{tr("仅核对、停止或忘记旧托管连接；不会检查模型、开始或续期。开始浏览器运行前必须确认旧执行已停止。", "Check, stop or forget legacy connections only. No model checks, starts or lease renewals occur. Confirm legacy execution has stopped before starting in the browser.")}</p></Show>
   <Show when={!managementOnly()}>
   <div class="model-connection-panel__boundary">
    <p>{tr("凭证接收方", "Credential recipient")}: {capabilities()?.trusted_executor?.display_name}</p>
    <p>{tr("执行位置", "Execution location")}: {capabilities()?.trusted_executor?.location} · {capabilities()?.trusted_executor?.origin}</p>
    <p>{tr("模型数据将发送至所选 DeepSeek/OpenRouter 预设。供应商账户由你付费；费用未知不等于零，本应用不能限制 Key 在其他应用的支出。", "Model data goes to the selected DeepSeek/OpenRouter preset. Your provider account pays; unknown cost is not zero, and this app cannot cap spending by other apps.")}</p>
    <p>{tr("Key 仅在本次执行端会话内存中使用。关闭网页后续期可能失效，不提供长期托管；已受理世界作业仍按世界规则推进。", "The key is held in executor session memory only. Closing the page may end renewal; long-term hosting is not provided. Accepted world jobs follow world rules.")}</p>
    <p>{tr("会话有效期 / 绝对上限（秒）", "Session lease / absolute limit (seconds)")}: {Math.floor((capabilities()?.limits?.lease_ms || 0) / 1000)} / {Math.floor((capabilities()?.limits?.absolute_ms || 0) / 1000)}</p>
    <p>{tr("调用 / token / 并发上限", "Call / token / concurrency limits")}: {capabilities()?.limits?.max_calls ?? "—"} / {capabilities()?.limits?.max_tokens ?? "—"} / {capabilities()?.limits?.max_concurrent ?? "—"}</p>
   </div>
   <Show when={canControl()} fallback={<p role="status">{tr("先选中你有权控制的 Agent；连接不能补授权限。", "Select an Agent you may control; a model connection cannot grant authority.")}</p>}>
    <form onSubmit={create} class="model-connection-panel__form">
     <label>{tr("供应商与模型", "Provider and model")}<select required value={preset()} onChange={(event) => setPreset(event.currentTarget.value)} disabled={busy()}><option value="">{tr("请选择本部署配置", "Choose a configured preset")}</option><For each={capabilities()?.presets}>{(item) => <option value={item.id}>{item.provider_id === "deepseek" ? "DeepSeek" : "OpenRouter"} · {item.model_id} · {item.protocol === "chat_completions" ? tr("工具对话", "Chat tools") : "Responses"}</option>}</For></select></label>
     <label>{tr("推理 API Key", "Inference API key")}<input ref={keyInput} type="password" name="model-inference-key" autocomplete="off" spellcheck={false} maxlength="4096" required disabled={busy()} /></label>
     <label class="model-connection-panel__check"><input type="checkbox" checked={consent()} onChange={(event) => setConsent(event.currentTarget.checked)} />{tr("我知晓凭证接收方、会话隐私和供应商账户费用，同意本次连接使用。", "I understand the recipient, session privacy and provider charges, and consent to this connection.")}</label>
     <button type="submit" disabled={busy() || !consent() || !preset()}>{tr("保存本次连接", "Create session connection")}</button>
    </form>
   </Show>
   </Show>
   <Show when={connections().length}><label>{tr("本会话连接", "Session connections")}<select value={selectedId()} onChange={(event) => setSelectedId(event.currentTarget.value)} disabled={busy()}><For each={connections()}>{(item) => <option value={item.id}>{capabilities()?.presets?.find((value) => value.id === item.preset_id)?.model_id || tr("模型配置", "Model configuration")} · {stateLabel(item.state)}</option>}</For></select></label></Show>
   <Show when={selected()}>{(current) => <div class="model-connection-panel__connection">
    <p role="status">{current().state === "revoked" ? stateLabel("revoked") : expired(current()) ? stateLabel("expired") : stateLabel(current().state)} · {stateLabel(current().run_state)}</p>
    <Show when={current().execution}>{(execution) => <p role="status">{execution().phase === "running" ? tr("本轮正在执行", "Run in progress") : (RUN_REASON[execution().reason] || ["执行已停止，结果需核对", "Execution stopped; verify the result"])[zh() ? 0 : 1]} · {tr("已确认效果", "Confirmed effects")}: {execution().successful_effects ?? "—"}</p>}</Show>
    <Show when={modelFailure(current())}>{(diagnostic) => <p role="status">{diagnostic()} {tr("不会自动重试或重置预算；后续执行仍受原预算和期限限制。", "There is no automatic retry or budget reset; further execution remains subject to the original budget and deadline.")}</p>}</Show>
    <p>{tr("已占用调用预算 / token", "Call budget used / tokens reserved")}: {current().calls_spent ?? "—"} / {current().tokens_reserved ?? "—"}</p>
    <Show when={current().error_code}><p role="status">{message(current().error_code)}</p></Show>
    <Show when={!managementOnly() && !capabilities()?.executor_available}><p role="status">{message("executor_unavailable")}</p></Show>
    <Show when={!managementOnly()}>
    <label class="model-connection-panel__check"><input type="checkbox" checked={probeConsent()} onChange={(event) => setProbeConsent(event.currentTarget.checked)} disabled={busy() || expired(current())} />{tr("同意一次有界、可能计费的模型检查；不写游戏世界。", "Consent to a bounded, potentially billable model check; it does not write the world.")}</label>
    <button type="button" disabled={busy() || expired(current()) || !probeConsent()} onClick={() => void act(async () => { apply(await client.probe(current())); setProbeConsent(false); setRunConsent(false); })}>{tr("测试连接", "Test connection")}</button>
    <label class="model-connection-panel__check"><input type="checkbox" checked={runConsent()} onChange={(event) => setRunConsent(event.currentTarget.checked)} disabled={busy() || expired(current()) || !capabilities()?.executor_available} />{tr("确认当前 Agent、目标与上述运行限制，开始后供应商账户继续计费。", "Confirm this Agent, goal and limits; starting permits further provider charges.")}</label>
    </Show>
    <div class="model-connection-panel__actions">
     <Show when={!managementOnly()}>
     <button type="button" disabled={busy() || expired(current()) || current().state !== "ready" || !runConsent() || !canControl() || !capabilities()?.executor_available} onClick={() => void act(async () => { apply(await client.activate(current(), { target_ref: { agent_id: agentId() }, limits: capabilities().limits })); setRunConsent(false); })}>{tr("确认开始", "Confirm start")}</button>
     </Show>
     <button type="button" disabled={busy() || (!managementOnly() && expired(current())) || current().state === "revoked"} onClick={() => void act(async () => { apply(await client.stop(current())); setRunConsent(false); })}>{tr("停止新决策", "Stop new decisions")}</button>
     <button type="button" disabled={busy() || current().state === "revoked"} onClick={() => void act(async () => { apply(await client.forget(current())); setProbeConsent(false); setRunConsent(false); })}>{tr("断开并忘记", "Disconnect and forget")}</button>
    </div>
    <p>{tr("停止保留本次有效连接；忘记会停用凭证。两者都不撤销供应商 Key、不保证已发调用退款，也不取消已提交世界作业。未知结果须按原操作核对。", "Stop retains the valid session connection; forget disables its credential. Neither revokes the provider key, refunds sent calls, nor cancels submitted world jobs. Unknown outcomes require original-operation lookup.")}</p>
   </div>}</Show>
  </Show>
  <button type="button" disabled={busy()} onClick={() => void refresh()}>{tr("刷新连接状态", "Refresh connection status")}</button>
 </details>;
}
