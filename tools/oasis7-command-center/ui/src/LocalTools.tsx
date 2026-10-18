import { createSignal, For, onMount, onCleanup, Show } from 'solid-js';
import { connected, contextBridge, toolsBridge, errorMessage, observedTime, prepareFailureEffect, type ContextSummary } from './bridge';
import type { ContextBinding, ContextEntry, FileSignPrepared, ToolJob, ToolObservation } from './generated/core';
import type { Project } from './types';

const stateLabel=(state:string)=>({available:'可用',not_installed:'未安装',incompatible:'不兼容',needs_configuration:'需要配置',running:'执行中',succeeded:'已完成',failed:'已失败',cancelled:'已取消',unknown:'结果未知',partial:'已核实部分结果',unverified:'尚未验证',current:'核验当前有效',content_changed:'内容已变化',expired:'已过期',future_dated:'核验时间在未来'} as Record<string,string>)[state]??'状态尚未识别';
const kindLabel=(kind:string)=>({machine:'机器',environment:'环境',runbook:'操作说明',bind:'资料库绑定',save:'保存资料',archive:'归档资料',restore_entry:'恢复历史版本',snapshot_export:'导出加密快照',snapshot_restore:'恢复加密快照',file_prepare:'准备签名请求',submit:'提交签名请求',cli_install:'安装资料 CLI'} as Record<string,string>)[kind]??kind;
const persistenceLabel=(value:string)=>({saved:'本机保存完成',file:'已写入文件',uncommitted:'尚未提交 Git',git_uncommitted:'尚未提交 Git',committed:'已提交 Git'} as Record<string,string>)[value]??'本机保存状态已核对';
function signerSummary(observation:ToolObservation|undefined){
  if(!observation)return '尚未检测签名安装。';
  const reason=observation.reason??'';
  if(reason.includes('App is not the bound ordinary caller'))return '当前 App 用户尚未绑定为签名调用者。';
  if(reason.includes('digest drift')||reason.includes('release identity drift'))return '签名安装与发行清单不匹配，请查看实际安装详情。';
  if(observation.state==='not_installed')return '尚未检测到本机签名工具安装。';
  if(observation.state==='incompatible')return '已安装工具的能力协议不兼容，请查看实际安装详情。';
  if(reason)return '签名环境需要配置或处理，请查看实际安装详情。';
  return '准备成功不代表 worker 已授权本次签名。';
}

export default function LocalTools(props: { project?: Project }) {
  const [bindings, setBindings] = createSignal<ContextBinding[]>([]), [binding, setBinding] = createSignal<ContextBinding>();
  const [entries, setEntries] = createSignal<ContextSummary[]>([]), [entry, setEntry] = createSignal<ContextEntry>();
  const [raw, setRaw] = createSignal(''), [dirty, setDirty] = createSignal(false), [advanced, setAdvanced] = createSignal(false);
  const [query, setQuery] = createSignal(''), [name, setName] = createSignal('');
  const [busy, setBusy] = createSignal(false), [message, setMessageRaw] = createSignal(''), [history, setHistory] = createSignal<unknown>();
  const [signer, setSigner] = createSignal<ToolObservation>(), [jobs, setJobs] = createSignal<ToolJob[]>([]), [result, setResult] = createSignal<unknown>();
  const [file, setFile] = createSignal(''), [jobId, setJobId] = createSignal(''), [signerId, setSignerId] = createSignal(''), [grantId, setGrantId] = createSignal(''), [context, setContext] = createSignal('');
  const [prepared, setPrepared] = createSignal<FileSignPrepared>(), [frozen, setFrozen] = createSignal(false);
  const [prepareFailure,setPrepareFailure]=createSignal<'none'|'unknown'>();
  const [committed,setCommitted]=createSignal(false);
  const [recoveredNeedsInspection,setRecoveredNeedsInspection]=createSignal(false);
  const [password,setPassword]=createSignal(''), [directoryName,setDirectoryName]=createSignal(''), [snapshotResult,setSnapshotResult]=createSignal<unknown>();
  const [expectedKey,setExpectedKey]=createSignal(''), [signature,setSignature]=createSignal(''), [verifyContext,setVerifyContext]=createSignal(''), [verifyFile,setVerifyFile]=createSignal('');
  const [restoreCommit,setRestoreCommit]=createSignal(''), [cli,setCli]=createSignal<unknown>();
  const [builtin,setBuiltin]=createSignal<unknown>();
  const [task,setTask]=createSignal('context');
  const [laneStatus,setLaneStatus]=createSignal<Record<string,string>>({});
  const [exportPassword,setExportPassword]=createSignal('');
  const [exportTarget,setExportTarget]=createSignal<{binding:ContextBinding;selection:string[];label:string}>();
  const [exportResult,setExportResult]=createSignal<unknown>();
  const [verifiedContext,setVerifiedContext]=createSignal<unknown>();
  const [verifyResult,setVerifyResult]=createSignal<unknown>();
  let passwordTimer:ReturnType<typeof setTimeout>|undefined;
  function exportSecret(value:string){clearTimeout(passwordTimer);setExportPassword(value);passwordTimer=setTimeout(()=>setExportPassword(''),60000);}
  onCleanup(()=>{clearTimeout(passwordTimer);setExportPassword('');setPassword('');});
  function setMessage(value:string){setMessageRaw(value);if(value)status(value);}
  function status(value:string,lane=task()){setLaneStatus(old=>({...old,[lane]:value}));}
  const taskStatus=(lane:string)=><Show when={laneStatus()[lane]}><p class="notice" role="status">{laneStatus()[lane]}</p></Show>;

  const [comparison,setComparison]=createSignal<ContextEntry>();
  const [operationBusy,setOperationBusy]=createSignal(false);
  const bindingOptions=()=>{const selected=binding(),values=bindings();return selected&&!values.some(value=>value.id===selected.id)?[selected,...values]:values;};
  async function runOperation(action:()=>Promise<unknown>){if(operationBusy())return;const lane=task();setOperationBusy(true);setMessage('');status('操作处理中；通知不会移动当前编辑焦点。',lane);try{await action();status('本次操作已返回，请在本任务结果区核对实际效果。',lane);}catch(e){const message=errorMessage(e);setMessage(message);status(message,lane);}finally{setOperationBusy(false);}}
  const bytes=(hex:string)=>{if(!/^(?:[0-9a-fA-F]{2})+$/.test(hex))throw new Error('请输入十六进制字节');return hex.match(/../g)!.map(v=>parseInt(v,16));};
  async function snapshot(purpose:'export'|'inspect'|'restore'){if(purpose==='export'){const target=exportTarget();if(!target)throw new Error('请明确选择并固定导出范围');const secret=exportPassword();exportSecret('');setExportResult(await contextBridge.snapshot('export',target.binding,secret,'',target.selection));}else{const secret=password();setPassword('');setSnapshotResult(await contextBridge.snapshot(purpose,binding()??null,secret,directoryName(),[]));}await refresh();}
  let editGeneration = 0;
  let saveOperation: string | undefined, prepareOperation: string | undefined, submitOperation: string | undefined;
  async function run(action: () => Promise<unknown>) { if (busy()) return; const lane=task();setBusy(true);setMessage('');status('正在处理本任务',lane);try{await action();status(message()||'本次操作已返回；请核对就地结果和持久记录。',lane);}catch(e){const message=errorMessage(e);setMessage(message);status(message,lane);}finally{setBusy(false);} }
  async function refresh() {
    const values=await Promise.allSettled([contextBridge.bindings(),contextBridge.info(),toolsBridge.signer(),toolsBridge.jobs(),toolsBridge.cli()]);
    if(values[0].status==='fulfilled')setBindings(values[0].value);else status(`资料来源读取失败，保留旧结果：${errorMessage(values[0].reason)}`,'context');
    if(values[1].status==='fulfilled')setBuiltin(values[1].value);
    if(values[2].status==='fulfilled')setSigner(values[2].value);else status(`签名环境读取失败，其他工具仍可用：${errorMessage(values[2].reason)}`,'signing');
    if(values[3].status==='fulfilled')setJobs(values[3].value);else status(`操作读取失败，未据此判断没有未决操作：${errorMessage(values[3].reason)}`,'operations');
    if(values[4].status==='fulfilled')setCli(values[4].value);else setCli({state:'未核实',reason:errorMessage(values[4].reason)});
  }
  onMount(() => { if (connected()) void run(refresh); });
  async function list(target = binding()) { if (target) { const found = await contextBridge.list(target, query()); setEntries(found.entries); if (found.warnings.length) setMessage(found.warnings.join('\n')); } }
  function untouched() { if (dirty()) { setMessage('当前草稿尚未保存。请保存或明确重新加载后再切换资料。'); return false; } return true; }
  async function selectEntry(id: string, discard = false) { if (!discard && !untouched()) return; const b = binding(); if (!b) return; const generation = editGeneration; const found = await contextBridge.get(b, id); if (generation !== editGeneration) { setMessage('读取期间的新草稿已保留；请明确重新加载后再切换资料。'); return; } setEntry(found); setRaw(found.rawText); setDirty(false); setHistory(undefined);setComparison(undefined); saveOperation = undefined; }
  function edit(text: string) { editGeneration += 1; setRaw(text); setDirty(true); saveOperation = undefined; }
  function discardNew() { setRaw(''); setDirty(false); setHistory(undefined); saveOperation=undefined; }
  function document() { try { const text = raw().replaceAll('\r\n', '\n'); const end = text.indexOf('\n---\n', 4); return { metadata: JSON.parse(text.slice(4, end)) as Record<string, unknown>, body: text.slice(end + 5) }; } catch { return undefined; } }
  function field(key: string) { return String(document()?.metadata[key] ?? ''); }
  function encode(metadata: Record<string, unknown>, body: string) { return `---\n${JSON.stringify(metadata, null, 2)}\n---\n${body}`; }
  function setField(key: string, value: string) { const d = document(); if (d) edit(encode({ ...d.metadata, [key]: value }, d.body)); }
  function createEntry() { if (!untouched() || !binding()) return; setEntry(undefined); setRaw(encode({ schema_version: 'oasis7.context_entry.v1', project_id: binding()!.projectId, id: '', kind: 'machine', title: '', environment: 'local', owner: 'human', tags: [], state: 'active', updated_at: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'), updated_by: 'command-center:human', verification: null, credential_refs: [], links: [] }, '')); setDirty(true); setHistory(undefined);setComparison(undefined); saveOperation = undefined; }
  async function save() {
    const b = binding(), id = field('id'); if (!b || !id) return;
    const generation = editGeneration, submitted = raw(), submittedMetadata = document()?.metadata, expectedRevision = entry()?.revision ?? null;
    saveOperation ??= crypto.randomUUID();
    const saved = await contextBridge.save({ bindingId: b.id, bindingVersion: b.bindingVersion, id, rawText: submitted, expectedRevision, actor: 'command-center:human' }, saveOperation);
    saveOperation = undefined;
    // Advance only to the exact saved revision; a later external edit still conflicts.
    const savedEntry: ContextEntry = { id, revision: saved.revision, rawText: submitted, metadataJson: JSON.stringify(submittedMetadata ?? {}), verificationState: 'unverified' };
    setEntry(savedEntry);
    const found = await contextBridge.get(b, id);
    if (generation === editGeneration) { setEntry(found); setRaw(found.rawText); setDirty(false); }
    else { setEntry(savedEntry); setDirty(true); }
    await list(b);
    setMessage(`已写入文件 · ${persistenceLabel(saved.persistence)}。${generation !== editGeneration ? '保存期间的新草稿已保留，请再次明确保存。' : ''}远端同步尚未核实。`);
  }
  async function prepare(){
    // Invalid JSON fails locally before either the operation ID or freeze exists.
    const parsed=JSON.parse(context());
    prepareOperation??=crypto.randomUUID();
    setPrepareFailure(undefined);setCommitted(false);setFrozen(true);
    try {
      setPrepared(await toolsBridge.prepare({job_id:jobId(),file_path:file(),signer_id:signerId(),grant_id:grantId(),context:parsed},prepareOperation));
    } catch(error) {
      const effect=prepareFailureEffect(error);setPrepareFailure(effect);
      if(effect==='none'){setFrozen(false);setPrepared(undefined);prepareOperation=undefined;}
      throw error;
    }
    await refresh();
  }
  async function recoverSigning(operationId: string) {
    if (frozen() && prepareOperation !== operationId) throw new Error('当前已有另一固定请求，请先核对原 Job；不能覆盖。');
    const recovered = await toolsBridge.recoverPrepare(operationId);
    if (recovered.operation_id !== operationId || recovered.input.job_id !== recovered.prepared.job_id || !recovered.prepared.prepared_fingerprint) throw new Error('原准备记录不完整，无法恢复；请手动核对原操作，不创建新 ID 重签。');
    const same = prepareOperation === operationId;
    setFile(recovered.input.file_path); setJobId(recovered.input.job_id);
    setSignerId(recovered.input.signer_id); setGrantId(recovered.input.grant_id);
    setContext(JSON.stringify(recovered.input.context));
    prepareOperation = operationId; if (!same) submitOperation = undefined;
    setPrepared(recovered.prepared); setResult(recovered.prepared); setFrozen(true);
    setPrepareFailure(undefined); setCommitted(false); setRecoveredNeedsInspection(true);
    setTask('signing');setMessage('已恢复原固定请求；请先核对原 Job 结果，再明确提交或开始新请求。');
  }
  async function inspectSigning() {
    setCommitted(false);
    const original=prepared();
    const observed=await toolsBridge.inspect(jobId(),original?.prepared_fingerprint);
    setResult(observed);
    if(original&&original.job_id===jobId()&&observed!==null&&typeof observed==='object'){
      const receipt=observed as Record<string,unknown>;
      if(receipt.command==='inspect'&&receipt.error_code===null&&['prepared','reserved','committed'].includes(String(receipt.status))){setRecoveredNeedsInspection(false);if(receipt.status==='committed')setCommitted(true);}
    }
    await refresh();
  }
  function newSigningRequest(){
    if(busy()||!committed()||!prepared())return;
    setPrepared(undefined);setFrozen(false);setPrepareFailure(undefined);setCommitted(false);setRecoveredNeedsInspection(false);
    setFile('');setJobId('');setSignerId('');setGrantId('');setContext('');setResult(undefined);
    prepareOperation=undefined;submitOperation=undefined;
    setMessage('原 Job 和操作记录保留。请选择新文件并填写新的签名请求。');
  }
  return <div class="local-tools"><nav class="actions" aria-label="本机任务"><For each={Object.entries({context:'资料',backup:'备份与恢复',signing:'签名',verify:'独立验签',cli:'终端 CLI',operations:'操作核对'})}>{([key,label])=><button aria-current={task()===key?'page':undefined} onClick={()=>{setTask(key);setPassword('');exportSecret('');}}>{label}</button>}</For></nav>
    <Show when={prepareFailure()==='none'}><p class="notice">本次准备未创建或修改签名 Job；可修正输入后重新准备。</p></Show>
    <Show when={prepareFailure()==='unknown'}><p class="notice">无法确认本次准备是否产生副作用；已保留原 Job 和固定输入。请核对原操作，不要用新 ID 重签。</p></Show>
    <Show when={operationBusy()}><p class="notice" role="status">本机操作处理中；可继续查阅资料与核对操作记录。</p></Show>
    <Show when={jobs().some(job=>job.state==='partial')}><p class="notice">资料文件的部分结果已核实，资料库关联尚未完成。可使用“连接已有资料库”显式补全；原操作不会自动重放。</p></Show>
    <section class="panel"><div class="panel-title"><h3>当前电脑 · 本机工具</h3><button disabled={busy() || !connected()} onClick={() => void run(refresh)}>检查本机事实</button></div><div class="panel-body"><p class="muted">当前项目：{props.project?.path ?? '未选择项目'}。远端机器资料不会证明远端安装已核实。</p><div class="tool-overview"><div><strong>内置私有资料</strong><span class="tag">{connected() ? '可用' : '未接入桌面核心'}</span><p>直接复用资料库；无需先安装独立 CLI。</p><details><summary>内置版本与能力</summary><pre>{JSON.stringify(builtin(),null,2)}</pre></details></div><div><strong>本地签名工具</strong><span class="tag">{({available:'可用',not_installed:'未安装',incompatible:'不兼容',needs_configuration:'需要配置'} as Record<string,string>)[signer()?.state??'']??'未检测'}</span><p>{signerSummary(signer()) ?? '准备成功不代表 worker 已授权本次签名。'}</p><details><summary>实际安装详情</summary><pre>{JSON.stringify(signer(), null, 2)}</pre></details></div></div><p class="muted small">管理员助手与安装授权正在等待单独讨论；已有兼容安装可以使用普通 caller。通用升级、轮换与 repair GUI 未提供。</p></div></section>
    <Show when={task()==='context'}><section class="panel"><div class="panel-title"><h3>私有资料</h3>{taskStatus('context')}<div class="actions"><button disabled={busy() || !connected() || !props.project} onClick={() => void run(async () => { if (!untouched()) return; const found = await contextBridge.select(false, name()); if (found) { await refresh(); setBinding(found); setEntry(undefined); setRaw(''); await list(found); } })}>连接已有资料库</button><button disabled={busy() || !connected() || !props.project || !name().trim()} onClick={() => void run(async () => { if (!untouched()) return; const found = await contextBridge.select(true, name().trim()); if (found) { await refresh(); setBinding(found); setEntry(undefined); setRaw(''); await list(found); } })}>创建资料库</button></div></div><div class="panel-body"><label>新资料库名称<input aria-label="新资料库名称" value={name()} onInput={e => setName(e.currentTarget.value)} /></label><label>资料库<select aria-label="资料库" value={binding()?.id ?? ''} onChange={e => { const id = e.currentTarget.value; void run(async () => { if (!untouched()) return; const b = bindings().find(b => b.id === id); setBinding(b); setEntry(undefined); setRaw(''); setEntries([]); await list(b); }); }}><option value="" selected={!binding()}>选择资料库</option><For each={bindingOptions()}>{b => <option value={b.id} selected={binding()?.id===b.id}>{b.name} · {b.root}</option>}</For></select></label><Show when={binding()}>{b => <p class="small muted">{b().root} · store {b().storeId} · 绑定版本 {String(b().bindingVersion)}</p>}</Show></div>
      <Show when={binding()}><div class="columns"><div class="panel-body"><form class="project-form" onSubmit={e => { e.preventDefault(); void run(() => list()); }}><input aria-label="搜索私有资料" value={query()} onInput={e => setQuery(e.currentTarget.value)} /><button disabled={busy()}>搜索资料</button></form><button disabled={busy() || dirty()} onClick={createEntry}>新建条目</button><For each={entries()}>{item => <div class="row"><div><button class="row-title" onClick={() => void run(() => selectEntry(item.id))}>{item.title}</button><div class="metadata">{kindLabel(item.kind)} · {item.environment} · {stateLabel(item.verification_state)}</div><p class="small muted">{item.summary}</p></div></div>}</For><Show when={!entries().length}><p class="muted">当前已读取范围没有条目。</p></Show></div><div class="panel-body editor"><Show when={raw()}><div class="actions"><span class="tag">{dirty() ? '未保存草稿' : '已读取修订'}</span><button onClick={() => setAdvanced(!advanced())}>{advanced() ? '简化表单' : '高级原文编辑'}</button></div><Show when={!advanced()}><label>条目 ID<input aria-label="条目 ID" disabled={busy() || !!entry()} value={field('id')} onInput={e => setField('id', e.currentTarget.value)} /></label><label>类型<select aria-label="资料类型" value={field('kind')} onChange={e => setField('kind', e.currentTarget.value)}><option value="machine">机器</option><option value="environment">环境</option><option value="runbook">操作说明</option></select></label><label>名称<input aria-label="资料名称" value={field('title')} onInput={e => setField('title', e.currentTarget.value)} /></label><label>环境标签<input aria-label="环境标签" value={field('environment')} onInput={e => setField('environment', e.currentTarget.value)} /></label><label>正文<textarea aria-label="资料正文" value={document()?.body ?? ''} onInput={e => { const d = document(); if (d) edit(encode(d.metadata, e.currentTarget.value)); }} /></label></Show><Show when={advanced()}><label>完整合法原文<textarea aria-label="资料原文" rows="20" value={raw()} onInput={e => edit(e.currentTarget.value)} /></label></Show><p class="small muted">未展示的合法元数据原样保留。资料以转义文本展示，不执行代码或加载远端资源。</p><div class="actions"><Show when={!entry()}><button disabled={busy()} onClick={discardNew}>放弃新条目草稿</button></Show><button class="primary" disabled={busy() || !dirty() || !field('id')} onClick={() => void run(save)}>保存资料</button><button disabled={busy()||!entry()||!dirty()} onClick={()=>void run(async()=>{setComparison(await contextBridge.get(binding()!,entry()!.id));})}>比较当前修订与草稿</button><button disabled={busy() || !entry()} onClick={() => void run(() => selectEntry(entry()!.id, true))}>重新加载并放弃草稿</button><button disabled={busy() || !entry()} onClick={() => void run(async () => setHistory(await contextBridge.history(binding()!, entry()!.id)))}>查看历史</button><button disabled={busy() || dirty() || !entry()} onClick={() => void run(async () => { await contextBridge.archive(binding()!, entry()!.id, entry()!.revision); await selectEntry(entry()!.id, true); await list(); })}>归档</button></div><Show when={comparison()}>{current=><section aria-label="资料冲突比较"><div class="panel-title"><h4>当前修订与未保存草稿</h4><button onClick={()=>setComparison(undefined)}>关闭比较并保留草稿</button></div><div class="columns"><div><h4>当前文件 · {current().revision}</h4><pre>{current().rawText}</pre></div><div><h4>当前草稿 · 基于 {entry()?.revision}</h4><pre>{raw()}</pre></div></div></section>}</Show><pre class="small">修订 {entry()?.revision ?? '新条目 · 创建时检查不存在'}</pre><Show when={history() !== undefined}><pre>{JSON.stringify(history(), null, 2)}</pre></Show></Show></div></div></Show>
    </section></Show>
    <Show when={task()==='signing'}><section class="panel"><div class="panel-title"><h3>文件签名 · file_ed25519_v1</h3>{taskStatus('signing')}<button disabled={busy() || signer()?.state !== 'available'} onClick={() => void run(async () => setResult(await toolsBridge.doctor()))}>检查签名环境</button></div><div class="panel-body editor"><p class="muted">需要已有准确密钥、policy 与 grant。上下文必须来自明确请求；资料不能产生签名授权。</p><button disabled={busy() || frozen() || signer()?.state !== 'available'} onClick={() => void run(async () => { const f = await toolsBridge.chooseFile(); if (f) setFile(f); })}>选择待签名文件</button><p>{file() || '未选择文件'}</p><label>Job ID<input aria-label="签名 Job ID" disabled={frozen()} value={jobId()} onInput={e => setJobId(e.currentTarget.value)} /></label><label>签名身份<input aria-label="签名身份" disabled={frozen()} value={signerId()} onInput={e => setSignerId(e.currentTarget.value)} /></label><label>实际 grant ID<input aria-label="实际 grant ID" disabled={frozen()} value={grantId()} onInput={e => setGrantId(e.currentTarget.value)} /></label><label>完整 SignContext JSON<textarea aria-label="完整签名上下文" disabled={frozen()} value={context()} onInput={e => {setContext(e.currentTarget.value);setVerifiedContext(undefined);}} /></label><button disabled={busy()||frozen()||!context()} onClick={()=>void run(async()=>{const imported=await toolsBridge.importSignContext(context());setVerifiedContext(imported);setContext(JSON.stringify(imported,null,2));})}>导入并严格校验既有 SignContext</button><Show when={verifiedContext()}><details open><summary>完整已导入上下文预览</summary><pre>{JSON.stringify(verifiedContext(),null,2)}</pre></details></Show><div class="actions"><button disabled={busy() || frozen() || !file() || !jobId() || !signerId() || !grantId() || !context() || signer()?.state !== 'available'} onClick={() => void run(prepare)}>准备固定请求</button><button disabled={busy() || !prepared() || committed() || recoveredNeedsInspection()} onClick={() => void run(async () => { submitOperation ??= crypto.randomUUID(); setResult(await toolsBridge.submit(prepared()!.job_id, prepared()!.prepared_fingerprint, submitOperation)); await refresh(); })}>提交此请求</button><button disabled={busy()||!frozen()} onClick={()=>void run(async()=>{if(!prepareOperation)throw new Error('原准备操作身份不可用，请核对本机操作记录。');const recovered=await toolsBridge.reconcilePrepare({job_id:jobId(),file_path:file(),signer_id:signerId(),grant_id:grantId(),context:JSON.parse(context())},prepareOperation);setPrepared(recovered);setResult(recovered);setPrepareFailure(undefined);await refresh();})}>核对原准备操作</button><button disabled={busy() || !jobId()} onClick={() => void run(inspectSigning)}>核对原 Job 结果</button><button disabled={busy()||!committed()||!prepared()} onClick={newSigningRequest}>开始新的签名请求</button></div><Show when={prepared()}><pre>{JSON.stringify(prepared(), null, 2)}</pre></Show><Show when={result() !== undefined}><pre>{JSON.stringify(result(), null, 2)}</pre></Show><p class="small muted">签名针对原文件与同一 context 构成的 envelope。超时或结果未知时核对原 Job，不创建新 ID 重签。</p></div></section></Show>
    <Show when={task()==='backup'}><section class="panel"><div class="panel-title"><h3>加密快照恢复</h3>{taskStatus('backup')}</div><div class="panel-body editor"><p class="muted">原资料库无需存在；新机器也可从加密快照恢复。先选择普通项目，验证快照身份后恢复到独立的新目录。{binding()?'当前选择的资料库记录仅用于核对快照身份。':'尚未连接资料库，将从通过口令认证的快照读取资料库身份。'}</p><label>快照口令<input type="password" autocomplete="off" aria-label="快照口令" value={password()} onInput={e=>setPassword(e.currentTarget.value)}/></label><label>恢复的新目录名称<input aria-label="恢复新目录名称" value={directoryName()} onInput={e=>setDirectoryName(e.currentTarget.value)}/></label><div class="actions"><button disabled={busy()||operationBusy()||!password()} onClick={()=>void runOperation(()=>snapshot('inspect'))}>检查加密快照</button><button disabled={busy()||operationBusy()||!connected()||!props.project||!password()||!directoryName()} onClick={()=>void runOperation(()=>snapshot('restore'))}>从加密快照恢复新资料库</button></div><Show when={snapshotResult()!==undefined}><pre>{JSON.stringify(snapshotResult(),null,2)}</pre></Show></div></section></Show>
    <Show when={binding()&&task()==='backup'}><section class="panel"><div class="panel-title"><h3>资料健康、历史恢复与加密快照</h3>{taskStatus('backup')}<button disabled={busy()} onClick={()=>void run(async()=>setSnapshotResult(await contextBridge.doctor(binding()!)))}>检查资料库健康</button></div><div class="panel-body editor"><p class="muted">导出范围必须明确选择并固定；切换资料条目不会静默改变范围。历史恢复按当前修订检查冲突；快照只恢复到独立的新目录。</p><label>历史提交 ID<input aria-label="历史提交 ID" value={restoreCommit()} onInput={e=>setRestoreCommit(e.currentTarget.value)}/></label><button disabled={busy()||dirty()||!entry()||!restoreCommit()} onClick={()=>void run(async()=>{await contextBridge.restoreEntry(binding()!,entry()!.id,restoreCommit(),entry()!.revision);await selectEntry(entry()!.id,true);await list();})}>恢复此历史版本</button><div class="actions"><button disabled={!entry()||!binding()} onClick={()=>setExportTarget({binding:binding()!,selection:[entry()!.id],label:`当前条目 ${entry()!.id}`})}>固定导出当前条目</button><button disabled={!binding()} onClick={()=>setExportTarget({binding:binding()!,selection:[],label:`整库 ${binding()!.name}`})}>固定导出整库</button></div><p>已固定范围：{exportTarget()?.label??'尚未选择'}；资料库 {exportTarget()?.binding.id??'未知'}</p><label>仅本次导出的加密口令<input type="password" autocomplete="off" aria-label="导出口令" value={exportPassword()} onInput={e=>exportSecret(e.currentTarget.value)}/></label><p class="small muted">与恢复口令独立；使用、取消或一分钟无操作后清除。</p><button onClick={()=>{exportSecret('');setExportTarget(undefined);}}>取消导出，清除秘密</button><div class="actions"><button disabled={busy()||operationBusy()||!exportPassword()||!exportTarget()} onClick={()=>void runOperation(()=>snapshot('export'))}>导出加密快照</button></div><Show when={exportResult()!==undefined}><pre>{JSON.stringify(exportResult(),null,2)}</pre></Show></div></section></Show>
    <Show when={task()==='verify'}><section class="panel"><div class="panel-title"><h3>独立验签</h3>{taskStatus('verify')}</div><div class="panel-body editor"><p class="muted">使用原文件、原 context 与独立核实的预期公钥重建 envelope。</p><button disabled={busy()||!connected()} onClick={()=>void run(async()=>{const f=await toolsBridge.chooseFile();if(f)setVerifyFile(f);})}>选择验签原文件</button><p>{verifyFile()}</p><label>验签 SignContext JSON<textarea aria-label="验签上下文" value={verifyContext()} onInput={e=>setVerifyContext(e.currentTarget.value)}/></label><label>预期公钥 hex<input aria-label="预期公钥" value={expectedKey()} onInput={e=>setExpectedKey(e.currentTarget.value)}/></label><label>签名 hex<input aria-label="签名值" value={signature()} onInput={e=>setSignature(e.currentTarget.value)}/></label><button disabled={busy()||!verifyFile()||!verifyContext()||!expectedKey()||!signature()} onClick={()=>void run(async()=>{const valid=await toolsBridge.verify(verifyFile(),JSON.parse(verifyContext()),bytes(expectedKey()),bytes(signature()));setMessage(valid?'独立 envelope 验签通过':'验签失败');})}>独立验签</button></div></section></Show>
    <Show when={task()==='cli'}><section class="panel"><div class="panel-title"><h3>可选终端 CLI</h3>{taskStatus('cli')}<button disabled={busy()||!connected()} onClick={()=>void run(async()=>{setCli(await toolsBridge.installCli());await refresh();})}>从本 App 启用资料 CLI</button></div><div class="panel-body"><p class="muted">核对随包发行清单后安装到普通用户版本目录。已有同名文件不会被覆盖；终端 PATH 配置单独核实。</p><p>{cli()===null?'随包版本尚未安装':cli()===undefined?'尚未检测':'实际安装核对结果'}</p><Show when={cli()!=null}><pre>{JSON.stringify(cli(),null,2)}</pre></Show></div></section></Show>
    <Show when={task()==='operations'}><section class="panel"><div class="panel-title"><h3>本机操作</h3>{taskStatus('operations')}<button disabled={busy() || !connected()} onClick={() => void run(async () => setJobs(await toolsBridge.jobs()))}>核对操作记录</button></div><For each={jobs()}>{job => <div class="row"><div>{kindLabel(job.kind)}<div class="metadata">{job.id} · {stateLabel(job.state)} · {observedTime(job.observed_at)}</div><Show when={job.kind==='file_prepare' && ['succeeded','unknown'].includes(job.state)}><button disabled={busy()} onClick={()=>void run(()=>recoverSigning(job.id))}>恢复此固定签名请求</button></Show><Show when={job.state==='unknown' && job.kind==='cli_install'}><button disabled={busy()} onClick={()=>void run(async()=>{setCli(await toolsBridge.reconcileCli(job.id));await refresh();})}>核对原 CLI 安装</button></Show><Show when={job.state==='unknown' && ['save','archive','restore_entry','bind','snapshot_export','snapshot_restore'].includes(job.kind)}><button disabled={busy()} onClick={()=>void run(async()=>{const secret=password();setPassword('');const reconciled=await contextBridge.reconcile(job.id,secret||undefined);setMessage(`${stateLabel(reconciled.state)} · ${reconciled.result??'已核对原操作'}`);await refresh();})}>核对原资料操作</button></Show></div></div>}</For><Show when={!jobs().length}><div class="empty">尚无已登记的本机操作。</div></Show></section></Show>
    <Show when={dirty()&&message()}><p class="small muted">草稿已保留。冲突时重新加载前请核对需要的修改。</p></Show>
  </div>;
}
