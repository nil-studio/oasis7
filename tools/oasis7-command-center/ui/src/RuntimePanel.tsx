import PrivateContextInput from './PrivateContextInput';
import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show, untrack } from 'solid-js';
import { acceptsEnter, connected, errorMessage, observedTime, runtimeBridge, subscribeRuntime } from './bridge';
import { canContinue, canInterrupt, canStart, canSteer, capabilityLabel, fitsUtf8, isRuntimeSnapshot, ownsCurrent, requestKey, runtimeLabel, sessionKey, targetOf, unresolved, unresolvedFor, visibleOperations } from './runtimeState';
import type { Project, QuitStatus, RequestAnswer, RuntimeRequest, RuntimeSession, RuntimeSnapshot } from './types';

type RuntimeApi = Omit<typeof runtimeBridge, 'startWork'|'startWorkRecords'|'publicSnapshot'>;
type Run = (action: () => Promise<RuntimeSnapshot>, management?: boolean) => Promise<void>;
type Props = { detailActive?: boolean; project?: Project; view: string; onState?: (state: string) => void; api?: RuntimeApi; subscribe?: typeof subscribeRuntime };

export default function RuntimePanel(props: Props) {
  const api = props.api ?? runtimeBridge;
  const [runtime, setRuntime] = createSignal<RuntimeSnapshot>();
  const [exit, setExit] = createSignal<QuitStatus>();
  const [exitVisible, setExitVisible] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [uncertainSend, setUncertainSend] = createSignal(false);
  const [error, setError] = createSignal('');
  const [binaryPath, setBinaryPath] = createSignal('');
  const [purpose, setPurpose] = createSignal('');
  const [scope, setScope] = createSignal('');
  const [model, setModel] = createSignal('');
  const [paused, setPaused] = createSignal(false);
  const [logWindow, setLogWindow] = createSignal<string[]>([]);
  const [lastRead, setLastRead] = createSignal('');
  let epoch = 0;
  let reading = false;
  let readQueued = false;
  let disposed = false;
  let unlisten: (() => void) | undefined;

  function apply(value: unknown) {
    if (!isRuntimeSnapshot(value)) throw new Error('运行时快照格式不兼容，未据此更新状态。');
    if (value.projectId !== props.project?.id) return;
    setRuntime(value);
    props.onState?.(value.state);
    setLastRead(new Date().toLocaleTimeString('zh-CN'));
  }
  async function read() {
    if ((!connected() && !props.api) || !props.project || disposed) return false;
    if (reading) { readQueued = true; return false; }
    if (busy()) return false;
    reading = true;
    const generation = epoch;
    try {
      const [source, status] = await Promise.allSettled([api.snapshot(), api.exitStatus()]);
      if (generation !== epoch || disposed) return false;
      if (status.status === 'fulfilled') setExit(status.value);
      if (source.status === 'fulfilled') apply(source.value);
      const failures = [source, status].filter(result => result.status === 'rejected');
      if (failures.length) setError(`观察读取失败，保留可用区域与最近状态：${failures.map(result => result.status === 'rejected' ? errorMessage(result.reason) : '').join('；')}`);
      else setError(previous => previous.startsWith('观察读取失败') ? '' : previous);
      return failures.length === 0;
    } catch (e) {
      if (generation === epoch && !disposed) setError(`观察读取失败，保留最近状态：${errorMessage(e)}`);
      return false;
    } finally {
      reading = false;
      if (readQueued) { readQueued = false; void read(); }
    }
  }
  const run: Run = async (action, management = false) => {
    if (busy() || (uncertainSend() && !management) || !props.project) return;
    const generation = ++epoch;
    setBusy(true); setError('');
    try {
      const next = await action();
      if (generation === epoch && !disposed) apply(next);
    } catch (e) {
      if (generation === epoch && !disposed) {
        if (typeof e === 'object' && e !== null && 'effect' in e && e.effect === 'none' && 'message' in e && typeof e.message === 'string') {
          setError(e.message);
        } else {
          setUncertainSend(true);
          setError(`操作未取得成功回执：${errorMessage(e)}。请核对下方持久操作记录；结果核对前发送入口保持关闭，不会自动重发。`);
        }
      }
    } finally { setBusy(false); void read(); }
  };
  async function exitAction(action: () => Promise<QuitStatus>, hide = false) {
    if (busy()) return;
    setBusy(true); setError('');
    try { setExit(await action()); if (hide) setExitVisible(false); }
    catch (e) { setError(`退出协调失败：${errorMessage(e)}。客户端不会据此宣称工作已结束。`); }
    finally { setBusy(false); void read(); }
  }
  async function reconcileOperation(operationId: string) {
    if (busy() || !props.project) return;
    const generation = ++epoch;
    setBusy(true); setError('');
    try {
      const next = await api.reconcile(operationId);
      if (generation === epoch && !disposed) apply(next);
    } catch (e) {
      if (generation === epoch && !disposed) setError(`原操作来源核对失败，保留原有记录：${errorMessage(e)}。未重发操作。`);
    } finally { setBusy(false); void read(); }
  }
  const projectIdentity = createMemo(() => props.project?.id);
  createEffect(() => {
    const id = projectIdentity();
    ++epoch;
    setRuntime(undefined); setExit(undefined); setError(''); setLastRead(''); setUncertainSend(false);
    props.onState?.('disconnected');
    if (id) untrack(() => void read());
  });
  createEffect(() => {
    const lines = runtime()?.logs ?? [];
    if (!paused()) setLogWindow(lines.slice(-200).map(line => line.slice(0, 2000)));
  });
  onMount(() => {
    const interval = setInterval(() => void read(), 2000);
    onCleanup(() => clearInterval(interval));
    if (connected() || props.subscribe) void (props.subscribe ?? subscribeRuntime)(
      () => void read(),
      () => { setExitVisible(true); void read(); },
    ).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(e => setError(`状态事件订阅不可用，继续只读轮询：${errorMessage(e)}`));
  });
  onCleanup(() => { disposed = true; ++epoch; unlisten?.(); });
  const visible = () => props.view === 'execution' || props.view === 'attention' || exitVisible();
  const sessionIds = createMemo(() => runtime()?.sessions.map(sessionKey) ?? []);
  const requestIds = createMemo(() => runtime()?.requests.map(requestKey) ?? []);
  const operationIds = createMemo(() => runtime() ? visibleOperations(runtime()!).map(op => op.id) : []);
  const sendingBlocked = () => busy() || uncertainSend();
  const configFits = () => fitsUtf8(purpose(),4096) && fitsUtf8(scope(),4096) && fitsUtf8(model(),128);
  const canCreate = () => !uncertainSend() && !!runtime()?.enabled && runtime()?.state === 'ready' && !runtime()?.exitWaiting && runtime()?.capabilities.includes('createSession') && !runtime()?.operations.some(op => unresolved(op) && op.method === 'thread/start');

  return <div class="runtime-workspace" style={{ display: visible() ? undefined : 'none' }}>
    <details class="panel" hidden={props.detailActive||!['execution','attention'].includes(props.view)} open={props.view==='attention'||!!runtime()?.enabled||!!runtime()?.sessions.length||!!runtime()?.requests.length}><summary>Codex 实验接入与原运行请求</summary>
    <section class="panel" aria-label="Codex 实验执行">
      <div class="panel-title"><h3>Codex 开发执行</h3><span class="tag">可选实验接入 · {runtimeLabel(runtime()?.state ?? 'disabled')}</span></div>
      <div class="panel-body">
        <Show when={error()}><div class="notice" role="status">{error()}</div></Show>
        <Show when={runtime()?.error}><div class="notice">运行时来源错误：{runtime()?.error}。观察中断不表示工作已结束。</div></Show>
        <Show when={uncertainSend()}><div class="notice">前一次发送未取得可核实回执。先只读核对操作记录；来源仍为未知时不能重发。<button onClick={() => void (async () => { const verified = await read(); if (verified && runtime()?.state === 'ready' && !runtime()!.operations.some(unresolved) && !runtime()!.sessions.some(s => s.state === 'unknown' || s.backgroundState === 'unknown')) setUncertainSend(false); })()}>只读核对发送边界</button></div></Show>
        <p class="muted">仅管理本项目、本环境明确建立的自管运行实例；不发现或接管其他 App、CLI、IDE 或远端工作。</p>
        <div class="metadata"><span>项目 {props.project?.path ?? '尚未选择'}</span><span>环境 {runtime()?.environment ?? props.project?.environment ?? '未知'}</span><span>实际版本 {runtime()?.version ?? '尚未取得'}</span><span>最近观察 {lastRead() || '尚未读取'}</span></div>
        <p class="muted small">读取范围：{runtime()?.coverage ?? '未连接运行时，无法核实是否有等待输入的工作。此状态不表示没有请求、任务已停止或已完成。'}</p>
        <p class="small">实际能力：{runtime()?.capabilities.map(capabilityLabel).join(' · ') || '未取得读/订阅/控制能力'}</p>
        <div class="actions runtime-controls">
          <button disabled={busy() || !props.project || !runtime()} onClick={() => void run(() => api.enabled(!runtime()!.enabled), true)}>{runtime()?.enabled ? '停用新实验接入' : '为本项目启用实验接入'}</button>
          <button disabled={busy() || !props.project} onClick={() => void read()}>只读核对状态</button>
          <button disabled={!props.project} onClick={() => { setExitVisible(true); void read(); }}>查看退出选项</button>
        </div>
        <Show when={runtime() && !runtime()!.enabled && runtime()!.sessions.some(s => ['active','waiting','unknown'].includes(s.state) || s.backgroundState !== 'clear')}><div class="notice">停止新接入，现有工作仍在管理。必要输入、原实例核对与明确中断仍可使用；停用不表示任务已停止。</div></Show>
        <label class="runtime-path">Codex 可执行文件绝对路径<input value={binaryPath()} placeholder="/opt/homebrew/bin/codex" onInput={e => setBinaryPath(e.currentTarget.value)} /></label>
        <button disabled={busy() || !props.project || !runtime()?.enabled || !binaryPath().startsWith('/') || runtime()?.state === 'ready'} onClick={() => void run(() => api.connect(binaryPath().trim()), true)}>连接选定实验运行时</button>
        <p class="muted small">启用只保存项目偏好。明确连接才启动 app-server；连接和只读刷新不会启动或重放模型任务。</p>
      </div>
    </section>
    <div style={{ display: props.view === 'execution' ? undefined : 'none' }}>
      <section class="panel"><div class="panel-title"><h3>建立明确工作会话</h3><span class="tag">随后手动发送本轮任务</span></div>
        <form class="panel-body editor" onSubmit={e => { e.preventDefault(); }}>
          <label>工作目的<input value={purpose()} onInput={e => setPurpose(e.currentTarget.value)} maxLength={2048} /></label>
          <label>工作范围<textarea value={scope()} onInput={e => setScope(e.currentTarget.value)} maxLength={8192} /></label>
          <label>模型（留空使用运行时默认）<input value={model()} onInput={e => setModel(e.currentTarget.value)} /></label>
          <div class="metadata"><span>执行环境 {props.project?.environment ?? '未知'}</span><span>依据版本 {props.project?.head ?? '尚未读取'}</span></div>
          <p class="muted small">依据版本是读取的 Git HEAD；未提交工作树文档不标为已提交依据。宿主会重新核实实际执行基线。</p>
          <Show when={!configFits()}><p class="notice">工作目的、范围或模型文本超过连接器上限，请缩短输入后建立会话。</p></Show>
          <button class="primary" disabled={busy() || !canCreate() || !configFits() || !purpose().trim() || !scope().trim()} onClick={() => void run(() => api.create({purpose:purpose().trim(), scope:scope().trim(), model:model().trim() || null, basisRevision:props.project!.head}))}>建立会话，不启动模型任务</button>
          <Show when={runtime()?.operations.some(op => unresolved(op) && op.method === 'thread/start')}><p class="notice">会话建立结果仍待核实。此入口不会重发建立操作，请只读核对已有记录。</p></Show>
        </form>
      </section>
      <section class="panel"><div class="panel-title"><h3>原始会话与运行身份</h3><span class="tag">PR / CI 状态独立保留</span></div>
        <For each={sessionIds()}>{key => <SessionCard data={() => runtime()!.sessions.find(s => sessionKey(s) === key)!} snapshot={() => runtime()!} api={api} run={run} busy={busy} blocked={uncertainSend} />}</For>
        <Show when={!sessionIds().length}><div class="empty">已观察范围中尚无自管会话；其他工具的任务状态仍未知。</div></Show>
      </section>
    </div>
    <section class="panel"><div class="panel-title"><h3>真实运行输入与权限请求</h3><span class="tag">绑定原实例与请求身份</span></div>
      <For each={requestIds()}>{key => <RequestCard data={() => runtime()!.requests.find(r => requestKey(r) === key)!} snapshot={() => runtime()!} api={api} run={run} busy={busy} />}</For>
      <Show when={!requestIds().length}><div class="empty">{runtime()?.state === 'ready' ? '当前已观察范围暂无运行请求。' : '运行时未连接，无法核实是否存在等待输入或批准；保留最近已知状态。'}</div></Show>
    </section>
    <section class="panel"><div class="panel-title"><h3>持久操作与真实回执</h3><span class="tag">最近100项 + 全部未决记录</span></div>
      <For each={operationIds()}>{id => { const operation = () => runtime()!.operations.find(op => op.id === id)!; return <div class="row"><div class="row-content"><strong>{operation().method} · {runtimeLabel(operation().state)}</strong><div class="metadata"><span>操作 {id}</span><span>实例 {operation().instanceId}</span><span>thread {operation().threadId ?? '尚未取得'}</span><span>turn {operation().turnId ?? '尚未取得'}</span><span>{observedTime(operation().createdAt)}</span></div><Show when={operation().error}><p class="muted">{operation().error}</p></Show><Show when={unresolved(operation())}><button disabled={busy()} onClick={() => void reconcileOperation(operation().id)}>仅核对原操作来源</button></Show><Show when={operation().state === 'unknown'}><p class="notice">尚未确认结果，不能推导失败或结束。此记录不提供重发按钮；只读核对不会重放原操作。</p></Show></div></div>; }}</For>
      <Show when={!operationIds().length}><div class="empty">已读取范围暂无操作记录。</div></Show>
    </section>
    <Show when={props.view === 'execution'}><section class="panel"><div class="panel-title"><h3>有界观察日志</h3><button onClick={() => setPaused(!paused())}>{paused() ? '恢复日志阅读' : '暂停日志阅读'}</button></div><div class="panel-body"><p class="muted small">最多显示最近200条摘要，每条最多2000字符；{paused() ? '日志视图已暂停，状态观察与工作继续。' : '状态观察持续。'} {runtime()?.logsTruncated ? '上游/本地窗口存在缺失片段，不能据此恢复完整历史。' : '仅覆盖来源提供的摘要，不代表完整对话。'}</p><pre>{logWindow().join('\n') || '暂无已读取摘要。'}</pre></div></section></Show>
    </details><Show when={exitVisible()}><section class="panel runtime-exit" aria-label="退出协调"><div class="panel-title"><h3>完整退出的工作处理</h3><span class="tag">默认留在后台</span></div><div class="panel-body"><Show when={error()}><div role="alert" class="notice">退出操作未确认：{error()}</div></Show><p>{exit()?.reason ?? '退出状态尚未核实，不会据此停止工作或宣称可以安全退出。'}</p><div class="metadata"><span>自管活跃 {exit()?.active ?? '未知'}</span><span>状态未知 {exit()?.unknown ?? '未知'}</span><span>待回复 {exit()?.pendingRequests ?? '未知'}</span><span>{exit()?.safe ? '来源已确认可退出' : '未确认可以安全退出'}</span><span>{exit()?.waiting ? '正在等待来源确认后退出' : '未进入等待退出'}</span></div><div class="actions runtime-controls"><button disabled={busy()} onClick={() => void exitAction(() => api.background(), true)}>留在后台 / 取消等待退出</button><button disabled={busy() || !exit()} onClick={() => void exitAction(() => api.waitExit())}>等待结束后退出</button><button disabled={busy() || !exit()} onClick={() => void exitAction(() => api.stopExit())}>明确请求停止后退出</button></div><p class="muted small">中断被接受不等于工作已停止。停止失败或结果未知时保留管理入口；退出不会停止外部运行时拥有的工作。</p></div></section></Show>
  </div>;
}

function SessionCard(props: { data: () => RuntimeSession; snapshot: () => RuntimeSnapshot; api: RuntimeApi; run: Run; busy: () => boolean; blocked: () => boolean }) {
  const [input, setInput] = createSignal('');
  const [composing, setComposing] = createSignal(false);
  const session = props.data;
  const current = () => ownsCurrent(props.snapshot(), session());
  const send = () => {
    if (props.busy() || props.blocked() || composing() || !input().trim() || !fitsUtf8(input(),32768)) return;
    if (canStart(props.snapshot(), session())) void props.run(() => props.api.start(session().threadId, input()));
    else if (canSteer(props.snapshot(), session())) void props.run(() => props.api.steer(targetOf(session())!, input()));
  };
  return <div class="row"><div class="row-content"><h3>{session().purpose}</h3><p class="muted">范围：{session().scope}</p><div class="metadata"><span>会话 {session().threadId}</span><span>实例 {session().instanceId}</span><span>本轮 {session().turnId ?? '尚未开始'}</span><span>模型 {session().model ?? '运行时默认 / 未取得'}</span><span>依据 {session().basisRevision}</span></div><div class="metadata"><span>本轮：{runtimeLabel(session().state)}</span><span>后台：{runtimeLabel(session().backgroundState)}</span><span>{session().owned ? '本客户端自管' : '外部所有权 / 仅观察'}</span></div>
    <Show when={!current()}><p class="notice">原运行实例当前未取得控制能力。相同 thread 或分支不自动转移控制。{session().restorableHistory ? '来源保存了此前已核实的空闲退出；可明确恢复原历史会话，收到真实 resume 回执后才重绑新实例。' : '此前活跃或未知结果尚未核实，不能恢复或重发，请核对原记录。'}</p></Show>
    <Show when={unresolvedFor(props.snapshot(), session())}><p class="notice">此目标有尚未核实的发送记录，追加/启动入口暂不可用，不会自动重发。</p></Show>
    <Show when={session().privateSources?.length}><p class="notice">此会话含私有资料来源；后续轮次、请求与恢复持续按私有会话处理。</p></Show>
    <PrivateContextInput threadId={session().threadId} input={input} disabled={() => composing() || props.busy() || props.blocked() || !canStart(props.snapshot(),session())} send={props.run} />
    <label class="runtime-path">向此原始会话发送明确输入<textarea disabled={!current()} value={input()} maxLength={32768} onInput={e => setInput(e.currentTarget.value)} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={e => { if ((e.ctrlKey || e.metaKey) && acceptsEnter(e, composing())) { e.preventDefault(); send(); } }} /></label>
    <div class="actions runtime-controls"><button disabled={props.busy() || props.blocked() || composing() || !input().trim() || !fitsUtf8(input(),32768) || (!canStart(props.snapshot(), session()) && !canSteer(props.snapshot(), session()))} onClick={send}>{session().state === 'active' ? '追加到当前运行' : '启动明确本轮任务'}</button><button disabled={props.busy() || props.blocked() || !canContinue(props.snapshot(),session())} onClick={() => void props.run(() => props.api.continue(session().threadId))}>{current() ? '核实并继续原会话' : '明确核实并恢复历史会话'}</button><button disabled={props.busy() || !canInterrupt(props.snapshot(), session())} onClick={() => void props.run(() => props.api.interrupt(targetOf(session())!), true)}>明确请求中断本轮</button></div><p class="muted small">普通 Enter 保留换行；⌘/Ctrl + Enter 明确发送。中文候选组合期间不发送。输入草稿保留，按钮回执不代表任务完成。</p>
  </div></div>;
}

function RequestCard(props: { data: () => RuntimeRequest; snapshot: () => RuntimeSnapshot; api: RuntimeApi; run: Run; busy: () => boolean }) {
  const [answers, setAnswers] = createSignal<Record<string,string>>({});
  const [decision, setDecision] = createSignal('');
  const [composing, setComposing] = createSignal(false);
  const request = props.data;
  const affectedWork = () => props.snapshot().sessions.find(s => s.instanceId === request().instanceId && s.threadId === request().threadId);
  const questionIds = createMemo(() => request().questions.map(q => q.id));
  createEffect(() => { if (['answered','resolved','confirmed'].includes(request().state)) { setAnswers({}); setDecision(''); } });
  const current = () => !props.snapshot().exitWaiting && ['ready','degraded'].includes(props.snapshot().state) && props.snapshot().instanceId === request().instanceId && props.snapshot().capabilities.includes('answerRequest') && request().state === 'pending' && props.snapshot().sessions.some(s => s.owned && s.instanceId === request().instanceId && s.threadId === request().threadId) && !props.snapshot().operations.some(op => op.instanceId === request().instanceId && op.threadId === request().threadId && op.turnId === request().turnId && (op.method === `answerRequest:${request().requestId}` && op.state !== 'failed'));
  const eligible = () => current() && !props.busy() && !composing() && (request().allowedDecisions.length ? request().allowedDecisions.includes(decision()) : request().questions.every(q => !!answers()[q.id]?.trim() && fitsUtf8(answers()[q.id],8192)));
  function submit() {
    if (!eligible()) return;
    const r = request();
    const reply: RequestAnswer = {requestId:r.requestId, target:{instanceId:r.instanceId, threadId:r.threadId, turnId:r.turnId}, decision:decision() || null, answers:Object.fromEntries(r.questions.map(q => [q.id,[answers()[q.id] ?? '']]))};
    void props.run(() => props.api.answer(reply),true);
  }
  const impacts: Record<string,string> = { accept:'允许此请求继续；按来源提供的实际范围执行。', acceptForSession:'允许范围可能覆盖本会话后续同类请求；请核对来源范围。', decline:'拒绝此请求；上游可能停止对应步骤。', cancel:'取消此次请求；后续状态按来源回执核实。' };
  return <div class="row"><div class="row-content"><h3>{request().summary}</h3><p class="muted">受影响工作：{affectedWork()?.purpose ?? '来源未提供关联目的'} · 范围：{affectedWork()?.scope ?? '未知'}</p><div class="metadata"><span>请求 {request().requestId}</span><span>实例 {request().instanceId}</span><span>thread {request().threadId}</span><span>turn {request().turnId}</span><span>{request().method} · {runtimeLabel(request().state)}</span></div>
    <Show when={!current()}><p class="notice">此请求当前不可回复：正在等待退出、原实例未核实、能力不可用或已有处理记录。不会迁移至另一个运行目标。</p></Show>
    <form class="editor runtime-controls" onSubmit={e => e.preventDefault()}>
      <For each={questionIds()}>{id => { const q = () => request().questions.find(question => question.id === id)!; return <label>{q().header} · {q().question}<Show when={q().options.length}><select value={answers()[id] ?? ''} onChange={e => setAnswers({...answers(),[id]:e.currentTarget.value})}><option value="">选择来源提供的回答</option><For each={q().options}>{option => <option value={option}>{option}</option>}</For></select></Show><input type={q().isSecret ? 'password' : 'text'} aria-label={`回答 ${id}`} value={answers()[id] ?? ''} maxLength={8192} onInput={e => setAnswers({...answers(),[id]:e.currentTarget.value})} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={e => { if (e.key === 'Enter' && !e.isComposing && !composing()) e.preventDefault(); }} /></label>; }}</For>
      <Show when={request().allowedDecisions.length}><label>仅选择运行时实际支持的决定<select value={decision()} onChange={e => setDecision(e.currentTarget.value)}><option value="">尚未选择</option><For each={request().allowedDecisions}>{value => <option value={value}>{value}</option>}</For></select><span class="muted small">{impacts[decision()] ?? '先核对请求背景与实际权限范围；不提供任意协议方法或参数。'}</span></label></Show>
      <button disabled={!eligible()} onClick={submit}>向原请求回复一次</button><p class="muted small">回复绑定完整原始请求身份。发送结果未知时保留操作记录，不自动重发；恢复或刷新不发送回答。</p>
    </form>
  </div></div>;
}
