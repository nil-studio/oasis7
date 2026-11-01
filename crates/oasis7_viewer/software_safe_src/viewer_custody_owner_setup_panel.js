import { createOwnerSetupClient, custodyCapability } from "./viewer_custody_owner_setup_client.js";
const notices = {
  custody_session_changed: "账号已经变化，原页面的迟到结果已丢弃；已发操作须由原账号核清。",
  custody_outcome_unknown: "结果未知。请核对或显式重送同一原件，不要创建新的批准。",
  custody_authenticator_cancelled: "已停止本地通行密钥交互；服务端挑战没有被撤销，请等待原期限或由原账号核清。",
  custody_cancelled_not_expired: "原挑战尚未到期，不能丢弃；请等待已显示的挑战期限后再操作。",
  custody_clock_invalid: "当前浏览器时间不可用于核对期限；原件仍保留，没有发送新请求。",
  custody_original_pending: "本页仍保留原操作，不能创建另一份批准。",
  custody_request_rejected: "请求被拒绝。请核对当前账号、范围和期限，不自动重做强认证。",
};
export function createOwnerSetupPanel({ client, browserPasskeyOptions, serializePasskey } = {}) {
  client ||= createOwnerSetupClient({ browserPasskeyOptions, serializePasskey });
  let root = null, active = false, busy = false, page = null, notice = "", goal = "", accountKey = null, method = null;
  let uiGeneration = 0, cancelBudgetConfirmation = null;
  const el = (tag, text) => { const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (["p", "dd"].includes(tag)) node.style.overflowWrap = "anywhere"; return node; };
  function clear() { cancelBudgetConfirmation?.(); cancelBudgetConfirmation = null; uiGeneration += 1; client.clear(); page = null; goal = ""; notice = ""; busy = false; active = false; accountKey = null; }
  function update(account, authentication, capability) {
    const key = account?.account_id || null;
    if (key !== accountKey || authentication?.method !== method) { clear(); accountKey = key; }
    method = authentication?.method || null;
    active = client.setSession(account, authentication, capability);
    if (!custodyCapability(capability)) { active = false; page = null; }
  }
  function action(label, work, disabled = false) {
    const button = el("button", label); button.type = "button"; button.disabled = busy || disabled;
    button.addEventListener("click", async () => {
      if (busy || !active) return;
      const revision = uiGeneration; busy = true; notice = ""; render();
      try { await work(); }
      catch (error) { if (revision === uiGeneration) notice = notices[error.code] || "未完成该阶段。请核对当前账号与已冻结原件，不会自动批准或继续。"; }
      finally { if (revision === uiGeneration) { busy = false; render(); } }
    }); return button;
  }
  function detail(list, name, value) { list.append(el("dt", name), el("dd", String(value))); }
  function confirmModelBudget(summary) {
    if (!root || !active || cancelBudgetConfirmation) return Promise.reject(Object.assign(new Error("custody_session_changed"), {code:"custody_session_changed"}));
    return new Promise((resolve, reject) => {
      const revision = uiGeneration, box = el("section"), list = el("dl");
      box.setAttribute("role", "dialog"); box.setAttribute("aria-modal", "true");
      box.append(el("h4", "确认受限推理预算"));
      box.append(el("p", "此批准只约束同一原件与预算，不启动推理；仍须核验当前权限与完整费用图。供应商未知结果仍占最大额，不自动重试。"));
      const b = summary.budget, p = b?.price_policy;
      const proxy = summary.contract?.contract === "full_test_session_proxy_v2" ? summary.contract : null;
      const nativeSummary = proxy?.full_session ?? summary.contract;
      if (summary.scope) {
        box.querySelector("h4").textContent = "确认完整 TestSession V2 范围与永久关闭";
        for (const [label, value] of Object.entries({
          "托管账号": summary.parent.custody_account_id, "Session": summary.scope.session_id,
          "获批动作": JSON.stringify(summary.plan.actions), "允许资源引用": JSON.stringify(summary.plan.allowed_resource_refs),
          "业务上限": summary.plan.business_limit, "写 / 读 / 清理截止毫秒": `${summary.plan.write_deadline} / ${summary.plan.read_until} / ${summary.plan.cleanup_until}`,
          "永久关闭合同": JSON.stringify(summary.plan.termination), "获批完整范围": JSON.stringify(summary.scope),
          "模型预算": b ? "按下列固定整会话预算批准" : "无模型推理权", "Native完整摘要": nativeSummary.summary_digest,
        })) detail(list, label, value);
      }
      if (b)
      for (const [label, value] of Object.entries({
        "托管账号":summary.parent.custody_account_id, "目标世界":(summary.plan.world_identity?.world_id ?? summary.scope?.world_identity?.world_id),
        "Setup":summary.plan.setup_id, "私有目标":(summary.plan.goal_content?.goal_text ?? "不在本阶段提交私有目标"),
        "资源选择":JSON.stringify(summary.plan.selection),
        "Native账号":b.native_account_id, "Native公钥":b.native_public_key, "Native实例公钥":b.native_instance_public_key,
        "固定供应商配置 / 修订 / 模型":`${b.preset_id} / ${b.preset_revision} / ${b.model}`,
        "整会话调用上限（包含两次真实探测）":b.max_calls,
        "每次输入 / 输出 token 上限":`${b.max_input_tokens} / ${b.max_output_tokens}`,
        "整会话 token 上限 / 并发":`${b.max_total_tokens} / ${b.max_concurrent}`,
        "两次探测物理 HTTP 上界":b.probe_physical_http.join(" / "),
        "每次推理物理 HTTP 上界（含授权与预付）":b.inference_physical_http,
        "声明的固定调用图摘要":b.physical_graph_digest,
        "本地政策货币 / 最大微单位总额":`${p.currency} / ${p.max_total_micro_units}`,
        "每百万输入 / 输出 token 微单位费率":`${p.input_micro_units_per_million} / ${p.output_micro_units_per_million}`,
        "价格快照摘要 / 截止毫秒":`${p.quote_digest} / ${p.valid_until}`,
        "推理绝对截止毫秒":b.deadline, "批准原件截止毫秒":summary.plan.deadline ?? summary.plan.write_deadline,
        "Skill / 工具 / 上下文策略摘要":`${b.skill_digest} / ${b.tools_digest} / ${b.context_policy_digest}`,
        "Native摘要合同":`${nativeSummary.api_version} / ${nativeSummary.summary_digest}`,
      })) detail(list, label, value);
      if (proxy) {
        const basis = proxy.basis;
        for (const [label, value] of Object.entries({
          "本次 Native 注册摘要": proxy.basis_digest,
          "本次连接引用 / 代际": `${basis.connection_ref_digest} / ${basis.connection_generation}`,
          "Native 注册有效截止毫秒": basis.expires_at,
          "Native 价格政策承诺": basis.price_policy_digest,
        })) detail(list, label, value);
        box.append(el("p", "所示金额是本地政策预算，不能视为供应商保证账单；缺少合格价格或完整调用图时不会启动推理。"));
      }
      box.append(list);
      let settled = false;
      const settle = accepted => { if (settled) return; settled = true; cancelBudgetConfirmation = null; box.remove();
        if (!accepted || revision !== uiGeneration || !active) reject(Object.assign(new Error("custody_budget_confirmation_cancelled"), {code:"custody_budget_confirmation_cancelled"}));
        else resolve(); };
      cancelBudgetConfirmation = () => settle(false);
      const cancel = el("button", "取消本次预算确认"); cancel.type = "button"; cancel.addEventListener("click", () => settle(false));
      const confirm = el("button", "已核对预算，继续通行密钥"); confirm.type = "button"; confirm.addEventListener("click", () => settle(true));
      box.append(cancel, confirm); root.append(box); cancel.focus();
    });
  }
  function renderSummary(summary) {
    const { plan, prepared } = summary;
    const session = summary.kind === "session_v2";
    root.append(el("h4", `确认同一份 ${session ? "TestSession" : "OwnerSetup"} V${plan.api_version} 原件`));
    root.append(el("p", plan.api_version === 1 ? "仅准备与批准：资源引用为空，仅支持 Move business 0；本页不启动任务、不继续执行、不写世界。" : "仅核对并批准同一份 V2 原件；目录和摘要不授资源资格，本页不继续执行，不以批准完成宣称模型或世界执行已开放。"));
    const list = el("dl"); list.className = "custody-approval-summary";
    for (const [name, value] of Object.entries(session ? { "托管账号": summary.parent.custody_account_id, "Session": summary.sessionScope.session_id, "动作": JSON.stringify(plan.actions), "资源约束": JSON.stringify(plan.allowed_resource_refs), "永久关闭": JSON.stringify(plan.termination), "范围": JSON.stringify(summary.sessionScope) } : { "托管账号": summary.parent.custody_account_id, "Provision": plan.provision_id,
      "Setup 操作": plan.setup_id, "私有目标": plan.goal_content.goal_text, "Owner key": plan.owner_key_ref, "Executor key": plan.executor_key_ref,
      "请求 / 传输 / 写入上限": `${plan.request_limit} / ${plan.transport_limit} / ${plan.write_limit}`,
      "截止时间": plan.api_version === 2 ? `Unix 毫秒 ${plan.deadline}` : new Date(Number(plan.deadline)).toISOString(), "参与者回执摘要": plan.participant_receipt_digest,
      "Bootstrap 版本": plan.bootstrap_config_version, "Create ID": plan.create_operation_id, "Prepare ID": plan.prepare_operation_id,
      "Publish ID": plan.publish_operation_id, "Delegation ID": plan.delegation_operation_id, "Bind ID": plan.bind_operation_id,
      "Executor ID": plan.executor_id, "Executor 公钥": plan.executor_public_key, "Delegation generation / nonce": `${plan.delegation_generation} / ${plan.delegation_nonce}` })) detail(list, name, value);
    if (plan.api_version === 2) detail(list, "获批资源选择", JSON.stringify(plan.selection));
    for (const [name, value] of Object.entries(plan.world_identity ?? {})) detail(list, `World ${name}`, value);
    if (prepared) {
      for (const [name, value] of Object.entries(prepared.context)) detail(list, `批准 ${name}`, value);
      detail(list, "Rust intent kind", prepared.intent.kind);
      if (plan.api_version === 1) detail(list, "Rust plan digest", prepared.intent.setup_plan_digest);
      detail(list, "批准请求 ID", prepared.approval_request_id);
    }
    root.append(list);
    if (summary.state === "prepared") root.append(action("确认摘要并使用通行密钥批准", async () => {
      const result = await client.approve({ confirmModelBudget, confirmSession: confirmModelBudget }); notice = `已保留批准。当前阶段：${result.stage}。本页未继续执行。`;
    }));
    if (summary.completeUnknown || summary.approveUnknown || summary.state === "prepare_unknown") {
      root.append(el("p", "原请求仍在本页内存中；重送复用确切正文与 IDs，不重生 assertion 或 challenge。"));
      root.append(action("显式重送同一原件", async () => {
        const result = await client.retry(); notice = result?.execution_started === false ? `原批准已确认：${result.stage}；未继续执行。` : "原准备响应已核对，请确认摘要。";
      }));
    }
    if (summary.approveUnknown) root.append(action("核对原 Provision 当前状态", async () => {
      const result = await client.progress(); notice = `Provision 当前阶段：${result.stage}。此投影不能单独确认原 OwnerSetup 批准，原件继续保留。`;
    }));
    if (summary.cancelled) {
      root.append(el("p", notices.custody_authenticator_cancelled));
      root.append(el("p", "仅在原挑战到期且从未提交完整批准时，才能显式丢弃本页原件；另行准备必须重新完成通行密钥批准。"));
      root.append(el("p", `原挑战截止时间（Unix 毫秒）：${summary.ceremonyExpiresAt}`));
      root.append(action("挑战到期后丢弃未提交原件", async () => {
        await client.discardCancelled();
        notice = "已丢弃本页未提交原件；没有撤销服务端挑战或发送新请求。请选择父项另行准备，并重新完成通行密钥批准。";
      }));
    }
    if (summary.state === "approved") root.append(el("p", "OwnerSetup 已批准；Execution 仍关闭，本页没有 continue 操作。"));
  }
  function render() {
    if (!root) return;
    // Replacing a pending confirmation must settle it before the DOM disappears.
    cancelBudgetConfirmation?.();
    root.replaceChildren(); root.setAttribute("aria-busy", String(busy)); root.className = "stack custody-owner-setup";
    root.append(el("h3", "受控 OwnerSetup 准备与批准"));
    const status = el("p", notice || "目录只是本人公共关联，不代表可批准资格；选择后仍会核对实际权限与当前来源。");
    status.setAttribute("role", "status"); root.append(status);
    if (!active) {
      root.append(el("p", method && method !== "webauthn" ? "请使用通行密钥重新登录后操作；当前登录方式不能代签 custody 批准。" : "当前环境未配置可用的 custody 管理入口。")); return;
    }
    const summary = client.summary();
    if (summary) { renderSummary(summary); return; }
    root.append(action("加载本人可见父项", async () => { page = await client.parents(); }));
    if (!page) return;
    if (page.frozen || !page.inventory_complete) { root.append(el("p", "目录已冻结或库存完整性待核清，不能据此创建批准。")); return; }
    if (!page.items.length) root.append(el("p", "当前目录没有可显示的父项；没有创建任何业务目录。"));
    const label = el("label", "本次私有目标（浏览器端仅以内存保存；目标和批准原件将发送所选托管服务端）");
    const input = el("textarea"); input.rows = 3; input.style.maxWidth = "100%"; input.maxLength = 3500; input.value = goal; input.disabled = busy;
    input.addEventListener("input", () => { goal = input.value; }); label.append(input); root.append(label);
    for (const parent of page.items) {
      const card = el("div"); card.className = "stack";
      card.append(el("p", `账号 ${parent.custody_account_id} · Provision ${parent.provision_id} · ${parent.environment} / ${parent.world_id}`));
      card.append(action("为此父项准备批准摘要", async () => { await client.prepare(parent, goal); })); root.append(card);
    }
    if (page.next_after_custody_account_id) root.append(action("显式加载下一页", async () => {
      page = await client.parents(page.next_after_custody_account_id, page.directory_revision);
    }));
  }
  return { update, clear, async prepareV2(parent, original) {
    if (!root || !active || busy) throw Object.assign(new Error("custody_not_ready"), {code:"custody_not_ready"});
    const revision = uiGeneration; busy = true; render();
    try { await client.prepareV2(parent, original); } finally { if (revision === uiGeneration) { busy = false; render(); } }
  }, async prepareSessionV2(parent, original) {
    if (!root || !active || busy) throw Object.assign(new Error("custody_not_ready"), {code:"custody_not_ready"});
    const revision = uiGeneration; busy = true; render();
    try { await client.prepareSessionV2(parent, original); } finally { if (revision === uiGeneration) { busy = false; render(); } }
  }, renderInto(parent) { root = el("section"); parent.append(root); render(); }, dispose() { clear(); root = null; } };
}
