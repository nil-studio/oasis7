import { createEffect, createMemo, createSignal, For, on, Show } from 'solid-js';
import { bridge, errorMessage, observedTime } from './bridge';
import DocumentRelations from './DocumentRelations';
import DocumentDirectory from './DocumentDirectory';
import DocumentSemanticTree from './DocumentSemanticTree';
import DocumentNavigation from './DocumentNavigation';
import DocumentAllocations from './DocumentAllocations';
import { readingNotice, type TreeFileRestore, type TreeSourcePosition } from './documentTreeReading';
import type { Snapshot, DocumentSection } from './types';
import type { DocumentInventoryPage, IndexedDocument } from './generated/core';

const reasons: Record<string, string> = {
  deleted: '工作树文件已删除', permission_denied: '没有读取权限',
  read_error: '读取失败', path_unavailable: '路径不可用', invalid_utf8: '不是有效 UTF-8',
  file_count_budget: '超过 3000 文件读取预算', file_budget: '超过单文件 1 MiB',
  total_budget: '超过总量 16 MiB', symlink: '符号链接未读取',
  not_regular_file: '不是普通文件', unsupported_confined_read: '当前平台不支持受限读取',
};
const states: Record<string, string> = { read: '已读取', skipped: '已跳过', deleted: '已删除', error: '读取错误' };
const categories: Record<string, string> = { proposed: '提议', draft: '草案', active: '现行', superseded: '已替代', retired: '已退役', other: '其他声明', unknown: '未声明' };
const identityIssues: Record<string, string> = {
  'multiple document ID declarations': '存在多条声明，未选定 ID',
  'document ID declaration exceeds 512 bytes': '声明超过 512 字节，仅保留原文样本',
  'multiline document ID declaration': '声明跨多行，未选定 ID',
  'invalid document ID declaration': 'ID 为空或格式不支持，未选定 ID',
};
const kinds: Record<string, string> = { entry: '结构入口', prd: '需求', design: '设计', other: '其他文档' };

export default function DocumentInventory(props: {
  snapshot: Snapshot;
  showSource: (section: DocumentSection, origin?: HTMLElement, restore?: {scrollTop:number;passive:boolean}) => void;
  sourcePosition?:TreeSourcePosition|null;
}) {
  const [view, setView] = createSignal('product');
  const [treeActivated,setTreeActivated] = createSignal(false);
  const [lastViewNotice,setLastViewNotice] = createSignal('');
  const [localSource,setLocalSource] = createSignal<TreeSourcePosition|null>(null);
  let viewIntent=0;
  let viewGeneration=0;
  const [page, setPage] = createSignal<DocumentInventoryPage>();
  const [selected, setSelected] = createSignal<IndexedDocument>();
  const [query, setQuery] = createSignal('');
  const [filter, setFilter] = createSignal('all');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal('');
  const [composing, setComposing] = createSignal(false);
  let generation = 0;
  let applied = { query: '', filter: 'all' };
  let list: HTMLDivElement | undefined;
  let detail: HTMLElement | undefined;
  let selectionOrigin: HTMLElement | undefined;
  let fileRestore:TreeFileRestore|undefined;
  function selectFile(file?: IndexedDocument, origin?: HTMLElement, restore?:TreeFileRestore) {
    selectionOrigin = origin;fileRestore=restore;
    setSelected(file);
    if(!file)setLocalSource(null);
  }
  function openSection(section:DocumentSection,origin?:HTMLElement) {
    setLocalSource({section,scrollTop:0,userInitiated:true});
    props.showSource(section,origin);
  }
  function closeFile() {
    const origin = selectionOrigin;
    selectFile(undefined);
    if (origin?.isConnected) { origin.focus(); origin.scrollIntoView?.({block:'nearest'}); }
  }
  createEffect(on(selected, file => {
    if (!file) return;
    queueMicrotask(() => {
      if (selected() !== file || !detail?.isConnected) return;
      if(!fileRestore?.passive){detail.focus();detail.scrollIntoView?.({block:'start'});}
      if(fileRestore?.section) {
        setLocalSource({section:fileRestore.section,scrollTop:fileRestore.scrollTop,userInitiated:false});
        props.showSource(fileRestore.section,selectionOrigin,{scrollTop:fileRestore.scrollTop,passive:fileRestore.passive});
      }
    });
  }));

  function browse(mode: string) {
    ++viewIntent;++generation;setTreeActivated(mode==='product'||mode==='professional');setBusy(false);setSelected(undefined);setView(mode);
    if (mode === 'files' && !page()) void load();
  }
  async function load(next = false) {
    const projectId = props.snapshot.project?.id;
    if (!projectId) return;
    const request = ++generation;
    const current = page();
    if (!next) {
      applied = { query: query().trim(), filter: filter() };
      setPage(undefined);
      setSelected(undefined);
    }
    setBusy(true); setError('');
    try {
      const result = await bridge.documentInventory({
        schemaVersion: 1, projectId,
        snapshotId: next ? current?.snapshotId ?? null : null,
        query: applied.query, readFilter: applied.filter,
        cursor: next ? current?.nextCursor ?? null : null,
      });
      if (request === generation && props.snapshot.project?.id === projectId) {
        setPage(result); setSelected(undefined);
        if (next) queueMicrotask(() => list?.focus());
      }
    } catch (failure) {
      if (request === generation) setError(errorMessage(failure));
    } finally { if (request === generation) setBusy(false); }
  }
  const sourceIdentity = createMemo(() => JSON.stringify([
    props.snapshot.project?.id, props.snapshot.sources.find(s => s.kind === 'local')?.generation,
  ]));
  createEffect(on(
    sourceIdentity,
    () => {
      ++generation; setPage(undefined); setSelected(undefined); setQuery(''); setFilter('all'); setError('');
      if (props.snapshot.project && view() === 'files') void load();
    },
  ));
  createEffect(on(()=>props.snapshot.project?.id,()=>{
    const project=props.snapshot.project;const token=++viewGeneration;const intent=viewIntent;
    setLastViewNotice('');if(!project)return;
    void bridge.loadDocumentTreeLastView({schemaVersion:1,projectId:project.id,baselineMode:'tracked-working-tree',baselineOid:project.head}).then(result=>{
      if(token!==viewGeneration||intent!==viewIntent||props.snapshot.project?.id!==project.id)return;
      if(result.notice)setLastViewNotice(readingNotice(result.notice));
      if(result.status==='ready'&&(result.view==='product'||result.view==='professional')){++generation;setSelected(undefined);setTreeActivated(false);setView(result.view);}
    }).catch(failure=>{if(token===viewGeneration&&intent===viewIntent)setLastViewNotice(`最近树视图未载入：${errorMessage(failure)}`);});
  }));
  const chapters = () => {
    const file = selected();
    return file?.state === 'read' ? props.snapshot.sections.filter(s => s.path === file.path && s.revision === file.revision) : [];
  };
  return <section class="panel" aria-label="文档文件清单">
    <div class="panel-title"><h3>文档导航与读取缺口</h3><Show when={view() === 'files'}><button disabled={busy()} onClick={() => void load()}>重新查询首页</button></Show></div>
    <div class="panel-body" classList={{'tree-reader-active':(view()==='product'||view()==='professional')&&!!selected()}}>
      <Show when={lastViewNotice()}><p role="status" class="notice">{lastViewNotice()}</p></Show>
      <details class="inventory-scope"><summary>文档范围与关系说明</summary><p class="muted small">仅列当前工作树纳入索引的 tracked Markdown，未跟踪文件不在此范围。按文件去重；读取状态不代表设计或实现完成。详情展示文件头或产品身份章节的显式配对设计和设计上游声明；可打开已解析的目标章节并返回。产品树和专业系统树按显式入口与导航组织，配对保留为详情关系；同一文件可出现在多个位置。</p></details>
      <div class="actions" role="group" aria-label="文档浏览方式"><button aria-pressed={view() === 'files'} onClick={() => browse('files')}>文件清单</button><button aria-pressed={view() === 'directory'} onClick={() => browse('directory')}>文档目录</button><button aria-pressed={view() === 'product'} onClick={() => browse('product')}>产品树</button><button aria-pressed={view() === 'professional'} onClick={() => browse('professional')}>专业系统树</button></div>
      <Show when={view() === 'directory'}><DocumentDirectory snapshot={props.snapshot} selectFile={selectFile} /></Show>
      <Show when={view() === 'product' || view() === 'professional'}><DocumentSemanticTree snapshot={props.snapshot} view={view() === 'product' ? 'product' : 'professional'} selectedFile={selected()} sourcePosition={props.sourcePosition === undefined ? localSource() : props.sourcePosition} userActivated={treeActivated()} selectFile={selectFile} /></Show>
      <Show when={view() === 'files'}>
      <form class="inventory-filters" onSubmit={event => { event.preventDefault(); if (!composing()) void load(); }}>
        <label>标题、路径或文档 ID<input aria-label="筛选文档文件" value={query()} onInput={e => setQuery(e.currentTarget.value)} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={e => { if (composing() || e.isComposing || e.keyCode === 229) { if (e.key === 'Enter') e.preventDefault(); } }} /></label>
        <label>读取结果<select aria-label="文档读取结果" value={filter()} onChange={e => setFilter(e.currentTarget.value)}><option value="all">全部文件</option><option value="read">已读取</option><option value="omitted">仅看遗漏</option></select></label>
        <button disabled={busy() || composing()} type="submit">应用筛选</button>
      </form>
      <Show when={busy()}><p role="status">正在读取文件清单…</p></Show>
      <Show when={error()}><div class="notice" role="alert">{error()}<p>可重新查询；若清单缺失或损坏，请刷新本地事实。个人记录保持保留。</p></div></Show>
      <Show when={page()}>{result => <>
        <p role="status">候选 {result().candidates} · 已读 {result().read} · 遗漏 {result().omitted} · 已读 {result().readBytes} 字节 · 筛选命中 {result().totalMatches}</p>
        <p class="muted small">工作树预览 · 基线 {result().baselineOid} · 生成 {observedTime(result().generatedAt)} · {result().partial ? '存在读取缺口' : '本次候选全部读取'}</p>
        <Show when={result().stale}><div class="notice" role="status">清单已过期：保留最近一次成功扫描的文件记录。{result().error}</div></Show>
        <div class="inventory-list" ref={list} tabindex="-1" aria-label="当前页文档文件">
          <For each={result().files}>{file => <div class="row"><div class="row-content">
            <button class="row-title" aria-pressed={selected()?.fileKey === file.fileKey} onClick={event => selectFile(file,event.currentTarget)}>{file.title}</button>
            <div class="metadata"><span>{file.path}</span><span>{kinds[file.kind] ?? file.kind}</span><span>{states[file.state] ?? file.state}</span><span>{file.dirty ? '工作树有修改' : '未检测到修改'}</span></div>
            <Show when={file.identity?.declaredId}><p class="muted small">文档 ID：{file.identity?.declaredId}<Show when={file.identity?.duplicateFiles}> · 相同 ID 另有 {file.identity?.duplicateFiles} 个已读文件</Show></p></Show>
            <Show when={file.reason}><p class="muted small">{reasons[file.reason!] ?? file.reason}</p></Show>
          </div></div>}</For>
          <Show when={!result().files.length}><p class="empty">当前筛选没有匹配文件；这不代表仓库没有文档。</p></Show>
        </div>
        <div class="actions"><button disabled={busy() || !result().nextCursor} onClick={() => void load(true)}>下一页（最多 100 项）</button></div>
      </>}</Show>
      </Show>
      <Show when={selected()}>{file => <section class="inventory-detail" aria-label="文件读取详情" ref={detail} tabindex="-1"><div class="actions"><button onClick={closeFile}>返回文档树与列表</button></div><h4>{file().path}</h4>
        <p>{states[file().state] ?? file().state} · {file().reason ? reasons[file().reason!] ?? file().reason : `${file().bytes ?? 0} 字节`}</p>
        <For each={chapters()}>{section => <p><button onClick={e => openSection(section, e.currentTarget)}>{section.title} · 第 {section.line} 行</button></p>}</For>
        <details><summary>文件身份与解析诊断</summary><Show when={file().revision}><p class="muted small">内容摘要 {file().revision}</p></Show>
        <Show when={file().lifecycle}><p class="muted small">生命周期原声明：{file().lifecycle?.rawStatus ?? file().lifecycle?.status ?? '未声明'} · 类别 {categories[file().lifecycle?.category ?? 'unknown'] ?? '未分类'}<Show when={file().lifecycle?.issue}> · {file().lifecycle?.issue}</Show></p></Show>
        <Show when={file().state === 'read'}>
          <section aria-label="文档身份声明"><h5>文档身份声明</h5>
            <p class="muted small">仅解析文件头顶层“文档 ID”字段，ID 不决定文件身份、权威或实现状态。重复核对仅覆盖本次已读文件，区分大小写；搜索忽略大小写。</p>
            <Show when={file().identity} fallback={<p>旧清单尚未解析文档 ID，请刷新本地事实。</p>}>{identity => <>
              <Show when={identity().declarationLine} fallback={<p>本次文件头范围未发现文档 ID 声明。</p>}><p>第 {identity().declarationLine} 行 · 原声明样本：{identity().raw}</p></Show>
              <Show when={identity().issue}><p role="status">文档 ID 解析不完整：{identityIssues[identity().issue!] ?? identity().issue}</p></Show>
              <Show when={identity().duplicateFiles}><p role="status">重复文档 ID：相同 ID 另有 {identity().duplicateFiles} 个已读文件；文件仍分别保留。</p><p class="muted small">路径样本 {identity().duplicatePaths.length} / {identity().duplicateFiles}（最多 20 条）</p><ul><For each={identity().duplicatePaths}>{path => <li>{path}</li>}</For></ul></Show>
            </>}</Show>
          </section>
        </Show></details><Show when={file().state === 'read'}><DocumentNavigation navigation={file().navigation} sections={props.snapshot.sections} showSource={openSection} />
          <DocumentRelations sourceTitle={file().path} relations={file().pairings} mode={file().path.startsWith('doc/product/') ? 'product_pairing' : 'pairing'} sections={props.snapshot.sections} showSource={openSection} />
          <Show when={file().kind === 'design'}><DocumentRelations relations={file().upstream} mode="upstream" sections={props.snapshot.sections} showSource={openSection} /><DocumentAllocations allocations={file().allocations} sections={props.snapshot.sections} showSource={openSection} /></Show>
        </Show>
        <Show when={file().state === 'read' && !chapters().length}><p>已读取，文件没有可展示的章节。</p></Show>
        <button onClick={closeFile}>收起文件详情</button>
      </section>}</Show>
    </div>
  </section>;
}
