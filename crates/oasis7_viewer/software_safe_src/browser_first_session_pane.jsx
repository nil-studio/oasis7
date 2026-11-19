import { createEffect, createSignal, For, Show } from "solid-js";
import { firstSessionFacts, firstSessionGrantDraft } from "./browser_first_session_facts.js";
import { directActionSelection } from "./browser_game_permit.js";

export function BrowserFirstSessionPane(props) {
 const tr = (cn, en) => String(typeof props.locale === "function" ? props.locale() : props.locale || "en").startsWith("zh") ? cn : en;
 const driver = () => props.driver(), state = () => props.state();
 const [facts, setFacts] = createSignal(null), [busy, setBusy] = createSignal(false), [error, setError] = createSignal("");
 const [offerId, setOfferId] = createSignal(""), [slotId, setSlotId] = createSignal("");
 const [draft, setDraft] = createSignal(null), [consent, setConsent] = createSignal(false);
 const [preview, setPreview] = createSignal(null), [transferKind, setTransferKind] = createSignal(""), [amount, setAmount] = createSignal("1");
 const [choice, setChoice] = createSignal("");
 let installed, generation = 0;
 const fence = () => state()?.fence, agentId = () => fence()?.agent_id;
 const disabled = () => busy() || !state()?.ready || ["running", "direct_running", "preparing"].includes(state()?.phase);
 const offers = () => Object.values(facts()?.starter.offers ?? {});
 const offer = () => facts()?.starter.offers?.[offerId()];
 const slot = () => offer()?.offer.slots?.[slotId()];
 const digest = (domain, value) => driver().operationId(domain, value);
 const materialName = (kind) => facts()?.state?.material_profiles?.[kind]?.display_name || ({ iron_ore: tr("铁矿", "Iron ore"), carbon_fuel: tr("碳燃料", "Carbon fuel"), iron_ingot: tr("铁锭", "Iron ingot"), slag: tr("炉渣", "Slag"), structural_frame: tr("结构框架", "Structural frame"), heat_coil: tr("加热线圈", "Heating coil"), refractory_brick: tr("耐火砖", "Refractory brick"), maintenance_part: tr("维护零件", "Maintenance part") })[kind] || kind;
 const readable = (materials) => (Array.isArray(materials) ? materials : Object.entries(materials || {}).map(([kind, amount]) => ({ kind, amount })))
  .map((item) => `${materialName(item.kind)} × ${item.amount}`).join(" · ") || "—";
 async function refresh() {
  const current = driver(), expected = generation;
  const observed = await current.firstSessionView();
  if (current !== driver() || expected !== generation) return;
  const value = firstSessionFacts(observed, agentId()); setFacts(value);
  if (!offerId() || !value.starter.offers?.[offerId()]) {
   const id = Object.keys(value.starter.offers || {})[0] || ""; setOfferId(id);
   setSlotId(value.starter.offers?.[id]?.available_slots?.[0] || "");
  }
 }
 async function act(action) {
  if (busy()) return;
  const expected = generation; setBusy(true); setError(""); setConsent(false);
  try { await action(); }
  catch (failure) { if (expected === generation) setError(failure?.code || "game_operation_failed"); }
  finally { if (expected === generation) { try { await refresh(); } catch (failure) { if (!error()) setError(failure?.code || "game_operation_failed"); } setBusy(false); } }
 }
 createEffect(() => {
  const key = `${driver() ? "session" : "none"}:${JSON.stringify(fence() || null)}:${state()?.ready === true}`;
  if (installed === key) return; installed = key; generation += 1;
  setFacts(null); setDraft(null); setPreview(null); setConsent(false); setBusy(false); setError("");
  if (state()?.ready && driver()?.firstSessionView) void act(async () => {});
 });
 function chooseDraft(value) { setDraft(value); setConsent(false); setPreview(null); }
 function grant(objectId, action, limit) {
  const reserved = facts().allGrants.find(({ grant }) => !grant.revoked && grant.object_id === objectId && grant.action_kinds.includes(action));
  if (reserved) {
   const current = reserved.grant;
   chooseDraft({ type: "grant", value: { permitId: digest("first-session-revoke-grant", { fence: fence(), id: current.grant_id, revision: current.revision }),
    confirmedFence: fence(), operation: { operation: "revoke", grant_id: current.grant_id, expected_revision: String(current.revision) } } });
   return;
  }
  const value = firstSessionGrantDraft({ digest, fence: fence(), tick: facts().tick, objectId, action, limit,
   priorAuthorities: facts().allGrants.filter(({ grant }) => grant.object_id === objectId && grant.action_kinds.includes(action))
    .map(({ grant }) => ({ id: grant.grant_id, revision: grant.revision, revoked: grant.revoked })) });
  chooseDraft({ type: "grant", value });
 }
 async function claimDraft() {
  const prior = facts().actionOriginals.filter((item) => item.mode === "direct" && item.selection?.action?.type === "WorldRule"
   && ["rejected", "expired"].includes(item.state)).at(-1);
  const permitId = digest("first-session-starter-claim", { fence: fence(), owner: facts().starter.owner_binding, offer: offerId(), slot: slotId(),
   ...(prior ? { explicitRetryOf: prior.operationId } : {}) });
  const action = await driver().starterClaim({ offerId: offerId(), slotId: slotId(), requestId: permitId });
  chooseDraft({ type: "direct", value: { permitId, confirmedFence: fence(), selection: directActionSelection(action, agentId()) } });
 }
 async function quote(kind) {
  const claim = facts().claim;
  const phase = facts().first ? `independent:${facts().first.job_id}` : "first";
  const permitId = await driver().nextPreviewPermitId({ kind, factory: claim.factory_id, phase });
  const result = await driver().preview({ permitId, confirmedFence: fence(), kind, selection: kind === "build"
   ? { site: claim.site_id, factory: claim.factory_id, kind: "factory.smelter.mk1" }
   : { factory: claim.factory_id, recipe: claim.recipe_id, batches: 1 } });
  setPreview(result); setDraft(null); setConsent(false);
 }
 function transferDraft() {
  const factory = facts().factory, count = Number(amount()), kind = transferKind();
  if (!facts().canTransfer || !factory?.input_ledger?.startsWith("site:") || !Number.isSafeInteger(count) || count <= 0
   || count > (facts().agentMaterials[kind] ?? 0) || !Object.hasOwn(facts().claim.materials, kind)) return;
  const action = { type: "TransferMaterial", data: { requester_agent_id: agentId(), from_ledger: `agent:${agentId()}`,
   to_ledger: factory.input_ledger, kind, amount: count, distance_km: 0, priority: null, route_ids: [], auto_reroute: false } };
  const objectId = `${action.data.from_ledger}->${action.data.to_ledger}:${kind}`;
  if (!facts().hasGrant(objectId, "TransferMaterial")) { grant(objectId, "TransferMaterial", 1); return; }
  const permitId = digest("first-session-transfer", { fence: fence(), action, available: facts().agentMaterials[kind], tick: facts().tick });
  chooseDraft({ type: "direct", value: { permitId, confirmedFence: fence(), selection: directActionSelection(action, agentId()) } });
 }
 async function executeDraft() {
  if (!draft() || !consent()) return;
  const current = draft();
  await act(async () => { if (current.type === "grant") await driver().nativeGrant(current.value); else await driver().directAction(current.value); setDraft(null); });
 }
 async function executePreview(agent) {
  if (!preview() || !consent()) return;
  const current = preview();
  const permitId = agent ? digest("first-session-agent-one-batch", { fence: fence(), factory: current.selection.factory, recipe: current.selection.recipe })
   : digest("first-session-player-action", { fence: fence(), context: current.contextOperationId });
  await act(async () => {
   if (agent) await driver().start({ delegation: { permitId, selection: current.selection, contextOperationId: current.contextOperationId } });
   else await driver().directAction({ permitId, confirmedFence: current.confirmedFence, selection: current.selection, contextOperationId: current.contextOperationId });
   setPreview(null);
  });
 }
 const modelReady = () => { state(); return driver()?.modelSession.snapshot().checked === true; };
 const previewExpired = (pending) => {
  try { return pending.context?.valid_until_tick !== undefined && BigInt(facts().tick) > BigInt(pending.context.valid_until_tick); } catch { return false; }
 };
 const rejectedCleanup = (pending) => pending.hasOriginal && pending.originalStates?.length > 0
  && pending.originalStates.every((value) => ["rejected", "expired"].includes(value));
 return <section class="stack first-session-pane" aria-label={tr("第一座工厂与一次 Agent 代办", "First factory and one Agent delegation")}>
  <h3>{tr("你的第一座工厂", "Your first factory")}</h3>
  <p>{tr("原料是生产投入，电力是每次行动的成本；授权规定 Agent 能操作哪些对象。请求确认、世界作业受理与产品结算分别核对。", "Materials are production inputs; electricity pays for actions. Authority determines which objects the Agent may operate. Request confirmation, job admission and product settlement are checked separately.")}</p>
  <button type="button" disabled={disabled()} onClick={() => void act(refresh)}>{tr("读取当前资源与作业", "Read current resources and jobs")}</button>
  <Show when={!state()?.ready}><p>{tr("先在上方核对任务并登记执行身份。领取、授权、建厂和补料都不需要模型 Key。", "Verify the task and register the executor above. Claiming, granting authority, building and supplying materials do not require a model key.")}</p></Show>
  <Show when={facts()}>
   <p>{tr("当前电力", "Current electricity")}: {facts()?.electricity ?? "—"}<br />{tr("我持有的材料", "Materials I hold")}: {readable(facts()?.agentMaterials)}</p>
   <For each={facts()?.openPreviews ?? []}>{(pending) => <div class="stack">
    <p role="status">{rejectedCleanup(pending) ? (previewExpired(pending)
     ? tr("先前请求已拒绝或过期，原件保留。原核对期限现已结束，你可以明确发起新的核对；原许可与模型预算保持原状。", "The prior request was rejected or expired and is retained. Its original Context deadline has now passed. You can explicitly request a new check; the action permission and model budget remain unchanged.")
     : tr("先前请求已拒绝或过期，但原核对尚在有效期内。请等待原期限后刷新；不能把被拒绝的结束请求当作已释放。", "The prior request was rejected or expired, but the original check remains active. Wait until its original deadline and refresh; a rejected cleanup does not release it."))
     : pending.hasOriginal ? tr("这次操作已有原件等待核对，请只读查询结果。", "This operation has an original awaiting verification. Look up its result.") : tr("这次条件核对仍占用当前决策。若需要补料、修改授权或报价已过期，可以明确结束后再重新核对。", "This condition check still holds the current decision. If supplies or authority need to change, or the quote expired, explicitly end it before checking again.")}</p>
    <button type="button" disabled={disabled() || pending.hasOriginal || pending.mode === "direct"} onClick={() => void act(async () => {
     await driver().endPreview({ confirmedFence: fence(), contextOperationId: pending.contextOperationId, permitId: pending.permitId }); setPreview(null); setConsent(false);
    })}>{tr("结束这次条件核对", "End this condition check")}</button>
    <Show when={rejectedCleanup(pending) && previewExpired(pending) && pending.mode === "direct"}>
     <p>{tr("现在可重新选择并明确确认这次玩家操作。它会创建新的许可；上一次失败原件仍保留，Agent 的单批许可不会重置。", "You may now select and explicitly confirm a new player operation. It receives a new permission; the previous failed original is retained, and the Agent's one-batch permission is unchanged.")}</p>
    </Show>
    <Show when={rejectedCleanup(pending) && previewExpired(pending) && pending.mode !== "direct" && pending.selection?.kind}>
     <button type="button" disabled={disabled()} onClick={() => void act(() => quote(pending.selection.kind))}>{tr("明确重新核对当前条件", "Explicitly check current conditions again")}</button>
    </Show>
   </div>}</For>
   <Show when={!facts()?.claim}>
    <p>{tr("公共 starter 来自有限供应与场地。领取凭世界回执绑定到本账号，不增加免费供给。", "The public starter uses finite supplies and sites. A world receipt binds the claim to this account.")}</p>
    <Show when={facts()?.starter.blocker}><p role="status">{facts()?.starter.blocker}</p></Show>
    <label>{tr("公开供应", "Published offer")}<select value={offerId()} onChange={(event) => { const id = event.currentTarget.value; setOfferId(id); setSlotId(facts().starter.offers[id]?.available_slots[0] || ""); setDraft(null); }}><For each={offers()}>{(item, index) => <option value={item.offer.offer_id}>{tr("冶炼厂起步包", "Smelter starter")} {index() + 1}</option>}</For></select></label>
    <label>{tr("可用场地", "Available site")}<select value={slotId()} onChange={(event) => { setSlotId(event.currentTarget.value); setDraft(null); }}><For each={offer()?.available_slots ?? []}>{(id, index) => <option value={id}>{tr("场地", "Site")} {index() + 1}</option>}</For></select></label>
    <p>{readable(offer()?.offer.materials)}<br />{tr("建造电力", "Construction electricity")}: {slot()?.construction_electricity ?? "—"}</p>
    <Show when={offer()?.blocker}><p role="status">{offer()?.blocker}</p></Show>
    <button type="button" disabled={disabled() || !slot() || Boolean(offer()?.blocker) || !facts()?.starter.eligible} onClick={() => void act(async () => {
     if (!facts().hasGrant(offerId(), "WorldRule")) grant(offerId(), "WorldRule", 1); else await claimDraft();
    })}>{facts()?.hasGrant(offerId(), "WorldRule") ? tr("核对这份领取操作", "Review this claim") : tr("准备领取权限", "Review claim authority")}</button>
   </Show>
   <Show when={facts()?.claim}>
    <p>{tr("起步材料、电力与场地已领取。", "Starter materials, electricity and site claimed.")}</p>
    <Show when={!facts()?.factory}>
     <Show when={facts()?.pendingBuild} fallback={<p>{tr("建厂会使用你持有的材料。先核对当前建厂条件与完整投入，拿到报价后，再由你确认提交建造。", "Construction uses the materials you hold. Review current conditions and full inputs, then confirm whether to build.")}</p>}><p role="status">{tr("建造作业已受理，等待世界结算；不要重复建厂。", "Construction is admitted and awaiting world settlement. Do not submit another build.")}</p></Show>
     <button type="button" disabled={disabled() || Boolean(facts()?.pendingBuild)} onClick={() => void act(async () => {
      const claim = facts().claim; const available = facts().starter.offers?.[claim.offer_id]?.offer.slots?.[claim.slot_id];
      if (!facts().hasGrant(claim.factory_id, "BuildFactory", available?.construction_electricity ?? 1)) grant(claim.factory_id, "BuildFactory", Math.max(1, available?.construction_electricity ?? 1));
      else await quote("build");
     })}>{tr("核对建厂权限与真实报价", "Review build authority and quote")}</button>
    </Show>
    <Show when={facts()?.factory}>
     <p>{tr("冶炼厂已建成。", "Smelter built.")}<br />{tr("工厂可用材料", "Factory input stock")}: {readable(facts()?.inputMaterials)}</p>
     <Show when={!facts()?.first || choice()}>
      <Show when={!facts()?.first}><p>{tr("你持有的材料需要送到工厂，才能用于生产。补料后核对单批报价；报价会列出完整投入，包含维护消耗。", "Materials you hold need to reach the factory before production. After supplying them, review the one-batch quote, including maintenance inputs.")}</p></Show>
      <label>{tr("补料种类", "Material to supply")}<select value={transferKind()} onChange={(event) => { setTransferKind(event.currentTarget.value); setDraft(null); }}><option value="">—</option><For each={Object.keys(facts()?.claim.materials ?? {}).filter((kind) => (facts()?.agentMaterials[kind] ?? 0) > 0)}>{(kind) => <option value={kind}>{materialName(kind)} ({facts()?.agentMaterials[kind]})</option>}</For></select></label>
      <label>{tr("补料数量", "Supply amount")}<input type="number" min="1" step="1" value={amount()} onInput={(event) => { setAmount(event.currentTarget.value); setDraft(null); }} /></label>
      <button type="button" disabled={disabled() || !transferKind() || !facts()?.canTransfer} onClick={transferDraft}>{tr("核对补料操作", "Review material transfer")}</button>
      <Show when={!facts()?.canTransfer}><p>{tr("当前地点或场地权限未匹配；不能按本地补料提交。请刷新并核对世界位置事实。", "Current location or site authority does not match. Local transfer is unavailable; refresh the world location facts.")}</p></Show>
     </Show>
     <Show when={facts()?.first}>
      <p role="status">{tr("首批产品已由世界结算", "First product settled by the world")}: {readable(facts()?.first.produce)} · {tr("副产物", "Byproducts")}: {readable(facts()?.first.byproducts)}<br />{tr("消耗", "Consumed")}: {readable(facts()?.first.consume)} · {facts()?.first.power_required} {tr("电力", "electricity")}</p>
      <h4>{tr("现在由你决定", "Now choose for yourself")}</h4>
      <p>{tr("查看剩余投入、电力和授权，再决定是否继续。首批回执不代表你已掌握全部基础。", "Inspect remaining inputs, electricity and authority, then decide whether to continue. The first receipt does not certify that you have learned every basic concept.")}</p>
      <label>{tr("下一步", "Next choice")}<select value={choice()} onChange={(event) => { setChoice(event.currentTarget.value); setPreview(null); setConsent(false); }}><option value="">—</option><option value="produce">{tr("我选择再生产一批", "I choose one more batch")}</option><option value="inspect">{tr("先看约束，暂不生产", "Inspect constraints before producing")}</option></select></label>
      <Show when={choice() === "inspect"}><p>{tr("核对上方两处材料账本、电力和权限余量。原件待确认时只查回执；材料不足时自行决定补哪种、多少。", "Inspect both material ledgers, electricity and authority remaining. Look up unresolved originals. If materials are short, choose which inputs and amounts to supply.")}</p></Show>
     </Show>
     <Show when={!facts()?.first || choice() === "produce"}>
      <button type="button" disabled={disabled()} onClick={() => void act(async () => {
       if (!facts().hasGrant(facts().claim.factory_id, "ScheduleRecipe", 8)) grant(facts().claim.factory_id, "ScheduleRecipe", 8);
       else await quote("schedule");
      })}>{tr("核对单批权限与完整报价", "Review one-batch authority and full quote")}</button>
     </Show>
    </Show>
   </Show>
   <Show when={draft()}>
    <h4>{tr("确认这次玩家操作", "Confirm this player operation")}</h4>
    <Show when={draft()?.type === "grant"} fallback={<p>{draft()?.value.selection.action.type === "TransferMaterial"
     ? `${tr("送入工厂", "Supply factory")}: ${materialName(draft()?.value.selection.action.data.kind)} × ${draft()?.value.selection.action.data.amount}`
     : tr("领取当前选择的公共起步材料、电力和场地。", "Claim the selected public starter materials, electricity and site.")}</p>}>
     <Show when={draft()?.value.operation.operation === "install"} fallback={<p>{tr("旧权限额度不足或已过期。先明确撤销该授权；如仍决定继续，再单独确认新的范围和额度。", "The previous allowance is insufficient or expired. Explicitly revoke it first; if you still choose to continue, separately confirm a new scope and allowance.")}</p>}>
      <p>{({ WorldRule: tr("领取所选起步包", "Claim the selected starter"), BuildFactory: tr("建造这座冶炼厂", "Build this smelter"), ScheduleRecipe: tr("操作这座冶炼厂的生产", "Operate this smelter"), TransferMaterial: tr("把所选材料送入工厂", "Supply the selected material to the factory") })[draft()?.value.operation.action_kinds?.[0]]}<br />{tr("授权电力额度", "Authorized electricity allowance")}: {draft()?.value.operation.limit_units}</p>
     </Show>
     <p>{tr("这是世界业务授权，和浏览器执行签名身份不同。原生授权限制对象、动作、电力及期限；首批的精确配方、单批和一次提交由本浏览器许可进一步限制。", "This is world action authority, separate from the browser executor signer. Native grants limit object, action, electricity and validity; this browser further limits the first recipe, batch and single original submission.")}</p>
    </Show>
    <label><input type="checkbox" checked={consent()} onChange={(event) => setConsent(event.currentTarget.checked)} />{tr("我确认上述具体对象、范围和操作", "I confirm this exact object, scope and action")}</label>
    <button type="button" disabled={disabled() || !consent()} onClick={() => void executeDraft()}>{tr("提交这次玩家操作", "Submit this player operation")}</button>
   </Show>
   <Show when={preview()}>
    <h4>{tr("正式报价", "Authenticated quote")}</h4>
    <p>{preview()?.selection.kind === "first_production" ? tr("这座冶炼厂：铁锭 1 批", "This smelter: one iron-ingot batch") : tr("建造这座冶炼厂", "Build this smelter")}<br />{tr("完整投入上限", "Full input limit")}: {readable(preview()?.selection.maxConsume)} · {tr("电力上限", "Electricity limit")}: {preview()?.selection.maxElectricity}</p>
    <Show when={preview()?.selection.kind === "first_production"}><p>{tr("本次仅铁锭配方 1 批。原件一旦保存就消耗这次许可；等待、未知或拒绝都不会改造另一批。世界回执确认后继续看产品结算。", "This permits one iron-ingot batch. Saving the original spends this permission. Pending, unknown or rejected results never create another batch. Check product settlement after the request receipt.")}</p></Show>
    <label><input type="checkbox" checked={consent()} onChange={(event) => setConsent(event.currentTarget.checked)} />{preview()?.selection.kind === "first_production" && !facts()?.first ? tr("我确认这份报价及当前任务；模型代办会在我的供应商账号计费", "I confirm this quote and current task; model delegation charges my provider account") : tr("我确认这份报价和这次玩家操作", "I confirm this quote and this player action")}</label>
    <Show when={preview()?.selection.kind === "first_production" && !facts()?.first} fallback={<button type="button" disabled={disabled() || !consent()} onClick={() => void executePreview(false)}>{tr("由我提交这次操作", "Submit this action myself")}</button>}>
     <button type="button" disabled={disabled() || !consent() || !modelReady()} onClick={() => void executePreview(true)}>{tr("让 Agent 只代办这一批", "Let the Agent handle only this batch")}</button>
     <p>{tr("先在下方配置并检查模型连接。真正调用模型后才记作 Agent 代办。", "Configure and check the model connection below. Agent delegation is recorded only after a real model invocation.")}</p>
    </Show>
   </Show>
   <Show when={state()?.submissionState}><p role="status">{tr("请求结果", "Request result")}: {({ committed: tr("已确认", "Confirmed"), rejected: tr("已拒绝", "Rejected"), expired: tr("已过期", "Expired"), pending: tr("待确认", "Pending"), unknown: tr("未知，需查询原件", "Unknown; look up the original") })[state()?.submissionState] || tr("待核对", "Unverified")} · {state()?.agentDelegated ? tr("已调用模型的 Agent 代办", "Agent work after model invocation") : tr("玩家直接操作", "Direct player action")}<br />{tr("请求确认后，继续以上方世界事实核对建成或产出。", "After request confirmation, check the world facts above for construction or product settlement.")}</p></Show>
   <Show when={state()?.modelNote}><p>{tr("Agent 的判断（建议，结果以世界事实为准）", "Agent assessment (advice; world facts determine the result)")}: {state()?.modelNote}</p></Show>
   <details><summary>{tr("查看身份、授权与原件凭据", "Inspect identity, authority and original evidence")}</summary><p>tick {facts()?.tick} · Agent {agentId()} · Task {fence()?.task_id}<br />{facts()?.claim?.offer_id || offerId()} · {facts()?.claim?.site_id || slot()?.site_id} · {facts()?.claim?.factory_id || slot()?.factory_id}<br />{facts()?.factory?.input_ledger} · job {facts()?.first?.job_id ?? "—"}<br />{state()?.originalOperationId || "—"}</p><pre style="white-space:pre-wrap;overflow-wrap:anywhere">{JSON.stringify(draft()?.value.operation || draft()?.value.selection.action || null, null, 2)}</pre></details>
  </Show>
  <Show when={error()}><p role="alert">{error()} · {tr("先核对当前事实和原件。期限或范围变化后需重新明确确认。", "Check current facts and original results. Changed validity or scope requires a new explicit confirmation.")}</p></Show>
 </section>;
}
