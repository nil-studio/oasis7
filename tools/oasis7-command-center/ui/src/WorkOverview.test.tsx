import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import WorkOverview from './WorkOverview';
import { bridge } from './bridge';
import type { Snapshot } from './types';
import type { WorkProjection } from './generated/core';
vi.mock('./bridge', async () => ({...await vi.importActual('./bridge'), bridge: {workProjection: vi.fn(), openSource: vi.fn(), refreshWorkChecks: vi.fn()}, subscribeRuntime: async () => () => {}}));
afterEach(() => {cleanup(); vi.clearAllMocks();});
const snapshot = (id='p') => ({project:{id,environment:'local'}, associations:[]}) as unknown as Snapshot;
function projection(id='p'): WorkProjection {
  const reference = {kind:'pullRequest',host:'github.com',repository:'o/r',number:'1'} as const;
  const evidence = {source:reference,revision:'head',verifiedAt:'20',eventAt:null,readAt:'30',coverage:'recent 100 only'};
  return {schemaVersion:1,projectId:id,environment:'local',readAt:'30',sources:[{kind:'github',state:'partial',coverage:'recent 100 only',verifiedAt:'20',error:null}],items:[{reference,purpose:{value:'修复本地工具',evidence},result:{value:'PR MERGED',evidence},remaining:'目标验收尚未核实',sourceUrl:'https://github.com/o/r/pull/1',goals:[{sectionId:'a',title:'目标 A',revision:'r',confirmedAt:'10'},{sectionId:'b',title:null,revision:null,confirmedAt:'10'}],checks:[{name:'test',state:'SUCCESS',testedOid:null,url:null}],runs:[]}]};
}
it('opens the same detail across home and execution and restores keyboard focus without sending', async () => {
  vi.mocked(bridge.workProjection).mockResolvedValue(projection());
  vi.mocked(bridge.openSource).mockResolvedValue();
  const [view,setView] = createSignal('direction');
  render(() => <WorkOverview snapshot={snapshot()} view={view()} sourceGeneration={0} onExecution={() => setView('execution')}/>);
  const button = await screen.findByRole('button',{name:'修复本地工具'});
  fireEvent.click(button);
  await waitFor(() => expect(screen.getByRole('region',{name:'工作详情'})).toHaveFocus());
  expect(screen.getByText('目标 A')).toBeInTheDocument();
  expect(screen.getByText(/原章节未读取或已变化 · b/)).toBeInTheDocument();
  expect(screen.getByText(/未知，不能由 PR HEAD 推定/)).toBeInTheDocument();
  setView('execution');
  expect(screen.getByRole('region',{name:'工作详情'})).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'打开原 PR'}));
  expect(bridge.openSource).toHaveBeenCalledWith('https://github.com/o/r/pull/1');
  expect(screen.getByText('目标验收尚未核实')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));
  expect(button).toHaveFocus();
});
it('retains loaded work on a failed read and exposes source coverage independently', async () => {
  vi.mocked(bridge.workProjection).mockResolvedValueOnce(projection()).mockRejectedValueOnce(new Error('offline'));
  render(() => <WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  await screen.findByRole('button',{name:'修复本地工具'});
  fireEvent.click(screen.getByRole('button',{name:'刷新工作列表'}));
  await screen.findByRole('alert');
  expect(screen.getByRole('button',{name:'修复本地工具'})).toBeInTheDocument();
  expect(screen.getByText('recent 100 only')).toBeInTheDocument();
});
it('rejects late results after project switch and clears the old detail', async () => {
  let resolve!: (value:WorkProjection)=>void;
  vi.mocked(bridge.workProjection).mockImplementationOnce(() => new Promise(done => {resolve=done;})).mockResolvedValueOnce(projection('other'));
  const [source,setSource] = createSignal(snapshot());
  render(() => <WorkOverview snapshot={source()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  setSource(snapshot('other'));
  await screen.findByRole('button',{name:'修复本地工具'});
  const old = projection(); old.items[0].purpose.value='旧项目工作'; resolve(old);
  await waitFor(() => expect(screen.queryByText('旧项目工作')).not.toBeInTheDocument());
});
it('shows a bounded first list with an explicit remaining-work entry', async () => {
  const value=projection(); value.items=Array.from({length:4},(_,i)=>({...value.items[0],reference:{kind:'pullRequest',host:'github.com',repository:'o/r',number:String(i)},purpose:{...value.items[0].purpose,value:`工作 ${i}`}}));
  vi.mocked(bridge.workProjection).mockResolvedValue(value);
  render(() => <WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  await screen.findByRole('button',{name:'工作 0'});
  expect(screen.queryByRole('button',{name:'工作 3'})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'显示其余 1 项工作'}));
  expect(screen.getByRole('button',{name:'工作 3'})).toBeInTheDocument();
});

it('restores focus to the current source row after a background projection replaces objects', async () => {
  vi.mocked(bridge.workProjection).mockResolvedValueOnce(projection()).mockResolvedValueOnce(projection());
  render(() => <WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  const original = await screen.findByRole('button',{name:'修复本地工具'});
  fireEvent.click(original);
  fireEvent.click(screen.getByRole('button',{name:'刷新工作列表'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'修复本地工具'})).not.toBe(original));
  fireEvent.click(screen.getByRole('button',{name:'返回工作列表'}));
  expect(screen.getByRole('button',{name:'修复本地工具'})).toHaveFocus();
});

it('reads only explicitly selected PR checks and separates unread from successful empty CI', async () => {
  const before=projection(); before.items[0].checks=[];
  const after=projection(); after.items[0].checks=[]; after.items[0].checksSource={schemaVersion:1,state:'partial',observedAt:'50',coverage:'REST first pages only',error:null};
  vi.mocked(bridge.workProjection).mockResolvedValueOnce(before).mockResolvedValueOnce(after);
  vi.mocked(bridge.refreshWorkChecks).mockResolvedValue({} as never);
  render(() => <WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
  expect(bridge.refreshWorkChecks).not.toHaveBeenCalled();
  expect(screen.getByText(/CI 尚未单独读取/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'只读刷新该 PR 的 CI'}));
  await screen.findByText(/已读取的 CI 页面为空/);
  expect(bridge.refreshWorkChecks).toHaveBeenCalledExactlyOnceWith('1',null);
  expect(screen.getByText('REST first pages only')).toBeInTheDocument();
});

it('never labels a REST associated commit as the actual tested checkout', async () => {
  const value=projection();value.items[0].checks[0].sourceOid='associated-sha';
  vi.mocked(bridge.workProjection).mockResolvedValue(value);
  render(() => <WorkOverview snapshot={snapshot()} view="direction" sourceGeneration={0} onExecution={() => {}}/>);
  fireEvent.click(await screen.findByRole('button',{name:'修复本地工具'}));
  expect(screen.getByText(/实际测试版本 未知/)).toBeInTheDocument();
  expect(screen.getByText(/GitHub 关联提交 associated-sha/)).toBeInTheDocument();
});
