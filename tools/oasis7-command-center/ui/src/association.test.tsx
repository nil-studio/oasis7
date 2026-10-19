import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, it, vi } from 'vitest';
import type { Snapshot } from './types';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: mock.invoke }));
import App from './App';
afterEach(() => { cleanup(); mock.invoke.mockReset(); });

it('keeps a saved missing goal identity unresolved instead of linking the replacement heading', async () => {
  const snapshot: Snapshot = {
    project: {id:'project',path:'/repo',gitCommonDir:'/repo/.git',branch:'work',head:'head',environment:'local'},
    preferences: {projectPath:'/repo',view:'direction',query:''},
    sections: [{id:'doc/core/prd.md#heading-new',path:'doc/core/prd.md',title:'新目标与原章节无关',content:'source',line:4,revision:'new',committedRevision:'head',dirty:false,factKind:'sourceFact',lifecycle:null}],
    associations: [{workUrl:'https://github.com/eng-cc/oasis7/pull/1',goalSectionIds:['doc/core/prd.md#heading-old'],confirmedAt:'123'}],
    decisions:[], readingPositions:[], sources:[], git:{status:'',worktrees:''},
  };
  const reference={kind:'pullRequest',host:'github.com',repository:'eng-cc/oasis7',number:'1'};
  const evidence={source:reference,revision:'head',verifiedAt:'1',eventAt:null,readAt:'2',coverage:'bounded'};
  mock.invoke.mockImplementation(async(command:string)=>{
    if(command==='query_view')return snapshot;
    if(command==='query_work_projection')return {schemaVersion:1,projectId:'project',environment:'local',readAt:'2',sources:[],items:[{reference,purpose:{value:'原工作',evidence},result:{value:'PR OPEN',evidence},remaining:'未知',sourceUrl:'https://github.com/eng-cc/oasis7/pull/1',goals:[{sectionId:'doc/core/prd.md#heading-old',title:null,revision:null,confirmedAt:'123'}],runs:[]}]};
    throw new Error('source not connected');
  });
  render(()=><App/>);fireEvent.click(await screen.findByRole('button',{name:'原工作'}));
  fireEvent.click(screen.getByRole('tab',{name:'依据'}));expect(screen.getByText(/原章节未读取或已变化 · doc\/core\/prd.md#heading-old/)).not.toHaveTextContent('新目标与原章节无关');expect(mock.invoke.mock.calls.some(([command])=>command==='read_source')).toBe(false);
});


it('offers summary chapters on the home view and opens their original revision without searching', async () => {
  const source = {id:'doc/engineering/command-center/README.md#cc-status-baseline',path:'doc/engineering/command-center/README.md',title:'核验基线与覆盖范围',content:'核验于旧窗口，部署未知',line:20,revision:'summary-revision',committedRevision:'summary-head',dirty:true,factKind:'sourceFact',lifecycle:null};
  const snapshot: Snapshot = {
    project:{id:'project',path:'/repo',gitCommonDir:'/repo/.git',branch:'work',head:'head',environment:'local'},
    preferences:{projectPath:'/repo',view:'direction',query:''},sections:[source],
    associations:[],decisions:[],readingPositions:[],sources:[],git:{status:'',worktrees:''},
  };
  mock.invoke.mockImplementation(async (command: string) => command === 'query_view' ? snapshot : {state:'unconnected',repository:null,account:null,observedAt:null,lastAttempt:null,coverage:'未接入',error:null,pulls:[]});
  render(() => <App />);
  await waitFor(() => expect(screen.getByRole('button', {name:source.title})).toBeEnabled());
  const {fireEvent} = await import('@solidjs/testing-library');
  const entry = screen.getByRole('button', {name:source.title});
  entry.focus();
  fireEvent.click(entry);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledWith('read_source', {sectionId:source.id,revision:source.revision,line:source.line}));
  expect(screen.getAllByText(source.content).length).toBeGreaterThan(0);
  expect(screen.getByRole('region', {name:'来源详情'})).toHaveFocus();
  expect(screen.getByText(source.committedRevision)).toBeInTheDocument();
  expect(mock.invoke.mock.calls.some(([command]) => command === 'search_rules')).toBe(false);
  fireEvent.click(screen.getByRole('button', {name:'关闭详情'}));
  expect(entry).toHaveFocus();
});

it('shows local omission counts and paths even when no sections were readable', async () => {
  const coverage = '候选 1，已读 0，遗漏 1; 遗漏原因: invalid_utf8 1; 路径样本（前 1 / 1）: invalid_utf8: doc/prd.md';
  const snapshot: Snapshot = {
    project:{id:'project',path:'/repo',gitCommonDir:'/repo/.git',branch:'work',head:'head',environment:'local'},
    preferences:{projectPath:'/repo',view:'direction',query:''},sections:[],
    associations:[],decisions:[],readingPositions:[],git:{status:'',worktrees:''},
    sources:[{kind:'local',state:'partial',lastAttempt:'123',lastSuccess:'123',sourceRevision:'head',generation:'1',coverage,error:null}],
  };
  mock.invoke.mockImplementation(async (command: string) => command === 'query_view' ? snapshot : {state:'unconnected',repository:null,account:null,observedAt:null,lastAttempt:null,coverage:'未接入',error:null,pulls:[]});
  render(() => <App />);
  await waitFor(() => expect(screen.getByText('local · 部分读取')).toBeInTheDocument());
  expect(document.body.textContent).toContain(coverage);
  expect(document.body.textContent).not.toContain('local · 读取成功 · 空');
});
