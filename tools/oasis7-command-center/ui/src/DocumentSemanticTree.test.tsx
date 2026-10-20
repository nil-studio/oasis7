import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createSignal } from 'solid-js';
import DocumentSemanticTree from './DocumentSemanticTree';
import type { DocumentSemanticTreePage, DocumentSemanticTreeEntry, IndexedDocument, DocumentTreeReadingState, DocumentTreeRouteStep } from './generated/core';
import type { Snapshot } from './types';
const api=vi.hoisted(()=>({documentSemanticTree:vi.fn(),loadDocumentTreeReading:vi.fn(),saveDocumentTreeReading:vi.fn(),resetDocumentTreeReading:vi.fn()}));
vi.mock('./bridge',()=>({bridge:api,errorMessage:String,observedTime:()=>'核验时间'}));
const file:IndexedDocument={fileKey:'file',path:'doc/product/module/prd.md',title:'模块需求',kind:'prd',state:'read',reason:null,revision:'rev',bytes:10,dirty:false,lifecycle:null,identity:null,pairings:null,upstream:null,allocations:null,navigation:null};
const snapshot=(id='project',generation='1'):Snapshot=>({project:{id,path:'/repo',head:'head',branch:'main',gitCommonDir:'/repo/.git',environment:'local'},sources:[{kind:'local',generation,state:'partial',lastAttempt:'1',lastSuccess:'1',sourceRevision:'head',coverage:'',error:null}],sections:[],decisions:[],preferences:{projectPath:'/repo',query:'',view:'rules'},associations:[],readingPositions:[],git:{status:'',worktrees:''}});
const step=(path=file.path):DocumentTreeRouteStep=>({path,sourcePath:'doc/product/README.md',destination:path,group:null,duplicateIndex:0,duplicateCount:1});
const route=[step()];const childRoute=[...route,step('doc/product/module/topic.prd.md')];
const entry=(extra:Partial<DocumentSemanticTreeEntry>={}):DocumentSemanticTreeEntry=>({occurrenceId:'module',stableRoute:route,label:'模块需求',path:file.path,kind:'prd',matchingFiles:1,historicalReferenceFiles:0,historicalReferencePaths:[],boundary:null,cycle:false,depthLimited:false,sourcePath:'doc/product/README.md',sourceLine:51,relationship:null,file,hasChildren:true,...extra});
const child=entry({occurrenceId:'module/topic',stableRoute:childRoute,label:'专题需求',path:'doc/product/module/topic.prd.md',file:{...file,path:'doc/product/module/topic.prd.md'},hasChildren:false});
const page=(extra:Partial<DocumentSemanticTreePage>={}):DocumentSemanticTreePage=>({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',view:'product',parentOccurrence:'',parentRoute:[],routeNotice:null,breadcrumbs:[],candidates:10,read:9,omitted:1,stale:false,error:null,eligibleFiles:8,matchingFiles:2,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:1,entries:[entry()],nextCursor:null,issues:[],...extra});
const state=(extra:Partial<DocumentTreeReadingState>={}):DocumentTreeReadingState=>({filters:{query:'',readFilter:'all',lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all'},expandedRoutes:[],parentRoute:[],selection:null,treeScrollTop:0,detailScrollTop:0,...extra});
function treeApi(){api.documentSemanticTree.mockImplementation(async(query)=>query.parentOccurrence==='module'||query.parentRoute?.length===1?page({parentOccurrence:'module',parentRoute:route,entries:[child]}):page());}
beforeEach(()=>{Object.values(api).forEach(mock=>mock.mockReset());api.loadDocumentTreeReading.mockResolvedValue({status:'missing',state:null,notice:null,resolvedSectionId:null});api.saveDocumentTreeReading.mockResolvedValue(undefined);api.resetDocumentTreeReading.mockResolvedValue(undefined);treeApi();});afterEach(cleanup);
const ready=()=>waitFor(()=>expect(screen.getByRole('tree')).toHaveAttribute('aria-busy','false'));

it('expands in place with tree semantics and supports the full keyboard interaction',async()=>{
  const select=vi.fn();render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={select}/>);await ready();
  const root=screen.getByRole('treeitem',{name:'模块需求'});expect(root).toHaveAttribute('aria-expanded','false');expect(root).toHaveAttribute('aria-level','1');root.focus();
  fireEvent.keyDown(root,{key:'ArrowRight'});await screen.findByRole('treeitem',{name:'专题需求'});expect(root).toHaveAttribute('aria-expanded','true');expect(root).toHaveFocus();
  expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({parentOccurrence:'module',inventory:expect.objectContaining({snapshotId:'snapshot'})}));
  fireEvent.keyDown(root,{key:'ArrowRight'});await waitFor(()=>expect(screen.getByRole('treeitem',{name:'专题需求'})).toHaveFocus());
  const topic=screen.getByRole('treeitem',{name:'专题需求'});expect(topic).toHaveAttribute('aria-level','2');fireEvent.keyDown(topic,{key:'ArrowLeft'});await waitFor(()=>expect(root).toHaveFocus());
  fireEvent.keyDown(root,{key:'End'});await waitFor(()=>expect(topic).toHaveFocus());fireEvent.keyDown(topic,{key:'Home'});await waitFor(()=>expect(root).toHaveFocus());
  fireEvent.keyDown(root,{key:'ArrowDown'});await waitFor(()=>expect(topic).toHaveFocus());fireEvent.keyDown(topic,{key:'ArrowUp'});await waitFor(()=>expect(root).toHaveFocus());
  fireEvent.keyDown(root,{key:'Enter'});expect(select).toHaveBeenLastCalledWith(file,root,undefined);
  fireEvent.keyDown(root,{key:'ArrowLeft'});await waitFor(()=>expect(screen.queryByRole('treeitem',{name:'专题需求'})).not.toBeInTheDocument());expect(root).toHaveAttribute('aria-expanded','false');
});
it('does not turn composing Enter or arrows into tree actions',async()=>{
  const select=vi.fn();render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={select}/>);await ready();select.mockClear();const root=screen.getByRole('treeitem',{name:'模块需求'});
  fireEvent.keyDown(root,{key:'Enter',isComposing:true});fireEvent.keyDown(root,{key:'ArrowRight',keyCode:229});expect(select).not.toHaveBeenCalled();expect(root).toHaveAttribute('aria-expanded','false');expect(api.documentSemanticTree).toHaveBeenCalledTimes(1);
});
it('keeps combined applied filters and snapshot while appending each branch page',async()=>{
  api.documentSemanticTree.mockImplementation(async(query)=>page({kindFilter:query.kindFilter,pairingFilter:query.pairingFilter,lifecycleFilter:query.lifecycleFilter,nextCursor:query.inventory.cursor?null:'next',entries:query.inventory.cursor?[entry({occurrenceId:'second',stableRoute:[step('doc/second.md')],label:'第二模块',hasChildren:false})]:[entry()]}));
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();const input=screen.getByRole('textbox',{name:'筛选树文档'});
  fireEvent.input(input,{target:{value:'  专题  '}});fireEvent.compositionStart(input);fireEvent.submit(input.closest('form')!);expect(api.documentSemanticTree).toHaveBeenCalledTimes(1);fireEvent.compositionEnd(input);
  fireEvent.change(screen.getByRole('combobox',{name:'树文档类型'}),{target:{value:'design'}});fireEvent.change(screen.getByRole('combobox',{name:'树配对缺口'}),{target:{value:'missing'}});fireEvent.submit(input.closest('form')!);await ready();
  fireEvent.input(input,{target:{value:'未提交'}});fireEvent.click(screen.getByRole('button',{name:'下一页树（最多 100 项）'}));await screen.findByRole('treeitem',{name:'第二模块'});
  expect(screen.getByRole('treeitem',{name:'模块需求'})).toBeInTheDocument();expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({kindFilter:'design',pairingFilter:'missing',inventory:expect.objectContaining({query:'专题',snapshotId:'snapshot',cursor:'next'})}));
});
it('saves stable routes and captured project keys only after user interaction',async()=>{
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'展开：模块需求（匹配 1 个文件）'}));await screen.findByRole('treeitem',{name:'专题需求'});
  await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());const request=api.saveDocumentTreeReading.mock.calls.at(-1)![0];
  expect(request.key).toEqual({schemaVersion:1,projectId:'project',baselineMode:'tracked-working-tree',baselineOid:'head',view:'product'});expect(request.state.expandedRoutes).toEqual([route]);expect(JSON.stringify(request.state)).not.toContain('occurrenceId');
});
it('restores stable expansion and exact section without moving focus or saving a restored record',async()=>{
  const section={id:'explicit',path:file.path,title:'同名标题',revision:'rev',committedRevision:'head',dirty:false,content:'# 原文',line:1,factKind:'sourceFact',lifecycle:null};
  api.loadDocumentTreeReading.mockResolvedValue({status:'ready',state:state({expandedRoutes:[route],parentRoute:childRoute,selection:{route,path:file.path,sectionPath:file.path,sectionId:'explicit',sectionRevision:'rev'},treeScrollTop:60,detailScrollTop:42}),notice:null,resolvedSectionId:'explicit'});
  const select=vi.fn();const focus=document.createElement('button');document.body.append(focus);focus.focus();
  render(()=><DocumentSemanticTree snapshot={{...snapshot(),sections:[section]}} view="product" selectFile={select}/>);await ready();await screen.findByRole('treeitem',{name:'专题需求'});
  expect(select).toHaveBeenLastCalledWith(file,expect.any(HTMLElement),{section,scrollTop:42,passive:true});expect(focus).toHaveFocus();expect(screen.getByRole('tree')).toHaveProperty('scrollTop',60);expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();focus.remove();
});
it('does not reopen an equally titled chapter when its path or content version changed',async()=>{
  api.loadDocumentTreeReading.mockResolvedValue({status:'ready',state:state({selection:{route,path:file.path,sectionPath:file.path,sectionId:'explicit',sectionRevision:'older'}}),notice:null,resolvedSectionId:null});
  const select=vi.fn();render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={select}/>);await ready();expect(screen.getByText(/原章节或内容版本已变化/)).toBeInTheDocument();expect(select.mock.calls.at(-1)![2]).toEqual({section:undefined,scrollTop:0,passive:true});
});
it('protects unsupported records until explicit reset and never saves programmatic scroll',async()=>{
  api.loadDocumentTreeReading.mockResolvedValue({status:'unsupported',state:null,notice:'未来版本记录',resolvedSectionId:null});
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();fireEvent.scroll(screen.getByRole('tree'),{target:{scrollTop:100}});fireEvent.click(screen.getByRole('button',{name:'查看文档：模块需求'}));await Promise.resolve();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'重置此树阅读记录'}));await waitFor(()=>expect(api.resetDocumentTreeReading).toHaveBeenCalledWith(expect.objectContaining({projectId:'project',view:'product'})));await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());
});
it('cancels late personal restoration when the user has selected another file',async()=>{
  let finish!:(value:unknown)=>void;api.loadDocumentTreeReading.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));const select=vi.fn();render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={select}/>);
  fireEvent.click(await screen.findByRole('button',{name:'查看文档：模块需求'}));finish({status:'ready',state:state({expandedRoutes:[route]}),resolvedSectionId:null,notice:null});await Promise.resolve();expect(screen.queryByRole('treeitem',{name:'专题需求'})).not.toBeInTheDocument();expect(screen.getByRole('treeitem',{name:'模块需求'})).toHaveAttribute('aria-selected','true');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});
it('rejects late project and generation queries while personal snapshot updates preserve expansion',async()=>{
  let finish!:(value:DocumentSemanticTreePage)=>void;api.documentSemanticTree.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValue(page({entries:[]}));const[data,setData]=createSignal(snapshot());render(()=><DocumentSemanticTree snapshot={data()} view="product" selectFile={vi.fn()}/>);
  setData(snapshot('new'));await ready();finish(page());await Promise.resolve();expect(screen.queryByRole('treeitem',{name:'模块需求'})).not.toBeInTheDocument();
  setData({...data(),preferences:{...data().preferences,query:'personal'}});await Promise.resolve();expect(api.documentSemanticTree).toHaveBeenCalledTimes(2);setData(snapshot('new','2'));await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenCalledTimes(3));
});
it('retains lifecycle, historical references, diagnostics and bounded cross-domain leaves',async()=>{
  const historical={...file,lifecycle:{status:'retired',category:'retired',rawStatus:'retired',successorDeclaration:null,successorTargets:[],successorReason:null,issue:null}};
  api.documentSemanticTree.mockResolvedValue(page({stale:true,error:'scan failed',issues:['导航解析诊断样本'],entries:[entry({file:historical,historicalReferenceFiles:24,historicalReferencePaths:['doc/source.prd.md']}),entry({occurrenceId:'cycle',stableRoute:[step('doc/cycle.md')],label:'循环位置',cycle:true}),entry({occurrenceId:'cross',stableRoute:[step('doc/cross.md')],label:'跨域位置',boundary:'cross_domain',hasChildren:false})]}));
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();expect(screen.getByText('生命周期：已退役 · 原声明：retired')).toBeInTheDocument();expect(screen.getByText(/历史引用占位：被 24 个/)).toBeInTheDocument();expect(screen.getByText(/来源路径样本.*doc\/source.prd.md/)).toBeInTheDocument();expect(screen.getByText(/检测到循环引用/)).toBeInTheDocument();expect(screen.getByText(/跨域引用：/)).toBeInTheDocument();expect(screen.queryByRole('button',{name:/展开：循环位置/})).not.toBeInTheDocument();expect(screen.getByText(/树使用过期清单/)).toBeInTheDocument();
});
it('keeps original lifecycle paging and history reveal separate from unsubmitted drafts',async()=>{
  api.documentSemanticTree.mockResolvedValue(page({nextCursor:'next',hiddenHistoryFiles:1}));render(()=><DocumentSemanticTree snapshot={snapshot()} view="professional" selectFile={vi.fn()}/>);await ready();fireEvent.change(screen.getByRole('combobox',{name:'树生命周期'}),{target:{value:'raw'}});expect(screen.getByRole('button',{name:'筛选树'})).toBeDisabled();
  const raw=screen.getByRole('textbox',{name:'树生命周期原始声明'});fireEvent.input(raw,{target:{value:'target-contract'}});fireEvent.submit(raw.closest('form')!);await ready();fireEvent.input(raw,{target:{value:'未提交'}});fireEvent.click(screen.getByRole('button',{name:'下一页树（最多 100 项）'}));await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({lifecycleFilter:'raw:target-contract',inventory:expect.objectContaining({cursor:'next'})})));
  fireEvent.click(screen.getByRole('button',{name:'显示树历史匹配'}));await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({lifecycleFilter:'all'})));expect(screen.getByRole('combobox',{name:'树生命周期'})).toHaveValue('all');
});

it('does not let a late filter query end another project’s pending restoration or save its state',async()=>{
  let finishFilter!:(value:DocumentSemanticTreePage)=>void;let finishReading!:(value:unknown)=>void;
  api.documentSemanticTree.mockImplementation(async(query)=>query.inventory.projectId==='project'&&query.inventory.query==='slow'?new Promise(resolve=>{finishFilter=resolve;}):page({projectId:query.inventory.projectId}));
  api.loadDocumentTreeReading.mockResolvedValueOnce({status:'missing',state:null,notice:null,resolvedSectionId:null}).mockImplementation(()=>new Promise(resolve=>{finishReading=resolve;}));
  const[data,setData]=createSignal(snapshot());render(()=><DocumentSemanticTree snapshot={data()} view="product" selectFile={vi.fn()}/>);await ready();
  const input=screen.getByRole('textbox',{name:'筛选树文档'});fireEvent.input(input,{target:{value:'slow'}});fireEvent.submit(input.closest('form')!);setData(snapshot('new'));
  await waitFor(()=>expect(api.loadDocumentTreeReading).toHaveBeenCalledTimes(2));finishFilter(page());await Promise.resolve();expect(screen.getByRole('tree')).toHaveAttribute('aria-busy','true');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
  finishReading({status:'missing',state:null,notice:null,resolvedSectionId:null});await ready();expect(screen.getByRole('textbox',{name:'筛选树文档'})).toHaveValue('');
});
it('virtualizes appended long levels while keeping full positions and keyboard End navigation',async()=>{
  const entries=Array.from({length:200},(_,i)=>entry({occurrenceId:`node-${i}`,stableRoute:[step(`doc/${i}.prd.md`)],label:`文档 ${i}`,hasChildren:false}));
  api.documentSemanticTree.mockImplementation(async(query)=>page({totalChildren:200,entries:query.inventory.cursor?entries.slice(100):entries.slice(0,100),nextCursor:query.inventory.cursor?null:'next'}));
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();fireEvent.click(screen.getByRole('button',{name:'下一页树（最多 100 项）'}));await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenCalledTimes(2));await ready();
  expect(screen.getAllByRole('treeitem').length).toBeLessThan(120);const first=screen.getByRole('treeitem',{name:'文档 0'});first.focus();fireEvent.keyDown(first,{key:'End'});
  const last=await screen.findByRole('treeitem',{name:'文档 199'});await waitFor(()=>expect(last).toHaveFocus());expect(last).toHaveAttribute('aria-posinset','200');expect(last).toHaveAttribute('aria-setsize','200');
});
it('keeps expired snapshot errors explicit until the user requests an unbound root',async()=>{
  api.documentSemanticTree.mockResolvedValueOnce(page()).mockRejectedValueOnce(new Error('snapshot expired')).mockResolvedValue(page());render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();
  fireEvent.click(screen.getByRole('button',{name:'展开：模块需求（匹配 1 个文件）'}));await screen.findByRole('alert');expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({inventory:expect.objectContaining({snapshotId:'snapshot'})}));
  fireEvent.click(screen.getByRole('button',{name:'重新查询树根'}));await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({parentOccurrence:'',inventory:expect.objectContaining({snapshotId:null})})));
});

it('does not complete passive restore after a newer interaction elsewhere in the app',async()=>{
  let finish!:(value:unknown)=>void;api.loadDocumentTreeReading.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));const select=vi.fn();const outside=document.createElement('input');document.body.append(outside);
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={select}/>);await waitFor(()=>expect(api.loadDocumentTreeReading).toHaveBeenCalled());outside.focus();fireEvent.keyDown(outside,{key:'x'});finish({status:'ready',state:state({selection:{route,path:file.path,sectionPath:null,sectionId:null,sectionRevision:null}}),resolvedSectionId:null,notice:null});await Promise.resolve();
  expect(select.mock.calls.every(call=>call[0]===undefined)).toBe(true);expect(outside).toHaveFocus();expect(screen.getByText(/已停止自动恢复，保留本次手动选择/)).toBeInTheDocument();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();outside.remove();
});
it('distinguishes a filtered original location from deletion without changing saved filters',async()=>{
  api.loadDocumentTreeReading.mockResolvedValue({status:'ready',state:state({filters:{query:'filtered',readFilter:'all',lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all'},selection:{route,path:file.path,sectionPath:null,sectionId:null,sectionRevision:null}}),resolvedSectionId:null,notice:null});
  api.documentSemanticTree.mockImplementation(async(query)=>query.parentRoute?.length?page({parentOccurrence:'module',parentRoute:route,entries:[]}):page({entries:query.inventory.query?[]:[entry()]}));
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();expect(screen.getByText(/原位置仍存在，但被当前筛选隐藏/)).toBeInTheDocument();expect(screen.getByRole('textbox',{name:'筛选树文档'})).toHaveValue('filtered');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});

it('does not let an old saved-filter restore clear a newer source’s pending reading state',async()=>{
  let finishFiltered!:(value:DocumentSemanticTreePage)=>void;let finishNew!:(value:unknown)=>void;
  api.loadDocumentTreeReading.mockResolvedValueOnce({status:'ready',state:state({filters:{query:'saved-filter',readFilter:'all',lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all'}}),notice:null,resolvedSectionId:null}).mockImplementation(()=>new Promise(resolve=>{finishNew=resolve;}));
  api.documentSemanticTree.mockImplementation(async(query)=>query.inventory.projectId==='project'&&query.inventory.query==='saved-filter'?new Promise(resolve=>{finishFiltered=resolve;}):page({projectId:query.inventory.projectId}));
  const[data,setData]=createSignal(snapshot());render(()=><DocumentSemanticTree snapshot={data()} view="product" selectFile={vi.fn()}/>);await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenCalledTimes(2));setData(snapshot('new'));await waitFor(()=>expect(api.loadDocumentTreeReading).toHaveBeenCalledTimes(2));
  finishFiltered(page());await Promise.resolve();expect(screen.getByRole('tree')).toHaveAttribute('aria-busy','true');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();finishNew({status:'missing',state:null,notice:null,resolvedSectionId:null});await ready();
});

it('does not expand an old last-level restore when the new project reuses the same occurrence ID',async()=>{
  let finish!:(value:DocumentSemanticTreePage)=>void;
  api.loadDocumentTreeReading.mockResolvedValueOnce({status:'ready',state:state({expandedRoutes:[route]}),notice:null,resolvedSectionId:null}).mockResolvedValue({status:'missing',state:null,notice:null,resolvedSectionId:null});
  api.documentSemanticTree.mockImplementation(async(query)=>query.inventory.projectId==='project'&&query.parentOccurrence==='module'?new Promise(resolve=>{finish=resolve;}):page({projectId:query.inventory.projectId}));
  const[data,setData]=createSignal(snapshot());render(()=><DocumentSemanticTree snapshot={data()} view="product" selectFile={vi.fn()}/>);await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenCalledTimes(2));
  setData(snapshot('new'));await ready();expect(screen.getByRole('treeitem',{name:'模块需求'})).toHaveAttribute('aria-expanded','false');
  finish(page({parentOccurrence:'module',parentRoute:route,entries:[child]}));await Promise.resolve();await Promise.resolve();expect(screen.getByRole('treeitem',{name:'模块需求'})).toHaveAttribute('aria-expanded','false');expect(screen.queryByRole('treeitem',{name:'专题需求'})).not.toBeInTheDocument();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});
it('leaves a restored branch collapsed when its child query failed',async()=>{
  api.loadDocumentTreeReading.mockResolvedValue({status:'ready',state:state({expandedRoutes:[route]}),notice:null,resolvedSectionId:null});api.documentSemanticTree.mockResolvedValueOnce(page()).mockRejectedValue(new Error('branch unavailable'));
  render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();expect(screen.getByRole('treeitem',{name:'模块需求'})).toHaveAttribute('aria-expanded','false');expect(screen.getByRole('alert')).toHaveTextContent('branch unavailable');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});

it('uses actual root entries as a map and switches to hierarchical rows on expansion',async()=>{
 render(()=><DocumentSemanticTree snapshot={snapshot()} view="product" selectFile={vi.fn()}/>);await ready();
 const tree=screen.getByRole('tree');expect(tree).toHaveClass('product-domain-map');expect(screen.getAllByRole('treeitem')).toHaveLength(1);expect(screen.getByText('个匹配文件')).toBeInTheDocument();
 const advanced=screen.getByText('高级筛选').closest('details')!;expect(advanced).not.toHaveAttribute('open');fireEvent.click(screen.getByText('高级筛选'));expect(advanced).toHaveAttribute('open');
 fireEvent.click(screen.getByRole('button',{name:'展开：模块需求（匹配 1 个文件）'}));await screen.findByRole('treeitem',{name:'专题需求'});expect(tree).not.toHaveClass('product-domain-map');
});
