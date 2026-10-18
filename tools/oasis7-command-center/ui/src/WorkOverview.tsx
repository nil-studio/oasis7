import { createEffect, createSignal, For, onCleanup, Show } from 'solid-js';
import { bridge, errorMessage, observedTime, statusLabel, subscribeRuntime } from './bridge';
import type { WorkItem, WorkProjection, WorkRef } from './generated/core';
import type { Snapshot, GitHubSnapshot } from './types';
import { runtimeLabel } from './runtimeState';

type Props = { snapshot?: Snapshot; view: string; sourceGeneration: unknown; onExecution: () => void; ghPath?: string; onGithub?: (value: GitHubSnapshot) => void };
const keyOf = (reference: WorkRef) => JSON.stringify(reference);
export default function WorkOverview(props: Props) {
  const [projection, setProjection] = createSignal<WorkProjection>();
  const [error, setError] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [expanded, setExpanded] = createSignal(false);
  const [checksBusy, setChecksBusy] = createSignal(false);
  const [selected, setSelected] = createSignal<string>();
  let generation = 0;
  let disposed = false;
  let origin: HTMLElement | undefined;
  let detail: HTMLElement | undefined;
  let list: HTMLElement | undefined;
  let unlisten: (() => void) | undefined;
  let currentScope = '';
  async function read() {
    const project = props.snapshot?.project;
    if (!project || disposed) return;
    const token = ++generation;
    setLoading(true);
    try {
      const value = await bridge.workProjection();
      if (token !== generation || disposed) return;
      if (value.schemaVersion !== 1 || value.projectId !== project.id || value.environment !== project.environment || !Array.isArray(value.items)) throw new Error('工作投影与当前项目不匹配，请刷新来源。');
      setProjection(value); setError('');
    } catch (e) { if (token === generation && !disposed) setError(errorMessage(e)); }
    finally { if (token === generation && !disposed) setLoading(false); }
  }
  createEffect(() => {
    const project = props.snapshot?.project;
    const scope = project ? JSON.stringify([project.id, project.environment]) : '';
    props.sourceGeneration;
    props.snapshot?.associations;
    if (scope !== currentScope) {
      currentScope = scope; ++generation; setProjection(undefined); setSelected(undefined); setExpanded(false); setError('');
    }
    if (project) void read();
  });
  void subscribeRuntime(() => { void read(); }, () => {}).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
  onCleanup(() => { disposed = true; ++generation; unlisten?.(); });
  const item = () => projection()?.items.find(item => keyOf(item.reference) === selected());
  async function readChecks(work: WorkItem) {
    if (work.reference.kind !== 'pullRequest' || checksBusy()) return;
    const scope = currentScope;
    setChecksBusy(true); setError('');
    try {
      const next = await bridge.refreshWorkChecks(work.reference.number, props.ghPath?.trim() || null);
      if (scope !== currentScope || disposed) return;
      if (props.onGithub) props.onGithub(next);
      else await read();
    } catch (e) { if (scope === currentScope && !disposed) setError(errorMessage(e)); }
    finally { setChecksBusy(false); }
  }
  function open(item: WorkItem, button: HTMLElement) {
    origin = button; setSelected(keyOf(item.reference));
    queueMicrotask(() => { detail?.focus(); detail?.scrollIntoView?.({block:'nearest'}); });
  }
  function close() {
    const reference = selected();
    const current = Array.from(list?.querySelectorAll<HTMLButtonElement>('button[data-work-ref]') ?? [])
      .find(button => button.dataset.workRef === reference);
    setSelected(undefined);
    const target = current ?? origin;
    if (target?.isConnected) target.focus();
  }
  return <Show when={props.snapshot?.project && (props.view === 'direction' || props.view === 'execution')}>
    <section class="panel" ref={list} aria-label="已有工作">
      <div class="panel-title"><h3>返回已有工作</h3><button disabled={loading()} onClick={() => void read()}>{loading() ? '正在读取工作' : '刷新工作列表'}</button></div>
      <div class="panel-body"><p class="muted">按已读取来源列出，未按优先级排序。进入详情核对结果、剩余条件和原工作入口。</p>
        <Show when={error()}><p role="alert" class="notice">工作读取失败：{error()}。保留本项目最近结果。</p></Show>
        <For each={projection()?.sources}>{source => <div class="metadata"><span>{source.kind === 'github' ? 'GitHub' : '自管会话'} · {source.kind === 'codex' ? runtimeLabel(source.state) : statusLabel(source.state)}</span><span>{source.kind === 'codex' ? '仅当前项目的公开自管会话；其他工具中的会话仍未知。' : source.coverage}</span><Show when={source.error}><span>{source.error}</span></Show></div>}</For>
        <Show when={!projection() && loading()}><p role="status">正在读取已有工作来源。</p></Show>
        <Show when={projection() && !projection()!.items.length}><p>当前已读取范围未列出工作；请核对来源覆盖。可在开发执行连接 GitHub 或查看自管会话。</p><button onClick={props.onExecution}>查看开发执行</button></Show>
      </div>
      <For each={expanded() ? projection()?.items : projection()?.items.slice(0, 3)}>{work => <div class="row"><div class="row-content"><button class="row-title" data-work-ref={keyOf(work.reference)} onClick={e => open(work, e.currentTarget)}>{work.purpose.value}</button><p>{work.result.value}</p><span class="muted small">{work.goals.length} 项明确目标关系 · {work.reference.kind === 'pullRequest' ? 'PR 来源' : '会话来源'}</span></div><span class="tag">完成条件待核实</span></div>}</For>
      <Show when={(projection()?.items.length ?? 0) > 3}><div class="panel-body"><button onClick={() => setExpanded(!expanded())}>{expanded() ? '收起其余工作' : `显示其余 ${projection()!.items.length - 3} 项工作`}</button></div></Show>
    </section>
    <Show when={item()} fallback={<Show when={selected()}><section class="panel"><div class="panel-body"><p>原工作不在当前来源窗口中；尚未据此认定工作完成或删除。</p><button onClick={close}>返回工作列表</button></div></section></Show>}>{work => <section class="panel detail" ref={detail} tabindex="-1" aria-label="工作详情">
      <div class="panel-title"><h3>{work().purpose.value}</h3><button onClick={close}>返回工作列表</button></div>
      <div class="panel-body"><h4>已知结果</h4><p>{work().result.value}</p><h4>剩余条件</h4><p>{work().remaining}</p>
        <Show when={work().sourceUrl} fallback={<button onClick={props.onExecution}>核对自管运行实例</button>}><button onClick={() => void bridge.openSource(work().sourceUrl!).catch(e => setError(errorMessage(e)))}>打开原 PR</button></Show>
        <h4>关联目标</h4><For each={work().goals}>{goal => <p>{goal.title ?? `原章节未读取或已变化 · ${goal.sectionId}`}<span class="muted small"> · 个人确认 {observedTime(goal.confirmedAt)}</span></p>}</For><Show when={!work().goals.length}><p>尚无明确目标关系；不影响读取来源。</p></Show>
        <Show when={work().reference.kind === 'pullRequest'}><div class="actions"><button disabled={checksBusy()} onClick={() => void readChecks(work())}>{checksBusy() ? '正在读取该 PR 的 CI' : '只读刷新该 PR 的 CI'}</button></div><p class="muted small">仅明确点击后读取该 HEAD 的 REST 检查来源，最多两个请求；不自动读取其他 PR。</p><Show when={work().checksSource} fallback={<p>该 PR 的 CI 尚未单独读取；评审状态未知。</p>}>{source => <div><p>{statusLabel(source().state)} · CI 最近成功读取 {observedTime(source().observedAt)}</p><p class="muted small">{source().coverage}</p><Show when={source().error}><p role="alert">{source().error}</p></Show><Show when={!work().checks.length && source().state !== 'error'}><p>已读取的 CI 页面为空；不代表全部必要检查通过。</p></Show></div>}</Show></Show>
        <Show when={work().checks.length}><h4>PR 检查</h4><For each={work().checks}>{check => <p>{check.name} · {check.state}<br/><span class="muted small">实际测试版本 {check.testedOid ?? '未知，不能由 PR HEAD 推定'}</span><Show when={check.sourceOid}><br/><span class="muted small">GitHub 关联提交 {check.sourceOid} · 不代表实际 checkout 对象</span></Show></p>}</For></Show>
        <For each={work().runs}>{run => <p>运行实例 {run.instanceId} · {run.state}<br/><span class="muted small">turn {run.turnId ?? '未取得'} · {run.owned ? '原自管实例' : '历史实例，控制能力需核实'}</span></p>}</For>
        <details><summary>来源身份与核验范围</summary><pre>{JSON.stringify(work().reference, null, 2)}</pre><p>来源版本 {work().result.evidence.revision ?? '未知'}</p><p>来源事实核验 {observedTime(work().result.evidence.verifiedAt)}</p><p>事件时间 {work().result.evidence.eventAt ?? '未知；不宣称自上次以来的完整成果'}</p><p>App 读取 {observedTime(work().result.evidence.readAt)}</p><p>{work().result.evidence.coverage}</p></details>
      </div>
    </section>}</Show>
  </Show>;
}
