import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { createBrowserModelSession } from "./browser_model_session.js";
import { modelConnectionStyles } from "./model_connection_styles.js";

export function BrowserModelConnectionPanel(props) {
  const read = (value) => typeof value === "function" ? value() : value;
  const zh = () => String(read(props.locale) || "en").startsWith("zh");
  const tr = (cn, en) => zh() ? cn : en;
  const [snapshot, setSnapshot] = createSignal(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [consent, setConsent] = createSignal(false);
  const [probeConsent, setProbeConsent] = createSignal(false);
  let session, previousAccount, unsubscribe, input, alive = true, context = 0;
  // Readiness must come from the compiled Game Client integration, never URL,
  // world metadata, model output or a page-global injected executor.
  const ready = () => Boolean(read(props.accountId)) && read(props.executionReady) === true;
  const notice = (code) => {
    if (code === "budget_exhausted") return tr("检查子预算或共享总预算已用尽，未发出新调用。", "The check allowance or shared total budget is exhausted; no new call was sent.");
    if (code === "authentication_failed") return tr("供应商认证失败，请检查 Key。", "Provider authentication failed. Check your key.");
    if (code === "revision_conflict" || code === "stopped" || code === "cancelled") return tr("配置或页面状态已改变；本次检查已停止。", "Configuration or page state changed; this check has stopped.");
    if (String(code).startsWith("storage_") || code === "locks_unavailable") return tr("本机存储或页面互斥不可用。已停止；若忘记失败，请清除本站数据。", "Local storage or page locking is unavailable. Stopped; clear site data if forgetting failed.");
    return tr("检查未完成。网络、跨域或模型协议可能不可用；用量未知时仍保留预算。", "The check did not complete. Network, CORS or protocol may be unavailable; unknown usage retains its budget.");
  };
  function refresh() {
    try { setSnapshot(session?.snapshot() || null); }
    catch (failure) { setSnapshot(null); setError(notice(failure.code)); }
  }
  createEffect(() => {
    const account = read(props.accountId);
    const enabled = ready();
    const external = read(props.session);
    context++;
    if (input) input.value = "";
    setConsent(false); setProbeConsent(false); setError(""); setBusy(false);
    unsubscribe?.();
    const old = session; session = null;
    if (old && !props.session) {
      if (previousAccount !== account) void old.logout().catch((failure) => { if (alive) setError(notice(failure.code)); });
      else void old.stop().catch((failure) => { if (alive) setError(notice(failure.code)); });
      old.dispose();
    }
    previousAccount = account;
    if (external) {
      session = external;
      unsubscribe = session.subscribe(refresh);
    } else if (enabled) {
      try {
        session = (props.createSession || createBrowserModelSession)({ accountId: account });
        unsubscribe = session.subscribe(refresh);
      } catch (failure) { setError(notice(failure.code)); }
    }
    refresh();
  });
  async function act(action) {
    if (busy() || !session) return;
    const current = context, active = session;
    setBusy(true); setError("");
    try { await action(active); }
    catch (failure) { if (alive && current === context) setError(notice(failure.code)); }
    finally { if (alive && current === context) { setBusy(false); setProbeConsent(false); refresh(); } }
  }
  onCleanup(() => {
    alive = false; context++; unsubscribe?.();
    if (!props.session) { void session?.stop().catch(() => {}); session?.dispose(); }
    if (input) input.value = "";
  });
  return <details class="model-connection-panel">
    <style>{modelConnectionStyles}</style>
    <summary>{tr("自带模型连接", "Use your own model")}</summary>
    <p>{tr("浏览器直接调用 DeepSeek 官方；Key 保存在本站 localStorage，刷新或关页后仍保留。同源脚本可读，共享设备请谨慎使用。Key 不上传 Oasis7 后端。", "Your browser calls DeepSeek directly. The key stays in this site's localStorage after reload or close. Same-origin scripts can read it; use care on shared devices. The key is never uploaded to the Oasis7 backend.")}</p>
    <Show when={!read(props.accountId)}><p role="status">{tr("请先登录玩家账号。", "Sign in to your player account first.")}</p></Show>
    <Show when={read(props.accountId) && !ready()}><p role="status">{tr("浏览器任务执行尚未准备，暂不接收 Key 或启动 Agent。请先核对实际任务并登记执行身份；刷新连接不能证明世界结果。", "The browser task is not prepared. Inspect the actual task and register this executor before saving a key or starting. Connection refresh cannot prove world outcomes.")}</p></Show>
    <Show when={error()}><p role="alert">{error()}</p></Show>
    <Show when={ready() && snapshot()}>
      <p>{tr("供应商：DeepSeek 官方 · deepseek-flash。OpenRouter 尚未支持。供应商账户付费；本页预算不限制 Key 在其他应用的消费。", "Provider: official DeepSeek · deepseek-flash. OpenRouter is not supported yet. Your provider account pays; this page cannot cap spending by other apps.")}</p>
      <form onSubmit={(event) => {
        event.preventDefault();
        if (!consent() || busy()) return;
        let key = input.value; input.value = "";
        void act(async (active) => { try { await active.save(key, snapshot().revision); setConsent(false); } finally { key = ""; } });
      }}>
        <label>{tr("推理 API Key", "Inference API key")}<input ref={input} type="password" autocomplete="off" spellcheck={false} disabled={busy()} /></label>
        <label class="model-connection-panel__check"><input type="checkbox" checked={consent()} onChange={(event) => setConsent(event.currentTarget.checked)} disabled={busy()} />{tr("我同意保存到此浏览器的本站数据，并仅发送给 DeepSeek。", "I agree to save it in this browser's site data and send it only to DeepSeek.")}</label>
        <button disabled={busy() || !consent()} type="submit">{tr("保存 Key", "Save key")}</button>
      </form>
      <p role="status">{snapshot()?.hasKey ? tr("Key 已保存；尚未开始 Agent。", "Key saved; Agent has not started.") : tr("尚未保存 Key。", "No key saved.")} {snapshot()?.checked ? tr("工具连接检查通过。", "Tool connectivity check passed.") : ""}</p>
      <p>{tr("已占用检查次数 / token 预留", "Check calls used / tokens reserved")}: {snapshot()?.probeCallsReserved ?? snapshot()?.callsReserved}/4 · {snapshot()?.probeTokensReserved ?? snapshot()?.tokensReserved}/2048</p>
      <p>{tr("检查与运行共享总次数 / token 预留", "Shared check and execution calls / tokens reserved")}: {snapshot()?.callsReserved}/8 · {snapshot()?.tokensReserved}/131072</p>
      <label class="model-connection-panel__check"><input type="checkbox" checked={probeConsent()} onChange={(event) => setProbeConsent(event.currentTarget.checked)} disabled={busy() || !snapshot()?.hasKey} />{tr("同意一次两请求的有限付费工具检查，不写游戏世界。", "Consent to one bounded, two-request, billable tool check; no world writes.")}</label>
      <div class="model-connection-panel__actions">
        <button type="button" disabled={busy() || !probeConsent() || !snapshot()?.hasKey} onClick={() => void act((active) => active.probe(snapshot().revision))}>{tr("测试连接", "Test connection")}</button>
        <button type="button" disabled={!session} onClick={() => { props.onStop?.(); setError(""); void session.stop().then(refresh).catch((failure) => { if (alive) setError(notice(failure.code)); }); }}>{tr("停止新决策", "Stop new decisions")}</button>
        <button type="button" disabled={!session} onClick={() => { props.onStop?.(); setError(""); void session.forget().then(refresh).catch((failure) => { if (alive) setError(notice(failure.code)); }); }}>{tr("断开并忘记", "Disconnect and forget")}</button>
      </div>
      <p>{tr("停止保留 Key；忘记删除本站保存的 Key。两者不撤销供应商 Key，不保证已发请求退款，也不取消已受理的世界作业。", "Stop retains the key; Forget removes it from site data. Neither revokes the provider key, guarantees refunds for sent requests, nor cancels accepted world jobs.")}</p>
    </Show>
  </details>;
}
