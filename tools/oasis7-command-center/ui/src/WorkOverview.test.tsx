import { cleanup, fireEvent, render, screen, waitFor, within } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import WorkOverview from './WorkOverview';
import { bridge } from './bridge';
import type { Snapshot } from './types';
import type { DailyWorkState, WorkProjection } from './generated/core';
vi.mock('./bridge', async () => ({...await vi.importActual('./bridge'), bridge: {workProjection:vi.fn(),dailyWork:vi.fn(),saveDailyWork:vi.fn(),openSource:vi.fn()}}));
const snapshot=(id='p')=>({project:{id,path:'/repo',head:'head',environment:'local'},sections:[],decisions:[],associations:[]}) as unknown as Snapshot;
const state=(id='p'):DailyWorkState=>({schemaVersion:1,projectId:id,environment:'local',revision:0,navigation:{selected:null,filter:'',expanded:false,scrollTop:0,detailScrollTop:0,returnView:'direction',draft:'',draftPurpose:'',draftScope:'',retainDraft:false,startIntentId:null,lastReadAt:null,pinned:[]},relations:[],attention:[],external:[],worktrees:[],legacyDigest:'legacy',legacyChanged:false});
function projection(id='p'):WorkProjection {
 const reference={kind:'pullRequest',host:'github.com',repository:'o/r',number:'1'} as const;
 const evidence={source:reference,revision:'head',verifiedAt:'20',eventAt:null,readAt:'30',coverage:'recent 100 only'};
 return {schemaVersion:1,projectId:id,environment:'local',readAt:'30',sources:[{kind:'github',state:'partial',coverage:'recent 100 only',verifiedAt:'20',error:null}],items:[{reference,purpose:{value:'修复本地工具',evidence},result:{value:'PR MERGED',evidence},remaining:'目标验收尚未核实',sourceUrl:'https://github.com/o/r/pull/1',goals:[{sectionId:'a',title:'目标 A',revision:'r',confirmedAt:'10'},{sectionId:'b',title:null,revision:null,confirmedAt:'10'}]}]};
}
beforeEach(()=>{
 vi.mocked(bridge.workProjection).mockResolvedValue(projection());vi.mocked(bridge.dailyWork).mockResolvedValue(state());
 vi.mocked(bridge.saveDailyWork).mockImplementation(async request=>({...state(request.projectId),revision:request.expectedRevision+1,...(request.action.kind==='navigate'?{navigation:request.action.navigation}:{})}));
 vi.mocked(bridge.openSource).mockResolvedValue();
 vi.stubGlobal('scrollTo',vi.fn());
});
afterEach(()=>{cleanup();vi.resetAllMocks();vi.unstubAllGlobals();});
it('keeps the empty personal-attention panel out of the home and reads no execution source',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);
 for(const name of ['当前重点与下一步','最近成果变化','返回工作'])expect(screen.getByRole('heading',{name})).toBeInTheDocument();
 await screen.findByRole('button',{name:'修复本地工具'});
 expect(screen.getByText(/尚未建立；显示最近七天已读取事件/)).toBeInTheDocument();
 expect(screen.queryByRole('heading',{name:'需要我处理'})).not.toBeInTheDocument();
});
it('keeps one source detail across views and returns to its original view and current row',async()=>{
 const [view,setView]=createSignal('direction');
 render(()=><WorkOverview snapshot={snapshot()} view={view()} sourceGeneration={0} onExecution={()=>setView('execution')} onView={setView}/>);
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));await screen.findByRole('region',{name:'工作详情'});
 setView('execution');expect(screen.getByRole('region',{name:'工作详情'})).toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'打开原 PR'}));expect(bridge.openSource).toHaveBeenCalledWith('https://github.com/o/r/pull/1');
 fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));await waitFor(()=>expect(view()).toBe('direction'));
 await waitFor(()=>expect(screen.getByRole('button',{name:'修复本地工具'})).toHaveFocus());
});
it('retains readable source facts if personal records or a later source read fail',async()=>{
 vi.mocked(bridge.dailyWork).mockRejectedValue(new Error('future personal version'));
 vi.mocked(bridge.workProjection).mockResolvedValueOnce(projection()).mockRejectedValueOnce(new Error('offline'));
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));fireEvent.click(screen.getByRole('tab',{name:'依据'}));expect(screen.getByText('目标 A')).toBeInTheDocument();expect(screen.getByText(/原章节未读取或已变化 · b/)).toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'刷新工作列表'}));await screen.findByText(/工作读取失败：offline/);
 expect(screen.getByRole('region',{name:'工作详情'})).toBeInTheDocument();expect(screen.getAllByText(/recent 100 only/).length).toBeGreaterThan(0);
});
it('rejects an older project result and does not restore an old selection into a new scope',async()=>{
 let finish!:(value:WorkProjection)=>void;
 vi.mocked(bridge.workProjection).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValue(projection('other'));
 vi.mocked(bridge.dailyWork).mockResolvedValue(state('other'));
 const [source,setSource]=createSignal(snapshot());render(()=><WorkOverview snapshot={source()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await waitFor(()=>expect(finish).toBeTypeOf('function'));setSource(snapshot('other'));await screen.findByRole('button',{name:'修复本地工具'});
 const old=projection();old.items[0].purpose.value='旧项目工作';finish(old);await Promise.resolve();expect(screen.queryByText('旧项目工作')).not.toBeInTheDocument();
});
it('pages the work list in bounded groups without losing remaining entries',async()=>{
 const value=projection();value.items=Array.from({length:21},(_,i)=>({...value.items[0],reference:{kind:'pullRequest',host:'github.com',repository:'o/r',number:String(i+1)},purpose:{...value.items[0].purpose,value:`工作 ${i}`}}));vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('button',{name:'工作 0'});expect(screen.queryByRole('button',{name:'工作 20'})).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'下一页工作'}));expect(screen.getByRole('button',{name:'工作 20'})).toBeInTheDocument();
 expect(screen.queryByRole('button',{name:'工作 0'})).not.toBeInTheDocument();
});
it('keeps original PR navigation without local check controls',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
 expect(screen.queryByText(/CI|实际测试版本|GitHub 关联提交/)).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'打开原 PR'}));expect(bridge.openSource).toHaveBeenCalledWith('https://github.com/o/r/pull/1');
});
it('sends relation repair/removal with the original scope and expected revision',async()=>{
 const record=state();record.revision=7;record.relations=[{id:'relation',work:projection().items[0].reference,sectionId:'a',sourceRevision:'old',confirmedAt:'10',removedAt:null,previous:[]}];vi.mocked(bridge.dailyWork).mockResolvedValue(record);vi.mocked(bridge.saveDailyWork).mockImplementation(async request=>({...record,revision:request.expectedRevision+1}));
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalled());
 fireEvent.click(screen.getByRole('tab',{name:'依据'}));const button=screen.getByRole('button',{name:'移除个人关系'});await waitFor(()=>expect(button).toBeEnabled());fireEvent.click(button);
 await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalledWith(expect.objectContaining({projectId:'p',environment:'local',expectedRevision:8,action:{kind:'removeRelation',id:'relation'}})));
});
it('keeps recent explicit navigation ahead of source order without inventing activity',async()=>{
 const value=projection();value.items.push({...value.items[0],reference:{kind:'pullRequest',host:'github.com',repository:'o/r',number:'2'},purpose:{...value.items[0].purpose,value:'第二项工作'}});vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'第二项工作'}));await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalledWith(expect.objectContaining({action:expect.objectContaining({navigation:expect.objectContaining({recentWork:[value.items[1].reference]})})})));
 fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));const rows=within(screen.getByRole('region',{name:'已有工作'})).getAllByRole('button').filter(b=>b.hasAttribute('data-work-ref'));expect(rows.map(b=>b.textContent)).toEqual(['第二项工作','修复本地工具']);
});
it('uses copy-only capability as the external session primary action',async()=>{
 const value=projection(),reference={kind:'session',namespace:'external-locator',projectId:'p',environment:'local',threadId:'external'} as const;value.items=[{...value.items[0],reference,sourceUrl:null,purpose:{...value.items[0].purpose,value:'原外部现场'}}];const record=state();record.external=[{id:'e',title:'原外部现场',work:reference,locator:'Original Tool / original-id',sourceUrl:null,recordedAt:'1'}];vi.mocked(bridge.workProjection).mockResolvedValue(value);vi.mocked(bridge.dailyWork).mockResolvedValue(record);const writeText=vi.fn().mockResolvedValue(undefined);vi.stubGlobal('navigator',{clipboard:{writeText}});
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'原外部现场'}));expect(screen.queryByRole('button',{name:'核对原自管实例与操作'})).not.toBeInTheDocument();fireEvent.click(screen.getByRole('button',{name:'复制原工具定位'}));await screen.findByText('原定位已复制；源工作状态保持原样。');expect(writeText).toHaveBeenCalledWith('Original Tool / original-id');
});
it('only compares canonical status summaries and excludes quoted verification examples',async()=>{
 const source=snapshot();const clause={id:'summary',title:'真实摘要',path:'doc/engineering/command-center/README.md',content:'PR #1 尚未合入',revision:'old'};source.sections=[clause,{...clause,id:'report',title:'验收引用',path:'doc/engineering/command-center/daily-work-verification.manual.md'}] as Snapshot['sections'];render(()=><WorkOverview snapshot={source} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);await screen.findByText('有旧摘要待核对（1 个章节）');expect(screen.getAllByRole('button',{name:'查看旧摘要及原核验范围',hidden:true})).toHaveLength(1);
});

it('replaces the home with a reading workspace and opens editing only on request',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
 expect(screen.queryByRole('heading',{name:'当前重点与下一步'})).not.toBeInTheDocument();
 expect(screen.getByRole('tab',{name:'概览'})).toHaveAttribute('aria-selected','true');
 expect(screen.queryByLabelText('需要我回答的问题')).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('tab',{name:'依据'}));
 expect(screen.getByRole('tab',{name:'依据'})).toHaveAttribute('aria-selected','true');
 fireEvent.click(screen.getByRole('button',{name:'关联目标或修复关系'}));
 expect(screen.getByLabelText('查找目标或规范')).toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'取消操作'}));
 expect(screen.queryByLabelText('查找目标或规范')).not.toBeInTheDocument();await waitFor(()=>expect(screen.getByRole('button',{name:'关联目标或修复关系'})).toHaveFocus());
});
it('sorts recent events before previewing and excludes old and invalid timestamps',async()=>{
 const value=projection();const now=Date.now();
 value.items=Array.from({length:6},(_,i)=>{const item=structuredClone(value.items[0]);item.reference={kind:'pullRequest',host:'github.com',repository:'o/r',number:String(i+1)};item.purpose.value='成果 '+i;item.result.evidence.eventAt=i===4?'2020-01-01T00:00:00Z':i===5?'invalid':new Date(now-(4-i)*60000).toISOString();return item;});vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);
 const heading=screen.getByRole('heading',{name:'最近成果变化'});await screen.findByRole('button',{name:'成果 3'});
 const region=heading.closest('section')!;expect(within(region).queryByRole('button',{name:'成果 4'})).not.toBeInTheDocument();expect(within(region).queryByRole('button',{name:'成果 5'})).not.toBeInTheDocument();
 expect(within(region).getAllByRole('button').filter(b=>b.textContent?.startsWith('成果')).slice(0,3).map(b=>b.textContent)).toEqual(['成果 3','成果 2','成果 1']);
 expect(within(region).getByText('查看其余 1 条已读成果')).toBeInTheDocument();
});

it('filters by the readable Chinese result without changing source facts',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('button',{name:'修复本地工具'});fireEvent.input(screen.getByLabelText('筛选工作'),{target:{value:'已合入'}});
 expect(screen.getByRole('button',{name:'修复本地工具'})).toBeInTheDocument();
});

it('restores the detail top independently of the old work list scroll offset',async()=>{
 const record=state();record.navigation={...record.navigation,selected:projection().items[0].reference,scrollTop:1800,detailScrollTop:0};vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('region',{name:'工作详情'});await waitFor(()=>expect(window.scrollTo).toHaveBeenCalledWith(0,0));
 expect(window.scrollTo).not.toHaveBeenCalledWith(0,1800);
});

it('keeps retired execution drafts inert and exposes no runtime controls',async()=>{
 const record=state();record.navigation={...record.navigation,selected:projection().items[0].reference,draft:'old input',retainDraft:true,startIntentId:'old-intent'};vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('region',{name:'工作详情'});
 expect(screen.queryByRole('button',{name:/发起自管|明确发起|回复请求|中断/})).not.toBeInTheDocument();
 expect(screen.queryByLabelText('实际输入')).not.toBeInTheDocument();
 expect(bridge.openSource).not.toHaveBeenCalled();expect(bridge.saveDailyWork).not.toHaveBeenCalled();
});
it('opens only an explicit Codex thread link while keeping arbitrary locators copy-only',async()=>{
 const value=projection(),ref={kind:'session',namespace:'external-locator',projectId:'p',environment:'local',threadId:'external'} as const;
 value.items=[{...value.items[0],reference:ref,sourceUrl:null,purpose:{...value.items[0].purpose,value:'原Codex工作'}}];
 const record=state();const url='codex://threads/019ea241-685c-79b3-a0b0-97ac2aa9f7e2';record.external=[{id:'e',title:'原Codex工作',work:ref,locator:url,sourceUrl:null,recordedAt:'1'}];
 vi.mocked(bridge.workProjection).mockResolvedValue(value);vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'原Codex工作'}));fireEvent.click(screen.getByRole('button',{name:'在 Codex 中打开原会话'}));
 expect(bridge.openSource).toHaveBeenCalledWith(url);await screen.findByText('已请求在 Codex 中打开原会话；执行与回答请在原工具完成。');
});

it('connects GitHub from the home goal instead of navigating to an unread empty list',async()=>{
 const empty=projection();empty.items=[];empty.sources[0].state='unconnected';vi.mocked(bridge.workProjection).mockResolvedValue(empty);
 const source=snapshot();source.sections=[{id:'goal',path:'doc/core/prd.md',title:'当前阶段交付目标与全局 P0',content:'实现明确目标'}] as Snapshot['sections'];
 const connect=vi.fn(),execution=vi.fn();render(()=><WorkOverview snapshot={source} view="direction" sourceGeneration={0} onExecution={execution} onConnectGithub={connect}/>);
 await screen.findByText('GitHub 尚未连接，已有 PR 工作还没有读取。');
 const focus=screen.getByRole('heading',{name:'当前重点与下一步'}).closest('section')!;
 fireEvent.click(within(focus).getByRole('button',{name:'连接 GitHub，读取工作'}));expect(connect).toHaveBeenCalledTimes(1);expect(execution).not.toHaveBeenCalled();
});
it('distinguishes source loading, failed reads and a successful empty window',async()=>{
 const empty=projection();empty.items=[];empty.sources[0].state='empty';vi.mocked(bridge.workProjection).mockResolvedValue(empty);
 const [busy,setBusy]=createSignal(true);const connect=vi.fn();render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} sourceBusy={busy()} onExecution={()=>{}} onConnectGithub={connect}/>);
 await screen.findByText('正在读取工作来源；尚不能判断是否为空。');expect(screen.getByRole('button',{name:'重新读取 GitHub 工作'})).toBeDisabled();
 setBusy(false);await screen.findByText('当前 GitHub 已读取范围没有工作；其他页面及来源仍未知。');
 const failed=structuredClone(empty);failed.sources[0].state='error';failed.sources[0].error='登录失效';vi.mocked(bridge.workProjection).mockResolvedValue(failed);
 fireEvent.click(screen.getByRole('button',{name:'刷新工作列表'}));await screen.findByText('工作来源读取失败；不能据此认定没有工作。');expect(screen.getByText('登录失效')).toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'重试读取 GitHub 工作'}));expect(connect).toHaveBeenCalledTimes(1);
});
it('keeps known work visible after a refresh fails and offers retry',async()=>{
 vi.mocked(bridge.workProjection).mockResolvedValueOnce(projection()).mockRejectedValueOnce(new Error('offline'));
 const connect=vi.fn();render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}} onConnectGithub={connect}/>);
 await screen.findByRole('button',{name:'修复本地工具'});fireEvent.click(screen.getByRole('button',{name:'刷新工作列表'}));await screen.findByText(/工作读取失败：offline/);
 expect(screen.getByRole('button',{name:'修复本地工具'})).toBeInTheDocument();fireEvent.click(screen.getByRole('button',{name:'重试读取 GitHub 工作'}));expect(connect).toHaveBeenCalledTimes(1);
});
it('clears a zero-match filter and places secondary setup after the work list',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 const work=await screen.findByRole('button',{name:'修复本地工具'});expect(screen.queryByText(/^下一步：/)).not.toBeInTheDocument();
 const setup=screen.getByText('接续仅有原工具定位的外部会话');expect(work.compareDocumentPosition(setup)&Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
 fireEvent.input(screen.getByLabelText('筛选工作'),{target:{value:'没有匹配'}});expect(screen.queryByRole('button',{name:'修复本地工具'})).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'清空筛选'}));expect(screen.getByLabelText('筛选工作')).toHaveValue('');expect(screen.getByRole('button',{name:'修复本地工具'})).toBeInTheDocument();
});

it('groups repeated summary claims by section version and keeps every matching PR reachable',async()=>{
 const value=projection();value.items.push({...value.items[0],reference:{kind:'pullRequest',host:'github.com',repository:'o/r',number:'2'},sourceUrl:'https://github.com/o/r/pull/2'});
 vi.mocked(bridge.workProjection).mockResolvedValue(value);
 const source=snapshot();const clause={id:'summary',title:'原摘要章节',path:'doc/engineering/command-center/README.md',content:'PR #1 与 #2 尚未合入\nPR #1 与 #2 尚未合入',revision:'old'};
 source.sections=[clause,{...clause,id:'other',title:'另一章节',revision:'other-version'}] as Snapshot['sections'];
 const showSource=vi.fn();render(()=><WorkOverview snapshot={source} view="direction" sourceGeneration={0} onExecution={()=>{}} showSource={showSource}/>);
 const summary=await screen.findByText('有旧摘要待核对（2 个章节）');const details=summary.closest('details')!;
 expect(details.open).toBe(false);details.open=true;
 expect(within(details).getAllByText('PR #1 与 #2 尚未合入')).toHaveLength(2);
 expect(within(details).getAllByRole('button',{name:'查看 PR #1 合入事实'})).toHaveLength(2);
 expect(within(details).getAllByRole('button',{name:'查看 PR #2 合入事实'})).toHaveLength(2);
 fireEvent.click(within(details).getAllByRole('button',{name:'查看旧摘要及原核验范围'})[0]);expect(showSource).toHaveBeenCalledWith(expect.objectContaining({id:'summary',revision:'old'}),expect.anything());
 fireEvent.click(within(details).getAllByRole('button',{name:'查看 PR #2 合入事实'})[0]);
 await screen.findByRole('region',{name:'工作详情'});fireEvent.click(screen.getByRole('button',{name:'打开原 PR'}));
 expect(bridge.openSource).toHaveBeenCalledWith(value.items[1].sourceUrl);
});
it('shows personal questions before any general work and returns from the original work',async()=>{
 const record=state();record.attention=[{id:'question',work:projection().items[0].reference,question:'是否保留这项方案？',state:'open',statement:'',sourceRevision:'head',updatedAt:'now',history:[]}];
 vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 vi.mocked(bridge.saveDailyWork).mockImplementation(async request=>({...structuredClone(record),revision:request.expectedRevision+1}));
 const execution=vi.fn();render(()=><WorkOverview snapshot={snapshot()} view="attention" sourceGeneration={0} onExecution={execution}/>);
 const question=await screen.findByRole('heading',{name:'是否保留这项方案？'});expect(question).toBeInTheDocument();
 expect(screen.queryByRole('region',{name:'已有工作'})).not.toBeInTheDocument();expect(screen.queryByLabelText('筛选工作')).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'进入原工作'}));await screen.findByRole('region',{name:'工作详情'});
 expect(screen.queryByRole('region',{name:'已有工作'})).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));await waitFor(()=>expect(screen.getByRole('button',{name:'进入原工作'})).toHaveFocus());
 fireEvent.click(screen.getByRole('button',{name:'此个人问题已回答'}));await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalledWith(expect.objectContaining({action:expect.objectContaining({kind:'attention',item:expect.objectContaining({id:'question',state:'resolved',work:record.attention[0].work})})})));
});
it('uses the filtered attention empty state while preserving history and read failures',async()=>{
 const record=state();record.attention=[{id:'done',work:projection().items[0].reference,question:'已回答的问题',state:'resolved',statement:'已回答',sourceRevision:'head',updatedAt:'now',history:[]}];vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 const execution=vi.fn();render(()=><WorkOverview snapshot={snapshot()} view="attention" sourceGeneration={0} onExecution={execution}/>);
 await screen.findByText(/当前没有未决个人问题/);fireEvent.click(screen.getByRole('button',{name:'查看工作'}));expect(execution).toHaveBeenCalledOnce();
 fireEvent.click(screen.getByRole('button',{name:'查看处理历史'}));expect(screen.getByRole('heading',{name:'已回答的问题'})).toBeInTheDocument();expect(screen.queryByText(/当前没有未决个人问题/)).not.toBeInTheDocument();
 cleanup();vi.mocked(bridge.dailyWork).mockRejectedValue(new Error('记录读取失败'));
 render(()=><WorkOverview snapshot={snapshot()} view="attention" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByText(/记录读取失败/);expect(screen.queryByText(/当前没有.*个人问题/)).not.toBeInTheDocument();
});

it('maps explicit goals with missing and changed evidence and respects removed personal overrides',async()=>{
 const record=state();record.relations=[{id:'removed',work:projection().items[0].reference,sectionId:'b',sourceRevision:'r',confirmedAt:'10',removedAt:'20',previous:[]}];vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 const source=snapshot();source.sections=[{id:'a',title:'目标 A 当前条款',path:'doc/a.md',revision:'new',content:'current'}] as Snapshot['sections'];const showSource=vi.fn();
 render(()=><WorkOverview snapshot={source} view="execution" sourceGeneration={0} onExecution={()=>{}} showSource={showSource}/>);
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));const graph=within(screen.getByRole('region',{name:'工作关系图'}));
 expect(graph.getByText('依据版本已变化 · 打开当前原文')).toBeInTheDocument();expect(graph.queryByText('b')).not.toBeInTheDocument();
 fireEvent.click(graph.getByRole('button',{name:'目标 A 当前条款'}));expect(showSource).toHaveBeenCalledWith(source.sections[0],expect.any(HTMLElement));
 fireEvent.click(graph.getByRole('button',{name:'查看原 PR #1'}));expect(bridge.openSource).toHaveBeenCalledWith('https://github.com/o/r/pull/1');
});
it('keeps unread goal nodes noninteractive without inferring relationships',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));const graph=within(screen.getByRole('region',{name:'工作关系图'}));
 expect(graph.getAllByText('原文未读取 · 保留原关系')).toHaveLength(2);expect(graph.queryByRole('button',{name:'目标 A'})).not.toBeInTheDocument();
});

it('keeps an unlinked work compact without fabricating a goal graph',async()=>{
 const value=projection();value.items[0].goals=[];vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
 const graph=screen.getByRole('region',{name:'工作关系图'});
 expect(within(graph).getByText('尚无明确目标关系')).toBeVisible();expect(graph.querySelector('.relation-map')).toBeNull();
 expect(within(graph).queryByText('修复本地工具')).not.toBeInTheDocument();
 const facts=within(screen.getByRole('group',{name:'工作事实状态'}));expect(facts.getByText('PR 已合入')).toBeVisible();expect(facts.getByText('目标验收')).toBeVisible();expect(facts.getByText('尚未核实')).toBeVisible();
 const verification=screen.getByText('目标验收尚未核实').closest('details')!;expect(verification).not.toHaveAttribute('open');expect(screen.getByText('目标验收尚未核实')).not.toBeVisible();
 fireEvent.click(verification.querySelector('summary')!);expect(screen.getByText('目标验收尚未核实')).toBeVisible();
 fireEvent.click(within(graph).getByRole('button',{name:'选择依据并确认关联'}));expect(screen.getByRole('tab',{name:'依据'})).toHaveAttribute('aria-selected','true');
});
it('opens the sole explicit home goal work and preserves removed relationship overrides',async()=>{
 const source=snapshot();source.sections=[{id:'a',path:'doc/core/prd.md',title:'当前阶段交付目标与全局 P0',content:'明确目标与可恢复的剩余条件',revision:'r'}] as Snapshot['sections'];
 const record=state();const [records,setRecords]=createSignal(0);vi.mocked(bridge.dailyWork).mockImplementation(async()=>record);
 render(()=><WorkOverview snapshot={source} view="direction" sourceGeneration={records()} onExecution={()=>{}}/>);
 await screen.findByText('1 项已读工作');expect(screen.getAllByText('明确目标与可恢复的剩余条件').some(node=>node.closest('[aria-label="具体交付目标"]'))).toBe(true);fireEvent.click(screen.getByRole('button',{name:'进入关联工作'}));
 await screen.findByRole('region',{name:'工作详情'});fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));
 record.revision=10;record.relations=[{id:'removed',work:projection().items[0].reference,sectionId:'a',sourceRevision:'r',confirmedAt:'1',removedAt:'2',previous:[]}];
 setRecords(1);await screen.findByText('尚无已读明确关联');expect(screen.queryByRole('button',{name:'进入关联工作'})).not.toBeInTheDocument();
 expect(screen.getByRole('button',{name:'选择工作，核对剩余条件'})).toBeEnabled();
});

it('combines source-status selection with text search without persisting a new schema',async()=>{
 const value=projection();const first=value.items[0];value.items=[first,{...first,reference:{...first.reference,number:'2'} as typeof first.reference,purpose:{...first.purpose,value:'进行中的工作'},result:{...first.result,value:'PR OPEN'}},{...first,reference:{kind:'session',namespace:'external-locator',projectId:'p',environment:'local',threadId:'external'},purpose:{...first.purpose,value:'原外部工作'},result:{...first.result,value:'PR OPEN'}}];
 vi.mocked(bridge.workProjection).mockResolvedValue(value);
 const [source,setSource]=createSignal(snapshot());render(()=><WorkOverview snapshot={source()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('button',{name:'进行中的工作'});const statuses=within(screen.getByRole('group',{name:'工作状态筛选'}));
 expect(statuses.getByRole('button',{name:/进行中/})).toHaveTextContent('1');
 fireEvent.click(statuses.getByRole('button',{name:/进行中/}));expect(statuses.getByRole('button',{name:/进行中/})).toHaveAttribute('aria-pressed','true');
 expect(screen.queryByRole('button',{name:'修复本地工具'})).not.toBeInTheDocument();expect(screen.queryByRole('button',{name:'原外部工作'})).not.toBeInTheDocument();
 expect(bridge.saveDailyWork).not.toHaveBeenCalled();
 fireEvent.input(screen.getByLabelText('筛选工作'),{target:{value:'没有匹配'}});expect(statuses.getByRole('button',{name:/进行中/})).toHaveTextContent('0');
 fireEvent.click(screen.getByRole('button',{name:'清空筛选'}));expect(statuses.getByRole('button',{name:/全部/})).toHaveAttribute('aria-pressed','true');await screen.findByRole('button',{name:'原外部工作'});
 fireEvent.click(statuses.getByRole('button',{name:/外部或未核实/}));expect(screen.queryByRole('button',{name:'进行中的工作'})).not.toBeInTheDocument();
 vi.mocked(bridge.workProjection).mockResolvedValue(projection('other'));vi.mocked(bridge.dailyWork).mockResolvedValue(state('other'));setSource(snapshot('other'));
 await screen.findByRole('button',{name:'修复本地工具'});expect(statuses.getByRole('button',{name:/全部/})).toHaveAttribute('aria-pressed','true');
});
it('prioritizes pinned and recent home returns before open-work fallback independently of list filters',async()=>{
 const value=projection(),first=value.items[0];value.items.push({...first,reference:{...first.reference,number:'2'} as typeof first.reference,purpose:{...first.purpose,value:'正在继续的工作'},result:{...first.result,value:'PR OPEN'}},{...first,reference:{...first.reference,number:'3'} as typeof first.reference,purpose:{...first.purpose,value:'最近查看的旧工作'}});
 const record=state();record.navigation.pinned=[first.reference];record.navigation.recentWork=[value.items[2].reference];record.navigation.filter='没有匹配';vi.mocked(bridge.workProjection).mockResolvedValue(value);vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 const [view,setView]=createSignal('direction');render(()=><WorkOverview snapshot={snapshot()} view={view()} sourceGeneration={0} onExecution={()=>setView('execution')} onView={setView}/>);
 const home=screen.getByRole('heading',{name:'返回工作'}).closest('section')!;await within(home).findByRole('button',{name:'正在继续的工作'});
 expect(within(home).getAllByRole('button').filter(b=>b.hasAttribute('data-work-ref')).map(b=>b.textContent)).toEqual(['修复本地工具','最近查看的旧工作','正在继续的工作']);
 fireEvent.click(within(home).getByRole('button',{name:'查看全部工作'}));expect(screen.getByLabelText('筛选工作')).toHaveValue('');
 const statuses=within(screen.getByRole('group',{name:'工作状态筛选'}));fireEvent.click(statuses.getByRole('button',{name:/已关闭/}));setView('direction');
 expect(within(screen.getByRole('heading',{name:'返回工作'}).closest('section')!).getByRole('button',{name:'正在继续的工作'})).toBeInTheDocument();
});
it('uses open work as the home fallback while keeping source order within the same status',async()=>{
 const value=projection(),first=value.items[0];value.items.push({...first,reference:{...first.reference,number:'2'} as typeof first.reference,purpose:{...first.purpose,value:'进行中甲'},result:{...first.result,value:'PR OPEN'}},{...first,reference:{...first.reference,number:'3'} as typeof first.reference,purpose:{...first.purpose,value:'进行中乙'},result:{...first.result,value:'PR OPEN'}});vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);const home=screen.getByRole('heading',{name:'返回工作'}).closest('section')!;await within(home).findByRole('button',{name:'进行中甲'});
 expect(within(home).getAllByRole('button').filter(b=>b.hasAttribute('data-work-ref')).map(b=>b.textContent)).toEqual(['进行中甲','进行中乙','修复本地工具']);
});

it('folds selected-work filters without losing conditions and restores them on return',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('button',{name:'修复本地工具'});
 fireEvent.input(screen.getByLabelText('筛选工作'),{target:{value:'工具'}});
 fireEvent.click(screen.getByRole('button',{name:/已合入\s*1/}));
 fireEvent.click(screen.getByRole('button',{name:'修复本地工具'}));
 const filters=screen.getByLabelText('筛选工作').closest('details')!;
 expect(filters).not.toHaveAttribute('open');expect(filters.querySelector('summary')).toHaveTextContent('已合入 · 工具 · 1 项已读匹配');
 fireEvent.click(filters.querySelector('summary')!);expect(screen.getByLabelText('筛选工作')).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));
 expect(filters).toHaveAttribute('open');expect(screen.getByLabelText('筛选工作')).toHaveValue('工具');
 expect(screen.getByRole('button',{name:/已合入\s*1/})).toHaveAttribute('aria-pressed','true');
});
