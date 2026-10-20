import { ArrowRight, CircleHelp, Link, ListFilter, Pin, Target } from 'lucide-solid';
import { StatusIcon } from './VisualStatus';
import WorkRelations from './WorkRelations';
import { createEffect, createMemo, createSignal, For, onCleanup, Show, untrack } from 'solid-js';
import { bridge, errorMessage, observedTime, statusLabel } from './bridge';
import type { DailyWorkAction, DailyWorkState, WorkItem, WorkNavigation, WorkProjection, WorkRef } from './generated/core';
import type { Snapshot, GitHubSnapshot, DocumentSection } from './types';
import MarkdownText from './MarkdownText';

type Props = { snapshot?: Snapshot; view: string; sourceGeneration: unknown; onExecution: () => void; onConnectGithub?: () => void; sourceBusy?: boolean; onSelection?: (selected: boolean) => void; showSource?: (section: DocumentSection, origin?: HTMLElement) => void; onView?: (view: string) => void; onOpenProject?: (path: string) => void };
export const workStatus = (value:string) => ({'PR MERGED':'PR 已合入','PR OPEN':'PR 进行中','PR CLOSED':'PR 已关闭'} as Record<string,string>)[value]??({'external':'外部工作 · 在原工具继续','unknown':'未核实','active':'旧会话记录 · 在原工具核实','idle':'旧会话记录 · 在原工具核实'} as Record<string,string>)[value]??value;
export const workKey = (reference: WorkRef) => JSON.stringify(reference);
type WorkFilter = 'all'|'PR OPEN'|'PR MERGED'|'PR CLOSED'|'other';
const statusFilters: [WorkFilter,string][]=[['all','全部'],['PR OPEN','进行中'],['PR MERGED','已合入'],['PR CLOSED','已关闭'],['other','外部或未核实']];
const workGroup=(work:WorkItem):WorkFilter=>work.reference.kind==='pullRequest'&&['PR OPEN','PR MERGED','PR CLOSED'].includes(work.result.value)?work.result.value as WorkFilter:'other';
const emptyNavigation = (): WorkNavigation => ({selected:null,filter:'',expanded:false,scrollTop:0,returnView:'direction',draft:'',draftPurpose:'',draftScope:'',detailScrollTop:0,retainDraft:false,startIntentId:null,lastReadAt:null,pinned:[],recentWork:[]});
function safely<T>(action:()=>Promise<T>):Promise<T>{try{return Promise.resolve(action());}catch(e){return Promise.reject(e);}}
const attentionLabel = (state: string) => ({open:'需要回答',awaitingEvidence:'待来源核实',resolved:'已处理',superseded:'已被取代'} as Record<string,string>)[state] ?? state;
export default function WorkOverview(props: Props) {
  const [projection,setProjection] = createSignal<WorkProjection>();
  const [personal,setPersonal] = createSignal<DailyWorkState>();
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
  const [workPage,setWorkPage]=createSignal(0);
  const [statusFilter,setStatusFilter]=createSignal<WorkFilter>('all');
  const [detailTab,setDetailTab] = createSignal("overview");
  const [action,setAction] = createSignal("");
  const workInputs = new Map<string,{draft:string;purpose:string;scope:string;locator:string;note:string;goalQuery:string;goalId:string;repairId:string|null;retainDraft:boolean;startIntentId:string|null;tab:string}>();
  createEffect(()=>props.onSelection?.(!!selected() && ["direction","execution","attention"].includes(props.view)));
  const [showHistory,setShowHistory] = createSignal(false);
  let generation=0, interaction=0, disposed=false, currentScope='', restorePending=true;
  let actionOrigin:HTMLElement|undefined, actionPanel:HTMLElement|undefined;
  function openAction(value:string,origin?:HTMLElement){actionOrigin=origin??(document.activeElement instanceof HTMLElement?document.activeElement:undefined);setAction(value);queueMicrotask(()=>{actionPanel?.focus();actionPanel?.scrollIntoView?.({block:'nearest'});});}
  let attentionOriginId='';
  let origin:HTMLElement|undefined, detail:HTMLElement|undefined, list:HTMLElement|undefined;
  const sessionInputs = new Map<string,{navigation:WorkNavigation;goalQuery:string;goalId:string;repairId:string|null;note:string;locator:string;purpose:string;scope:string}>();
  const key = () => props.snapshot?.project ? JSON.stringify([props.snapshot.project.id,props.snapshot.project.environment]) : '';
  async function read(restore=false) {
    const project=props.snapshot?.project;if(!project||disposed)return;
    const token=++generation, intent=interaction, restoring=restore||restorePending;setLoading(true);
    const [work,records]=await Promise.allSettled([safely(()=>bridge.workProjection()),safely(()=>bridge.dailyWork())]);
    if(token!==generation||disposed||key()!==currentScope)return;
    if(work.status==='fulfilled') {
      const value=work.value;
      if(value.schemaVersion===1&&value.projectId===project.id&&value.environment===project.environment&&Array.isArray(value.items)){setProjection(value);setError('');}
      else setError('工作投影范围或版本不匹配，保留旧结果。');
    } else setError(errorMessage(work.reason));
    if(records.status==='fulfilled'&&records.value.projectId===project.id&&records.value.environment===project.environment&&records.value.schemaVersion===1){
      if(!personal()||records.value.revision>=personal()!.revision)setPersonal(records.value);setPersonalError('');
      if(restoring&&intent===interaction){restorePending=false;const transient=sessionInputs.get(currentScope);const saved=transient?.navigation??records.value.navigation;if(transient){setGoalQuery(transient.goalQuery);setGoalId(transient.goalId);setRepairId(transient.repairId);setNote(transient.note);setLocator(transient.locator);}setNav(saved.selected?saved:{...saved,draft:'',draftPurpose:'',draftScope:'',startIntentId:null,retainDraft:false});setPurpose(transient?.purpose??(saved.selected?saved.draftPurpose:''));setScopeText(transient?.scope??(saved.selected?saved.draftScope:''));setSelected(saved.selected?workKey(saved.selected):undefined);setDetailTab('overview');setAction('');queueMicrotask(()=>{if(intent===interaction){window.scrollTo?.(0,saved.selected?saved.detailScrollTop:saved.scrollTop);}});}
    }else setPersonalError(records.status==='rejected'?errorMessage(records.reason):'个人记录范围不匹配，未恢复。');
    setLoading(false);
  }
  createEffect(()=>{
    props.sourceGeneration;props.snapshot?.associations;props.snapshot?.decisions;
    const next=key();untrack(()=>{const changed=next!==currentScope;
    if(changed){stashInputs();if(currentScope)sessionInputs.set(currentScope,{navigation:{...nav(),selected:item()?.reference??null},goalQuery:goalQuery(),goalId:goalId(),repairId:repairId(),note:note(),locator:locator(),purpose:purpose(),scope:scopeText()});currentScope=next;setStatusFilter('all');restorePending=true;++generation;++interaction;setWorkPage(0);setDetailTab('overview');setAction('');setProjection(undefined);setPersonal(undefined);setSelected(undefined);setNav(emptyNavigation());setPersonalError('');setGoalQuery('');setGoalId('');setRepairId(null);setNote('');setLocator('');setPurpose('');setScopeText('');}
    if(next)void read(changed);});
  });
  onCleanup(()=>{disposed=true;++generation;});
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
  const githubSource=()=>projection()?.sources.find(source=>source.kind==='github');
  const sourceReading=()=>loading()||props.sourceBusy||githubSource()?.state==='loading';
  const needsConnection=()=>['unconnected','disconnected'].includes(githubSource()?.state??'');
  const sourceFailed=()=>!!error()||githubSource()?.state==='error';
  const githubAction=()=>needsConnection()?'连接 GitHub，读取工作':sourceFailed()?'重试读取 GitHub 工作':'重新读取 GitHub 工作';
  const sourceMessage=()=>sourceReading()?'正在读取工作来源；尚不能判断是否为空。':needsConnection()?'GitHub 尚未连接，已有 PR 工作还没有读取。':sourceFailed()?'工作来源读取失败；不能据此认定没有工作。':!githubSource()?'GitHub 工作来源尚未核实；请读取来源后查看已有工作。':githubSource()?.state==='partial'?'当前只读到部分来源，已读范围未列出工作；其他范围仍未知。':githubSource()?.state==='stale'?'当前来源已过期，已读范围未列出工作；请重新读取。':'当前 GitHub 已读取范围没有工作；其他页面及来源仍未知。';
  const sourcePrompt=()=><div class="empty work-source-prompt"><p role="status">{sourceMessage()}</p><Show when={githubSource()?.error}><p>{githubSource()!.error}</p></Show><button disabled={!!sourceReading()||!props.onConnectGithub} onClick={()=>props.onConnectGithub?.()}>{githubAction()}</button></div>;
  function enterExisting(){if(!works().length&&needsConnection()){if(!sourceReading())props.onConnectGithub?.();}else props.onExecution();}

  const textFiltered=createMemo(()=>works().filter(w=>!nav().filter||`${w.purpose.value} ${workStatus(w.result.value)}`.toLocaleLowerCase().includes(nav().filter.toLocaleLowerCase())));
  const filtered=createMemo(()=>textFiltered().filter(w=>statusFilter()==='all'||workGroup(w)===statusFilter()));
  function orderWork(values:WorkItem[],preferOpen=false){const recent=nav().recentWork??[];const rank=(w:WorkItem)=>{const i=recent.findIndex(r=>workKey(r)===workKey(w.reference));return i<0?recent.length:i;};return [...values].sort((a,b)=>Number(nav().pinned.some(p=>workKey(p)===workKey(b.reference)))-Number(nav().pinned.some(p=>workKey(p)===workKey(a.reference)))||rank(a)-rank(b)||(preferOpen?Number(workGroup(b)==='PR OPEN')-Number(workGroup(a)==='PR OPEN'):0));}
  const ordered=createMemo(()=>orderWork(filtered()));
  const homeOrdered=createMemo(()=>orderWork(works(),true));
  function allWork(){setStatusFilter('all');setWorkPage(0);changed({filter:''});props.onExecution();}

  const pageIndex=()=>Math.min(workPage(),Math.max(0,Math.ceil(ordered().length/20)-1));
  const pageItems=()=>ordered().slice(pageIndex()*20,(pageIndex()+1)*20);
  const goals=createMemo(()=>props.snapshot?.sections?.filter(s=>s.path==='doc/core/prd.md'&&/当前阶段交付目标与全局 P0/.test(s.title))??[]);
  const goalOptions=createMemo(()=>{
    const source=props.snapshot?.sections??[];const query=goalQuery().trim().toLowerCase();
    const selected=source.filter(s=>s.id===goalId());
    const found=query?source.filter(s=>`${s.title} ${s.path} ${s.id}`.toLowerCase().includes(query)).slice(0,30):goals();
    return [...new Map([...selected,...found].map(s=>[s.id,s])).values()];
  });
  const pinnedWorks=()=>homeOrdered().filter(w=>nav().pinned.some(p=>workKey(p)===workKey(w.reference)));
  const focusGoals=()=>goals().slice(0,Math.max(0,3-pinnedWorks().length));
  const focusSummary=(content:string)=>content.split('\n').map(s=>s.trim()).find(s=>s&&!s.startsWith('#')&&!s.startsWith('|')&&!s.startsWith('<a '))??'请查看主责原文，未推测下一动作。';
  const goalWorks=(id:string)=>works().filter(work=>{const overrides=personal()?.relations.filter(r=>r.sectionId===id&&workKey(r.work)===workKey(work.reference))??[];return overrides.length?overrides.some(r=>!r.removedAt):work.goals.some(g=>g.sectionId===id);});
  const shortGoal=(content:string)=>{const summary=focusSummary(content);return summary.length>120?summary.slice(0,120)+'…':summary;};
  const relations=()=>personal()?.relations.filter(r=>!r.removedAt&&workKey(r.work)===selected())??[];
  const conflicts=createMemo(()=>{
    const groups = new Map<string, {section:DocumentSection;claims:Map<string,WorkItem[]>}>();
    for(const section of props.snapshot?.sections??[]){
      if(!['doc/engineering/command-center/README.md','doc/engineering/command-center/architecture/README.md'].includes(section.path))continue;
      for(const claim of section.content.split('\n')){
        if(!/待合入|尚未合入|未合入/.test(claim))continue;
        for(const work of works())if(work.reference.kind==='pullRequest'&&work.result.value==='PR MERGED'&&new RegExp(`(?:pull/|#)${work.reference.number}(?:\\D|$)`).test(claim)){
          const key=JSON.stringify([section.id,section.revision]);
          let group=groups.get(key);
          if(!group){group={section,claims:new Map()};groups.set(key,group);}
          const matches=group.claims.get(claim)??[];
          if(!matches.some(item=>workKey(item.reference)===workKey(work.reference)))matches.push(work);
          group.claims.set(claim,matches);
        }
      }
    }
    return [...groups.values()].map(group=>({...group,claims:[...group.claims].map(([claim,works])=>({claim,works}))}));
  });
  const recent=()=>works().filter(w=>{const time=Date.parse(w.result.evidence.eventAt??'');const cutoff=nav().lastReadAt?Date.parse(nav().lastReadAt!):Date.now()-7*24*60*60*1000;return Number.isFinite(time)&&time>cutoff&&time<=Date.now();}).sort((a,b)=>Date.parse(b.result.evidence.eventAt!)-Date.parse(a.result.evidence.eventAt!));
  const observed=()=>works().filter(w=>!w.result.evidence.eventAt).slice(0,3);
  const openAttention=()=>personal()?.attention.filter(a=>a.state==='open')??[];
  const visibleAttention=()=>personal()?.attention.filter(a=>showHistory()||!['resolved','superseded'].includes(a.state))??[];
  const selectedAttention=()=>openAttention().filter(a=>workKey(a.work)===selected());
  const selectedPending=()=>selectedAttention().length;
  function handleRequests(){close();props.onView?.('attention');}
  function stashInputs(){if(!selected())return;workInputs.set(currentScope+selected()!,{draft:nav().draft,purpose:purpose(),scope:scopeText(),locator:locator(),note:note(),goalQuery:goalQuery(),goalId:goalId(),repairId:repairId(),retainDraft:nav().retainDraft,startIntentId:nav().startIntentId,tab:detailTab()});}
  function open(work:WorkItem,button?:HTMLElement){
    if(selected()===workKey(work.reference))return;
    stashInputs();setCopyStatus('');origin=button;
    const input=workInputs.get(currentScope+workKey(work.reference));
    const scrollTop=selected()?nav().scrollTop:window.scrollY;
    setWorkPage(0);setSelected(workKey(work.reference));setPurpose(input?.purpose??work.purpose.value);setScopeText(input?.scope??work.goals.map(g=>g.title??g.sectionId).join('；'));
    setLocator(input?.locator??'');setNote(input?.note??'');setGoalQuery(input?.goalQuery??'');setGoalId(input?.goalId??'');setRepairId(input?.repairId??null);
    setDetailTab(input?.tab??'overview');setAction('');
    changed({selected:work.reference,draft:input?.draft??'',draftPurpose:input?.purpose??work.purpose.value,draftScope:input?.scope??'',retainDraft:input?.retainDraft??false,startIntentId:input?.startIntentId??null,returnView:nav().selected?nav().returnView:props.view,scrollTop,recentWork:[work.reference,...(nav().recentWork??[]).filter(r=>workKey(r)!==workKey(work.reference))].slice(0,20)});
    void remember();queueMicrotask(()=>{detail?.focus({preventScroll:true});window.scrollTo?.(0,0);});
  }

  function close(){
    stashInputs();setAction('');++interaction;const id=selected(),returnView=nav().returnView;setSelected(undefined);changed({selected:null});
    const intent=interaction;
    const restoreFocus=()=>{if(interaction!==intent||selected())return;const current=Array.from(list?.querySelectorAll<HTMLButtonElement>('button[data-work-ref]')??[]).find(b=>b.dataset.workRef===id);const attentionOrigin=Array.from(document.querySelectorAll<HTMLElement>('[data-attention-ref]')).find(button=>button.dataset.attentionRef===attentionOriginId);const target=returnView==='attention'?attentionOrigin??origin:current??origin;if(target?.isConnected){target.focus({preventScroll:true});window.scrollTo?.(0,nav().scrollTop);}};
    void save({kind:'navigate',navigation:{...nav(),selected:null}}).then(()=>queueMicrotask(restoreFocus));
    if(returnView&&returnView!==props.view)props.onView?.(returnView);
    queueMicrotask(restoreFocus);
  }

  const savedLocator=(work:WorkItem)=>personal()?.external.find(e=>workKey(e.work)===workKey(work.reference));
  const [copyStatus,setCopyStatus]=createSignal('');
  async function copyLocator(value:string){try{if(!navigator.clipboard?.writeText)throw new Error('当前环境无法自动复制，请选中原定位文本手动复制');await navigator.clipboard.writeText(value);setCopyStatus('原定位已复制；源工作状态保持原样。');}catch(e){setError(errorMessage(e));}}
  const codexLink=(value:string)=>/^codex:\/\/threads\/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(value);
  async function openCodex(value:string){try{await bridge.openSource(value);setCopyStatus('已请求在 Codex 中打开原会话；执行与回答请在原工具完成。');}catch(e){setError(errorMessage(e));}}
  const nextAction=(work:WorkItem)=>work.sourceUrl?'回到原 PR 核对交付与剩余条件':savedLocator(work)?'回到原工具继续工作':'补充原工具定位';
  const row=(work:WorkItem)=><div class="row work-row" classList={{'is-selected':selected()===workKey(work.reference)}}><div class="row-content"><button class="row-title" data-work-ref={workKey(work.reference)} onClick={e=>open(work,e.currentTarget)}>{work.purpose.value}</button><p class="work-result"><StatusIcon value={work.result.value}/>{workStatus(work.result.value)}</p></div><button aria-label={`置顶 ${work.purpose.value}`} onClick={()=>void remember({pinned:nav().pinned.some(p=>workKey(p)===workKey(work.reference))?nav().pinned.filter(p=>workKey(p)!==workKey(work.reference)):[...nav().pinned,work.reference]})}><Pin size={16} aria-hidden="true"/>{nav().pinned.some(p=>workKey(p)===workKey(work.reference))?'取消置顶':'置顶'}</button></div>;
  return <Show when={props.snapshot?.project}>
    <Show when={['direction','execution','attention'].includes(props.view)}><section class="work-feedback" aria-label="工作来源状态"><Show when={error()}><p role="alert" class="notice">工作读取失败：{error()}。保留本项目最近结果。</p></Show><Show when={works().length&&sourceFailed()}><div class="notice"><p>工作来源读取失败；已有工作保留，请重试核对最新来源。</p><button disabled={!!sourceReading()||!props.onConnectGithub} onClick={()=>props.onConnectGithub?.()}>重试读取 GitHub 工作</button></div></Show><Show when={personalError()}><p role="alert" class="notice">{personalError()}<button onClick={()=>void read()}>重读个人记录</button></p></Show><Show when={personal()?.legacyChanged}><p class="notice">旧版个人决定或关联已变化；原记录保留，请在决定和关系详情核对后接纳变化。<button onClick={()=>void save({kind:'acceptLegacy'})}>已核对旧记录变化</button></p></Show></section></Show>
    <Show when={props.view==='direction'&&!selected()}><div class="home-grid">
      <section class="panel home-focus"><div class="panel-title"><h3>当前重点与下一步</h3><span class="tag">主责顺序 · 个人置顶</span></div><Show when={goals().length} fallback={<div class="empty">当前阶段主责尚未读取；先查看已有工作，不能从 PR 数量推算重点。</div>}><For each={focusGoals()}>{goal=><div class="panel-body"><div class="focus-route" aria-label="目标与下一步"><div class="focus-stop"><Target size={22} aria-hidden="true"/><div><span class="eyebrow">当前目标</span><button class="row-title" onClick={e=>props.showSource?.(goal,e.currentTarget)}>{goal.title}</button><MarkdownText text={shortGoal(goal.content)} sourcePath={goal.path} sections={props.snapshot?.sections} showSource={props.showSource}/></div></div><div class="focus-stop"><Link size={22} aria-hidden="true"/><div><span class="eyebrow">明确关联工作</span><p>{goalWorks(goal.id).length?`${goalWorks(goal.id).length} 项已读工作`:'尚无已读明确关联'}</p><span class="small muted">目标验收尚未核实</span></div></div><div class="focus-stop"><ArrowRight size={22} aria-hidden="true"/><div><span class="eyebrow">下一动作</span><button class="primary" onClick={e=>{const matches=goalWorks(goal.id);if(matches.length===1)open(matches[0],e.currentTarget);else enterExisting();}} disabled={!!sourceReading()&&!works().length}>{!works().length&&needsConnection()?'连接 GitHub，读取工作':goalWorks(goal.id).length===1?'进入关联工作':'选择工作，核对剩余条件'}</button></div></div></div><details><summary>完整目标与依据</summary><MarkdownText text={goal.content} sourcePath={goal.path} sections={props.snapshot?.sections} showSource={props.showSource}/></details></div>}</For></Show><For each={pinnedWorks().slice(0,3)}>{row}</For><Show when={pinnedWorks().length+goals().length>3}><div class="panel-body"><button onClick={props.onExecution}>查看其余重点与完整工作范围</button></div></Show></section>
      <Show when={openAttention().length}><section class="panel home-attention"><div class="panel-title"><h3>需要我处理</h3><span class="tag">{openAttention().length} 项个人问题</span></div><div class="panel-body"><For each={openAttention().slice(0,2)}>{a=><p>{a.question}</p>}</For><button onClick={()=>props.onView?.('attention')}>查看个人问题</button></div></section></Show>
      <section class="panel home-return" ref={list}><div class="panel-title"><h3>返回工作</h3><button disabled={loading()} onClick={()=>void read()}>刷新工作列表</button><button onClick={allWork}>查看全部工作</button></div><For each={nav().expanded?homeOrdered():homeOrdered().slice(0,3)}>{row}</For><Show when={homeOrdered().length>3}><div class="panel-body"><button onClick={()=>void remember({expanded:!nav().expanded})}>{nav().expanded?'收起其余工作':`显示其余 ${homeOrdered().length-3} 项工作`}</button></div></Show><Show when={!works().length}>{sourcePrompt()}</Show></section>
      <section class="panel home-recent"><div class="panel-title"><h3>最近成果变化</h3><button onClick={()=>void remember({lastReadAt:new Date().toISOString()})}>记录本次阅读基线</button></div><div class="panel-body"><p class="small muted">比较基线：{nav().lastReadAt?observedTime(nav().lastReadAt):'尚未建立；显示最近七天已读取事件，不宣称完整的自上次以来变化'}</p><For each={recent().slice(0,3)}>{w=><div class="timeline-entry"><button class="row-title" onClick={e=>open(w,e.currentTarget)}>{w.purpose.value}</button><p><StatusIcon value={w.result.value}/>{workStatus(w.result.value)} · {observedTime(w.result.evidence.eventAt)}</p><p class="small muted">合入事实只更新 PR 轨道；目标验收和部署仍按各自来源。</p></div>}</For><Show when={recent().length>3}><details><summary>查看其余 {recent().length-3} 条已读成果</summary><For each={recent().slice(3)}>{w=><div class="timeline-entry"><button onClick={e=>open(w,e.currentTarget)}>{w.purpose.value}</button><p>{observedTime(w.result.evidence.eventAt)}</p></div>}</For></details></Show><Show when={!recent().length}><p>本次已读窗口没有匹配事件；其他页及未接入来源仍未知。</p></Show><Show when={conflicts().length}><details class="summary-conflicts"><summary>有旧摘要待核对（{conflicts().length} 个章节）</summary><p class="small muted">GitHub 已更新相关 PR 合入事实；旧摘要保留供核对，不连带更新部署、目标验收或实现语义。</p><For each={conflicts()}>{group=><section class="summary-conflict"><h4>{group.section.title}</h4><button onClick={e=>props.showSource?.(group.section,e.currentTarget)}>查看旧摘要及原核验范围</button><For each={group.claims}>{entry=><div><MarkdownText text={entry.claim} sourcePath={group.section.path} sections={props.snapshot?.sections} showSource={props.showSource}/><div class="actions"><For each={entry.works}>{work=><button onClick={e=>open(work,e.currentTarget)}>查看 PR #{work.reference.kind==='pullRequest'?work.reference.number:''} 合入事实</button>}</For></div></div>}</For><details><summary>摘要来源与版本</summary><p>{group.section.path} · {group.section.revision}</p></details></section>}</For></details></Show><details><summary>事件时间未知的本次观察</summary><For each={observed()}>{w=><p><button aria-label={`查看未知事件时间的工作 ${w.purpose.value}`} onClick={e=>open(w,e.currentTarget)}>{w.purpose.value}</button> · <StatusIcon value={w.result.value}/>{workStatus(w.result.value)}</p>}</For></details></div></section>
    </div></Show>
    <Show when={['direction','execution','attention'].includes(props.view)}><div class="work-workspace" classList={{'has-selection':!!selected()&&props.view!=='attention'}}><Show when={props.view==='execution'||(!!selected()&&props.view!=='attention')}><section class="panel work-list" ref={list} aria-label="已有工作"><div class="panel-title"><h3>返回已有工作</h3><button disabled={loading()} onClick={()=>void read()}>刷新工作列表</button></div><div class="panel-body"><label>筛选工作<input value={nav().filter} onInput={e=>{setWorkPage(0);changed({filter:e.currentTarget.value});}} onBlur={()=>void remember()}/></label><p class="muted">置顶优先，再按明确打开记录返回；无记录的其余项保留来源顺序。</p><div class="work-status-filters" role="group" aria-label="工作状态筛选"><For each={statusFilters}>{([value,label])=><button aria-pressed={statusFilter()===value} onClick={()=>{setStatusFilter(value);setWorkPage(0);}}><Show when={value==='all'} fallback={<StatusIcon value={value==='other'?'unknown':value}/>}><ListFilter size={18} aria-hidden="true"/></Show><span>{label}</span><strong>{value==='all'?textFiltered().length:textFiltered().filter(work=>workGroup(work)===value).length}</strong></button>}</For></div><p class="small muted">数量仅为已读且命中文本筛选的工作，不代表全仓进度。</p></div><For each={pageItems()}>{row}</For><div class="work-pagination"><p class="small muted">当前已读匹配 {ordered().length} 项；每页最多 20 项，其他来源仍按覆盖说明。</p><Show when={ordered().length>20}><div class="actions"><button disabled={pageIndex()===0} onClick={()=>setWorkPage(Math.max(0,pageIndex()-1))}>上一页工作</button><span>第 {pageIndex()+1} / {Math.ceil(ordered().length/20)} 页</span><button disabled={(pageIndex()+1)*20>=ordered().length} onClick={()=>setWorkPage(pageIndex()+1)}>下一页工作</button></div></Show></div><Show when={!ordered().length&&!!works().length}><div class="empty"><p>当前筛选没有匹配；已有工作仍保留。</p><button onClick={()=>{setWorkPage(0);setStatusFilter('all');void remember({filter:''});}}>清空筛选</button></div></Show><Show when={!works().length}>{sourcePrompt()}</Show></section></Show>
    <Show when={['direction','execution','attention'].includes(props.view)}><Show when={item()} fallback={<Show when={selected()}><section class="panel"><p>原工作未在当前来源窗口中；不能据此认定完成或删除。请在开发执行按 PR 身份补读。</p><button onClick={close}>返回工作列表</button></section></Show>}>{work=><section class="work-detail" ref={detail} tabindex="-1" aria-label="工作详情">
      <div class="work-breadcrumb"><span>当前工作 · {props.snapshot?.project?.environment}</span><button onClick={close}>返回工作列表</button></div>
      <Show when={selectedPending()}><div class="notice"><p>{selectedPending()} 项个人问题：{selectedAttention()[0]?.question}</p><button onClick={handleRequests}>查看此工作的问题</button></div></Show><div class="work-heading"><h2>{work().purpose.value}</h2><span class="tag"><StatusIcon value={work().result.value}/>{workStatus(work().result.value)}</span></div>
      <div class="work-tabs" role="tablist" aria-label="工作阅读页签"><For each={[["overview","概览"],["basis","依据"],["history","历史"]]}>{([id,label])=><button id={`work-tab-${id}`} role="tab" aria-selected={detailTab()===id} aria-controls={`work-pane-${id}`} tabindex={detailTab()===id?0:-1} onClick={()=>setDetailTab(id)} onKeyDown={e=>{const tabs=['overview','basis','history'];let target=-1;if(e.key==='ArrowRight')target=(tabs.indexOf(id)+1)%3;if(e.key==='ArrowLeft')target=(tabs.indexOf(id)+2)%3;if(e.key==='Home')target=0;if(e.key==='End')target=2;if(target>=0){e.preventDefault();setDetailTab(tabs[target]);document.getElementById(`work-tab-${tabs[target]}`)?.focus();}}}>{label}</button>}</For></div>
      <Show when={detailTab()==='overview'}><div id="work-pane-overview" role="tabpanel" aria-labelledby="work-tab-overview">
        <div class="work-next"><div><p class="eyebrow">下一步</p><h3>{selectedPending()?'查看这项工作的个人问题':nextAction(work())}</h3></div><div class="work-primary"><Show when={selectedPending()} fallback={<><Show when={work().sourceUrl} fallback={<Show when={savedLocator(work())} fallback={<button class="primary" onClick={e=>openAction('locator',e.currentTarget)}>保存原工具定位</button>}>{entry=><><Show when={codexLink(entry().locator)}><button class="primary" onClick={()=>void openCodex(entry().locator)}>在 Codex 中打开原会话</button></Show><button classList={{primary:!codexLink(entry().locator)}} onClick={()=>void copyLocator(entry().locator)}>复制原工具定位</button></>}</Show>}><button class="primary" onClick={()=>void bridge.openSource(work().sourceUrl!).catch(e=>setError(errorMessage(e)))}>打开原 PR</button></Show><p class="small muted">打开入口只证明导航已请求，不代表来源工作完成。</p>
      <Show when={copyStatus()}><p role="status">{copyStatus()}</p></Show></>}><button class="primary" onClick={handleRequests}>查看个人问题（{selectedPending()}）</button></Show></div></div>
        <div class="work-unknown"><CircleHelp size={18} aria-hidden="true"/><div><strong>尚未核实</strong><p>{work().remaining}</p></div></div>
        <WorkRelations work={work()} relations={personal()?.relations.filter(r=>workKey(r.work)===selected())??[]} sections={props.snapshot?.sections??[]} showSource={props.showSource} openSource={()=>void bridge.openSource(work().sourceUrl!).catch(e=>setError(errorMessage(e)))} onBasis={()=>setDetailTab('basis')}/>
        <div class="work-observation"><h3>成果与最近观察</h3><p class="muted">{work().result.evidence.eventAt?'来源事件时间：'+observedTime(work().result.evidence.eventAt):'事件时间未知；本次读取不能视为新成果。'}</p><Show when={work().result.evidence.eventAt}><p><StatusIcon value={work().result.value}/>{workStatus(work().result.value)}</p></Show><button onClick={()=>setDetailTab('history')}>查看此工作历史与核验范围</button></div><div class="work-actions"><h3>工作操作</h3><div class="actions"><button onClick={e=>openAction('locator',e.currentTarget)}>保存原工具定位</button><button onClick={e=>openAction('question',e.currentTarget)}>记录真实问题</button><Show when={work().sourceUrl&&savedLocator(work())&&codexLink(savedLocator(work())!.locator)}><button onClick={()=>void openCodex(savedLocator(work())!.locator)}>在 Codex 中打开原会话</button></Show></div></div>
      </div></Show>
      <Show when={detailTab()==='basis'}><div id="work-pane-basis" role="tabpanel" aria-labelledby="work-tab-basis" class="work-basis"><h4>目标与依据</h4><For each={work().goals.filter(goal=>!personal()?.relations.some(r=>workKey(r.work)===selected()&&r.sectionId===goal.sectionId))}>{goal=><p><Show when={props.snapshot?.sections.find(s=>s.id===goal.sectionId)} fallback={<span>{goal.title??`原章节未读取或已变化 · ${goal.sectionId}`}</span>}>{section=><button data-source-ref={goal.sectionId} onClick={e=>props.showSource?.(section(),e.currentTarget)}>{goal.title??section().title}</button>}</Show> · 原个人确认 {observedTime(goal.confirmedAt)}</p>}</For><For each={relations()}>{relation=>{const section=()=>props.snapshot?.sections.find(s=>s.id===relation.sectionId);return <div class="row"><div><button data-source-ref={relation.sectionId} onClick={e=>{const s=section();if(s)props.showSource?.(s,e.currentTarget);}}>{section()?.title??`原条款未读取 · ${relation.sectionId}`}</button><p class="small muted">{!section()?'不可读，保留原关系':section()!.revision===relation.sourceRevision?'原依据版本仍匹配':'依据已变化，需核对差异；不按标题迁移'}</p></div><button onClick={e=>{setRepairId(relation.id);setGoalId(relation.sectionId);openAction('relation',e.currentTarget);}}>修复关系</button><button onClick={()=>void save({kind:'removeRelation',id:relation.id})}>移除个人关系</button></div>;}}</For>
      <details><summary>来源明确引用与关系历史</summary><p>只有正文明确说明服务目标的引用才可确认关系；普通链接是候选，不自动成为依赖。</p><For each={props.snapshot?.sections?.filter(s=>props.sourceGeneration&&String((props.sourceGeneration as GitHubSnapshot).pulls?.find(p=>p.url===work().sourceUrl)?.metadata?.body??'').includes(s.id)).slice(0,20)}>{s=><button onClick={()=>setGoalId(s.id)}>{s.title} · 正文出现原 ID，语义待确认</button>}</For><For each={personal()?.relations.filter(r=>workKey(r.work)===selected())}>{r=><p>{r.sectionId} · {r.removedAt?'个人已移除':'已确认'} · {observedTime(r.confirmedAt)} · 历史 {r.previous.length} 次</p>}</For></details>
<button onClick={e=>openAction('relation',e.currentTarget)}>关联目标或修复关系</button></div></Show>
      <Show when={detailTab()==='history'}><div id="work-pane-history" role="tabpanel" aria-labelledby="work-tab-history"><h3>来源事件与观察</h3><Show when={work().result.evidence.eventAt} fallback={<p>来源未提供事件时间；下面是观察记录，不计为近期成果。</p>}><div class="timeline-entry"><time>{observedTime(work().result.evidence.eventAt)}</time><p><StatusIcon value={work().result.value}/>{workStatus(work().result.value)}</p></div></Show><p class="muted">App 读取：{observedTime(work().result.evidence.readAt)}</p><For each={personal()?.attention.filter(a=>workKey(a.work)===selected())}>{a=><div class="timeline-entry"><time>{observedTime(a.updatedAt)}</time><p>{a.question} · <StatusIcon value={a.state}/>{attentionLabel(a.state)}</p></div>}</For>      <details><summary>来源身份与核验范围</summary><pre>{JSON.stringify(work().reference,null,2)}</pre><p>来源版本 {work().result.evidence.revision??'未知'}</p><p>来源事实核验 {observedTime(work().result.evidence.verifiedAt)}</p><p>事件时间 {work().result.evidence.eventAt??'未知；不宣称自上次以来的完整成果'}</p><p>App 读取 {observedTime(work().result.evidence.readAt)}</p><p>{work().result.evidence.coverage}</p></details>
</div></Show>
      <Show when={action()}><section ref={actionPanel} tabindex="-1" class="work-action-panel" aria-label="工作操作表单"><div class="panel-title"><h3>{{relation:'关联目标',locator:'原工具接续',question:'记录问题'}[action()]}</h3><button onClick={()=>{setAction('');queueMicrotask(()=>{const target=actionOrigin?.isConnected?actionOrigin:detail?.querySelector<HTMLElement>('.work-tabs [aria-selected=true]');target?.focus();});}}>取消操作</button></div><div class="panel-body editor"><Show when={action()==='relation'}>      <label>查找目标或规范<input value={goalQuery()} onInput={e=>setGoalQuery(e.currentTarget.value)} placeholder="输入标题、路径或条款 ID；不需要手填 PR 地址"/></label><label>{repairId()?'新的关系端点':'关联已读取条款'}<select value={goalId()} onChange={e=>setGoalId(e.currentTarget.value)}><option value="">选择明确条款</option><For each={goalOptions()}>{s=><option value={s.id}>{s.title} · {s.path}</option>}</For></select></label><button disabled={!goalId()||saving()||!personal()} onClick={()=>void save({kind:'relate',work:work().reference,sectionId:goalId(),replaceId:repairId()})}>{repairId()?'确认修复，保留原版本':'确认关联此工作'}</button><Show when={repairId()}><button onClick={()=>setRepairId(null)}>取消修复</button></Show>
</Show><Show when={action()==='locator'}>      <h4>原工具接续</h4><label>工具名称与明确定位<input value={locator()} onInput={e=>setLocator(e.currentTarget.value)} placeholder="codex://threads/会话 UUID，或原工具定位"/></label><button disabled={!locator().trim()||saving()} onClick={()=>void save({kind:'external',item:{id:workKey(work().reference),title:work().purpose.value,work:work().reference,locator:locator(),sourceUrl:work().sourceUrl,recordedAt:''}})}>保存原现场定位</button><For each={personal()?.external.filter(e=>workKey(e.work)===selected())}>{external=><div><p>{external.locator}</p><p class="small muted">{codexLink(external.locator)?'可在 Codex 中打开此原会话；执行和回答仍在原工具完成。':'此定位仅支持复制，请回到原工具继续。'}</p><Show when={codexLink(external.locator)}><button onClick={()=>void openCodex(external.locator)}>在 Codex 中打开原会话</button></Show><button onClick={()=>void copyLocator(external.locator)}>复制原定位</button><button onClick={()=>void save({kind:'removeExternal',id:external.id})}>移除此个人定位</button></div>}</For>
</Show><Show when={action()==='question'}>      <h4>记录真实问题</h4><label>需要我回答的问题<input value={note()} onInput={e=>setNote(e.currentTarget.value)}/></label><button disabled={!note().trim()||saving()} onClick={()=>void save({kind:'attention',item:{id:crypto.randomUUID(),work:work().reference,question:note(),sourceRevision:work().result.evidence.revision??'',state:'open',statement:'',updatedAt:'',history:[]}})}>保存为个人待答事项</button>
</Show></div></section></Show>
    </section>}</Show></Show>
    </div></Show>
    <Show when={props.view==='execution'&&!selected()}><details class="panel"><summary>接续仅有原工具定位的外部会话</summary><div class="panel-body editor"><p>保存普通工作名称与定位，不读取私有资料或外部消息，执行仍在原工具完成。可以单独保存，不要求已有 PR。</p><label>工作名称<input value={externalTitle()} onInput={e=>setExternalTitle(e.currentTarget.value)}/></label><label>原工具定位或 Codex 会话链接<input value={externalLocator()} onInput={e=>setExternalLocator(e.currentTarget.value)}/></label><button disabled={saving()||!personal()||!externalTitle().trim()||!externalLocator().trim()} onClick={()=>{const p=props.snapshot!.project!;const id=crypto.randomUUID();void save({kind:'external',item:{id,title:externalTitle(),work:{kind:'session',namespace:'external-locator',projectId:p.id,environment:p.environment,threadId:id},locator:externalLocator(),sourceUrl:null,recordedAt:''}}).then(()=>read());}}>保存原现场定位</button></div></details></Show>
    <Show when={props.view==='execution'&&!selected()}><details class="panel"><summary>仓库阅读范围与执行工作树</summary><div class="panel-body"><p>阅读聚合只包含明确选择并核实为同一仓库的工作树；执行仍使用原 Project / 环境身份，切换不会发送任务。</p><label>添加同仓库工作树绝对路径<input value={worktreePath()} onInput={e=>setWorktreePath(e.currentTarget.value)}/></label><button disabled={saving()||!personal()||!worktreePath().trim()} onClick={()=>{const state=personal();if(!state)return;const original=currentScope;setSaving(true);void bridge.addWorktreeReference(worktreePath(),state.revision).then(value=>{if(original===currentScope){setPersonal(value);void read();}}).catch(e=>setPersonalError(errorMessage(e))).finally(()=>setSaving(false));}}>核实并加入阅读范围</button><For each={personal()?.worktrees}>{reference=><div class="row"><div><p>{reference.path}</p><p class="small muted">{reference.environment} · 原核实 {observedTime(reference.verifiedAt)}；当前观察见来源覆盖。</p></div><button onClick={()=>props.onOpenProject?.(reference.path)}>切换到此原执行范围</button><button disabled={saving()} onClick={()=>void save({kind:'removeWorktree',projectId:reference.projectId})}>移出阅读范围</button></div>}</For></div></details></Show>
    <Show when={props.view==='attention'&&!selected()}><section class="panel"><div class="panel-title"><h3>个人问题与处理历史</h3><button onClick={()=>setShowHistory(!showHistory())}>{showHistory()?'仅显示未决':'查看处理历史'}</button></div><Show when={loading()&&!personal()}><p class="panel-body" role="status">正在读取个人问题…</p></Show><For each={visibleAttention()}>{a=><div class="panel-body"><h4>{a.question}</h4><p><StatusIcon value={a.state}/>{attentionLabel(a.state)} · {a.statement||'尚无处理陈述'}</p><button data-attention-ref={a.id} onClick={e=>{attentionOriginId=a.id;const work=works().find(w=>workKey(w.reference)===workKey(a.work));if(work)open(work,e.currentTarget);else{origin=e.currentTarget;setSelected(workKey(a.work));changed({selected:a.work,returnView:'attention'});}}}>进入原工作</button><Show when={a.state==='open'}><button onClick={()=>void save({kind:'attention',item:{...a,state:'awaitingEvidence',statement:'用户记已在原工具处理，源结果尚未回读核实'}})}>已在原工具处理，待核实</button><button onClick={()=>void save({kind:'attention',item:{...a,state:'resolved',statement:'用户明确回答此个人问题；不代表源工作完成'}})}>此个人问题已回答</button></Show><Show when={a.state==='awaitingEvidence'}><p>个人陈述不冒充运行回执；回到原工具核对来源。</p><button onClick={()=>void save({kind:'attention',item:{...a,state:'superseded',statement:'用户结束此个人关注，源状态仍未知'}})}>结束个人关注，源状态保持未知</button></Show></div>}</For><Show when={personal()&&!personalError()&&!visibleAttention().length}><div class="empty"><p>{showHistory()?'当前没有已记录的个人问题。':'当前没有未决个人问题。'}需要关注的事项可在工作详情中记录。</p><button onClick={props.onExecution}>查看工作</button></div></Show></section></Show>
    <Show when={['direction','execution','attention'].includes(props.view)}><details class="panel"><summary>工作来源覆盖与读取时间</summary><div class="panel-body"><For each={projection()?.sources}>{source=><p>{source.kind==='github'?'GitHub':source.kind==='personal'?'个人记录':source.kind} · {statusLabel(source.state)} · {source.coverage} {source.error}</p>}</For><p>当前个人记录 revision {personal()?.revision.toString()??'未读取'}；共享来源视图 {projection()?.viewRevision??'尚未发布版本'} · 组合个人版本 {projection()?.personalRevision??'未知'}；来源不是远端原子时刻。</p></div></details></Show>
  </Show>;
}
