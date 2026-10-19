import type { TreeSourcePosition } from './documentTreeReading';
import { batch, createEffect, createMemo, createSignal, For, onMount, Show } from 'solid-js';
import { acceptsEnter, bridge, connected, errorMessage, observedTime, statusLabel } from './bridge';
import type { DecisionDraft, DocumentSection, GitHubSnapshot, Snapshot, DecisionRecord } from './types';
import WorkOverview from './WorkOverview';
import MarkdownText from './MarkdownText';
import LocalTools from './LocalTools';
import SummarySources from './SummarySources';
import DocumentInventory from './DocumentInventory';
import SourceSuccession, { lifecycleLabel } from './SourceSuccession';

const views = [ ['direction', '方向与进度'], ['rules', '规范与决策'], ['execution', '开发执行'], ['attention', '待我处理'], ['tools', '本机工具'] ] as const;
const blankDraft = (): DecisionDraft => ({ id: null, content: '', reason: '', scope: '', relatedSection: null, confirmed: false });
const disconnectedGithub: GitHubSnapshot = { state: 'unconnected', repository: null, account: null, observedAt: null, lastAttempt: null, coverage: '尚未授权读取 GitHub。使用已有 gh 登录凭据，只读最近 PR。', error: null, pulls: [] };

export default function App(props: { preview?: boolean } = {}) {
  const [navigationOpen,setNavigationOpen]=createSignal(false);
  const [showConnections,setShowConnections]=createSignal(false);
  const [workSelected,setWorkSelected] = createSignal(false);
  const [displayScale,setDisplayScale] = createSignal((()=>{try{const value=Number(localStorage.getItem('oasis7.display-scale'));return [1,1.5,2].includes(value)?value:1;}catch{return 1;}})());
  createEffect(()=>document.documentElement.style.setProperty('--display-scale',String(displayScale())));
  function resizeDisplay(value:number){setDisplayScale(value);try{localStorage.setItem('oasis7.display-scale',String(value));}catch{setMessage('显示大小已应用；本机偏好未保存，重开后将使用默认大小。');}}
  const [snapshot, setSnapshot] = createSignal<Snapshot>();
  const [view, setView] = createSignal('direction');
  const [path, setPath] = createSignal('');
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<DocumentSection[] | null>(null);
  const [selected, setSelected] = createSignal<DocumentSection>();
  const [draft, setDraft] = createSignal(blankDraft());
  const [decisionHistory,setDecisionHistory]=createSignal<DecisionRecord[]>([]);
  const [historyDecision,setHistoryDecision]=createSignal('');
  const [github, setGithub] = createSignal(disconnectedGithub);
  const [ghPath, setGhPath] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [sourceBusy,setSourceBusy]=createSignal(false);
  async function readSources(action:()=>Promise<void>){if(sourceBusy())return;setSourceBusy(true);try{await action();}catch(e){setMessage(errorMessage(e));}finally{setSourceBusy(false);}}
  const [message, setMessage] = createSignal('');
  const [composing, setComposing] = createSignal(false);
  const [workUrl, setWorkUrl] = createSignal('');
  const [githubPage,setGithubPage] = createSignal(1);
  const [knownPr,setKnownPr] = createSignal('');
  const [goalIds, setGoalIds] = createSignal<string[]>([]);
  // Project switching never saves or sends inputs. Search keeps existing reading preferences.
  const projectInputs = new Map<string, { draft: DecisionDraft; workUrl: string; goalIds: string[]; query: string }>();
  let searchGeneration = 0;
  const [sourcePosition,setSourcePosition]=createSignal<TreeSourcePosition|null>(null);
  let sourceContent:HTMLPreElement|undefined;
  let sourceScrollIntent=false;
  let programmaticSourceScroll:number|null=null;
  let sourceDetail: HTMLElement | undefined;
  let sourceOrigin: HTMLElement | undefined;
  let sourceOriginId='';
  let sourceIntent=0;

  async function run(action: () => Promise<void>) {
    if (busy()) return;
    setBusy(true); setMessage('');
    try { await action(); } catch (error) { setMessage(errorMessage(error)); }
    finally { setBusy(false); }
  }
  function apply(next: Snapshot) {
    const previousId = snapshot()?.project?.id;
    const nextId = next.project?.id;
    batch(() => {
      if (previousId !== nextId) {
        if (previousId) projectInputs.set(previousId, { draft: { ...draft() }, workUrl: workUrl(), goalIds: [...goalIds()], query: query() });
        const inputs = nextId ? projectInputs.get(nextId) : undefined;
        ++searchGeneration;
        setResults(null);
        setSelected(undefined);
        sourceOrigin = undefined;sourceOriginId='';setSourcePosition(null);
        setDraft(inputs?.draft ?? blankDraft());
        setWorkUrl(inputs?.workUrl ?? '');
        setGoalIds(inputs?.goalIds ?? []);
        setQuery(inputs?.query ?? '');
      }
      setSnapshot(next);
      setPath(next.project?.path ?? next.preferences.projectPath ?? '');
    });
  }
  onMount(() => { if (connected() || props.preview) void run(async () => {
    const next = await bridge.snapshot(); apply(next);
    if (views.some(([key]) => key === next.preferences.view)) setView(next.preferences.view);
    setQuery(next.preferences.query);
    const reading = next.readingPositions.at(-1);
    const sourceToken=sourceIntent;
    let treeOwnsReading=false;
    if(next.project&&next.preferences.view==='rules'){
      try{const recent=await bridge.loadDocumentTreeLastView({schemaVersion:1,projectId:next.project.id,baselineMode:'tracked-working-tree',baselineOid:next.project.head});treeOwnsReading=recent.status==='ready'&&(recent.view==='product'||recent.view==='professional');}
      catch{/* The document panel reports the repairable preference error. */}
    }
    if(reading&&!treeOwnsReading&&sourceToken===sourceIntent){const restored=next.sections.find(s=>s.id===reading.sectionId&&s.revision===reading.revision)??await bridge.readVersion(reading.sectionId,reading.revision)??undefined;if(sourceToken===sourceIntent)setSelected(restored);}
    setGithub(await bridge.github());
  }); });
  async function persistPreference(nextView = view(), nextQuery = query()) {
    if (!connected()) return;
    try { await bridge.savePreferences({ projectPath: snapshot()?.project?.path ?? null, view: nextView, query: nextQuery }); }
    catch (error) { setMessage(`阅读偏好未保存：${errorMessage(error)}`); }
  }
  function changeView(key: string) { setNavigationOpen(false);setView(key); void persistPreference(key); }
  async function search() {
    const generation = ++searchGeneration;
    if (!query().trim()) { setResults(null); return; }
    try {
      const found = await bridge.search(query().trim());
      if (generation === searchGeneration) { setResults(found); void persistPreference(); }
    } catch (error) { if (generation === searchGeneration) setMessage(errorMessage(error)); }
  }
  function showSource(section: DocumentSection, origin?: HTMLElement, restore?:{scrollTop:number;passive:boolean}) {
    ++sourceIntent;
    // Following a link within the detail keeps the entry point outside this replaceable DOM.
    const fromDetail = !!selected() && sourceDetail?.contains(origin ?? document.activeElement);
    if (!fromDetail) {
      sourceOriginId=section.id;
      if (origin) sourceOrigin = origin;
      else if (document.activeElement instanceof HTMLElement) sourceOrigin = document.activeElement;
    }
    sourceScrollIntent=false;setSelected(section);setSourcePosition({section,scrollTop:restore?.scrollTop??0,userInitiated:!restore?.passive});
    queueMicrotask(() => { if(selected()!==section)return;if(sourceContent){void sourceContent.clientHeight;sourceContent.scrollTop=restore?.scrollTop??0;programmaticSourceScroll=sourceContent.scrollTop;}if(!restore?.passive){sourceDetail?.focus({preventScroll:true});sourceDetail?.scrollIntoView?.({block:'start'});} });
    if(!restore?.passive)void bridge.recordReading(section).catch(error => setMessage(`阅读位置未保存：${errorMessage(error)}`));
  }
  const sections = () => snapshot()?.sections ?? [];
  const current = createMemo(() => sections().filter(s => s.path === 'doc/core/prd.md' && /当前阶段交付目标与全局 P0/.test(s.title)));
  const longTerm = createMemo(() => sections().filter(s => s.path === 'doc/core/prd.md' && /长期|愿景|定位/.test(s.title) && !current().includes(s)).slice(0, 8));
  const rules = createMemo(() => sections().filter(s => s.path === 'doc/engineering/workflow/source-of-truth.md' || s.path === 'AGENTS.md' || s.path.startsWith('doc/product/')).slice(0, 100));
  const decisionOptions = createMemo(() => { const prioritized = [...(results() ?? []), ...(selected() ? [selected()!] : []), ...current(), ...longTerm(), ...rules()]; return [...new Map(prioritized.map(s => [s.id, s])).values()].slice(0, 200); });
  const goalOptions = createMemo(() => { const candidates = [...sections().filter(s => goalIds().includes(s.id)), ...(results() ?? []), ...current(), ...longTerm(), ...sections().filter(s => /目标|里程碑|验收/.test(s.title))]; return [...new Map(candidates.map(s => [s.id, s])).values()].slice(0, 100); });
  const decisions = () => snapshot()?.decisions ?? [];
  const pending = () => decisions().filter(d => d.state === 'confirmed' || d.confirmedAt !== null);
  const heading = () => views.find(([key]) => key === view())?.[1] ?? '方向与进度';
  const description = () => ({ direction: '从当前主责条款理解下一项交付，核实依据与未知范围。', rules: '检索现行条款，保存理由与范围，保留决定的真实状态。', execution: '围绕工作目的查看分支、工作树和 PR 状态，打开 GitHub 核对详情。', attention: '查看个人记录的问题，以及决定的同步与核实事项。', tools: '管理当前电脑的资料与本机工具，核对真实结果。' } as Record<string, string>)[view()];
  const sectionRows = (items: DocumentSection[]) => <For each={items}>{s => <div class="row">
    <div class="row-content"><button class="row-title" data-source-ref={s.id} onClick={e => showSource(s, e.currentTarget)}>{s.title}</button><div class="metadata"><span>{s.path}:{s.line}</span><span>{s.dirty ? '工作树有修改' : '已提交内容'}</span><span>源事实 · 实现验收未知</span><span>{lifecycleLabel(s)}</span></div></div><span class="tag">原文</span>
  </div>}</For>;

  return <div class="app-frame" ><header><div><div class="brand">OASIS7 / COMMAND</div><h1>指挥客户端</h1></div><div class="header-context"><strong>{snapshot()?.project ? 'oasis7 · 本地项目' : '选择项目，建立真实上下文'}</strong><div class="muted small">{snapshot()?.project?.path ?? '本地文档与 Git 优先 · 可选来源独立接入'}</div></div><button onClick={()=>setShowConnections(!showConnections())} aria-expanded={showConnections()}>项目与来源</button><div class="display-settings"><label>文字与界面<select aria-label="文字与界面大小" value={displayScale()} onChange={e=>resizeDisplay(Number(e.currentTarget.value))}><option value="1">100%</option><option value="1.5">150%</option><option value="2">200%</option></select></label><button onClick={()=>resizeDisplay(1)}>重置</button></div><span class="tag desktop-state">{props.preview?'演示数据 · 非真实状态':connected() ? '桌面核心' : '浏览器预览 · 未接入'}</span></header>
    <div class="shell"><aside classList={{'navigation-open':navigationOpen()}}><button class="navigation-toggle" aria-expanded={navigationOpen()} aria-controls="main-navigation" onClick={()=>setNavigationOpen(!navigationOpen())}>导航 · {heading()}</button><nav id="main-navigation" aria-label="主视图"><For each={views}>{([key, label], index) => <button aria-current={view() === key ? 'page' : undefined} onClick={() => changeView(key)}><span class="nav-index">0{index() + 1}</span>{label}</button>}</For></nav><div class="side-caption">来源 / 读取范围</div><For each={snapshot()?.sources.filter(s => s.kind !== 'github' && s.kind !== 'codex') ?? []}>{source => <div class="source-status"><span>{source.kind}</span><span class={`tag ${source.state}`}>{statusLabel(source.state)}</span></div>}</For><div class="source-status"><span>GitHub</span><span class={`tag ${github().state}`}>{statusLabel(github().state)}</span></div><div class="side-caption">环境</div><div class="panel-body small muted">{snapshot()?.project?.environment ?? '未选择项目'}<br />{snapshot()?.project?.branch ?? '分支尚未读取'}</div></aside>
    <main classList={{"reading-work":workSelected()}}><div class="toolbar" hidden={workSelected()}><div><div class="eyebrow">PROJECT CONTROL / {views.findIndex(([key]) => key === view()) + 1}</div><h2>{heading()}</h2><p class="muted">{description()}</p></div><div class="search" role="search"><input aria-label="搜索规范、标题、路径或条款 ID" placeholder="搜索规范、合入、条款 ID…" value={query()} onInput={e => { setQuery(e.currentTarget.value); ++searchGeneration; }} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={e => { if (acceptsEnter(e, composing())) { e.preventDefault(); void search(); } }} /><button disabled={!connected() || composing()} onClick={() => void search()}>搜索</button></div></div>
    <Show when={message()}><div class="notice" role="status">{message()}</div></Show>
    <Show when={!connected()&&!props.preview}><div class="notice">浏览器预览未接入桌面 IPC。这里不展示模拟项目、PR 或完成状态。请启动 macOS App 读取真实仓库。</div></Show>
    <details class="panel" hidden={!!snapshot()?.project&&!showConnections()} open={!snapshot()?.project||showConnections()}><summary>项目上下文与连接设置</summary><section class="panel"><div class="panel-title"><h3>项目上下文</h3><button onClick={()=>void bridge.cancelSourceReads().then(()=>setMessage('已取消只读候选发布；不会中断模型或工具操作。')).catch(e=>setMessage(errorMessage(e)))}>取消只读来源刷新</button><button disabled={sourceBusy() || !snapshot()?.project} onClick={() => void readSources(async () => apply(await bridge.refresh()))}>刷新本地事实</button></div><div class="panel-body"><p class="muted small">刷新只读取文档与 Git，不 fetch、不切换分支、不运行模型。路径按实际仓库校验。</p><form class="project-form" onSubmit={e => { e.preventDefault(); void readSources(async () => { const next = await bridge.openProject(path().trim()); ++searchGeneration; setResults(null); setSelected(undefined); apply(next); setGithub(await bridge.github()); }); }}><input aria-label="本地仓库绝对路径" placeholder="/Users/…/oasis7" value={path()} onInput={e => setPath(e.currentTarget.value)} /><button type="button" disabled={sourceBusy() || !connected()} onClick={() => void readSources(async () => { const next = await bridge.chooseProject(); if (next) { ++searchGeneration; setResults(null); setSelected(undefined); apply(next); setGithub(await bridge.github()); } })}>选择文件夹</button><button class="primary" disabled={sourceBusy() || !connected() || !path().trim()} type="submit">{sourceBusy() ? '正在读取…' : '打开项目'}</button></form><Show when={snapshot()?.project}>{project => <div class="metadata"><span>环境 {project().environment}</span><span>分支 {project().branch ?? 'detached HEAD'}</span><span>HEAD {project().head}</span></div>}</Show></div></section></details>
    <details class="source-notices" hidden={!showConnections()}><summary>来源状态与读取范围</summary><For each={snapshot()?.sources.filter(s => s.error || /遗漏\s*[1-9]|omitted[^0]*[1-9]/i.test(s.coverage))}>{s => <div class="notice"><strong>{s.kind} · {statusLabel(s.state)}</strong>　{s.error ?? s.coverage}<div class="small">最近成功：{observedTime(s.lastSuccess)} · {s.coverage}</div></div>}</For></details>
    <Show when={results() !== null}><section class="panel"><div class="panel-title"><h3>检索结果 · {results()?.length ?? 0} 项</h3><button onClick={() => { setResults(null); setQuery(''); }}>返回当前视图</button></div>{sectionRows(results() ?? [])}<Show when={results()?.length === 0}><div class="empty">本次查询成功，已读取范围内没有匹配。最多显示 100 项。</div></Show></section></Show>
    <WorkOverview onSelection={setWorkSelected} snapshot={snapshot()} view={view()} sourceGeneration={github()} onExecution={() => changeView('execution')} onView={changeView} showSource={showSource} onOpenProject={path=>void run(async()=>apply(await bridge.openProject(path)))} />
    <Show when={view() === 'direction'&&!workSelected()}><details class="panel"><summary>项目摘要、长期方向与原文依据</summary><SummarySources sections={sections()} hasProject={!!snapshot()?.project} showSource={showSource} /><For each={longTerm()}>{s=><div class="panel-body"><button class="row-title" onClick={e=>showSource(s,e.currentTarget)}>{s.title}</button><MarkdownText text={s.content} sourcePath={s.path} sections={sections()} showSource={showSource}/></div>}</For></details></Show>
    <Show when={view() === 'rules' && snapshot()?.project}><DocumentInventory snapshot={snapshot()!} showSource={showSource} sourcePosition={sourcePosition()} /></Show><Show when={view() === 'rules'}><div class="columns"><section class="panel"><div class="panel-title"><h3>主责入口与产品文档 · 已显示 {rules().length} 个章节</h3><span class="tag">文档声明 · 效力仍需核实</span></div>{sectionRows(rules())}<Show when={!rules().length}><div class="empty">选择项目后读取主责入口；中文两字查询可从上方搜索。</div></Show></section><section class="panel"><div class="panel-title"><h3>记录个人决定</h3><div class="actions"><span class="tag">不修改正式规范</span><button type="button" onClick={() => setDraft(blankDraft())}>新建决定</button></div></div><form class="panel-body editor" onSubmit={e => e.preventDefault()}><label>决定内容<textarea value={draft().content} onInput={e => setDraft({ ...draft(), content: e.currentTarget.value })} /></label><label>理由<textarea value={draft().reason} onInput={e => setDraft({ ...draft(), reason: e.currentTarget.value })} /></label><label>适用范围<input value={draft().scope} onInput={e => setDraft({ ...draft(), scope: e.currentTarget.value })} /></label><label>关联条款<select value={draft().relatedSection ?? ''} onChange={e => setDraft({ ...draft(), relatedSection: e.currentTarget.value || null })}><option value="">未选择条款 / 可先搜索目标条款</option><For each={decisionOptions()}>{s => <option value={s.id}>{s.title} · {s.path}</option>}</For></select><span class="small muted">当前选项来自检索、详情及主责入口，最多200项；正式承载位置仍需核实。</span></label><div class="actions"><Show when={draft().id&&draft().confirmed}><button disabled={busy()||!draft().reason.trim()||!draft().scope.trim()} onClick={()=>void run(async()=>{const previous=draft().id!;const record=await bridge.replaceDecision({...draft(),id:null,confirmed:true},previous);apply(await bridge.snapshot());setDraft({...draft(),id:record.id});setMessage('已建立替代决定；原决定保留在历史，新决定同步义务重新核实。');})}>建立新决定并替代原决定</button></Show><For each={[[false, '保存草稿'], [true, '记录已决定']] as const}>{([confirmed, label]) => <button disabled={busy() || !snapshot()?.project || !draft().content.trim() || (!confirmed && draft().confirmed) || (confirmed && (!draft().reason.trim() || !draft().scope.trim()))} onClick={() => void run(async () => { const record = await bridge.saveDecision({ ...draft(), confirmed }); apply(await bridge.snapshot()); setDraft({ ...draft(), id: record.id, confirmed: record.confirmedAt !== null }); setMessage(confirmed ? '明确决定已保存；正式规范与实现同步仍需核实。' : '草稿已保存；正式规范保持原有状态。'); })}>{label}</button>}</For></div></form></section></div><section class="panel"><div class="panel-title"><h3>个人决定与同步状态</h3><button onClick={()=>void run(async()=>{setDecisionHistory(await bridge.decisionHistory(''));setHistoryDecision('当前范围全部已存修订（含旧版冲突保留）');})}>查看旧版冲突与修订历史</button><button disabled={busy() || !snapshot()?.project} onClick={() => void run(async () => { apply(await bridge.rebuildCache()); setMessage('来源缓存已重建，个人决定与阅读位置保留。'); })}>重建来源缓存</button></div><For each={decisions()}>{d => <div class="row"><div class="row-content"><button class="row-title" disabled={d.state==='superseded'} onClick={() => setDraft({ id: d.id, content: d.content, reason: d.reason, scope: d.scope, relatedSection: d.relatedSection, confirmed: d.confirmedAt !== null })}>{d.content}</button><p class="muted">理由：{d.reason || '尚未填写'} · 范围：{d.scope || '尚未填写'}</p><div class="metadata"><span>正式规范：{d.formalUpdate === 'notVerified' ? '尚未核实' : d.formalUpdate}</span><span>实现同步：{d.implementationSync === 'notVerified' ? '尚未核实' : d.implementationSync}</span><span>{d.confirmedAt ? `确认时间 ${observedTime(d.confirmedAt)}` : '未确认'}</span></div></div><div><span class="tag">{d.state==='superseded'?'已被替代 · 历史':d.confirmedAt?'用户已决定':'草稿'}</span><Show when={d.supersedes}><p class="small">替代原决定 {decisions().find(old=>old.id===d.supersedes)?.content??d.supersedes}</p></Show><button onClick={()=>void run(async()=>{const entries=await bridge.decisionHistory(d.id);setDecisionHistory(entries);setHistoryDecision(d.content);})}>查看修订历史</button></div></div>}</For><Show when={historyDecision()}><details open class="panel-body"><summary>{historyDecision()} · 历史修订（最多1000项）</summary><For each={decisionHistory()}>{d=><p>{d.content}<br/>理由：{d.reason} · 范围：{d.scope} · 原确认 {observedTime(d.confirmedAt)}<button onClick={()=>setDraft({id:null,content:d.content,reason:d.reason,scope:d.scope,relatedSection:d.relatedSection,confirmed:false})}>从此历史建立新草稿</button></p>}</For><Show when={!decisionHistory().length}><p>当前没有更早的修订；替代关系保留在各决定上。</p></Show></details></Show><Show when={!decisions().length}><div class="empty">尚无个人决定。</div></Show></section></Show>
    <Show when={view() === 'execution'&&!workSelected()}><details class="panel"><summary>Git 与 GitHub 来源检查 / 连接</summary><section class="panel"><div class="panel-title"><h3>Git · 分支与工作树</h3><span class="tag">只读事实</span></div><div class="panel-body"><pre>{snapshot()?.git.status.replaceAll('\0', '\n') || 'Git 尚未读取。'}</pre><details><summary>工作树范围</summary><pre>{snapshot()?.git.worktrees.replaceAll('\0', '\n') || '尚未读取'}</pre></details></div></section><section class="panel"><div class="panel-title"><h3>GitHub · 工作目的与交付</h3><span class={`tag ${github().state}`}>{statusLabel(github().state)}</span></div><div class="panel-body"><p class="muted">{github().coverage}</p><div class="metadata"><span>账号 {github().account ?? '未知'}</span><span>仓库 {github().repository ?? '未读取'}</span><span>最近成功 {observedTime(github().observedAt)}</span></div><div class="project-form"><input aria-label="可选 gh 可执行文件绝对路径" placeholder="可选：/opt/homebrew/bin/gh" value={ghPath()} onInput={e => setGhPath(e.currentTarget.value)} /><button disabled={sourceBusy() || !snapshot()?.project} onClick={() => void readSources(async () => { setGithub(await bridge.refreshGithub(ghPath().trim() || null)); setGithubPage(1); })}>只读连接 / 刷新 GitHub</button></div><Show when={github().error}><p class="notice">{github().error}</p></Show><div class="actions"><button disabled={sourceBusy()||!github().repository||githubPage()>=10} onClick={()=>void readSources(async()=>{const next=githubPage()+1;setGithub(await bridge.readGithubWindow(next,null,ghPath().trim()||null));setGithubPage(next);})}>继续读取第 {githubPage()+1} 页（最多一次一页）</button><label>已知 PR 编号<input inputmode="numeric" value={knownPr()} onInput={e=>setKnownPr(e.currentTarget.value)}/></label><button disabled={sourceBusy()||!/^\d+$/.test(knownPr())||Number(knownPr())<1||!github().repository} onClick={()=>void readSources(async()=>{setGithub(await bridge.readGithubWindow(null,Number(knownPr()),ghPath().trim()||null));})}>按原身份补读此 PR</button></div></div><For each={github().pulls}>{pr => <div class="row"><div class="row-content"><button class="row-title" onClick={() => void run(async () => bridge.openSource(pr.url))}>#{pr.id} {pr.title}</button><div class="metadata"><span>PR {pr.state}</span><span>评审 {pr.reviewDecision ?? '未知'}</span><span>HEAD {pr.headOid}</span></div></div><span class="tag">源事实</span></div>}</For><Show when={github().state !== 'unconnected' && !github().error && !github().pulls.length}><div class="empty">读取成功；本次读取窗口没有 PR。</div></Show></section></details></Show>

    <div hidden={view() !== 'tools'}><LocalTools project={snapshot()?.project ?? undefined}/></div>
    <Show when={selected()}>{s => <section ref={sourceDetail} tabindex="-1" class="panel detail source-inspector" aria-label="来源详情"><div class="panel-title"><h3>{s().title}</h3><div class="actions"><button onClick={() => void run(async () => bridge.openSource(s().path))}>打开当前文件</button><button onClick={() => { ++sourceIntent;setSelected(undefined);setSourcePosition(null); const target=sourceOrigin?.isConnected?sourceOrigin:Array.from(document.querySelectorAll<HTMLElement>('[data-source-ref]')).find(e=>e.dataset.sourceRef===sourceOriginId);if(target){target.focus({preventScroll:true});target.scrollIntoView?.({block:'nearest'});} }}>关闭详情</button></div></div><div class="panel-body"><MarkdownText text={s().content} sourcePath={s().path} sections={sections()} showSource={showSource}/><details><summary>来源定位、版本与诊断</summary><dl><dt>来源</dt><dd>{s().path}:{s().line}</dd><dt>章节 ID</dt><dd>{s().id}</dd><dt>性质</dt><dd>源事实 · 已读取原文；实现验收未知</dd><dt>工作版本</dt><dd>{s().revision} · {s().dirty ? '未提交工作树内容' : '与提交内容一致'}</dd><dt>提交基线</dt><dd>{s().committedRevision}</dd><dt>覆盖</dt><dd>当前单个索引章节。已保存阅读版本可恢复；后继仅按文档明确声明核对。</dd></dl><SourceSuccession source={s()} sections={sections()} showSource={showSource} /></details><details><summary>原始章节文本</summary><pre ref={sourceContent} tabindex="0" aria-label="来源章节原文" onWheel={()=>{sourceScrollIntent=true;}} onTouchMove={()=>{sourceScrollIntent=true;}} onKeyDown={()=>{sourceScrollIntent=true;}} onScroll={event=>{const top=event.currentTarget.scrollTop;const passive=!sourceScrollIntent&&programmaticSourceScroll!==null&&Math.abs(top-programmaticSourceScroll)<1;programmaticSourceScroll=null;setSourcePosition({section:s(),scrollTop:top,userInitiated:!passive});}}>{s().content}</pre></details></div></section>}</Show><footer><Show when={props.preview}>布局演示：所有工作与结果均为示例，不连接仓库、模型或本机数据。<br/></Show>事实来自明确来源 · 未知范围保持可见 · 不从文档状态推导实现完成</footer></main></div></div>;
}
