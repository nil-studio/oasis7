import { createEffect, createMemo, createSignal, For, onCleanup, Show, untrack } from 'solid-js';
import { bridge, errorMessage, observedTime, runtimeBridge, statusLabel, subscribeRuntime } from './bridge';
import type { DailyWorkAction, DailyWorkState, RuntimeSnapshot, StartWorkRecord, WorkItem, WorkNavigation, WorkProjection, WorkRef } from './generated/core';
import type { Snapshot, GitHubSnapshot, DocumentSection } from './types';
import { runtimeLabel } from './runtimeState';
import MarkdownText from './MarkdownText';
import DecisionFollowthroughPanel from './DecisionFollowthroughPanel';

type Props = { snapshot?: Snapshot; view: string; sourceGeneration: unknown; onExecution: () => void; ghPath?: string; onGithub?: (value: GitHubSnapshot) => void; showSource?: (section: DocumentSection, origin?: HTMLElement) => void; onView?: (view: string) => void; onOpenProject?: (path: string) => void };
export const workKey = (reference: WorkRef) => JSON.stringify(reference);
const emptyNavigation = (): WorkNavigation => ({selected:null,filter:'',expanded:false,scrollTop:0,returnView:'direction',draft:'',draftPurpose:'',draftScope:'',detailScrollTop:0,retainDraft:false,startIntentId:null,lastReadAt:null,pinned:[],recentWork:[]});
function safely<T>(action:()=>Promise<T>):Promise<T>{try{return Promise.resolve(action());}catch(e){return Promise.reject(e);}}
const attentionLabel = (state: string) => ({open:'需要回答',awaitingEvidence:'待来源核实',resolved:'已处理',superseded:'已被取代'} as Record<string,string>)[state] ?? state;
export default function WorkOverview(props: Props) {
  const [projection,setProjection] = createSignal<WorkProjection>();
  const [personal,setPersonal] = createSignal<DailyWorkState>();
  const [runtime,setRuntime] = createSignal<RuntimeSnapshot>();
  const [intents,setIntents] = createSignal<StartWorkRecord[]>([]);
  const [error,setError] = createSignal('');
  const [personalError,setPersonalError] = createSignal('');
  const [loading,setLoading] = createSignal(false);
  const [saving,setSaving] = createSignal(false);
  const [nav,setNav] = createSignal(emptyNavigation());
  const [selected,setSelected] = createSignal<string>();
  const [goalQuery,setGoalQuery] = createSignal('');
  const [goalId,setGoalId] = createSignal('');
  const [repairId,setRepairId] = createSignal<string|null>(null);
  const [note,setNote] = createSignal('');
  const [locator,setLocator] = createSignal('');
  const [externalTitle,setExternalTitle]=createSignal('');
  const [externalLocator,setExternalLocator]=createSignal('');
  const [worktreePath,setWorktreePath] = createSignal('');
  const [purpose,setPurpose] = createSignal('');
  const [scopeText,setScopeText] = createSignal('');
  const [preview,setPreview] = createSignal(false);
  const [sendLocked,setSendLocked] = createSignal(false);
  const [showHistory,setShowHistory] = createSignal(false);
  let generation=0, interaction=0, disposed=false, currentScope='', restorePending=true;
  let origin:HTMLElement|undefined, detail:HTMLElement|undefined, list:HTMLElement|undefined;
  let unlisten:(()=>void)|undefined;
  const sessionInputs = new Map<string,{navigation:WorkNavigation;goalQuery:string;goalId:string;repairId:string|null;note:string;locator:string;purpose:string;scope:string}>();
  const key = () => props.snapshot?.project ? JSON.stringify([props.snapshot.project.id,props.snapshot.project.environment]) : '';
  async function read(restore=false) {
    const project=props.snapshot?.project;if(!project||disposed)return;
    const token=++generation, intent=interaction, restoring=restore||restorePending;setLoading(true);
    const [work,records,source,starts]=await Promise.allSettled([safely(()=>bridge.workProjection()),safely(()=>bridge.dailyWork()),safely(()=>runtimeBridge.publicSnapshot()),safely(()=>runtimeBridge.startWorkRecords())]);
    if(token!==generation||disposed||key()!==currentScope)return;
    if(work.status==='fulfilled') {
      const value=work.value;
      if(value.schemaVersion===1&&value.projectId===project.id&&value.environment===project.environment&&Array.isArray(value.items)){setProjection(value);setError('');}
      else setError('工作投影范围或版本不匹配，保留旧结果。');
    } else setError(errorMessage(work.reason));
    if(records.status==='fulfilled'&&records.value.projectId===project.id&&records.value.environment===project.environment&&records.value.schemaVersion===1){
      if(!personal()||records.value.revision>=personal()!.revision)setPersonal(records.value);setPersonalError('');
      if(restoring&&intent===interaction){restorePending=false;const transient=sessionInputs.get(currentScope);const saved=transient?.navigation??records.value.navigation;if(transient){setGoalQuery(transient.goalQuery);setGoalId(transient.goalId);setRepairId(transient.repairId);setNote(transient.note);setLocator(transient.locator);}setNav(saved);setPurpose(transient?.purpose??saved.draftPurpose);setScopeText(transient?.scope??saved.draftScope);setSelected(saved.selected?workKey(saved.selected):undefined);queueMicrotask(()=>{if(intent===interaction){window.scrollTo?.(0,saved.detailScrollTop||saved.scrollTop);}});}
    }else setPersonalError(records.status==='rejected'?errorMessage(records.reason):'个人记录范围不匹配，未恢复。');
    if(source.status==='fulfilled'&&source.value.projectId===project.id&&source.value.environment===project.environment)setRuntime(source.value);
    if(starts.status==='fulfilled')setIntents(starts.value);
    setLoading(false);
  }
  createEffect(()=>{
    props.sourceGeneration;props.snapshot?.associations;props.snapshot?.decisions;
    const next=key();untrack(()=>{const changed=next!==currentScope;
    if(changed){if(currentScope)sessionInputs.set(currentScope,{navigation:{...nav(),selected:item()?.reference??null},goalQuery:goalQuery(),goalId:goalId(),repairId:repairId(),note:note(),locator:locator(),purpose:purpose(),scope:scopeText()});currentScope=next;restorePending=true;++generation;++interaction;setProjection(undefined);setPersonal(undefined);setRuntime(undefined);setIntents([]);setSelected(undefined);setNav(emptyNavigation());setSendLocked(false);setPreview(false);setPersonalError('');setGoalQuery('');setGoalId('');setRepairId(null);setNote('');setLocator('');setPurpose('');setScopeText('');}
    if(next)void read(changed);});
  });
  void subscribeRuntime(()=>void read(),()=>{}).then(stop=>{if(disposed)stop();else unlisten=stop;}).catch(()=>{});
  onCleanup(()=>{disposed=true;++generation;unlisten?.();});
  async function save(action:DailyWorkAction) {
    const state=personal();if(!state||saving()){setPersonalError('个人记录尚未读取或正在保存，草稿已保留。');return;}
    const scope=currentScope;setSaving(true);setPersonalError('');
    try{const value=await bridge.saveDailyWork({projectId:state.projectId,environment:state.environment,expectedRevision:state.revision,action});if(scope===currentScope&&!disposed)setPersonal(value);}
    catch(e){if(scope===currentScope&&!disposed)setPersonalError(`未保存：${errorMessage(e)}。当前输入已保留，请先重读并核对差异。`);}
    finally{setSaving(false);}
  }
  function changed(patch:Partial<WorkNavigation>){restorePending=false;++interaction;setNav({...nav(),...patch});}
  async function remember(patch:Partial<WorkNavigation>={}){changed(patch);await save({kind:'navigate',navigation:{...nav(),selected:item()?.reference??null,returnView:nav().returnView}});}
  const works=createMemo(()=>projection()?.items??[]);
  const item=()=>works().find(w=>workKey(w.reference)===selected());
  const filtered=createMemo(()=>works().filter(w=>!nav().filter||`${w.purpose.value} ${w.result.value}`.toLocaleLowerCase().includes(nav().filter.toLocaleLowerCase())));
  const ordered=createMemo(()=>{const recent=nav().recentWork??[];const rank=(w:WorkItem)=>{const i=recent.findIndex(r=>workKey(r)===workKey(w.reference));return i<0?100:i;};return [...filtered()].sort((a,b)=>Number(nav().pinned.some(p=>workKey(p)===workKey(b.reference)))-Number(nav().pinned.some(p=>workKey(p)===workKey(a.reference)))||rank(a)-rank(b));});
  const goals=createMemo(()=>props.snapshot?.sections?.filter(s=>s.path==='doc/core/prd.md'&&/当前阶段交付目标与全局 P0/.test(s.title))??[]);
  const goalOptions=createMemo(()=>{
    const source=props.snapshot?.sections??[];const query=goalQuery().trim().toLowerCase();
    const selected=source.filter(s=>s.id===goalId());
    const found=query?source.filter(s=>`${s.title} ${s.path} ${s.id}`.toLowerCase().includes(query)).slice(0,30):goals();
    return [...new Map([...selected,...found].map(s=>[s.id,s])).values()];
  });
  const pinnedWorks=()=>ordered().filter(w=>nav().pinned.some(p=>workKey(p)===workKey(w.reference)));
  const focusGoals=()=>goals().slice(0,Math.max(0,3-pinnedWorks().length));
  const focusSummary=(content:string)=>content.split('\n').map(s=>s.trim()).find(s=>s&&!s.startsWith('#')&&!s.startsWith('|')&&!s.startsWith('<a '))?.slice(0,240)??'请查看主责原文，未推测下一动作。';
  const relations=()=>personal()?.relations.filter(r=>!r.removedAt&&workKey(r.work)===selected())??[];
  const conflicts=createMemo(()=>{
    const result:{section:DocumentSection;claim:string;work:WorkItem}[]=[];
    for(const section of props.snapshot?.sections??[]){
      if(!['doc/engineering/command-center/README.md','doc/engineering/command-center/architecture/README.md'].includes(section.path))continue;
      for(const claim of section.content.split('\n')){
        if(!/待合入|尚未合入|未合入/.test(claim))continue;
        for(const work of works())if(work.reference.kind==='pullRequest'&&work.result.value==='PR MERGED'&&new RegExp(`(?:pull/|#)${work.reference.number}(?:\\D|$)`).test(claim))result.push({section,claim,work});
      }
    }
    return result.slice(0,20);
  });
  const recent=()=>works().filter(w=>w.result.evidence.eventAt&&(!nav().lastReadAt||Date.parse(w.result.evidence.eventAt)>Date.parse(nav().lastReadAt!)));
  const observed=()=>works().filter(w=>!w.result.evidence.eventAt).slice(0,3);
  const openAttention=()=>personal()?.attention.filter(a=>a.state==='open')??[];
  const liveRequests=()=>runtime()?.requests.filter(r=>['pending','awaiting','unknown'].includes(r.state))??[];
  function open(work:WorkItem,button?:HTMLElement){setCopyStatus('');++interaction;origin=button;setSelected(workKey(work.reference));setPurpose(work.purpose.value);setScopeText(work.goals.map(g=>g.title??g.sectionId).join('；'));changed({returnView:props.view,scrollTop:window.scrollY,recentWork:[work.reference,...(nav().recentWork??[]).filter(r=>workKey(r)!==workKey(work.reference))].slice(0,20)});setGoalId('');setRepairId(null);setPreview(false);setNote('');void remember();queueMicrotask(()=>{detail?.focus();detail?.scrollIntoView?.({block:'start'});});}
  function close(){
    ++interaction;const id=selected(),returnView=nav().returnView;setSelected(undefined);changed({selected:null});
    void save({kind:'navigate',navigation:{...nav(),selected:null}});
    if(returnView&&returnView!==props.view)props.onView?.(returnView);
    queueMicrotask(()=>{const current=Array.from(list?.querySelectorAll<HTMLButtonElement>('button[data-work-ref]')??[]).find(b=>b.dataset.workRef===id);const target=current??origin;if(target?.isConnected){target.focus({preventScroll:true});window.scrollTo?.(0,nav().scrollTop);}});
  }

  async function checks(){const work=item();if(!work||work.reference.kind!=='pullRequest'||saving())return;const scope=currentScope;setSaving(true);try{const value=await bridge.refreshWorkChecks(work.reference.number,props.ghPath?.trim()||null);if(scope===currentScope){props.onGithub?.(value);await read();}}catch(e){if(scope===currentScope)setError(errorMessage(e));}finally{setSaving(false);}}
  async function start(){
    const project=props.snapshot?.project;
    if(!project||sendLocked()||!preview()||!nav().draft.trim()||!personal())return;
    const original=currentScope;setSendLocked(true);setError('');
    const intentId=nav().startIntentId??crypto.randomUUID();changed({startIntentId:intentId});
    try{
      const state=personal()!;
      const stored=await bridge.saveDailyWork({projectId:state.projectId,environment:state.environment,expectedRevision:state.revision,action:{kind:'navigate',navigation:{...nav(),selected:item()?.reference??null,returnView:props.view}}});
      if(original!==currentScope||disposed)return;
      setPersonal(stored);
      await runtimeBridge.startWork(intentId,{purpose:purpose(),scope:scopeText(),model:null,basisRevision:project.head},nav().draft);
      if(original===currentScope){setPreview(false);await read();}
    }catch(e){if(original===currentScope)setError(`发起未取得完整回执：${errorMessage(e)}。原意图身份已保留，请核对原会话与子操作，禁止重复发起。`);}
  }

  const savedLocator=(work:WorkItem)=>personal()?.external.find(e=>workKey(e.work)===workKey(work.reference));
  const externalOnly=(work:WorkItem)=>work.reference.kind==='session'&&work.reference.namespace==='external-locator';
  const [copyStatus,setCopyStatus]=createSignal('');
  async function copyLocator(value:string){try{if(!navigator.clipboard?.writeText)throw new Error('当前环境无法自动复制，请选中原定位文本手动复制');await navigator.clipboard.writeText(value);setCopyStatus('原定位已复制；源工作状态保持原样。');}catch(e){setError(errorMessage(e));}}
  const row=(work:WorkItem)=><div class="row"><div class="row-content"><button class="row-title" data-work-ref={workKey(work.reference)} onClick={e=>open(work,e.currentTarget)}>{work.purpose.value}</button><p>{work.result.value}</p><p class="small muted">{work.remaining}</p></div><button aria-label={`置顶 ${work.purpose.value}`} onClick={()=>void remember({pinned:nav().pinned.some(p=>workKey(p)===workKey(work.reference))?nav().pinned.filter(p=>workKey(p)!==workKey(work.reference)):[...nav().pinned,work.reference]})}>{nav().pinned.some(p=>workKey(p)===workKey(work.reference))?'取消置顶':'置顶'}</button></div>;
  return <Show when={props.snapshot?.project}>
    <Show when={['direction','execution','attention'].includes(props.view)}><section class="work-feedback" aria-label="工作来源状态"><Show when={error()}><p role="alert" class="notice">工作读取失败：{error()}。保留本项目最近结果。</p></Show><Show when={personalError()}><p role="alert" class="notice">{personalError()}<button onClick={()=>void read()}>重读个人记录</button></p></Show><Show when={personal()?.legacyChanged}><p class="notice">旧版决定或关联已变化。原记录与新修复/证据分别保留，请在规范和关系详情核对后接纳变化。<button onClick={()=>void save({kind:'acceptLegacy'})}>已核对旧记录变化</button></p></Show></section></Show>
    <Show when={props.view==='direction'}><div class="home-grid">
      <section class="panel"><div class="panel-title"><h3>当前重点与下一步</h3><span class="tag">主责顺序 · 个人置顶</span></div><Show when={goals().length} fallback={<div class="empty">当前阶段主责尚未读取；先查看已有工作，不能从 PR 数量推算重点。</div>}><For each={focusGoals()}>{goal=><div class="panel-body"><button class="row-title" onClick={e=>props.showSource?.(goal,e.currentTarget)}>{goal.title}</button><p>{focusSummary(goal.content)}</p><p class="small muted">已知结果：只读到主责目标，目标验收尚未核实。下一步：进入关联工作核对真实交付和剩余条件。</p><button onClick={props.onExecution}>进入已有工作，核对下一动作</button><details><summary>完整目标与依据</summary><MarkdownText text={goal.content} sourcePath={goal.path} sections={props.snapshot?.sections} showSource={props.showSource}/></details></div>}</For></Show><For each={pinnedWorks().slice(0,3)}>{row}</For><Show when={pinnedWorks().length+goals().length>3}><div class="panel-body"><button onClick={props.onExecution}>查看其余重点与完整工作范围</button></div></Show></section>
      <section class="panel"><div class="panel-title"><h3>最近成果变化</h3><button onClick={()=>void remember({lastReadAt:new Date().toISOString()})}>记录本次阅读基线</button></div><div class="panel-body"><p class="small muted">比较基线：{nav().lastReadAt?observedTime(nav().lastReadAt):'尚未建立；显示已读取合入事件，不宣称完整的自上次以来变化'}</p><For each={recent().slice(0,3)}>{w=><div><button class="row-title" onClick={e=>open(w,e.currentTarget)}>{w.purpose.value}</button><p>{w.result.value} · {observedTime(w.result.evidence.eventAt)}</p><p class="small muted">合入事实只更新 PR 轨道；目标验收和部署仍按各自来源。</p></div>}</For><Show when={!recent().length}><p>本次已读窗口没有匹配事件；其他页及未接入来源仍未知。</p></Show><For each={conflicts()}>{conflict=><div class="notice"><p>PR 合入轨道已由 GitHub 更新，旧摘要表述保留供核对：</p><p>{conflict.claim.slice(0,120)}…</p><details><summary>展开原摘要表述</summary><p>{conflict.claim}</p></details><button onClick={e=>props.showSource?.(conflict.section,e.currentTarget)}>查看旧摘要及原核验范围</button><button onClick={e=>open(conflict.work,e.currentTarget)}>查看当前 PR 合入事实</button><p class="small muted">不连带更新部署、目标验收或语义符合；摘要版本 {conflict.section.revision}</p></div>}</For><details><summary>事件时间未知的本次观察</summary><For each={observed()}>{w=><p><button aria-label={`查看未知事件时间的工作 ${w.purpose.value}`} onClick={e=>open(w,e.currentTarget)}>{w.purpose.value}</button> · {w.result.value}</p>}</For></details></div></section>
      <section class="panel"><div class="panel-title"><h3>需要我处理</h3><span class="tag">{openAttention().length+liveRequests().filter(r=>r.state==='pending').length} 项已知待答</span></div><div class="panel-body"><For each={openAttention().slice(0,3)}>{a=><p>{a.question}</p>}</For><For each={liveRequests().slice(0,3)}>{r=><p>{r.summary} · {runtimeLabel(r.state)}</p>}</For><Show when={!openAttention().length&&!liveRequests().length}><p>已读取范围暂无真实待答；未接入会话的请求仍未知。</p></Show><button onClick={()=>props.onView?.('attention')}>进入待处理与核实</button></div></section>
      <section class="panel" ref={list}><div class="panel-title"><h3>返回工作</h3><button disabled={loading()} onClick={()=>void read()}>刷新工作列表</button><button onClick={props.onExecution}>查看全部工作</button></div><For each={nav().expanded?ordered():ordered().slice(0,3)}>{row}</For><Show when={ordered().length>3}><div class="panel-body"><button onClick={()=>void remember({expanded:!nav().expanded})}>{nav().expanded?'收起其余工作':`显示其余 ${ordered().length-3} 项工作`}</button></div></Show><Show when={loading()&&!works().length}><p class="panel-body" role="status">正在读取工作来源；尚不能判断是否为空。</p></Show><Show when={!loading()&&!works().length}><div class="empty">尚未读取已有工作；可连接 GitHub 或明确保存原工具定位。</div></Show></section>
    </div></Show>
    <Show when={props.view==='execution'}><details class="panel"><summary>接续仅有原工具定位的外部会话</summary><div class="panel-body editor"><p>保存普通工作名称与定位，不读取私有资料或外部消息，不取得控制权。无需 PR 或空白自管会话。</p><label>工作名称<input value={externalTitle()} onInput={e=>setExternalTitle(e.currentTarget.value)}/></label><label>原工具定位（可复制）<input value={externalLocator()} onInput={e=>setExternalLocator(e.currentTarget.value)}/></label><button disabled={saving()||!personal()||!externalTitle().trim()||!externalLocator().trim()} onClick={()=>{const p=props.snapshot!.project!;const id=crypto.randomUUID();void save({kind:'external',item:{id,title:externalTitle(),work:{kind:'session',namespace:'external-locator',projectId:p.id,environment:p.environment,threadId:id},locator:externalLocator(),sourceUrl:null,recordedAt:''}}).then(()=>read());}}>保存原现场定位</button></div></details></Show>
    <Show when={props.view==='execution'}><details class="panel"><summary>仓库阅读范围与执行工作树</summary><div class="panel-body"><p>阅读聚合只包含明确选择并核实为同一仓库的工作树；执行仍使用原 Project / 环境身份，切换不会发送任务。</p><label>添加同仓库工作树绝对路径<input value={worktreePath()} onInput={e=>setWorktreePath(e.currentTarget.value)}/></label><button disabled={saving()||!personal()||!worktreePath().trim()} onClick={()=>{const state=personal();if(!state)return;const original=currentScope;setSaving(true);void bridge.addWorktreeReference(worktreePath(),state.revision).then(value=>{if(original===currentScope){setPersonal(value);void read();}}).catch(e=>setPersonalError(errorMessage(e))).finally(()=>setSaving(false));}}>核实并加入阅读范围</button><For each={personal()?.worktrees}>{reference=><div class="row"><div><p>{reference.path}</p><p class="small muted">{reference.environment} · 原核实 {observedTime(reference.verifiedAt)}；当前观察见来源覆盖。</p></div><button onClick={()=>props.onOpenProject?.(reference.path)}>切换到此原执行范围</button><button disabled={saving()} onClick={()=>void save({kind:'removeWorktree',projectId:reference.projectId})}>移出阅读范围</button></div>}</For></div></details></Show>
    <Show when={props.view==='execution'||props.view==='attention'}><section class="panel" ref={list} aria-label="已有工作"><div class="panel-title"><h3>返回已有工作</h3><button disabled={loading()} onClick={()=>void read()}>刷新工作列表</button></div><div class="panel-body"><label>筛选工作<input value={nav().filter} onInput={e=>changed({filter:e.currentTarget.value})} onBlur={()=>void remember()}/></label><p class="muted">置顶优先，再按明确打开记录返回；无记录的其余项保留来源顺序。</p></div><For each={nav().expanded?ordered():ordered().slice(0,3)}>{row}</For><Show when={ordered().length>3}><div class="panel-body"><button onClick={()=>void remember({expanded:!nav().expanded})}>{nav().expanded?'收起其余工作':`显示其余 ${ordered().length-3} 项工作`}</button></div></Show><Show when={loading()&&!works().length}><p class="panel-body" role="status">正在读取工作来源；尚不能判断是否为空。</p></Show><Show when={!loading()&&!works().length}><div class="empty">当前已读取范围未列出工作；请核对来源覆盖。</div></Show></section></Show>
    <Show when={['direction','execution','attention'].includes(props.view)}><Show when={item()} fallback={<Show when={selected()}><section class="panel"><p>原工作未在当前来源窗口中；不能据此认定完成或删除。请在开发执行按 PR 身份补读。</p><button onClick={close}>返回工作列表</button></section></Show>}>{work=><section class="panel detail" ref={detail} tabindex="-1" aria-label="工作详情"><div class="panel-title"><h3>{work().purpose.value}</h3><button onClick={close}>返回工作列表</button></div><div class="panel-body">
      <h4>已知结果</h4><p>{work().result.value}</p><h4>剩余条件</h4><p>{work().remaining}</p>
      <Show when={work().sourceUrl} fallback={<Show when={externalOnly(work())} fallback={<button onClick={props.onExecution}>核对原自管实例与操作</button>}><button disabled={!savedLocator(work())} onClick={()=>{const entry=savedLocator(work());if(entry)void copyLocator(entry.locator);}}>复制原工具定位</button></Show>}><button onClick={()=>void bridge.openSource(work().sourceUrl!).catch(e=>setError(errorMessage(e)))}>打开原 PR</button></Show><p class="small muted">打开入口只证明导航已请求，不代表来源工作完成。</p>
      <Show when={copyStatus()}><p role="status">{copyStatus()}</p></Show><h4>目标与依据</h4><Show when={!personal()}><For each={work().goals}>{goal=><p><span>{goal.title??`原章节未读取或已变化 · ${goal.sectionId}`}</span> · 原个人确认 {observedTime(goal.confirmedAt)}</p>}</For></Show><For each={relations()}>{relation=>{const section=()=>props.snapshot?.sections.find(s=>s.id===relation.sectionId);return <div class="row"><div><button data-source-ref={relation.sectionId} onClick={e=>{const s=section();if(s)props.showSource?.(s,e.currentTarget);}}>{section()?.title??`原条款未读取 · ${relation.sectionId}`}</button><p class="small muted">{!section()?'不可读，保留原关系':section()!.revision===relation.sourceRevision?'原依据版本仍匹配':'依据已变化，需核对差异；不按标题迁移'}</p></div><button onClick={()=>{setRepairId(relation.id);setGoalId(relation.sectionId);}}>修复关系</button><button onClick={()=>void save({kind:'removeRelation',id:relation.id})}>移除个人关系</button></div>;}}</For>
      <label>查找目标或规范<input value={goalQuery()} onInput={e=>setGoalQuery(e.currentTarget.value)} placeholder="输入标题、路径或条款 ID；不需要手填 PR 地址"/></label><label>{repairId()?'新的关系端点':'关联已读取条款'}<select value={goalId()} onChange={e=>setGoalId(e.currentTarget.value)}><option value="">选择明确条款</option><For each={goalOptions()}>{s=><option value={s.id}>{s.title} · {s.path}</option>}</For></select></label><button disabled={!goalId()||saving()||!personal()} onClick={()=>void save({kind:'relate',work:work().reference,sectionId:goalId(),replaceId:repairId()})}>{repairId()?'确认修复，保留原版本':'确认关联此工作'}</button><Show when={repairId()}><button onClick={()=>setRepairId(null)}>取消修复</button></Show>
      <details><summary>来源明确引用与关系历史</summary><p>只有正文明确说明服务目标的引用才可确认关系；普通链接是候选，不自动成为依赖。</p><For each={props.snapshot?.sections?.filter(s=>props.sourceGeneration&&String((props.sourceGeneration as GitHubSnapshot).pulls?.find(p=>p.url===work().sourceUrl)?.metadata?.body??'').includes(s.id)).slice(0,20)}>{s=><button onClick={()=>setGoalId(s.id)}>{s.title} · 正文出现原 ID，语义待确认</button>}</For><For each={personal()?.relations.filter(r=>workKey(r.work)===selected())}>{r=><p>{r.sectionId} · {r.removedAt?'个人已移除':'已确认'} · {observedTime(r.confirmedAt)} · 历史 {r.previous.length} 次</p>}</For></details>
      <Show when={work().reference.kind==='pullRequest'}><h4>PR / CI</h4><button disabled={saving()} onClick={()=>void checks()}>只读刷新该 PR 的 CI</button><p class="small muted">仅点击后最多两次 REST 请求；当前来源未读取的必要检查仍未知。</p><Show when={work().checksSource} fallback={<p>该 PR 的 CI 尚未单独读取；评审状态未知。</p>}>{source=><p>{statusLabel(source().state)} · {observedTime(source().observedAt)} · {source().coverage} {source().error}</p>}</Show><Show when={work().checksSource&&work().checksSource!.state!=='error'&&!work().checks.length}><p>已读取的 CI 页面为空；不代表全部必要检查通过。</p></Show><For each={work().checks}>{check=><p>{check.name} · {check.state}<br/><span class="small muted">实际测试版本 {check.testedOid??'未知，不能由 PR HEAD 推定'}<br/>GitHub 关联提交 {check.sourceOid??'未知'} · 不代表实际 checkout 对象</span></p>}</For></Show>
      <For each={work().runs}>{run=><p>原运行实例 {run.instanceId} · {runtimeLabel(run.state)} · turn {run.turnId??'未取得'}</p>}</For>
      <h4>原工具接续</h4><label>工具名称与明确定位<input value={locator()} onInput={e=>setLocator(e.currentTarget.value)} placeholder="例如 Codex App 中的原会话 ID / 工作树路径"/></label><button disabled={!locator().trim()||saving()} onClick={()=>void save({kind:'external',item:{id:workKey(work().reference),title:work().purpose.value,work:work().reference,locator:locator(),sourceUrl:work().sourceUrl,recordedAt:''}})}>保存原现场定位</button><For each={personal()?.external.filter(e=>workKey(e.work)===selected())}>{external=><div><p>{external.locator}</p><p class="small muted">{external.sourceUrl?'已有 PR 原入口可打开；此工具定位只支持复制。':'此工具只支持复制定位；打开、历史读取、实时观察和控制权均未接入。'}不因同分支认领会话。</p><button onClick={()=>void copyLocator(external.locator)}>复制原定位</button><button onClick={()=>void save({kind:'removeExternal',id:external.id})}>移除此个人定位</button></div>}</For>
      <h4>记录真实问题</h4><label>需要我回答的问题<input value={note()} onInput={e=>setNote(e.currentTarget.value)}/></label><button disabled={!note().trim()||saving()} onClick={()=>void save({kind:'attention',item:{id:crypto.randomUUID(),work:work().reference,question:note(),sourceRevision:work().result.evidence.revision??'',state:'open',statement:'',updatedAt:'',history:[]}})}>保存为个人待答事项</button>
      <details><summary>带着上下文发起自管工作（实验）</summary><p>只有明确发送才建立会话并发送一次输入；失败/未知先核对原意图，不重发。私有资料输入请使用原隔离输入入口。</p><label>目的<input value={purpose()} onInput={e=>{setPurpose(e.currentTarget.value);changed({draftPurpose:e.currentTarget.value});setPreview(false);}}/></label><label>范围<input value={scopeText()} onInput={e=>{setScopeText(e.currentTarget.value);changed({draftScope:e.currentTarget.value});setPreview(false);}}/></label><label>实际输入<textarea value={nav().draft} onInput={e=>{changed({draft:e.currentTarget.value});setPreview(false);}}/></label><label><input type="checkbox" checked={nav().retainDraft} onChange={e=>void remember({retainDraft:e.currentTarget.checked})}/>留存这份普通草稿，供重开恢复</label><button onClick={()=>void remember({draftPurpose:purpose(),draftScope:scopeText(),detailScrollTop:window.scrollY})}>保存草稿选择</button><button disabled={!nav().draft.trim()||!purpose().trim()||!scopeText().trim()} onClick={()=>setPreview(true)}>预览实际发送内容</button><Show when={preview()}><pre>{nav().draft}</pre><p>项目 {props.snapshot?.project?.path} · 基线 {props.snapshot?.project?.head}</p><button disabled={sendLocked()||!!nav().startIntentId||!runtime()?.enabled||runtime()?.state!=='ready'} onClick={()=>void start()}>明确发起一次工作</button></Show><Show when={!runtime()?.enabled}><p>实验连接未启用；已有 PR 和外部现场仍可使用。</p></Show><Show when={nav().startIntentId}><p>当前预览已绑定原发起意图 {nav().startIntentId}；重开只核对，不重新发送。</p><button disabled={intents().some(i=>!['inputAcknowledged','inputRejected','inputNotSentConfirmed','sourceResolved'].includes(i.state))} onClick={()=>{changed({startIntentId:null,draft:''});setPreview(false);setSendLocked(false);void remember();}}>明确另起一项工作，清空原预览</button></Show><For each={intents()}>{intent=><p>原发起意图 {intent.intentId} · {{inputAcknowledged:'输入已有原操作回执',inputRejected:'输入已明确拒绝',inputNotSentConfirmed:'原操作确认输入未发送',sourceResolved:'来源已核实结束；原输入效应未证实',unknown:'结果未知，需核对原操作',pending:'建立会话中',inputPending:'输入发送中',threadCreatedInputNotSent:'会话已建立，输入尚未发送'}[intent.state]??intent.state} · thread {intent.threadId??'未知'} · 子操作 {intent.operationIds.join(', ')||'待核对'}<br/><span class="small muted">会话已建而输入未发送时，可在原会话输入区明确续发；未知不能重试。</span></p>}</For></details>
      <details><summary>来源身份与核验范围</summary><pre>{JSON.stringify(work().reference,null,2)}</pre><p>来源版本 {work().result.evidence.revision??'未知'}</p><p>来源事实核验 {observedTime(work().result.evidence.verifiedAt)}</p><p>事件时间 {work().result.evidence.eventAt??'未知；不宣称自上次以来的完整成果'}</p><p>App 读取 {observedTime(work().result.evidence.readAt)}</p><p>{work().result.evidence.coverage}</p></details>
    </div></section>}</Show></Show>
    <Show when={props.view==='attention'}><section class="panel"><div class="panel-title"><h3>个人问题与处理历史</h3><button onClick={()=>setShowHistory(!showHistory())}>{showHistory()?'仅显示未决':'查看处理历史'}</button></div><For each={personal()?.attention.filter(a=>showHistory()||!['resolved','superseded'].includes(a.state))}>{a=><div class="panel-body"><h4>{a.question}</h4><p>{attentionLabel(a.state)} · {a.statement||'尚无处理陈述'}</p><button onClick={()=>{setSelected(workKey(a.work));props.onView?.('attention');}}>进入原工作</button><Show when={a.state==='open'}><button onClick={()=>void save({kind:'attention',item:{...a,state:'awaitingEvidence',statement:'用户记已在原工具处理，源结果尚未回读核实'}})}>已在原工具处理，待核实</button><button onClick={()=>void save({kind:'attention',item:{...a,state:'resolved',statement:'用户明确回答此个人问题；不代表源工作完成'}})}>此个人问题已回答</button></Show><Show when={a.state==='awaitingEvidence'}><p>个人陈述不冒充运行回执；回到原工具核对来源。</p><button onClick={()=>void save({kind:'attention',item:{...a,state:'superseded',statement:'用户结束此个人关注，源状态仍未知'}})}>结束个人关注，源状态保持未知</button></Show></div>}</For><Show when={!personal()?.attention.length}><div class="empty">当前没有已记录的个人问题；真实运行请求显示在下方原请求入口。</div></Show></section></Show>
    <Show when={['rules','attention'].includes(props.view)&&personal()}><DecisionFollowthroughPanel snapshot={props.snapshot!} personal={personal()!} save={save} busy={saving()} showSource={props.showSource}/></Show>
    <Show when={['direction','execution','attention'].includes(props.view)}><details class="panel"><summary>工作来源覆盖与读取时间</summary><div class="panel-body"><For each={projection()?.sources}>{source=><p>{source.kind==='github'?'GitHub':source.kind==='personal'?'个人记录':'自管会话'} · {source.kind==='codex'?runtimeLabel(source.state):statusLabel(source.state)} · {source.coverage} {source.error}</p>}</For><p>当前个人记录 revision {personal()?.revision.toString()??'未读取'}；共享来源视图 {projection()?.viewRevision??'尚未发布版本'} · 组合个人版本 {projection()?.personalRevision??'未知'}；来源不是远端原子时刻。</p></div></details></Show>
  </Show>;
}
