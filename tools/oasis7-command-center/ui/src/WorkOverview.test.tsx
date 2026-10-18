import { cleanup, fireEvent, render, screen, waitFor, within } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import WorkOverview from './WorkOverview';
import { bridge, runtimeBridge } from './bridge';
import type { Snapshot } from './types';
import type { DailyWorkState, RuntimeSnapshot, WorkProjection } from './generated/core';
vi.mock('./bridge', async () => ({...await vi.importActual('./bridge'), bridge: {workProjection:vi.fn(),dailyWork:vi.fn(),saveDailyWork:vi.fn(),openSource:vi.fn(),refreshWorkChecks:vi.fn()},runtimeBridge:{publicSnapshot:vi.fn(),snapshot:vi.fn(),startWorkRecords:vi.fn(),startWork:vi.fn()},subscribeRuntime:async()=>()=>{}}));
const snapshot=(id='p')=>({project:{id,path:'/repo',head:'head',environment:'local'},sections:[],decisions:[],associations:[]}) as unknown as Snapshot;
const state=(id='p'):DailyWorkState=>({schemaVersion:1,projectId:id,environment:'local',revision:0,navigation:{selected:null,filter:'',expanded:false,scrollTop:0,detailScrollTop:0,returnView:'direction',draft:'',draftPurpose:'',draftScope:'',retainDraft:false,startIntentId:null,lastReadAt:null,pinned:[]},relations:[],decisions:[],attention:[],external:[],worktrees:[],legacyDigest:'legacy',legacyChanged:false});
function projection(id='p'):WorkProjection {
 const reference={kind:'pullRequest',host:'github.com',repository:'o/r',number:'1'} as const;
 const evidence={source:reference,revision:'head',verifiedAt:'20',eventAt:null,readAt:'30',coverage:'recent 100 only'};
 return {schemaVersion:1,projectId:id,environment:'local',readAt:'30',sources:[{kind:'github',state:'partial',coverage:'recent 100 only',verifiedAt:'20',error:null}],items:[{reference,purpose:{value:'修复本地工具',evidence},result:{value:'PR MERGED',evidence},remaining:'目标验收尚未核实',sourceUrl:'https://github.com/o/r/pull/1',goals:[{sectionId:'a',title:'目标 A',revision:'r',confirmedAt:'10'},{sectionId:'b',title:null,revision:null,confirmedAt:'10'}],checks:[{name:'test',state:'SUCCESS',testedOid:null,url:null}],runs:[]}]};
}
beforeEach(()=>{
 vi.mocked(bridge.workProjection).mockResolvedValue(projection());vi.mocked(bridge.dailyWork).mockResolvedValue(state());
 vi.mocked(bridge.saveDailyWork).mockImplementation(async request=>({...state(request.projectId),revision:request.expectedRevision+1,...(request.action.kind==='navigate'?{navigation:request.action.navigation}:{})}));
 vi.mocked(runtimeBridge.publicSnapshot).mockResolvedValue({projectId:'p',environment:'local',enabled:false,state:'disabled',sessions:[],requests:[],operations:[]} as unknown as RuntimeSnapshot);
 vi.mocked(runtimeBridge.startWorkRecords).mockResolvedValue([]);vi.mocked(bridge.openSource).mockResolvedValue();
 vi.stubGlobal('scrollTo',vi.fn());
});
afterEach(()=>{cleanup();vi.resetAllMocks();vi.unstubAllGlobals();});
it('presents four home regions with explicit unknown coverage and no eager CI or model action',async()=>{
 render(()=><WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={()=>{}}/>);
 for(const name of ['当前重点与下一步','最近成果变化','需要我处理','返回工作'])expect(screen.getByRole('heading',{name})).toBeInTheDocument();
 await screen.findByRole('button',{name:'修复本地工具'});
 expect(screen.getByText(/尚未建立；显示已读取合入事件/)).toBeInTheDocument();
 expect(runtimeBridge.snapshot).not.toHaveBeenCalled();expect(runtimeBridge.publicSnapshot).toHaveBeenCalled();
 expect(bridge.refreshWorkChecks).not.toHaveBeenCalled();expect(runtimeBridge.startWork).not.toHaveBeenCalled();
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
 fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));expect(screen.getByText('目标 A')).toBeInTheDocument();expect(screen.getByText(/原章节未读取或已变化 · b/)).toBeInTheDocument();
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
it('limits the default return list to three and retains its explicit expansion',async()=>{
 const value=projection();value.items=Array.from({length:4},(_,i)=>({...value.items[0],reference:{kind:'pullRequest',host:'github.com',repository:'o/r',number:String(i+1)},purpose:{...value.items[0].purpose,value:`工作 ${i}`}}));vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);
 await screen.findByRole('button',{name:'工作 0'});expect(screen.queryByRole('button',{name:'工作 3'})).not.toBeInTheDocument();
 fireEvent.click(screen.getByRole('button',{name:'显示其余 1 项工作'}));expect(screen.getByRole('button',{name:'工作 3'})).toBeInTheDocument();
 await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalledWith(expect.objectContaining({action:expect.objectContaining({kind:'navigate',navigation:expect.objectContaining({expanded:true})})})));
});
it('refreshes only selected PR checks and distinguishes empty from unread',async()=>{
 let value=projection();value.items[0].checks=[];vi.mocked(bridge.workProjection).mockImplementation(async()=>value);vi.mocked(bridge.refreshWorkChecks).mockResolvedValue({} as never);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
 expect(screen.getByText(/该 PR 的 CI 尚未单独读取/)).toBeInTheDocument();expect(bridge.refreshWorkChecks).not.toHaveBeenCalled();
 value=structuredClone(value);value.items[0].checksSource={schemaVersion:1,state:'empty',observedAt:'40',coverage:'two REST pages',error:null};
 await waitFor(()=>expect(screen.getByRole('button',{name:'只读刷新该 PR 的 CI'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'只读刷新该 PR 的 CI'}));await screen.findByText(/已读取的 CI 页面为空/);expect(bridge.refreshWorkChecks).toHaveBeenCalledWith('1',null);
});
it('never labels a GitHub associated SHA as the workflow checkout',async()=>{
 const value=projection();value.items[0].checks[0].sourceOid='associated';vi.mocked(bridge.workProjection).mockResolvedValue(value);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
 expect(screen.getByText(/实际测试版本 未知/)).toBeInTheDocument();expect(screen.getByText(/GitHub 关联提交 associated/)).toBeInTheDocument();expect(screen.getByText(/不代表实际 checkout 对象/)).toBeInTheDocument();
});
it('restores a persisted original intent as locked metadata and never sends on remount',async()=>{
 const record=state();record.navigation={...record.navigation,selected:projection().items[0].reference,draft:'retained ordinary input',draftPurpose:'work',draftScope:'local',retainDraft:true,startIntentId:'original-intent'};vi.mocked(bridge.dailyWork).mockResolvedValue(record);
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);await screen.findByRole('region',{name:'工作详情'});
 expect(screen.getByLabelText('实际输入')).toHaveValue('retained ordinary input');expect(screen.getByText(/当前预览已绑定原发起意图 original-intent/)).toBeInTheDocument();expect(runtimeBridge.startWork).not.toHaveBeenCalled();
});
it('sends relation repair/removal with the original scope and expected revision',async()=>{
 const record=state();record.revision=7;record.relations=[{id:'relation',work:projection().items[0].reference,sectionId:'a',sourceRevision:'old',confirmedAt:'10',removedAt:null,previous:[]}];vi.mocked(bridge.dailyWork).mockResolvedValue(record);vi.mocked(bridge.saveDailyWork).mockImplementation(async request=>({...record,revision:request.expectedRevision+1}));
 render(()=><WorkOverview snapshot={snapshot()} view="execution" sourceGeneration={0} onExecution={()=>{}}/>);fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalled());
 const button=screen.getByRole('button',{name:'移除个人关系'});await waitFor(()=>expect(button).toBeEnabled());fireEvent.click(button);
 await waitFor(()=>expect(bridge.saveDailyWork).toHaveBeenCalledWith(expect.objectContaining({projectId:'p',environment:'local',expectedRevision:8,action:{kind:'removeRelation',id:'relation'}})));
});
