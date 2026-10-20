import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createSignal } from 'solid-js';
import DocumentInventory from './DocumentInventory';
const renderInventory: typeof render = ((...args: Parameters<typeof render>) => { const result=render(...args);fireEvent.click(screen.getByRole('button',{name:'文件清单'}));return result; }) as typeof render;
import type { Snapshot, DocumentSection } from './types';
import type { DocumentInventoryPage, IndexedDocument } from './generated/core';
const api = vi.hoisted(() => ({ documentInventory: vi.fn(), documentDirectory: vi.fn(), documentSemanticTree: vi.fn(), loadDocumentTreeReading:vi.fn(),saveDocumentTreeReading:vi.fn(),loadDocumentTreeLastView:vi.fn(),resetDocumentTreeReading:vi.fn() }));
vi.mock('./bridge', () => ({ bridge: api, errorMessage: (error: unknown) => String(error), observedTime: () => '核验时间' }));
const section: DocumentSection = { id: 'source', path: 'doc/design.md', title: '设计章节', revision: 'rev', committedRevision: 'head', dirty: false, content: '# 设计', line: 1, factKind: 'sourceFact', lifecycle: null };
const file = (path = 'doc/design.md', extra: Partial<IndexedDocument> = {}): IndexedDocument => ({
  fileKey: path, path, title: path === section.path ? '设计文档' : path,
  kind: 'design', state: 'read', reason: null, revision: 'rev', bytes: 10, dirty: false, lifecycle: null, pairings: null, upstream: null, identity: null, allocations: null, navigation: null, ...extra,
});
const snapshot = (id = 'project', generation = '1'): Snapshot => ({
  project: { id, path: '/repo', head: 'head', branch: 'main', gitCommonDir: '/repo/.git', environment: 'local' },
  sources: [{ kind: 'local', generation, state: 'partial', lastAttempt: '1', lastSuccess: '1', sourceRevision: 'head', coverage: '', error: null }],
  sections: [section], decisions: [], preferences: { projectPath: '/repo', query: '', view: 'rules' }, associations: [], readingPositions: [], git: {status:'',worktrees:''},
});
const page = (files = [file()], extra: Partial<DocumentInventoryPage> = {}): DocumentInventoryPage => ({
  schemaVersion: 1, projectId: 'project', snapshotId: 'snapshot', baselineOid: 'head', generatedAt: '1',
  candidates: 3, read: 1, omitted: 2, readBytes: 10, totalMatches: files.length,
  partial: true, stale: false, error: null, files, nextCursor: null, ...extra,
});
beforeEach(() => { api.documentInventory.mockReset(); api.documentSemanticTree.mockReset();api.loadDocumentTreeLastView.mockResolvedValue({status:'missing',view:null,notice:null});api.loadDocumentTreeReading.mockResolvedValue({status:'missing',state:null,notice:null,resolvedSectionId:null});api.saveDocumentTreeReading.mockResolvedValue(undefined); });
afterEach(cleanup);
it('shows per-file omissions and opens only chapters from the matching file revision', async () => {
  api.documentInventory.mockResolvedValue(page([file(), file('doc/empty.md', {bytes:0}), file('doc/deleted.md', {state:'deleted',reason:'deleted',revision:null,bytes:null})]));
  const showSource = vi.fn();
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={showSource} />);
  await screen.findByRole('button', {name:'设计文档'});
  fireEvent.click(screen.getByRole('button', {name:'设计文档'}));
  fireEvent.click(screen.getByRole('button', {name:'设计章节 · 第 1 行'}));
  expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('设计');expect(showSource).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', {name:'doc/empty.md'}));
  expect(screen.getByText('已读取，文件没有可展示的章节。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/deleted.md'}));
  expect(screen.queryByRole('button', {name:/第 1 行/})).not.toBeInTheDocument();
  expect(screen.getAllByText(/工作树文件已删除/).length).toBeGreaterThan(0);
});
it('binds paging to applied filters and snapshot, and preserves IME composition', async () => {
  api.documentInventory.mockResolvedValueOnce(page()).mockResolvedValue(page([file()], {nextCursor:'next'}));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  await screen.findByRole('button', {name:'设计文档'});
  const input = screen.getByRole('textbox', {name:'筛选文档文件'});
  fireEvent.input(input, {target:{value:'设计'}});
  fireEvent.compositionStart(input);
  fireEvent.submit(input.closest('form')!);
  expect(api.documentInventory).toHaveBeenCalledTimes(1);
  fireEvent.compositionEnd(input);
  fireEvent.change(screen.getByRole('combobox'), {target:{value:'omitted'}});
  fireEvent.click(screen.getByRole('button', {name:'应用筛选'}));
  await waitFor(() => expect(api.documentInventory).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.getByRole('button', {name:/下一页/})).toBeEnabled());
  // Unsubmitted edits must not silently change the next cursor's filter.
  fireEvent.input(input, {target:{value:'未提交的新筛选'}});
  fireEvent.click(screen.getByRole('button', {name:/下一页/}));
  await waitFor(() => expect(api.documentInventory).toHaveBeenCalledTimes(3));
  expect(api.documentInventory).toHaveBeenLastCalledWith({ schemaVersion:1, projectId:'project', snapshotId:'snapshot', query:'设计', readFilter:'omitted', cursor:'next' });
});
it('rejects late responses after a project switch and after a same-project refresh', async () => {
  let late!: (value: DocumentInventoryPage) => void;
  api.documentInventory.mockImplementationOnce(() => new Promise(resolve => {late = resolve;})).mockResolvedValue(page([file('doc/new.md')]));
  const [state, setState] = createSignal(snapshot());
  renderInventory(() => <DocumentInventory snapshot={state()} showSource={vi.fn()} />);
  setState(snapshot('other', '2'));
  await screen.findByRole('button', {name:'doc/new.md'});
  late(page([file('doc/old.md')]));
  await Promise.resolve();
  expect(screen.queryByRole('button', {name:'doc/old.md'})).not.toBeInTheDocument();
  api.documentInventory.mockImplementationOnce(() => new Promise(resolve => {late = resolve;})).mockResolvedValue(page([file('doc/refreshed.md')]));
  fireEvent.click(screen.getByRole('button', {name:'重新查询首页'}));
  setState(snapshot('other', '3'));
  await screen.findByRole('button', {name:'doc/refreshed.md'});
  late(page([file('doc/obsolete.md')]));
  await Promise.resolve();
  expect(screen.queryByRole('button', {name:'doc/obsolete.md'})).not.toBeInTheDocument();
});
it('distinguishes cache failure from empty results and keeps stale coverage visible', async () => {
  api.documentInventory.mockRejectedValueOnce(new Error('清单缺失')).mockResolvedValue(page([], {stale:true,error:'源读取失败',totalMatches:0}));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  await screen.findByRole('alert');
  expect(screen.queryByText(/当前筛选没有匹配文件/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'重新查询首页'}));
  await screen.findByText(/当前筛选没有匹配文件/);
  expect(screen.getByText(/清单已过期/)).toBeInTheDocument();
});

it('preserves filters, page and file selection when only personal snapshot data changes', async () => {
  api.documentInventory.mockResolvedValue(page([file()], {nextCursor:'next'}));
  const [state, setState] = createSignal(snapshot());
  renderInventory(() => <DocumentInventory snapshot={state()} showSource={vi.fn()} />);
  await screen.findByRole('button', {name:'设计文档'});
  const input = screen.getByRole('textbox', {name:'筛选文档文件'});
  fireEvent.input(input, {target:{value:'设计'}});
  fireEvent.click(screen.getByRole('button', {name:'应用筛选'}));
  await waitFor(() => expect(api.documentInventory).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.getByRole('button', {name:/下一页/})).toBeEnabled());
  fireEvent.click(screen.getByRole('button', {name:/下一页/}));
  await waitFor(() => expect(api.documentInventory).toHaveBeenCalledTimes(3));
  fireEvent.click(await screen.findByRole('button', {name:'设计文档'}));
  expect(screen.getByRole('region', {name:'文件读取详情'})).toBeInTheDocument();
  setState({...snapshot(), preferences:{...snapshot().preferences, query:'个人偏好更新'}});
  await Promise.resolve();
  expect(api.documentInventory).toHaveBeenCalledTimes(3);
  expect(input).toHaveValue('设计');
  expect(screen.getByRole('region', {name:'文件读取详情'})).toBeInTheDocument();
});

it('distinguishes legacy pairing coverage, explicit absence and unresolved targets', async () => {
  api.documentInventory.mockResolvedValue(page([file(), file('doc/none.md', {pairings:{targets:[],issue:null}}), file('doc/paired.md', {pairings:{targets:[{declarationLine:8,destination:'design.md#section',target:'doc/design.md#section',resolution:'fragment_unchecked',targetKind:null,targetSection:null,targetRevision:null}],issue:'incomplete'}})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  fireEvent.click(await screen.findByRole('button', {name:'设计文档'}));
  expect(screen.getByText('旧清单尚未解析配对，请刷新本地事实。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/none.md'}));
  expect(screen.getByText('本次范围未发现显式配对系统设计链接。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/paired.md'}));
  expect(screen.getByText('文件已读取，章节锚点尚未核对')).toBeVisible();
  expect(screen.getByText(/第 8 行 · doc\/design.md#section/).closest('details')).not.toHaveAttribute('open');
  expect(screen.getByText('配对解析不完整：incomplete')).toBeInTheDocument();
});

it('opens only the exact resolved pairing chapter and preserves its origin across personal updates', async () => {
  const target = { ...section, path:'doc/target.md', id:'target', title:'配对章节', revision:'target-rev' };
  const edge = {declarationLine:8,destination:'target.md#contract',target:'doc/target.md#contract',resolution:'resolved',targetKind:null,targetSection:target.id,targetRevision:target.revision};
  api.documentInventory.mockResolvedValue(page([file('doc/prd.md', {pairings:{targets:[edge],issue:null}})], {nextCursor:'next'}));
  const [state, setState] = createSignal({...snapshot(),sections:[section,target]});
  const showSource = vi.fn();
  renderInventory(() => <DocumentInventory snapshot={state()} showSource={showSource} />);
  fireEvent.click(await screen.findByRole('button', {name:'doc/prd.md'}));
  const open = screen.getByRole('button', {name:'打开配对章节：配对章节'});
  expect(open.textContent).toBe('配对章节');expect(screen.getByText('目标文件已读取')).toBeVisible();
  expect(screen.getByText(/第 8 行 · doc\/target.md#contract/).closest('details')).not.toHaveAttribute('open');
  fireEvent.click(open);
  expect(showSource).toHaveBeenCalledWith(target, open);
  setState({...state(),decisions:[]});
  await Promise.resolve();
  expect(screen.getByRole('button', {name:'打开配对章节：配对章节'})).toBe(open);
  expect(api.documentInventory).toHaveBeenCalledTimes(1);
  setState({...state(),sections:[section,{...target,revision:'other-revision'}]});
  await waitFor(() => expect(screen.queryByRole('button', {name:'打开配对章节：配对章节'})).not.toBeInTheDocument());
});

it('keeps ambiguous, missing and legacy pairings informational without fallback jumps', async () => {
  const base = {declarationLine:3,destination:'design.md#same',target:'doc/design.md#same',targetKind:null,targetSection:null,targetRevision:'rev'};
  api.documentInventory.mockResolvedValue(page([file('doc/prd.md', {pairings:{targets:[
    {...base,resolution:'ambiguous'}, {...base,resolution:'missing_fragment'},
    {...base,resolution:'unread'}, {...base,resolution:'resolved',targetRevision:null},
  ],issue:null}})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  fireEvent.click(await screen.findByRole('button', {name:'doc/prd.md'}));
  expect(screen.getByText(/目标锚点重复，无法唯一定位/)).toBeInTheDocument();
  expect(screen.getByText(/目标文件中未找到章节锚点/)).toBeInTheDocument();
  expect(screen.getByText('旧清单尚无章节定位，请刷新本地事实。')).toBeInTheDocument();
  expect(screen.queryByRole('button', {name:/打开配对章节/})).not.toBeInTheDocument();
});

it('shows explicit design upstream requirements separately from design references and keeps pairing navigation', async () => {
  const target = {...section,path:'doc/prd.md',id:'REQ-1',title:'上游需求',revision:'req-rev'};
  const edge = {declarationLine:7,destination:'prd.md#REQ-1',target:'doc/prd.md#REQ-1',resolution:'resolved',targetKind:'prd',targetSection:target.id,targetRevision:target.revision};
  api.documentInventory.mockResolvedValue(page([file('doc/topic.design.md',{upstream:{targets:[edge,{...edge,destination:'parent.design.md',target:'doc/parent.design.md',targetKind:'design',resolution:'unread',targetSection:null,targetRevision:null}],issue:null}})]));
  const showSource = vi.fn();
  renderInventory(() => <DocumentInventory snapshot={{...snapshot(),sections:[section,target]}} showSource={showSource} />);
  fireEvent.click(await screen.findByRole('button',{name:'doc/topic.design.md'}));
  expect(screen.getByRole('region',{name:'显式上游声明'})).toBeInTheDocument();
  expect(screen.getByText('目标类型：需求文档')).toBeInTheDocument();
  expect(screen.getByText('目标类型：设计引用')).toBeInTheDocument();
  const open = screen.getByRole('button',{name:'打开上游章节：上游需求'});
  fireEvent.click(open);
  expect(showSource).toHaveBeenCalledWith(target,open);
  expect(screen.queryByRole('button',{name:'打开配对章节：上游需求'})).not.toBeInTheDocument();
});

it('distinguishes legacy upstream coverage from an empty parsed design header', async () => {
  api.documentInventory.mockResolvedValue(page([file('doc/legacy.design.md'),file('doc/none.design.md',{upstream:{targets:[],issue:null}}),file('doc/partial.design.md',{upstream:{targets:[],issue:'pairing declaration has no explicit Markdown link'}})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  fireEvent.click(await screen.findByRole('button',{name:'doc/legacy.design.md'}));
  expect(screen.getByText('旧清单尚未解析上游，请刷新本地事实。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'doc/none.design.md'}));
  expect(screen.getByText('本次文件头范围未发现显式上游链接。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'doc/partial.design.md'}));
  expect(screen.getByText('上游解析不完整：声明中没有可识别的 Markdown 链接')).toBeInTheDocument();
});

it('shows declared ID collisions without merging files and submits ID search unchanged', async () => {
  const identity = { raw:'`DUP-123`', declaredId:'DUP-123', declarationLine:3, issue:null, duplicateFiles:1, duplicatePaths:['doc/other.md'] };
  api.documentInventory.mockResolvedValue(page([file(undefined, {identity}), file('doc/other.md', {identity:{...identity,duplicatePaths:['doc/design.md']}})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  await screen.findByRole('button', {name:'设计文档'});
  expect(screen.getByRole('button', {name:'doc/other.md'})).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'设计文档'}));
  expect(screen.getByText(/重复文档 ID：相同 ID 另有 1/)).toBeInTheDocument();
  expect(screen.getByText(/第 3 行 · 原声明样本/)).toHaveTextContent('`DUP-123`');
  fireEvent.input(screen.getByRole('textbox', {name:'筛选文档文件'}), {target:{value:'dup-123'}});
  fireEvent.click(screen.getByRole('button', {name:'应用筛选'}));
  await waitFor(() => expect(api.documentInventory).toHaveBeenLastCalledWith(expect.objectContaining({query:'dup-123',cursor:null})));
});
it('distinguishes unparsed, absent, invalid and unread document identities', async () => {
  const empty = {raw:null,declaredId:null,declarationLine:null,issue:null,duplicateFiles:0,duplicatePaths:[]};
  api.documentInventory.mockResolvedValue(page([file(),file('doc/absent.md',{identity:empty}),file('doc/invalid.md',{identity:{...empty,raw:'bad id',declarationLine:2,issue:'invalid document ID declaration'}}),file('doc/unread.md',{state:'skipped'})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  await screen.findByRole('button', {name:'设计文档'});
  fireEvent.click(screen.getByRole('button', {name:'设计文档'}));
  expect(screen.getByText('旧清单尚未解析文档 ID，请刷新本地事实。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/absent.md'}));
  expect(screen.getByText('本次文件头范围未发现文档 ID 声明。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/invalid.md'}));
  expect(screen.getByText(/ID 为空或格式不支持/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', {name:'doc/unread.md'}));
  expect(screen.queryByRole('region', {name:'文档身份声明'})).not.toBeInTheDocument();
});
it('keeps allocation rows and sides distinct and navigates only exact candidate chapters', async () => {
  const upstream = {...section,id:'requirement',path:'doc/prd.md',title:'上游要求'};
  const edge = (target: string, targetSection: string, targetRevision = 'rev') => ({declarationLine:8,destination:target,target,resolution:'resolved',targetSection,targetRevision,targetKind:target.startsWith('doc/prd')?'prd':'design'});
  const allocations = {issue:null,rows:[{declarationLine:8,rawSample:'| requirement | <script>bad()</script> | design |',upstream:{issue:null,targets:[edge('doc/prd.md#REQ-1','requirement')]},design:{issue:null,targets:[edge('doc/design.md#DES-1','source')]}},{declarationLine:9,rawSample:'second row',upstream:{issue:'allocation cell has no explicit Markdown link',targets:[]},design:{issue:null,targets:[edge('doc/design.md#wrong','source','old-revision')]}}]};
  api.documentInventory.mockResolvedValue(page([file(undefined,{allocations})]));
  const showSource = vi.fn();
  const data = snapshot(); data.sections.push(upstream);
  const result = renderInventory(() => <DocumentInventory snapshot={data} showSource={showSource} />);
  await screen.findByRole('button',{name:'设计文档'});
  fireEvent.click(screen.getByRole('button',{name:'设计文档'}));
  expect(screen.getByRole('region',{name:'承接表第 8 行'})).toBeInTheDocument();
  expect(screen.getByRole('region',{name:'承接表第 9 行'})).toBeInTheDocument();
  expect(screen.getByText(/<script>bad\(\)<\/script>/)).toBeInTheDocument();
  expect(result.container.querySelector('script')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'打开承接上游章节：上游要求'}));
  expect(showSource).toHaveBeenLastCalledWith(upstream,expect.any(HTMLElement));
  expect(screen.getAllByRole('button',{name:'打开承接设计章节：设计章节'})).toHaveLength(1);
  fireEvent.click(screen.getByRole('button',{name:'打开承接设计章节：设计章节'}));
  expect(showSource).toHaveBeenLastCalledWith(section,expect.any(HTMLElement));
  expect(screen.getByText(/未推断条款关系/)).toBeInTheDocument();
});
it('distinguishes unparsed absent and incomplete allocation tables', async () => {
  api.documentInventory.mockResolvedValue(page([file(),file('doc/none.design.md',{allocations:{issue:null,rows:[]}}),file('doc/limited.design.md',{allocations:{issue:'allocation row limit 32 exceeded',rows:[]}})]));
  renderInventory(() => <DocumentInventory snapshot={snapshot()} showSource={vi.fn()} />);
  await screen.findByRole('button',{name:'设计文档'});
  fireEvent.click(screen.getByRole('button',{name:'设计文档'}));
  expect(screen.getByText('旧清单尚未解析承接表，请刷新本地事实。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'doc/none.design.md'}));
  expect(screen.getByText('本次明确章节范围未发现需求承接表行。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'doc/limited.design.md'}));
  expect(screen.getByText(/承接表解析不完整：超过每文件 32 行/)).toBeInTheDocument();
  expect(screen.queryByText('本次明确章节范围未发现需求承接表行。')).not.toBeInTheDocument();
});

it('reuses file details when selecting a directory leaf', async () => {
  api.documentInventory.mockResolvedValue(page());
  const selected = file();
  api.documentDirectory.mockResolvedValue({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',parentPath:'',candidates:1,read:1,omitted:0,stale:false,error:null,eligibleFiles:1,matchingFiles:1,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:1,entries:[{path:selected.path,name:'design.md',kind:'design',matchingFiles:1,depthLimited:false,historicalReferenceFiles:0,historicalReferencePaths:[],file:selected}],nextCursor:null});
  const showSource=vi.fn();renderInventory(()=><DocumentInventory snapshot={snapshot()} showSource={showSource}/>);
  fireEvent.click(screen.getByRole('button',{name:'文档目录'}));
  fireEvent.click(await screen.findByRole('button',{name:'设计文档 · doc/design.md'}));
  fireEvent.click(screen.getByRole('button',{name:'设计章节 · 第 1 行'}));
  expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('设计');expect(showSource).not.toHaveBeenCalled();
  expect(screen.getByRole('button',{name:'文档目录'})).toHaveAttribute('aria-pressed','true');
});
it('does not let a late file-list response clear the selected directory leaf', async () => {
  let finish!: (value:DocumentInventoryPage)=>void;
  api.documentInventory.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const selected=file();
  api.documentDirectory.mockResolvedValue({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',parentPath:'',candidates:1,read:1,omitted:0,stale:false,error:null,eligibleFiles:1,matchingFiles:1,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:1,entries:[{path:selected.path,name:'design.md',kind:'design',matchingFiles:1,depthLimited:false,historicalReferenceFiles:0,historicalReferencePaths:[],file:selected}],nextCursor:null});
  renderInventory(()=><DocumentInventory snapshot={snapshot()} showSource={vi.fn()}/>);
  fireEvent.click(screen.getByRole('button',{name:'文档目录'}));
  fireEvent.click(await screen.findByRole('button',{name:'设计文档 · doc/design.md'}));
  await screen.findByRole('button',{name:'设计章节 · 第 1 行'});
  finish(page()); await Promise.resolve();
  expect(screen.getByRole('button',{name:'设计章节 · 第 1 行'})).toBeInTheDocument();
});

it('switches both semantic trees and reuses same-revision details despite a late file-list response', async () => {
  let finish!: (value:DocumentInventoryPage)=>void;
  api.documentInventory.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const selected=file();
  const tree={schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',view:'product',parentOccurrence:'',parentRoute:[],routeNotice:null,breadcrumbs:[],candidates:1,read:1,omitted:0,stale:false,error:null,eligibleFiles:1,matchingFiles:1,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:1,entries:[{occurrenceId:'module',stableRoute:[{path:selected.path,sourcePath:null,destination:null,group:null,duplicateIndex:0,duplicateCount:1}],label:'模块设计',path:selected.path,kind:'design',matchingFiles:1,historicalReferenceFiles:0,historicalReferencePaths:[],boundary:null,cycle:false,depthLimited:false,sourcePath:null,sourceLine:null,relationship:null,file:selected,hasChildren:false}],nextCursor:null,issues:[]};
  api.documentSemanticTree.mockResolvedValue(tree);
  const showSource=vi.fn();renderInventory(()=><DocumentInventory snapshot={snapshot()} showSource={showSource}/>);
  fireEvent.click(screen.getByRole('button',{name:'产品树'}));
  fireEvent.click(await screen.findByRole('button',{name:'查看文档：模块设计'}));
  fireEvent.click(screen.getByRole('button',{name:'设计章节 · 第 1 行'}));
  expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('设计');expect(showSource).not.toHaveBeenCalled();
  finish(page());await Promise.resolve();expect(screen.getByRole('button',{name:'设计章节 · 第 1 行'})).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'专业系统树'}));
  await waitFor(()=>expect(api.documentSemanticTree).toHaveBeenLastCalledWith(expect.objectContaining({view:'professional'})));
  await screen.findByRole('button',{name:'查看文档：模块设计'});
  expect(screen.queryByRole('button',{name:'设计章节 · 第 1 行'})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'专业系统树'})).toHaveAttribute('aria-pressed','true');
});

it('focuses selected file context and restores its originating button when closing details', async () => {
  api.documentInventory.mockResolvedValue(page());
  renderInventory(()=><DocumentInventory snapshot={snapshot()} showSource={vi.fn()}/>);
  const origin=await screen.findByRole('button',{name:'设计文档'});fireEvent.click(origin);
  await waitFor(()=>expect(screen.getByRole('region',{name:'文件读取详情'})).toHaveFocus());
  fireEvent.click(screen.getByRole('button',{name:'收起文件详情'}));
  expect(origin).toHaveFocus();expect(screen.queryByRole('region',{name:'文件读取详情'})).not.toBeInTheDocument();
});

it('labels product pairings separately and opens the same-snapshot product design chapter', async () => {
  const design={...section,id:'product-design',path:'doc/product/module/design.md',title:'产品设计章节',revision:'product-rev'};
  const product=file('doc/product/module/topic.prd.md',{title:'产品专题',kind:'prd',pairings:{targets:[{declarationLine:12,destination:'design.md#产品设计',target:'doc/product/module/design.md#产品设计',resolution:'resolved',targetSection:design.id,targetRevision:design.revision,targetKind:'design'}],issue:null}});
  api.documentInventory.mockResolvedValue(page([product]));
  const showSource=vi.fn();renderInventory(()=><DocumentInventory snapshot={{...snapshot(),sections:[design]}} showSource={showSource}/>);
  fireEvent.click(await screen.findByRole('button',{name:'产品专题'}));
  expect(screen.getByRole('region',{name:'显式配对产品设计'})).toBeInTheDocument();
  expect(screen.queryByRole('region',{name:'显式配对系统设计'})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'打开配对章节：产品设计章节'}));
  expect(showSource).toHaveBeenLastCalledWith(design,expect.any(HTMLElement));
});

it('restores a bounded last tree view after restart without overtaking an explicit browsing choice',async()=>{
  let finish!:(value:unknown)=>void;api.documentInventory.mockResolvedValue(page());api.loadDocumentTreeLastView.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
  renderInventory(()=><DocumentInventory snapshot={snapshot()} showSource={vi.fn()}/>);fireEvent.click(screen.getByRole('button',{name:'文档目录'}));finish({status:'ready',view:'professional',notice:null});await Promise.resolve();expect(screen.getByRole('button',{name:'文档目录'})).toHaveAttribute('aria-pressed','true');expect(screen.getByRole('button',{name:'专业系统树'})).toHaveAttribute('aria-pressed','false');
});
it('opens the stored product tree on restart when no newer browsing intent exists',async()=>{
  api.documentInventory.mockResolvedValue(page());api.loadDocumentTreeLastView.mockResolvedValue({status:'ready',view:'product',notice:null});
  api.documentSemanticTree.mockResolvedValue({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',view:'product',parentOccurrence:'',parentRoute:[],routeNotice:null,breadcrumbs:[],candidates:1,read:1,omitted:0,stale:false,error:null,eligibleFiles:1,matchingFiles:1,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:0,entries:[],nextCursor:null,issues:[]});
  render(()=><DocumentInventory snapshot={snapshot()} showSource={vi.fn()}/>);await waitFor(()=>expect(screen.getByRole('button',{name:'产品树'})).toHaveAttribute('aria-pressed','true'));await screen.findByRole('tree',{name:'产品树文档树'});expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});

it('opens content immediately, skips a heading-only chapter and preserves local chapter selection on personal updates',async()=>{
 const intro={...section,id:'intro',content:'# 设计',title:'标题'};const first={...section,id:'first',title:'第一条',content:'正文第一条'};const second={...section,id:'second',title:'第二条',content:'正文第二条',line:20};
 const [state,setState]=createSignal({...snapshot(),sections:[intro,first,second]});api.documentInventory.mockResolvedValue(page());const showSource=vi.fn();
 renderInventory(()=><DocumentInventory snapshot={state()} showSource={showSource}/>);fireEvent.click(await screen.findByRole('button',{name:'设计文档'}));
 await waitFor(()=>expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('正文第一条'));expect(showSource).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'第二条 · 第 20 行'}));expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('正文第二条');
 setState({...state(),decisions:[]});expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('正文第二条');
});
