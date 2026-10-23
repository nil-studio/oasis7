import { Search } from 'lucide-solid';
import { NavigationIcon } from './VisualStatus';
import type { TreeSourcePosition } from './documentTreeReading';
import { batch, createEffect, createSignal, For, onMount, Show } from 'solid-js';
import { acceptsEnter, bridge, connected, errorMessage, observedTime, statusLabel } from './bridge';
import type { DocumentSection, Snapshot } from './types';
import MarkdownText from './MarkdownText';
import SearchResults from './SearchResults';
import SourceSearchMatches from './SourceSearchMatches';
import { documentSearchName } from './SearchText';
import LocalTools from './LocalTools';
import DocumentInventory from './DocumentInventory';
import SourceSuccession from './SourceSuccession';

const views = [['tools', '本机工具'], ['documents', '本地文档预览']] as const;

export default function App(props: { preview?: boolean } = {}) {
  const [navigationOpen,setNavigationOpen]=createSignal(false);
  const [searchOpen,setSearchOpen]=createSignal(false);
  const [showConnections,setShowConnections]=createSignal(false);
  const [displayScale,setDisplayScale] = createSignal((()=>{try{const value=Number(localStorage.getItem('oasis7.display-scale'));return [1,1.5,2].includes(value)?value:1;}catch{return 1;}})());
  createEffect(()=>document.documentElement.style.setProperty('--display-scale',String(displayScale())));
  function resizeDisplay(value:number){setDisplayScale(value);try{localStorage.setItem('oasis7.display-scale',String(value));}catch{setMessage('显示大小已应用；本机偏好未保存，重开后将使用默认大小。');}}
  const [snapshot, setSnapshot] = createSignal<Snapshot>();
  const [view, setView] = createSignal('tools');
  const [path, setPath] = createSignal('');
  const [query, setQuery] = createSignal('');
  const [resultQuery,setResultQuery]=createSignal('');
  const [selectedQuery,setSelectedQuery]=createSignal('');
  const [results, setResults] = createSignal<DocumentSection[] | null>(null);
  const [selected, setSelected] = createSignal<DocumentSection>();
  const [busy, setBusy] = createSignal(false);
  const [sourceBusy,setSourceBusy]=createSignal(false);
  async function readSources(action:()=>Promise<void>){if(sourceBusy())return;setSourceBusy(true);try{await action();}catch(e){setMessage(errorMessage(e));}finally{setSourceBusy(false);}}
  const [message, setMessage] = createSignal('');
  const [composing, setComposing] = createSignal(false);
  async function openProject(next:Snapshot){++searchGeneration;setResults(null);setSelected(undefined);apply(next);}
  const projectInputs = new Map<string, { query: string }>();
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
        if (previousId) projectInputs.set(previousId, { query: query() });
        const inputs = nextId ? projectInputs.get(nextId) : undefined;
        ++searchGeneration;
        setResults(null);
        setSelected(undefined);
        sourceOrigin = undefined;sourceOriginId='';setSourcePosition(null);
        setQuery(inputs?.query ?? '');
      }
      setSnapshot(next);
      setPath(next.project?.path ?? next.preferences.projectPath ?? '');
    });
  }
  onMount(() => { if (connected() || props.preview) void run(async () => {
    const next = await bridge.snapshot(); apply(next);
    setView(next.preferences.view === 'documents' || next.preferences.view === 'rules' ? 'documents' : 'tools');
    setQuery(next.preferences.query);
    const reading = next.readingPositions.at(-1);
    const sourceToken=sourceIntent;
    let treeOwnsReading=false;
    if(next.project&&(next.preferences.view==='documents'||next.preferences.view==='rules')){
      try{const recent=await bridge.loadDocumentTreeLastView({schemaVersion:1,projectId:next.project.id,baselineMode:'tracked-working-tree',baselineOid:next.project.head});treeOwnsReading=recent.status==='ready'&&(recent.view==='product'||recent.view==='professional');}
      catch{/* The document panel reports the repairable preference error. */}
    }
    if(reading&&!treeOwnsReading&&sourceToken===sourceIntent){const restored=next.sections.find(s=>s.id===reading.sectionId&&s.revision===reading.revision)??await bridge.readVersion(reading.sectionId,reading.revision)??undefined;if(sourceToken===sourceIntent)setSelected(restored);}
  }); });
  async function persistPreference(nextView = view(), nextQuery = query()) {
    if (!connected()) return;
    try { await bridge.savePreferences({ projectPath: snapshot()?.project?.path ?? null, view: nextView, query: nextQuery }); }
    catch (error) { setMessage(`阅读偏好未保存：${errorMessage(error)}`); }
  }
  function changeView(key: string) { setNavigationOpen(false);setView(key); void persistPreference(key); }
  async function search() {
    const generation = ++searchGeneration;
    const submitted=query().trim();
    if (!submitted) { setResults(null); return; }
    try {
      const found = await bridge.search(submitted);
      if (generation === searchGeneration) { batch(()=>{setResults(found);setResultQuery(submitted);}); void persistPreference(); }
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
    setSelectedQuery('');sourceScrollIntent=false;setSelected(section);setSourcePosition({section,scrollTop:restore?.scrollTop??0,userInitiated:!restore?.passive});
    queueMicrotask(() => { if(selected()!==section)return;if(sourceContent){void sourceContent.clientHeight;sourceContent.scrollTop=restore?.scrollTop??0;programmaticSourceScroll=sourceContent.scrollTop;}if(!restore?.passive){sourceDetail?.focus({preventScroll:true});sourceDetail?.scrollIntoView?.({block:'start'});} });
    if(!restore?.passive)void bridge.recordReading(section).catch(error => setMessage(`阅读位置未保存：${errorMessage(error)}`));
  }
  const sections = () => snapshot()?.sections ?? [];
  const heading = () => views.find(([key]) => key === view())?.[1] ?? '本机工具';
  const description = () => view() === 'tools' ? '管理当前电脑的资料与本机工具，核对真实结果。' : '预览当前工作树的 tracked Markdown，包括未提交修改。';
  return <div class="app-frame" ><header><div><h1>Oasis7 本机工具</h1></div><div class="header-context"><strong>{snapshot()?.project ? 'oasis7 · 本地项目' : '先打开你的本地项目'}</strong><div class="muted small">{snapshot()?.project ? snapshot()!.project!.environment : '选择文件夹后即可预览本地文档'}</div></div><button onClick={()=>setShowConnections(!showConnections())} aria-expanded={showConnections()}>项目与来源</button><span class="tag desktop-state">{props.preview?'演示数据 · 非真实状态':connected() ? '桌面核心' : '浏览器预览 · 未接入'}</span></header>
    <div class="shell"><aside classList={{'navigation-open':navigationOpen()}}><button class="navigation-toggle" aria-expanded={navigationOpen()} aria-controls="main-navigation" onClick={()=>setNavigationOpen(!navigationOpen())}>导航 · {heading()}</button><nav id="main-navigation" aria-label="主视图"><For each={views}>{([key, label]) => <button aria-current={view() === key ? 'page' : undefined} onClick={() => changeView(key)}><NavigationIcon view={key}/>{label}</button>}</For></nav><div class="side-caption">来源 / 读取范围</div><For each={snapshot()?.sources ?? []}>{source => <div class="source-status"><span>{source.kind}</span><span class={`tag ${source.state}`}>{statusLabel(source.state)}</span></div>}</For><div class="side-caption">环境</div><div class="panel-body small muted">{snapshot()?.project?.environment ?? '未选择项目'}<br />{snapshot()?.project?.branch ?? '分支尚未读取'}</div></aside>
    <main><div class="toolbar"><div><div class="eyebrow">LOCAL TOOLS / {views.findIndex(([key]) => key === view()) + 1}</div><h2>{heading()}</h2><p class="muted">{description()}</p></div><div class="global-search"><button class="search-toggle" aria-expanded={searchOpen()} aria-controls="global-document-search" onClick={()=>setSearchOpen(!searchOpen())}><Search size={18} aria-hidden="true"/>搜索文档</button><div id="global-document-search" class="search" classList={{'is-open':searchOpen()}} role="search"><input aria-label="搜索规范、标题、路径或条款 ID" placeholder="搜索规范、合入、条款 ID…" value={query()} onInput={e => { setQuery(e.currentTarget.value); ++searchGeneration; }} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={e => { if (acceptsEnter(e, composing())) { e.preventDefault(); void search(); } }} /><button disabled={!connected() || composing()} onClick={() => void search()}>搜索</button></div></div></div>
    <Show when={message()}><div class="notice" role="status">{message()}</div></Show>
    <Show when={!connected()&&!props.preview}><div class="notice">浏览器预览未接入桌面 IPC。这里不展示模拟项目、PR 或完成状态。请启动 macOS App 读取真实仓库。</div></Show>
    <details class="panel project-entry" hidden={!!snapshot()?.project&&!showConnections()} open={!snapshot()?.project||showConnections()}><summary>{snapshot()?.project?'项目文件夹与读取设置':'打开本地项目'}</summary><div class="panel-body"><div class="display-settings"><label>文字与界面<select aria-label="文字与界面大小" value={displayScale()} onChange={e=>resizeDisplay(Number(e.currentTarget.value))}><option value="1">100%</option><option value="1.5">150%</option><option value="2">200%</option></select></label><button onClick={()=>resizeDisplay(1)}>重置</button></div><h3>{snapshot()?.project?'当前项目':'从你的项目开始'}</h3><p class="muted">选择 oasis7 工作树，预览本地文档和未提交修改。私有资料仍从本机工具进入。</p><div class="actions"><button class="primary" type="button" disabled={sourceBusy()||busy()||!connected()} onClick={()=>void readSources(async()=>{const next=await bridge.chooseProject();if(next)await openProject(next);})}>{sourceBusy()?'正在读取…':'选择项目文件夹'}</button><Show when={snapshot()?.project}><button disabled={sourceBusy()} onClick={()=>void readSources(async()=>apply(await bridge.refresh()))}>刷新本地文档与 Git</button></Show></div><details><summary>手动输入文件夹路径</summary><form class="project-form" onSubmit={e=>{e.preventDefault();void readSources(async()=>openProject(await bridge.openProject(path().trim())));}}><input aria-label="本地仓库绝对路径" placeholder="/Users/…/oasis7" value={path()} onInput={e=>setPath(e.currentTarget.value)}/><button disabled={sourceBusy()||busy()||!connected()||!path().trim()} type="submit">打开项目</button></form></details><Show when={snapshot()?.project}>{project=><div class="metadata"><span>{project().path}</span><span>环境 {project().environment}</span><span>分支 {project().branch??'detached HEAD'}</span><span>HEAD {project().head}</span></div>}</Show></div></details>
    <details class="source-notices" hidden={!showConnections()}><summary>来源状态与读取范围</summary><For each={snapshot()?.sources.filter(s => s.error || /遗漏\s*[1-9]|omitted[^0]*[1-9]/i.test(s.coverage))}>{s => <div class="notice"><strong>{s.kind} · {statusLabel(s.state)}</strong>　{s.error ?? s.coverage}<div class="small">最近成功：{observedTime(s.lastSuccess)} · {s.coverage}</div></div>}</For></details>
    <Show when={results() !== null}><SearchResults items={results()??[]} query={resultQuery()} onOpen={(s,origin)=>{showSource(s,origin);setSelectedQuery(resultQuery());}} onEdit={()=>{setSearchOpen(true);queueMicrotask(()=>document.querySelector<HTMLInputElement>('.global-search input')?.focus());}} onClear={()=>{++searchGeneration;setResults(null);setQuery('');setSelected(undefined);setSelectedQuery('');}} /></Show>
    <Show when={view() === 'documents' && snapshot()?.project}><DocumentInventory snapshot={snapshot()!} showSource={showSource} sourcePosition={sourcePosition()} /><button disabled={busy()} onClick={()=>void run(async()=>{apply(await bridge.rebuildCache());setMessage('本地文档缓存已重建，阅读位置保留。');})}>重建本地文档缓存</button></Show>
    <div hidden={view() !== 'tools'}><LocalTools project={snapshot()?.project ?? undefined}/></div>
    <Show when={selected()}>{s => <section ref={sourceDetail} tabindex="-1" class="panel detail source-inspector" aria-label="来源详情"><div class="panel-title"><h3>{s().title}</h3><div class="actions"><button onClick={() => void run(async () => bridge.openSource(s().path))}>打开当前文件</button><button onClick={() => { ++sourceIntent;setSelected(undefined);setSourcePosition(null); const target=sourceOrigin?.isConnected?sourceOrigin:Array.from(document.querySelectorAll<HTMLElement>('[data-source-ref]')).find(e=>e.dataset.sourceRef===sourceOriginId);if(target){target.focus({preventScroll:true});target.scrollIntoView?.({block:'nearest'});} }}>关闭详情</button></div></div><div class="panel-body"><p class="source-document-name">{documentSearchName(s().path)}</p><SourceSearchMatches query={selectedQuery()} identity={`${s().id}:${s().revision}`}><MarkdownText text={s().content} searchQuery={selectedQuery()} sourcePath={s().path} sections={sections()} showSource={showSource}/></SourceSearchMatches><details><summary>来源定位、版本与诊断</summary><dl><dt>来源</dt><dd>{s().path}:{s().line}</dd><dt>章节 ID</dt><dd>{s().id}</dd><dt>性质</dt><dd>源事实 · 已读取原文；实现验收未知</dd><dt>工作版本</dt><dd>{s().revision} · {s().dirty ? '未提交工作树内容' : '与提交内容一致'}</dd><dt>提交基线</dt><dd>{s().committedRevision}</dd><dt>覆盖</dt><dd>当前单个索引章节。已保存阅读版本可恢复；后继仅按文档明确声明核对。</dd></dl><SourceSuccession source={s()} sections={sections()} showSource={showSource} /></details><details><summary>原始章节文本</summary><pre ref={sourceContent} tabindex="0" aria-label="来源章节原文" onWheel={()=>{sourceScrollIntent=true;}} onTouchMove={()=>{sourceScrollIntent=true;}} onKeyDown={()=>{sourceScrollIntent=true;}} onScroll={event=>{const top=event.currentTarget.scrollTop;const passive=!sourceScrollIntent&&programmaticSourceScroll!==null&&Math.abs(top-programmaticSourceScroll)<1;programmaticSourceScroll=null;setSourcePosition({section:s(),scrollTop:top,userInitiated:!passive});}}>{s().content}</pre></details></div></section>}</Show><footer><Show when={props.preview}>布局演示：文档为示例，不连接本机数据。<br/></Show>事实来自明确来源 · 未知范围保持可见 · 不从文档状态推导实现完成</footer></main></div></div>;
}
