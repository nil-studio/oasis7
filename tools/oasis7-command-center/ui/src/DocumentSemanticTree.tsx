import { DomainIcon } from './VisualStatus';
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { bridge, errorMessage, observedTime } from './bridge';
import type { Snapshot } from './types';
import type { DocumentSemanticTreePage, DocumentSemanticTreeEntry, DocumentTreeReadingKey, DocumentTreeReadingState, DocumentTreeReadingSelection, DocumentTreeRouteStep, IndexedDocument } from './generated/core';
import { readingNotice, sameRoute, selectedSection, beginTreeReadingWriter, queueTreeReadingWrite, type TreeFileRestore, type TreeSourcePosition } from './documentTreeReading';

const lifecycleLabels: Record<string,string> = {current:'隐藏明确历史',all:'全部（含历史）',proposed:'提议',draft:'草案',active:'现行',superseded:'已替代',retired:'已退役',other:'其他声明',unknown:'类别未知'};
const kindLabels: Record<string,string> = {all:'全部类型',prd:'需求',design:'设计',entry:'结构入口'};
const pairingLabels: Record<string,string> = {all:'全部配对状态',unpaired:'未发现显式配对',unparsed:'尚未解析配对',missing:'配对目标或章节缺失',unread:'配对目标未读取',ambiguous:'配对锚点歧义',outside_scope:'配对范围外或路径不安全',incomplete:'配对解析不完整'};
const resolutionLabels: Record<string,string> = {resolved:'目标文件已读取',fragment_unchecked:'章节尚未核对',missing:'目标已删除',missing_fragment:'目标章节缺失',ambiguous:'目标锚点歧义',unread:'目标未读取',outside_scope:'范围外或路径不安全'};
const defaults = () => ({query:'',read:'all',lifecycle:'current',kind:'all',pairing:'all'});
type Row = {id:string;parent:string;entry?:DocumentSemanticTreeEntry;level:number;position:number;size:number};
const MAX_EXPANDED = 128;
const MAX_ROWS = 5000;
const MAX_RESTORE_CALLS = 256;

export default function DocumentSemanticTree(props: {
  snapshot:Snapshot;
  view:'product'|'professional';
  selectedFile?:IndexedDocument;
  sourcePosition?:TreeSourcePosition|null;
  userActivated?:boolean;
  selectFile:(file?:IndexedDocument,origin?:HTMLElement,restore?:TreeFileRestore)=>void;
}) {
  const [levels,setLevels] = createSignal(new Map<string,DocumentSemanticTreePage>());
  const [expanded,setExpanded] = createSignal(new Set<string>());
  const [focused,setFocused] = createSignal('');
  const [selection,setSelection] = createSignal<DocumentTreeReadingSelection|null>(null);
  const [query,setQuery] = createSignal('');
  const [read,setRead] = createSignal('all');
  const [lifecycle,setLifecycle] = createSignal('current');
  const [rawStatus,setRawStatus] = createSignal('');
  const [kind,setKind] = createSignal('all');
  const [pairing,setPairing] = createSignal('all');
  const [busy,setBusy] = createSignal(false);
  const [loading,setLoading] = createSignal(new Set<string>());
  const [error,setError] = createSignal('');
  const [notice,setNotice] = createSignal('');
  const [preferenceError,setPreferenceError] = createSignal('');
  const [protectedRecord,setProtectedRecord] = createSignal(false);
  const [viewportTop,setViewportTop]=createSignal(0);
  const [heightVersion,setHeightVersion]=createSignal(0);
  const rowHeights=new Map<string,number>();
  const [composing,setComposing] = createSignal(false);
  const title = () => props.view === 'product' ? '产品树' : '专业系统树';
  const page = () => levels().get('');
  const identity = createMemo(() => JSON.stringify([props.snapshot.project?.id,props.snapshot.sources.find(s => s.kind === 'local')?.generation,props.view]));
  let generation = 0;
  let interaction = 0;
  let snapshotId:string|null = null;
  let key:DocumentTreeReadingKey|undefined;
  let applied = defaults();
  let restoring = true;
  let canSave = false;
  let activationConsumed = false;
  let failedParent = '';
  let list:HTMLDivElement|undefined;
  let scrollIntent = false;
  let detailScrollTop = 0;
  let writer=beginTreeReadingWriter();
  let alive=true;
  let saveIntent=0;
  let scrollTimer:ReturnType<typeof setTimeout>|undefined;
  const nodes = new Map<string,HTMLElement>();
  const rowCache = new Map<string,Row>();
  const levelRequests = new Map<string,number>();
  const rows = createMemo(() => {
    const result:Row[] = [];
    function add(row:Row){const old=rowCache.get(row.id);if(old&&old.entry===row.entry&&old.level===row.level&&old.position===row.position&&old.size===row.size)result.push(old);else{rowCache.set(row.id,row);result.push(row);}}
    function append(parent:string,level:number) {
      const p=levels().get(parent); if (!p || level>65 || result.length>=MAX_ROWS) return;
      p.entries.forEach((entry,index) => {
        if (result.length>=MAX_ROWS) return;
        add({id:entry.occurrenceId,parent,entry,level,position:index+1,size:p.totalChildren});
        if (expanded().has(entry.occurrenceId)) append(entry.occurrenceId,level+1);
      });
      if (p.nextCursor && result.length<MAX_ROWS) add({id:`${parent}:more`,parent,level,position:p.entries.length+1,size:p.totalChildren});
    }
    append('',1); return result;
  });
  const layout=createMemo(()=>{heightVersion();let total=0;const offsets=rows().map(row=>{const top=total;total+=rowHeights.get(row.id)??144;return top;});return{offsets,total};});
  const windowRange=createMemo(()=>{
    const all=rows();if(all.length<=120)return{start:0,end:all.length};
    const {offsets}=layout();const top=Math.max(0,viewportTop()-600);const bottom=viewportTop()+(list?.clientHeight||640)+600;
    let start=0;while(start<all.length-1&&offsets[start+1]<top)start++;
    let end=start;while(end<all.length&&end-start<120&&offsets[end]<bottom)end++;
    return{start,end:Math.max(start+1,end)};
  });
  const renderedRows=()=>rows().slice(windowRange().start,windowRange().end);
  const measureObserver=typeof ResizeObserver==='undefined'?undefined:new ResizeObserver(entries=>{
    if(!alive)return;let changedHeight=false;
    for(const measured of entries){const element=measured.target as HTMLElement;const id=element.dataset.treeId;const height=element.getBoundingClientRect().height;
      if(id&&height>0&&Math.abs((rowHeights.get(id)??144)-height)>1){rowHeights.set(id,height);changedHeight=true;}}
    if(changedHeight)setHeightVersion(value=>value+1);
  });
  function registerNode(id:string,element:HTMLElement){nodes.set(id,element);element.dataset.treeId=id;measureObserver?.observe(element);onCleanup(()=>{measureObserver?.unobserve(element);if(nodes.get(id)===element)nodes.delete(id);});}
  function findEntry(id:string) { for (const p of levels().values()) { const found=p.entries.find(e=>e.occurrenceId===id); if (found) return found; } }
  function controlsFromApplied() {
    setQuery(applied.query);setRead(applied.read);setKind(applied.kind);setPairing(applied.pairing);
    setLifecycle(applied.lifecycle.startsWith('raw:')?'raw':applied.lifecycle);setRawStatus(applied.lifecycle.startsWith('raw:')?applied.lifecycle.slice(4):'');
  }
  function readingState():DocumentTreeReadingState {
    return {filters:{query:applied.query,readFilter:applied.read,lifecycleFilter:applied.lifecycle,kindFilter:applied.kind,pairingFilter:applied.pairing},
      expandedRoutes:[...expanded()].map(id=>findEntry(id)?.stableRoute).filter((route):route is DocumentTreeRouteStep[]=>!!route).slice(0,MAX_EXPANDED),
      parentRoute:findEntry(focused())?.stableRoute??[],selection:selection(),treeScrollTop:list?.scrollTop??0,detailScrollTop};
  }
  function persist(flush=false) {
    if (restoring || !canSave || !key) return;
    const request=JSON.parse(JSON.stringify({key,state:readingState()}));
    const source=identity();const epoch=generation;const intent=++saveIntent;
    void queueTreeReadingWrite(writer,()=>intent===saveIntent&&(flush||(alive&&epoch===generation&&source===identity())),()=>bridge.saveDocumentTreeReading(request)).catch(failure=>{
      if (source===identity()) setPreferenceError(`阅读位置未保存：${errorMessage(failure)}`);
    });
  }
  function changed() {
    ++interaction;scrollIntent=true;
    if(restoring){++generation;levelRequests.clear();setLoading(new Set<string>());restoring=false;setBusy(false);setNotice('已停止自动恢复，保留本次手动选择。');if(!canSave)setProtectedRecord(true);}
  }
  function focusRow(id:string,move=true) {
    setFocused(id);
    const index=rows().findIndex(row=>row.id===id);
    if(move&&index>=0&&(index<windowRange().start||index>=windowRange().end)){const top=layout().offsets[index];setViewportTop(top);if(list)list.scrollTop=top;}
    if (move) queueMicrotask(()=>{const node=nodes.get(id);if (focused()===id && node?.isConnected) {node.focus({preventScroll:true});node.scrollIntoView?.({block:'nearest'});}});
  }
  async function fetchLevel(parent:string,next=false,route?:DocumentTreeRouteStep[],expected=generation) {
    const projectId=props.snapshot.project?.id;if (!projectId) return;
    const source=identity();const request=(levelRequests.get(parent)??0)+1;levelRequests.set(parent,request);
    const previous=levels().get(parent);
    setLoading(old=>new Set(old).add(parent));
    try {
      const result=await bridge.documentSemanticTree({view:props.view,parentOccurrence:parent,parentRoute:route,lifecycleFilter:applied.lifecycle,kindFilter:applied.kind,pairingFilter:applied.pairing,
        inventory:{schemaVersion:1,projectId,snapshotId,query:applied.query,readFilter:applied.read,cursor:next?previous?.nextCursor??null:null}});
      if (expected!==generation || source!==identity() || request!==levelRequests.get(parent)) return;
      snapshotId=result.snapshotId;
      const resolvedParent=result.parentOccurrence;
      const entries=next && previous?[...previous.entries,...result.entries.filter(e=>!previous.entries.some(old=>old.occurrenceId===e.occurrenceId))]:result.entries;
      const total=[...levels()].filter(([id])=>id!==resolvedParent).reduce((sum,[,p])=>sum+p.entries.length,0)+entries.length;
      if (total>MAX_ROWS) throw new Error('本次已加载树节点达到 5000 项预算；请收窄筛选后重试。');
      setLevels(old=>new Map(old).set(resolvedParent,{...result,entries}));
      if (result.routeNotice) setNotice(readingNotice(result.routeNotice));
      setError('');return result;
    } catch(failure) {if (expected===generation && source===identity()) {failedParent=parent;setError(errorMessage(failure));}return undefined;}
    finally {if (expected===generation && request===levelRequests.get(parent)) setLoading(old=>{const copy=new Set(old);copy.delete(parent);return copy;});}
  }
  async function expand(entry:DocumentSemanticTreeEntry,user=true) {
    if (!entry.hasChildren || entry.cycle || entry.depthLimited || entry.boundary) return;
    if (user) changed();
    const epoch=generation;
    if (!expanded().has(entry.occurrenceId) && expanded().size>=MAX_EXPANDED) {setNotice('已展开 128 个位置；请先折叠部分分支再展开。');return;}
    setExpanded(old=>new Set(old).add(entry.occurrenceId));
    if (!levels().has(entry.occurrenceId)) await fetchLevel(entry.occurrenceId);
    if (user && epoch===generation) persist();
  }
  function collapse(entry:DocumentSemanticTreeEntry) {changed();setExpanded(old=>{const copy=new Set(old);copy.delete(entry.occurrenceId);return copy;});focusRow(entry.occurrenceId);persist();}
  function openFile(entry:DocumentSemanticTreeEntry,passive=false,restore?:TreeFileRestore) {
    if (!entry.file) return;
    if (!passive) {changed();detailScrollTop=0;}
    setSelection({route:entry.stableRoute,path:entry.file.path,sectionPath:null,sectionId:null,sectionRevision:null});
    setFocused(entry.occurrenceId);
    props.selectFile(entry.file,nodes.get(entry.occurrenceId)?.isConnected?nodes.get(entry.occurrenceId):list,restore);
    if (!passive) persist();
  }
  async function nextPage(parent:string,user=true) {if(user)changed();const epoch=generation;await fetchLevel(parent,true);if(user&&epoch===generation)persist();}
  async function applyFilters(history=false) {
    changed();++generation;levelRequests.clear();setLoading(new Set<string>());setBusy(true);restoring=true;
    if(history){applied={...applied,lifecycle:'all'};controlsFromApplied();}
    else applied={query:query().trim(),read:read(),lifecycle:lifecycle()==='raw'?`raw:${rawStatus()}`:lifecycle(),kind:kind(),pairing:pairing()};
    setExpanded(new Set<string>());setLevels(new Map());setSelection(null);props.selectFile(undefined);setFocused('');setNotice('');
    const epoch=generation;const result=await fetchLevel('');if(epoch!==generation)return;restoring=false;setBusy(false);
    if(result){focusRow(result.entries[0]?.occurrenceId??'');persist();}
  }
  async function initialize() {
    if(scrollTimer){clearTimeout(scrollTimer);scrollTimer=undefined;}
    writer=beginTreeReadingWriter();const epoch=++generation;const intent=interaction;levelRequests.clear();snapshotId=null;key=undefined;canSave=false;restoring=true;scrollIntent=false;
    rowCache.clear();nodes.clear();rowHeights.clear();setViewportTop(0);applied=defaults();controlsFromApplied();setLevels(new Map());setExpanded(new Set<string>());setSelection(null);setFocused('');setNotice('');setError('');setPreferenceError('');setProtectedRecord(false);setBusy(true);setLoading(new Set<string>());props.selectFile(undefined);
    const root=await fetchLevel('',false,undefined,epoch);
    if(!root || epoch!==generation || intent!==interaction){if(epoch===generation){restoring=false;setBusy(false);}return;}
    key={schemaVersion:1,projectId:root.projectId,baselineMode:'tracked-working-tree',baselineOid:root.baselineOid,view:props.view};
    let state:DocumentTreeReadingState|null=null;let resolvedSectionId:string|null=null;
    try {
      const stored=await bridge.loadDocumentTreeReading({...key});
      if(epoch!==generation || intent!==interaction) return;
      canSave=stored.status==='missing'||stored.status==='ready';setProtectedRecord(!canSave);
      if(stored.notice)setNotice(readingNotice(stored.notice));state=stored.status==='ready'?stored.state:null;resolvedSectionId=stored.status==='ready'?stored.resolvedSectionId:null;
    } catch(failure){if(epoch===generation&&intent===interaction){setPreferenceError(`阅读位置未载入：${errorMessage(failure)}`);setProtectedRecord(true);}}
    if(epoch!==generation || intent!==interaction) return;
    if(state){
      applied={query:state.filters.query,read:state.filters.readFilter,lifecycle:state.filters.lifecycleFilter,kind:state.filters.kindFilter,pairing:state.filters.pairingFilter};controlsFromApplied();
      if(JSON.stringify(applied)!==JSON.stringify(defaults())) {setLevels(new Map());if(!await fetchLevel('',false,undefined,epoch)){if(epoch===generation&&intent===interaction){restoring=false;setBusy(false);}return;}}
      let calls=0;
      async function ensureRoute(route:DocumentTreeRouteStep[]):Promise<DocumentSemanticTreeEntry|undefined> {
        if(!route.length)return undefined;
        let parent='';
        for(let depth=0;depth<route.length;depth++){
          if(epoch!==generation || intent!==interaction)return;
          const prefix=route.slice(0,depth+1);
          let p=levels().get(parent);
          let entry=p?.entries.find(e=>sameRoute(e.stableRoute,prefix));
          while(!entry && p?.nextCursor && calls<MAX_RESTORE_CALLS){++calls;await fetchLevel(parent,true,undefined,epoch);p=levels().get(parent);entry=p?.entries.find(e=>sameRoute(e.stableRoute,prefix));}
          if(!entry || route[depth].duplicateCount>1){
            if(calls>=MAX_RESTORE_CALLS){setNotice('阅读恢复达到 256 次查询预算；保留已恢复祖先。');focusRow(parent,false);return;}
            let hidden=false;
            if(calls<MAX_RESTORE_CALLS){++calls;const resolved=await fetchLevel(parent,false,prefix,epoch);hidden=!!resolved&&sameRoute(resolved.parentRoute,prefix);}
            if(epoch!==generation||intent!==interaction)return;
            setNotice(old=>`${old?`${old} `:''}${hidden?'原位置仍存在，但被当前筛选隐藏':'原位置已删除或导航身份歧义'}；保留筛选并返回最近可用祖先。`);focusRow(parent,false);return;
          }
          if(depth===route.length-1)return entry;
          if(!entry.hasChildren || entry.cycle || entry.depthLimited || entry.boundary)return;
          if(!levels().has(entry.occurrenceId)){
            if(calls>=MAX_RESTORE_CALLS){setNotice('阅读恢复达到 256 次查询预算；保留已恢复祖先。');return;}
            ++calls;if(!await fetchLevel(entry.occurrenceId,false,prefix,epoch))return;
          }
          if(!expanded().has(entry.occurrenceId)&&expanded().size>=MAX_EXPANDED){setNotice('阅读恢复达到 128 个展开位置预算；保留已恢复祖先。');return;}
          setExpanded(old=>new Set(old).add(entry!.occurrenceId));parent=entry.occurrenceId;
        }
      }
      for(const route of state.expandedRoutes.slice(0,MAX_EXPANDED).sort((a,b)=>a.length-b.length)){
        if(epoch!==generation || intent!==interaction) return;
        const entry=await ensureRoute(route);
        if(epoch!==generation||intent!==interaction)return;
        if(entry?.hasChildren && !entry.cycle && !entry.depthLimited && !entry.boundary){
          if(!levels().has(entry.occurrenceId)&&calls<MAX_RESTORE_CALLS){++calls;await fetchLevel(entry.occurrenceId,false,entry.stableRoute,epoch);}
          if(epoch!==generation||intent!==interaction)return;
          if(!levels().has(entry.occurrenceId)){if(calls>=MAX_RESTORE_CALLS)setNotice('阅读恢复达到 256 次查询预算；保留已恢复祖先。');continue;}
          if(!expanded().has(entry.occurrenceId)&&expanded().size>=MAX_EXPANDED){setNotice('阅读恢复达到 128 个展开位置预算；保留已恢复祖先。');break;}
          setExpanded(old=>new Set(old).add(entry.occurrenceId));
        }
      }
      const focus=await ensureRoute(state.parentRoute);if(epoch!==generation||intent!==interaction)return;if(focus)setFocused(focus.occurrenceId);
      if(state.selection){
        const entry=await ensureRoute(state.selection.route);
        if(epoch!==generation || intent!==interaction)return;
        if(entry?.file?.path===state.selection.path&&entry.file.state==='read'){
          detailScrollTop=state.detailScrollTop;
          const section=resolvedSectionId?selectedSection(state.selection,props.snapshot.sections):undefined;
          if(state.selection.sectionId && !section)setNotice(old=>`${old?`${old} `:''}原章节或内容版本已变化，未自动打开同名章节。`);
          openFile(entry,true,{section,unresolved:!!state.selection.sectionId&&!section,scrollTop:state.detailScrollTop,passive:true});setSelection(state.selection);
        }else {
          const original=state.selection.path;const ancestorRoute=state.selection.route.slice(0,-1);const ancestor=await ensureRoute(ancestorRoute);
          if(epoch!==generation||intent!==interaction)return;focusRow(ancestor?.occurrenceId??'',false);
          setNotice(old=>`${old?`${old} `:''}原选中文件已删除或未读取：${original}；返回最近可用祖先，未自动打开原文件或章节。`);
        }
      }
      if(epoch===generation && intent===interaction && list){setViewportTop(state.treeScrollTop);list.scrollTop=state.treeScrollTop;}
    }
    if(epoch===generation && intent===interaction){restoring=false;setBusy(false);if(!focused())setFocused(rows()[0]?.id??'');if(props.userActivated && !activationConsumed){activationConsumed=true;persist();}}
  }
  createEffect(on(identity,()=>{++interaction;void initialize();}));
  onMount(()=>{
    const interruptRestore=()=>{if(!restoring)return;++interaction;if(levels().has('')){++generation;levelRequests.clear();setLoading(new Set<string>());}restoring=false;setBusy(false);setNotice('已停止自动恢复，保留本次手动选择。');if(!canSave)setProtectedRecord(true);};
    document.addEventListener('pointerdown',interruptRestore,true);document.addEventListener('keydown',interruptRestore,true);
    onCleanup(()=>{document.removeEventListener('pointerdown',interruptRestore,true);document.removeEventListener('keydown',interruptRestore,true);});
  });
  createEffect(on(()=>props.sourcePosition,position=>{
    if(position?.userInitiated&&restoring){++interaction;++generation;restoring=false;setBusy(false);setNotice('已停止自动恢复，保留本次手动阅读。');if(!canSave)setProtectedRecord(true);}
    if(!selection() || !props.selectedFile)return;
    if(!position){if(!restoring && selection()?.sectionId){setSelection(old=>old?{...old,sectionPath:null,sectionId:null,sectionRevision:null}:null);changed();persist();}return;}
    const section=position.section;
    setSelection(old=>old?{...old,sectionPath:section.path,sectionId:section.id,sectionRevision:section.revision}:null);
    detailScrollTop=position.scrollTop;
    if(position.userInitiated){changed();persist();}
  }));
  createEffect(on(()=>props.selectedFile,file=>{if(!file && !restoring && selection()){changed();setSelection(null);persist();}}));
  onCleanup(()=>{measureObserver?.disconnect();if(scrollTimer)clearTimeout(scrollTimer);if(scrollIntent)persist(true);alive=false;++generation;});
  function keyboard(event:KeyboardEvent,row:Row) {
    if(composing() || event.isComposing || event.keyCode===229)return;
    if(!['ArrowDown','ArrowUp','ArrowLeft','ArrowRight','Home','End','Enter',' '].includes(event.key))return;
    event.preventDefault();event.stopPropagation();changed();
    const visible=rows();const index=visible.findIndex(item=>item.id===row.id);const entry=row.entry;
    if(event.key==='ArrowDown')focusRow(visible[Math.min(index+1,visible.length-1)]?.id??row.id);
    else if(event.key==='ArrowUp')focusRow(visible[Math.max(index-1,0)]?.id??row.id);
    else if(event.key==='Home')focusRow(visible[0]?.id??row.id);
    else if(event.key==='End')focusRow(visible.at(-1)?.id??row.id);
    else if(event.key==='ArrowLeft'){if(entry && expanded().has(row.id))collapse(entry);else if(row.parent)focusRow(row.parent);}
    else if(event.key==='ArrowRight' && entry){if(!expanded().has(row.id))void expand(entry);else if(visible[index+1]?.parent===row.id)focusRow(visible[index+1].id);}
    else if(event.key==='Enter'){if(entry?.file)openFile(entry);else if(entry)void expand(entry);else void nextPage(row.parent);}
    else if(event.key===' ' && entry){if(expanded().has(row.id))collapse(entry);else void expand(entry);}
    persist();
  }
  function scrollChanged(){setViewportTop(list?.scrollTop??0);if(restoring||!scrollIntent)return;if(scrollTimer)clearTimeout(scrollTimer);scrollTimer=setTimeout(()=>{scrollTimer=undefined;persist();},250);}
  async function resetReading(){if(!key)return;changed();const epoch=generation;const captured={...key};try{await bridge.resetDocumentTreeReading(captured);if(epoch!==generation)return;canSave=true;setProtectedRecord(false);setPreferenceError('');setNotice('此树阅读记录已重置；其他个人数据保持保留。');persist();}catch(failure){if(epoch===generation)setPreferenceError(errorMessage(failure));}}
  const stopCompositionEnter=(event:KeyboardEvent)=>{if((composing()||event.isComposing||event.keyCode===229)&&event.key==='Enter')event.preventDefault();};

  const controls=()=> <>    <p class="muted small">选择模块，展开专题，再打开原文。数字表示本次筛选匹配的文件，不表示完成进度。</p>
    <form class="inventory-filters" onSubmit={event => {event.preventDefault(); if (!composing() && (lifecycle() !== 'raw' || rawStatus())) void applyFilters();}}>
      <label>标题、路径或文档 ID<input aria-label="筛选树文档" value={query()} onInput={e => setQuery(e.currentTarget.value)} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={stopCompositionEnter}/></label>
      <details class="advanced-tree-filters"><summary>高级筛选</summary><div class="inventory-filters"><label>读取结果<select aria-label="树读取结果" value={read()} onChange={e => setRead(e.currentTarget.value)}><option value="all">全部文件</option><option value="read">已读取</option><option value="omitted">仅看遗漏</option></select></label>
      <label>文档类型<select aria-label="树文档类型" value={kind()} onChange={e => setKind(e.currentTarget.value)}><For each={Object.entries(kindLabels)}>{([value,label]) => <option value={value}>{label}</option>}</For></select></label>
      <label>配对缺口<select aria-label="树配对缺口" value={pairing()} onChange={e => setPairing(e.currentTarget.value)}><For each={Object.entries(pairingLabels)}>{([value,label]) => <option value={value}>{label}</option>}</For></select></label>
      <label>生命周期<select aria-label="树生命周期" value={lifecycle()} onChange={e => setLifecycle(e.currentTarget.value)}><For each={Object.entries(lifecycleLabels)}>{([value,label]) => <option value={value}>{label}</option>}</For><option value="raw">原始声明</option></select></label>
      <Show when={lifecycle() === 'raw'}><label>原始声明（精确匹配）<input aria-label="树生命周期原始声明" value={rawStatus()} maxLength={1020} onInput={event => setRawStatus(event.currentTarget.value)} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)} onKeyDown={stopCompositionEnter}/></label></Show>
      </div></details>
      <button type="submit" disabled={busy() || composing() || (lifecycle() === 'raw' && !rawStatus())}>筛选树</button>
    </form>

    <div class="actions"><button disabled={busy()} onClick={()=>{changed();void initialize();}}>重新查询树根</button><button disabled={busy()} onClick={()=>{changed();setExpanded(new Set<string>());focusRow(rows()[0]?.id??'');persist();}}>折叠全部分支</button></div>
</>;
  return <section class="semantic-tree" aria-label={`${title()}导航`}>
    <Show when={busy()}><p role="status">正在查询与恢复{title()}…</p></Show>
    <Show when={notice()}><p role="status" class="notice">{notice()}</p></Show>
    <Show when={preferenceError()}><p role="status" class="notice">{preferenceError()}；树仍可阅读。</p></Show>
    <Show when={protectedRecord()}><div class="notice"><p>原阅读记录未自动覆盖。可重新载入，或明确重置此树的阅读记录后保存新位置。</p><button onClick={()=>{changed();void initialize();}}>重新载入阅读位置</button><button disabled={!key} onClick={()=>void resetReading()}>重置此树阅读记录</button></div></Show>
    <Show when={error()}><div class="notice" role="alert">{error()}<p>重试保持当前快照和已应用筛选；快照失效时重新查询树根。</p><button onClick={()=>{changed();void fetchLevel(failedParent);}}>重试树查询</button></div></Show>
    <Show when={page()}>{result=><>
      <p role="status">扫描候选 {result().candidates} · 已读 {result().read} · 遗漏 {result().omitted}；树范围 {result().eligibleFiles} 个文件 · 全范围匹配 {result().matchingFiles}</p>
      <details class="tree-diagnostics"><summary>版本与阅读说明</summary><p class="muted small">基线 {result().baselineOid} · 生成 {observedTime(result().generatedAt)} · 计数按文件去重。类型筛选：{kindLabels[result().kindFilter]}；配对筛选：{pairingLabels[result().pairingFilter]}；生命周期筛选：{lifecycleLabels[result().lifecycleFilter]??result().lifecycleFilter}。</p><p class="muted small">方向键移动与展开，Enter 打开原文，空格折叠或展开；个人阅读位置按项目及视图保存。</p></details>
      <Show when={result().hiddenHistoryFiles>0}><p role="status">同一筛选范围中隐藏了 {result().hiddenHistoryFiles} 个明确历史文件。<button onClick={()=>void applyFilters(true)}>显示树历史匹配</button></p></Show>
      <Show when={result().stale}><p role="status" class="notice">树使用过期清单，保留原版本：{result().error}</p></Show>
      <Show when={result().issues.length}><div class="notice" role="status"><p>树解析诊断</p><ul><For each={result().issues}>{issue=><li>{issue}</li>}</For></ul></div></Show>

      <div role="tree" class="semantic-tree-list" classList={{'product-domain-map':props.view==='product'&&expanded().size===0&&rows().length<=120&&!page()?.nextCursor}} aria-label={`${title()}文档树`} aria-busy={busy()||loading().size>0} ref={list} tabindex={renderedRows().some(row=>row.id===focused())?-1:0} onKeyDown={event=>{if(event.target===event.currentTarget){const row=rows().find(item=>item.id===focused())??renderedRows()[0];if(row)keyboard(event,row);}}} onWheel={()=>{scrollIntent=true;}} onTouchMove={()=>{scrollIntent=true;}} onScroll={scrollChanged}>
        <div role="presentation" aria-hidden="true" style={{height:`${layout().offsets[windowRange().start]??0}px`}}/>
        <For each={renderedRows()}>{row=><div role="treeitem" ref={element=>registerNode(row.id,element)} tabindex={focused()===row.id?0:-1}
          aria-label={row.entry?.label??'加载下一页文档'} aria-describedby={row.entry?`tree-description-${row.id}`:undefined} aria-level={row.level} aria-posinset={row.position} aria-setsize={row.size}
          aria-expanded={row.entry?.hasChildren&&!row.entry.cycle&&!row.entry.depthLimited&&!row.entry.boundary?expanded().has(row.id):undefined}
          aria-selected={row.entry?!!selection()&&selection()!.path===row.entry.file?.path&&sameRoute(selection()!.route,row.entry.stableRoute):false}
          style={{'--tree-level':Math.min(row.level-1,8)}}
          onFocus={()=>setFocused(row.id)} onKeyDown={event=>keyboard(event,row)} onClick={event=>{if(event.target===event.currentTarget){changed();focusRow(row.id);persist();}}}>
          <Show when={row.entry} fallback={<button tabindex="-1" disabled={loading().has(row.parent)} onClick={()=>void nextPage(row.parent)}>加载此层下一页（最多 100 项）</button>}>{entryAccessor=>{const entry=entryAccessor();return <>
          <Show when={row.level===1&&props.view==='product'}><DomainIcon path={entry.path}/></Show><div class="actions">
            <Show when={entry.hasChildren&&!entry.cycle&&!entry.depthLimited&&!entry.boundary}><button tabindex="-1" disabled={loading().has(row.id)} aria-label={`${expanded().has(row.id)?'折叠':'展开'}：${entry.label}（匹配 ${entry.matchingFiles} 个文件）`} onClick={()=>{focusRow(row.id,false);if(expanded().has(row.id))collapse(entry);else void expand(entry);}}>{expanded().has(row.id)?'▾':'▸'} {entry.label}</button></Show>
            <Show when={entry.file} fallback={<Show when={!entry.hasChildren||entry.cycle||entry.depthLimited}><span>{entry.label}</span></Show>}>{file=><button tabindex="-1" onClick={()=>openFile(entry)}>查看文档：{entry.label}</button>}</Show>
          </div>
          <Show when={row.level===1}><p class="domain-count"><strong>{entry.matchingFiles}</strong> 个匹配文件</p></Show>
          <div id={`tree-description-${row.id}`}>
          <details class="tree-node-source" open={!(row.level===1&&props.view==='product'&&expanded().size===0)}><summary onKeyDown={event=>event.stopPropagation()}>来源与文档状态</summary><Show when={entry.path}><p class="muted small">{entry.path}</p></Show>
          <Show when={entry.file}>{file => <p class="muted small">生命周期：{lifecycleLabels[file().lifecycle?.category ?? 'unknown'] ?? '类别未知'} · 原声明：{file().lifecycle?.rawStatus ?? file().lifecycle?.status ?? '未声明'}</p>}</Show>
          <Show when={entry.historicalReferenceFiles > 0}><div class="notice"><p>历史引用占位：被 {entry.historicalReferenceFiles} 个非明确历史的已读文档显式引用；保留原生命周期，不代表现行、主责或实现完成。</p><p class="muted small">来源路径样本（最多 20）：{entry.historicalReferencePaths.join('、')}</p></div></Show>
          <Show when={entry.sourcePath}><p class="muted small">声明来源：{entry.sourcePath}<Show when={entry.sourceLine != null}> · 第 {entry.sourceLine} 行</Show></p></Show>
          <Show when={entry.relationship}>{edge => <p class="muted small">关系目标：{edge().target ?? edge().destination} · {resolutionLabels[edge().resolution] ?? edge().resolution}</p>}</Show>
          </details><Show when={entry.boundary === 'cross_domain'}><p class="muted small">跨域引用：仅保留导航目标，不展开，不提升为当前树的归属。</p></Show>
          <Show when={entry.cycle}><p role="status" class="notice">检测到循环引用，已停止此位置展开；原文件仍可查看。</p></Show>
          <Show when={entry.depthLimited}><p role="status" class="notice">已达树展开预算，停止此位置展开；完整文件仍可从文件清单查询。</p></Show>

          </div>
          <Show when={loading().has(row.id)}><p role="status">正在加载此分支…</p></Show>
          </>;}}</Show>
        </div>}</For>
        <div role="presentation" aria-hidden="true" style={{height:`${windowRange().end<rows().length?layout().total-layout().offsets[windowRange().end]:0}px`}}/>
      </div>
      <Show when={!rows().length}><p class="empty">当前树没有匹配项目；这不代表仓库没有文档。</p></Show>
      <Show when={result().nextCursor}><button disabled={loading().has('')} onClick={()=>void nextPage('')}>下一页树（最多 100 项）</button></Show>
    </>}</Show>
    <div class="tree-controls">{controls()}</div>
  </section>;
}
