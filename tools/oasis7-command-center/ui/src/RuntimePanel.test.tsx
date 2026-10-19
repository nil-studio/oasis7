import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import RuntimePanel from './RuntimePanel';
import { canContinue, canInterrupt, canStart, canSteer, requestKey, sessionKey, visibleOperations } from './runtimeState';
import type { Project, RuntimeSession, RuntimeSnapshot } from './types';
import { runtimeBridge } from './bridge';

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => false, invoke: vi.fn() }));
afterEach(cleanup);
const project: Project = {id:'repo',path:'/repo',gitCommonDir:'/repo/.git',head:'head',branch:'work',environment:'local'};
const session: RuntimeSession = {purpose:'验证修复',scope:'受控测试目录',model:null,basisRevision:'head',threadId:'thread-1',instanceId:'instance-1',turnId:'turn-1',state:'active',owned:true,backgroundState:'clear',restorableHistory:false,privateSources:[]};
function snapshot(overrides: Partial<RuntimeSnapshot> = {}): RuntimeSnapshot {
  return {projectId:'repo',environment:'local',enabled:true,experimental:true,state:'ready',version:'codex test',instanceId:'instance-1',capabilities:['createSession','continueSession','startTurn','steerTurn','interruptTurn','answerRequest'],coverage:'本客户端明确建立的实例',error:null,sessions:[session],requests:[],operations:[],logs:[],logsTruncated:false,exitWaiting:false,...overrides};
}
function fakeApi(initial: RuntimeSnapshot) {
  let current = initial;
  const api = {
    snapshot:vi.fn(async () => structuredClone(current)),
    reconcile:vi.fn(async () => structuredClone(current)),
    exitStatus:vi.fn(async () => ({safe:false,active:1,unknown:0,pendingRequests:0,waiting:false,reason:'仍有活跃工作'})),
    enabled:vi.fn(async (enabled:boolean) => { current = {...current,enabled}; return structuredClone(current); }),
    connect:vi.fn(async () => structuredClone(current)), create:vi.fn(async () => structuredClone(current)),
    continue:vi.fn(async () => structuredClone(current)), start:vi.fn(async () => structuredClone(current)),
    steer:vi.fn(async () => structuredClone(current)), interrupt:vi.fn(async () => structuredClone(current)),
    answer:vi.fn(async () => { current = {...current,requests:current.requests.map(r => ({...r,state:'awaiting'}))}; return structuredClone(current); }),
    background:vi.fn(async () => ({safe:false,active:1,unknown:0,pendingRequests:0,waiting:false,reason:'留在后台'})),
    waitExit:vi.fn(async () => ({safe:false,active:1,unknown:0,pendingRequests:0,waiting:true,reason:'等待实际工作结束'})),
    stopExit:vi.fn(async () => ({safe:false,active:1,unknown:1,pendingRequests:0,waiting:true,reason:'中断已接受，停止结果仍未知'})),
  } satisfies Omit<typeof runtimeBridge, 'startWork'|'startWorkRecords'|'publicSnapshot'>;
  return {api, setSnapshot:(next:RuntimeSnapshot) => { current = next; }};
}
const subscribe = vi.fn(async () => () => {});

it('freezes original request replies during exit and preserves the draft until exit is cancelled', async () => {
  const request = {requestId:'original',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'input',state:'pending',summary:'退出期间请求',questions:[{id:'q',header:'输入',question:'必要答案',options:[],isSecret:false}],allowedDecisions:[]};
  const {api,setSnapshot} = fakeApi(snapshot({requests:[request]}));
  render(() => <RuntimePanel project={project} view="attention" api={api} subscribe={subscribe} />);
  const input=await screen.findByLabelText('回答 q');
  fireEvent.input(input,{target:{value:'保留原请求答案'}});
  const submit=screen.getByRole('button',{name:'向原请求回复一次'});
  expect(submit).toBeEnabled();
  setSnapshot(snapshot({requests:[request],exitWaiting:true}));
  fireEvent.click(screen.getByRole('button',{name:'只读核对状态'}));
  await waitFor(()=>expect(submit).toBeDisabled());
  fireEvent.click(submit); expect(api.answer).not.toHaveBeenCalled();
  expect(input).toHaveValue('保留原请求答案');
  setSnapshot(snapshot({requests:[request],exitWaiting:false}));
  fireEvent.click(screen.getByRole('button',{name:'只读核对状态'}));
  await waitFor(()=>expect(submit).toBeEnabled());
  fireEvent.click(submit);
  await waitFor(()=>expect(api.answer).toHaveBeenCalledTimes(1));
  expect(api.answer).toHaveBeenCalledWith({requestId:'original',target:{instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1'},decision:null,answers:{q:['保留原请求答案']}});
});

it('allows one genuine original request answer after an unrelated lost acknowledgement', async () => {
  const {api,setSnapshot} = fakeApi(snapshot());
  api.steer.mockRejectedValue(new Error('lost acknowledgement'));
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  const input = await screen.findByLabelText('向此原始会话发送明确输入');
  fireEvent.input(input,{target:{value:'明确追加'}});
  fireEvent.click(screen.getByRole('button',{name:'追加到当前运行'}));
  await waitFor(() => expect(api.steer).toHaveBeenCalledTimes(1));
  const request = {requestId:'new-original',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'input',state:'pending',summary:'真实新请求',questions:[{id:'q',header:'输入',question:'必要答案',options:[],isSecret:false}],allowedDecisions:[]};
  setSnapshot(snapshot({state:'degraded',requests:[request],operations:[{id:'lost',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'turn/steer',state:'unknown',fingerprint:'f',createdAt:'1',error:null}]}));
  fireEvent.click(screen.getByRole('button',{name:'只读核对状态'}));
  const answer = await screen.findByLabelText('回答 q');
  fireEvent.input(answer,{target:{value:'原请求答案'}});
  const submit = screen.getByRole('button',{name:'向原请求回复一次'});
  expect(submit).toBeEnabled(); fireEvent.click(submit);
  await waitFor(() => expect(api.answer).toHaveBeenCalledTimes(1));
  expect(api.answer).toHaveBeenCalledWith({requestId:'new-original',target:{instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1'},decision:null,answers:{q:['原请求答案']}});
  await waitFor(() => expect(submit).toBeDisabled());
  fireEvent.click(submit); expect(api.answer).toHaveBeenCalledTimes(1); expect(api.steer).toHaveBeenCalledTimes(1);
});

it('requires explicit verified historical restoration without starting a task', async () => {
  const historical = {...session,instanceId:'previous-instance',turnId:null,state:'idle',restorableHistory:true};
  const initial = snapshot({sessions:[historical]});
  expect(canContinue(initial,historical)).toBe(true);
  expect(canContinue(initial,{...historical,restorableHistory:false})).toBe(false);
  expect(canContinue(snapshot({requests:[{requestId:'old',instanceId:historical.instanceId,threadId:historical.threadId,turnId:'old-turn',method:'input',state:'unknown',summary:'未核实',questions:[],allowedDecisions:[]}]}),historical)).toBe(false);
  const {api} = fakeApi(initial);
  api.continue.mockImplementation(async () => snapshot({sessions:[{...historical,instanceId:'instance-1',restorableHistory:false}]}));
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  const restore = await screen.findByRole('button',{name:'明确核实并恢复历史会话'});
  expect(restore).toBeEnabled(); expect(api.continue).not.toHaveBeenCalled();
  expect(screen.getByLabelText('向此原始会话发送明确输入')).toBeDisabled();
  fireEvent.click(restore);
  await waitFor(() => expect(api.continue).toHaveBeenCalledWith('thread-1'));
  expect(api.start).not.toHaveBeenCalled(); expect(api.steer).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.getByLabelText('向此原始会话发送明确输入')).toBeEnabled());
});

it('does not enable, connect, start or replay anything during initial read-only observation', async () => {
  const {api} = fakeApi(snapshot({enabled:false,state:'disabled',capabilities:[],sessions:[]}));
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  await waitFor(() => expect(api.snapshot).toHaveBeenCalled());
  expect(api.enabled).not.toHaveBeenCalled(); expect(api.connect).not.toHaveBeenCalled(); expect(api.create).not.toHaveBeenCalled(); expect(api.start).not.toHaveBeenCalled();
  expect(screen.getByRole('button',{name:'建立会话，不启动模型任务'})).toBeDisabled();
  expect(screen.getByRole('textbox',{name:'Codex 可执行文件绝对路径'})).toHaveValue('');
});

it('keeps exact instance, thread and turn identity; rejects unknown sends while retaining disabled-session management', () => {
  const pending = snapshot({sessions:[{...session,state:'completed'}],operations:[{id:'op',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'turn/start',state:'unknown',fingerprint:'f',createdAt:'1',error:null}]});
  expect(canStart(pending,pending.sessions[0])).toBe(false);
  expect(canInterrupt(snapshot({enabled:false}),session)).toBe(true);
  expect(canInterrupt(snapshot({exitWaiting:true}),session)).toBe(false);
  expect(canInterrupt(snapshot({enabled:false,state:'degraded'}),session)).toBe(true);
  expect(canSteer(snapshot({state:'degraded'}),session)).toBe(false);
  expect(canInterrupt(snapshot({operations:[{...pending.operations[0],method:'turn/interrupt',state:'confirmed'}]}),session)).toBe(false);
  expect(canInterrupt(snapshot({instanceId:'other-instance'}),session)).toBe(false);
  expect(sessionKey(session)).not.toBe(sessionKey({...session,instanceId:'other'}));
  const request = {requestId:'7',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'input',state:'pending',summary:'question',questions:[],allowedDecisions:[]};
  expect(requestKey(request)).not.toBe(requestKey({...request,turnId:'turn-2'}));
});

it('preserves input, composition and focus across snapshots and view changes without implicit send', async () => {
  const {api} = fakeApi(snapshot());
  const [view,setView] = createSignal('execution');
  render(() => <RuntimePanel project={project} view={view()} api={api} subscribe={subscribe} />);
  const input = await screen.findByLabelText('向此原始会话发送明确输入');
  fireEvent.input(input,{target:{value:'中文候选内容'}}); input.focus();
  fireEvent.compositionStart(input);
  fireEvent.keyDown(input,{key:'Enter',ctrlKey:true,isComposing:true});
  expect(api.steer).not.toHaveBeenCalled();
  expect(screen.getByRole('button',{name:'追加到当前运行'})).toBeDisabled();
  fireEvent.compositionEnd(input);
  fireEvent.click(screen.getByRole('button',{name:'只读核对状态'}));
  await waitFor(() => expect(api.snapshot.mock.calls.length).toBeGreaterThan(1));
  expect(input).toHaveFocus(); expect(input).toHaveValue('中文候选内容');
  setView('attention'); setView('execution');
  expect(screen.getByLabelText('向此原始会话发送明确输入')).toBe(input);
  expect(input).toHaveValue('中文候选内容'); expect(api.steer).not.toHaveBeenCalled();
});

it('sends a secret answer once to the original request, retains the unresolved draft, and clears after source resolution', async () => {
  const request = {requestId:'{"jsonrpc":9}',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'item/tool/requestUserInput',state:'pending',summary:'必要输入',questions:[{id:'secret',header:'密钥',question:'输入必要值',options:[],isSecret:true}],allowedDecisions:[]};
  const {api,setSnapshot} = fakeApi(snapshot({requests:[request],state:'degraded'}));
  const setItem = vi.spyOn(Storage.prototype,'setItem');
  render(() => <RuntimePanel project={project} view="attention" api={api} subscribe={subscribe} />);
  const input = await screen.findByLabelText('回答 secret');
  expect(input).toHaveAttribute('type','password');
  fireEvent.input(input,{target:{value:'local-secret-value'}});
  fireEvent.keyDown(input,{key:'Enter'}); expect(api.answer).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'向原请求回复一次'}));
  await waitFor(() => expect(api.answer).toHaveBeenCalledTimes(1));
  expect(api.answer).toHaveBeenCalledWith({requestId:'{"jsonrpc":9}',target:{instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1'},decision:null,answers:{secret:['local-secret-value']}});
  await waitFor(() => expect(screen.getByRole('button',{name:'向原请求回复一次'})).toBeDisabled());
  expect(input).toHaveValue('local-secret-value');
  setSnapshot(snapshot({requests:[{...request,state:'resolved'}]}));
  fireEvent.click(screen.getByRole('button',{name:'只读核对状态'}));
  await waitFor(() => expect(input).toHaveValue(''));
  expect(screen.getByRole('button',{name:'向原请求回复一次'})).toBeDisabled();
  expect(setItem).not.toHaveBeenCalled(); setItem.mockRestore();
});
it('shows newest receipt records and keeps every older unresolved operation visible', () => {
  const operations = Array.from({length:120},(_,index) => ({id:`op-${index}`,instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'turn/start',state:index === 119 ? 'unknown' : 'confirmed',fingerprint:'f',createdAt:String(120-index),error:null}));
  const shown = visibleOperations(snapshot({operations}));
  expect(shown[0].id).toBe('op-0');
  expect(shown).toHaveLength(101);
  expect(shown.some(op => op.id === 'op-119')).toBe(true);
  expect(shown.some(op => op.id === 'op-118')).toBe(false);
});

it('does not resend unknown input or equate accepted interruption with a stopped task', async () => {
  const unknown = {id:'op',instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1',method:'turn/steer',state:'unknown',fingerprint:'f',createdAt:'1',error:null};
  const {api} = fakeApi(snapshot({enabled:false,operations:[unknown]}));
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  await screen.findByText(/此记录不提供重发按钮/);
  const input = screen.getByLabelText('向此原始会话发送明确输入'); fireEvent.input(input,{target:{value:'不要重发'}});
  expect(screen.getByRole('button',{name:'追加到当前运行'})).toBeDisabled();
  fireEvent.click(screen.getByRole('button',{name:'仅核对原操作来源'}));
  await waitFor(() => expect(api.reconcile).toHaveBeenCalledWith('op'));
  await waitFor(() => expect(screen.getByRole('button',{name:'明确请求中断本轮'})).toBeEnabled());
  fireEvent.click(screen.getByRole('button',{name:'明确请求中断本轮'}));
  await waitFor(() => expect(api.interrupt).toHaveBeenCalledWith({instanceId:'instance-1',threadId:'thread-1',turnId:'turn-1'}));
  expect(screen.getByText('本轮：运行中')).toBeInTheDocument(); expect(api.steer).not.toHaveBeenCalled();
});

it('retains unsafe exit status after explicit stop and offers background without stopping work', async () => {
  const {api} = fakeApi(snapshot());
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  await waitFor(() => expect(api.exitStatus).toHaveBeenCalled());
  fireEvent.click(screen.getByRole('button',{name:'查看退出选项'}));
  await waitFor(() => expect(screen.getByRole('button',{name:'明确请求停止后退出'})).toBeEnabled());
  fireEvent.click(screen.getByRole('button',{name:'明确请求停止后退出'}));
  await waitFor(() => expect(api.stopExit).toHaveBeenCalledTimes(1));
  expect(screen.getByText('未确认可以安全退出')).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('button',{name:'留在后台 / 取消等待退出'})).toBeEnabled());
  fireEvent.click(screen.getByRole('button',{name:'留在后台 / 取消等待退出'}));
  await waitFor(() => expect(api.background).toHaveBeenCalledTimes(1));
  expect(api.interrupt).not.toHaveBeenCalled();
});
it('keeps exit diagnostics available when the optional runtime source fails', async () => {
  const {api} = fakeApi(snapshot());
  api.snapshot.mockRejectedValue(new Error('执行数据库不可用，请备份并修复'));
  api.exitStatus.mockResolvedValue({safe:false,active:0,unknown:1,pendingRequests:0,waiting:false,reason:'旧自管工作状态未知，请保留数据库'});
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  await waitFor(() => expect(api.exitStatus).toHaveBeenCalled());
  fireEvent.click(screen.getByRole('button',{name:'查看退出选项'}));
  expect(await screen.findByText('旧自管工作状态未知，请保留数据库')).toBeInTheDocument();
  expect(screen.getByText('未确认可以安全退出')).toBeInTheDocument();
  expect(api.start).not.toHaveBeenCalled();
});

it.each(['合计输入超限；尚未发送，草稿保留', '资料绑定项目与会话项目不匹配；尚未发送，草稿保留'])('keeps correctable no-effect input rejection editable: %s', async (message) => {
  const {api}=fakeApi(snapshot());
  api.steer.mockRejectedValueOnce({effect:'none',message});
  render(() => <RuntimePanel project={project} view="execution" api={api} subscribe={subscribe} />);
  const input=await screen.findByLabelText('向此原始会话发送明确输入');
  fireEvent.input(input,{target:{value:'保留待缩小草稿'}});
  const send=screen.getByRole('button',{name:'追加到当前运行'});
  fireEvent.click(send);
  await screen.findByText(message);
  await waitFor(()=>expect(send).toBeEnabled());
  expect(input).toHaveValue('保留待缩小草稿');
  expect(screen.queryByText(/结果核对前发送入口保持关闭/)).toBeNull();
  fireEvent.click(send);
  await waitFor(()=>expect(api.steer).toHaveBeenCalledTimes(2));
});

it('keeps exit coordination visible while a unified work detail hides runtime forms',async()=>{
 const {api}=fakeApi(snapshot());let trigger!:()=>void;
 render(()=><RuntimePanel project={project} view="direction" detailActive api={api} subscribe={async(_changed,onExit)=>{trigger=onExit;return()=>{};}}/>);
 await waitFor(()=>expect(trigger).toBeTypeOf('function'));trigger();
 expect(await screen.findByRole('region',{name:'退出协调'})).toBeVisible();
 expect(api.stopExit).not.toHaveBeenCalled();expect(api.waitExit).not.toHaveBeenCalled();
 expect(screen.getByRole('button',{name:'留在后台 / 取消等待退出'})).toBeVisible();
});

it('shows a failed exit action inside the visible exit coordinator while detail forms are hidden',async()=>{
 const {api}=fakeApi(snapshot());api.stopExit.mockRejectedValue(new Error('退出回执不可用'));let trigger!:()=>void;
 render(()=><RuntimePanel project={project} view="direction" detailActive api={api} subscribe={async(_changed,onExit)=>{trigger=onExit;return()=>{};}}/>);
 await waitFor(()=>expect(trigger).toBeTypeOf('function'));trigger();
 await waitFor(()=>expect(screen.getByRole('button',{name:'明确请求停止后退出'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'明确请求停止后退出'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('退出回执不可用');expect(screen.getByRole('alert')).toBeVisible();expect(screen.getByText('未确认可以安全退出')).toBeVisible();
});
