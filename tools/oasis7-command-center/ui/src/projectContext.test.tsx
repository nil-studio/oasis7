import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, it, vi } from 'vitest';
import type { Snapshot } from './types';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: mock.invoke }));
import App from './App';
afterEach(() => { cleanup(); mock.invoke.mockReset(); });
const github = {state:'unconnected',repository:null,account:null,observedAt:null,lastAttempt:null,coverage:'未接入',error:null,pulls:[]};
function snapshot(id: string): Snapshot {
  return {project:{id,path:`/${id}`,gitCommonDir:`/${id}/.git`,branch:'work',head:'head',environment:'local'},preferences:{projectPath:`/${id}`,view:'direction',query:''},sections:[{id:'doc/core/prd.md#goal',path:'doc/core/prd.md',title:`${id}目标`,content:'source',line:1,revision:id,committedRevision:'head',dirty:false,factKind:'sourceFact',lifecycle:null}],associations:[],decisions:[],readingPositions:[],sources:[],git:{status:'',worktrees:''}};
}
async function open(id: string) {
  fireEvent.input(screen.getByLabelText('本地仓库绝对路径'),{target:{value:`/${id}`}});
  fireEvent.click(screen.getByRole('button',{name:'打开项目'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
}
it('isolates unsaved inputs by project and restores them on return without saving', async () => {
  let current = snapshot('A');
  mock.invoke.mockImplementation(async (command: string, args?: {path?: string}) => {
    if (command === 'open_project') current = snapshot(args!.path!.slice(1));
    if (['query_view','open_project','refresh_source'].includes(command)) return current;
    return github;
  });
  render(() => <App />);fireEvent.click(screen.getByRole('button',{name:'项目与来源'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
  fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
  fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:'A 的决定'}});
  fireEvent.input(screen.getByLabelText('理由'),{target:{value:'A 的理由'}});
  fireEvent.click(screen.getByRole('button',{name:'刷新本地事实'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
  expect(screen.getByLabelText('决定内容')).toHaveValue('A 的决定');
  await open('B');
  expect(screen.getByLabelText('决定内容')).toHaveValue('');
  fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:'B 的决定'}});
  fireEvent.click(screen.getByRole('button',{name:/方向与进度/}));
  await open('A');
  fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
  expect(screen.getByLabelText('决定内容')).toHaveValue('A 的决定');
  expect(screen.getByLabelText('理由')).toHaveValue('A 的理由');
  await open('B');
  expect(screen.getByLabelText('决定内容')).toHaveValue('B 的决定');
  expect(mock.invoke.mock.calls.some(([command]) => command === 'save_decision_draft' || command === 'associate_work')).toBe(false);
}, 15000);
it('rejects search results returned after another project opens', async () => {
  let resolveSearch!: (value: Snapshot['sections']) => void;
  mock.invoke.mockImplementation(async (command: string) => {
    if (command === 'query_view') return snapshot('A');
    if (command === 'open_project') return snapshot('B');
    if (command === 'search_rules') return new Promise<Snapshot['sections']>(resolve => {resolveSearch = resolve;});
    return github;
  });
  render(() => <App />);fireEvent.click(screen.getByRole('button',{name:'项目与来源'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
  fireEvent.input(screen.getByLabelText('搜索规范、标题、路径或条款 ID'),{target:{value:'旧查询'}});
  fireEvent.click(screen.getByRole('button',{name:'搜索'}));
  await open('B');
  resolveSearch(snapshot('A').sections);
  await Promise.resolve();
  expect(screen.queryByRole('heading',{name:/检索结果/})).not.toBeInTheDocument();
  expect(screen.getByLabelText('搜索规范、标题、路径或条款 ID')).toHaveValue('');
});

it('preserves the current project inputs when opening fails or the picker is cancelled', async () => {
  mock.invoke.mockImplementation(async (command: string) => {
    if (command === 'query_view') return snapshot('A');
    if (command === 'open_project') throw new Error('路径不可读取');
    if (command === 'choose_project') return null;
    return github;
  });
  render(() => <App />);fireEvent.click(screen.getByRole('button',{name:'项目与来源'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
  fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
  fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:'尚未保存'}});
  await open('B');
  expect(screen.getByText('路径不可读取')).toBeInTheDocument();
  expect(screen.getByLabelText('决定内容')).toHaveValue('尚未保存');
  fireEvent.click(screen.getByRole('button',{name:'选择文件夹'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
  expect(screen.getByLabelText('决定内容')).toHaveValue('尚未保存');
});

it('keeps an unsent search query per project and never turns navigation into a relationship write',async()=>{
 let current=snapshot('A');mock.invoke.mockImplementation(async(command:string,args?:{path?:string})=>{if(command==='open_project')current=snapshot(args!.path!.slice(1));if(['query_view','open_project'].includes(command))return current;return github;});
 render(()=><App/>);fireEvent.click(screen.getByRole('button',{name:'项目与来源'}));await waitFor(()=>expect(screen.getByRole('button',{name:'打开项目'})).toBeEnabled());
 fireEvent.input(screen.getByLabelText('搜索规范、标题、路径或条款 ID'),{target:{value:'A 未提交查询'}});await open('B');expect(screen.getByLabelText('搜索规范、标题、路径或条款 ID')).toHaveValue('');await open('A');expect(screen.getByLabelText('搜索规范、标题、路径或条款 ID')).toHaveValue('A 未提交查询');expect(mock.invoke.mock.calls.some(([command])=>command==='save_daily_work'||command==='associate_work')).toBe(false);
});
