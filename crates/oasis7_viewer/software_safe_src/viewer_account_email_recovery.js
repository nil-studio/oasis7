// Optional email and recovery use the v2 authority exclusively. OTPs and pending
// browser ceremony options live only in this surface's memory.
export function renderAccountEmailRecovery(root, { state, account, credentials, capabilities, client, run, render }) {
  function element(tag, text) { const node = document.createElement(tag); if (text) node.textContent = text; return node; }
  function button(text, action) { const node = element("button", text); node.type = "button";
    node.addEventListener("click", () => run(action)); root.append(node); }
  function field(label, name, type = "text") {
    const wrapper = element("label", label); const input = element("input"); input.type = type;
    input.value = state[name] || ""; input.autocomplete = type === "email" ? "email" : name === "code" ? "one-time-code" : "off";
    if (name === "code") { input.inputMode = "numeric"; input.maxLength = 6; }
    input.addEventListener("input", () => { state[name] = input.value; }); wrapper.append(input); root.append(wrapper);
  }
  function mode(value) { state.mode = value; state.code = ""; render(); }
  if (!account && capabilities.email?.available) button("使用邮箱登录", () => mode("login_address"));
  if (account && capabilities.email?.available) button("绑定或替换恢复邮箱", () => mode("binding_address"));
  if (account && !credentials.some((key) => key.state === "active" && ["passkey", "native_key"].includes(key.type))) {
    button("验证邮箱并添加首个通行密钥", async () => { state.otp = await client.upgradeEmailOptions(); mode("upgrade_code"); });
  }
  if (capabilities.email_recovery?.available) {
    button(account ? "查看恢复申请" : "丢失通行密钥？通过已绑定邮箱恢复", () => mode(account ? "recovery_lookup" : "recovery_address"));
  }
  if (["login_address", "binding_address", "recovery_address"].includes(state.mode)) {
    field("邮箱", "email", "email");
    if (state.mode === "recovery_address") {
      root.append(element("p", "只能使用此前绑定并满足等待条件的邮箱。恢复不会立即解除已有通行密钥保护。"));
      field("已有恢复申请编号（可选）", "recoveryId");
    }
    button("发送本次操作验证码", async () => {
      const flow = state.mode;
      state.otp = flow === "login_address" ? await client.emailOptions(state.email)
        : flow === "binding_address" ? await client.emailBindingOptions(state.email)
          : await client.recoveryEmailOptions(state.email, state.recoveryId || undefined);
      mode(flow.replace("address", "code"));
    });
  }
  if (["login_code", "binding_code", "recovery_code", "upgrade_code", "completion_code"].includes(state.mode)) {
    root.append(element("p", "验证码仅用于本次指定操作，两分钟内有效。")); field("验证码", "code");
    button(state.mode === "completion_code" ? "验证邮箱与候选通行密钥，完成恢复" : "确认本次操作", async () => {
      const code = state.code; state.code = "";
      if (state.mode === "login_code") await client.emailComplete(state.otp, code);
      else if (state.mode === "binding_code") await client.emailBindingComplete(state.otp, code);
      else if (state.mode === "upgrade_code") await client.upgradeEmailComplete(state.otp, code);
      else if (state.mode === "completion_code") await client.recoveryComplete(state.recovery.recovery_id, state.otp, code);
      else {
        await client.recoveryEligibility(state.otp, code, state.recoveryId || undefined);
        state.otp = null;
        if (state.recoveryId) { state.recovery = await client.recoveryStatus(state.recoveryId); mode("recovery_status"); }
        else mode("recovery_candidate");
        return;
      }
      state.otp = null; mode(null);
    });
  }
  if (state.mode === "recovery_candidate") {
    root.append(element("p", "创建一个固定的候选通行密钥。它在恢复完成前不能登录原账号；完成时还需再次验证邮箱和此候选密钥。"));
    button("创建候选通行密钥并申请恢复", async () => {
      state.recovery = await client.createRecovery(); state.recoveryId = state.recovery.recovery_id; mode("recovery_status");
      state.recovery = await client.recoveryStatus(state.recoveryId);
    });
  }
  if (state.mode === "recovery_lookup") {
    field("恢复申请编号", "recoveryId");
    button("查看此申请", async () => { state.recovery = await client.recoveryStatus(state.recoveryId); mode("recovery_status"); });
  }
  if (state.mode === "recovery_status" && state.recovery) {
    const recovery = state.recovery;
    const stateLabel = { waiting: "等待保护期", approved: "原通行密钥已批准", cancelled: "已取消", completed: "已完成" };
    root.append(element("p", `恢复申请：${recovery.recovery_id}`));
    root.append(element("p", `状态：${stateLabel[recovery.state] || "状态待核对"}`));
    if (recovery.not_before) root.append(element("p", `最早可完成时间：${new Date(recovery.not_before).toLocaleString()}`));
    if (recovery.expires_at) root.append(element("p", `申请到期时间：${new Date(recovery.expires_at).toLocaleString()}`));
    if (recovery.candidate_digest) root.append(element("p", `固定候选：${recovery.candidate_digest}`));
    button("刷新申请状态", async () => { state.recovery = await client.recoveryStatus(recovery.recovery_id); });
    if (["waiting", "approved"].includes(recovery.state)) {
      button("取消此固定候选的恢复申请", async () => {
        await client.recoveryDecision(recovery, "cancel", Boolean(account)); state.recovery = await client.recoveryStatus(recovery.recovery_id);
      });
      if (account) button("用原通行密钥批准此固定候选", async () => {
        await client.recoveryDecision(recovery, "approve", true); state.recovery = await client.recoveryStatus(recovery.recovery_id);
      });
      if (!account) button("请求最终验证码并验证候选通行密钥", async () => {
        state.otp = await client.recoveryCompletionOptions(recovery.recovery_id); mode("completion_code");
      });
    }
  }
  if (state.mode) button("关闭此流程", () => { state.otp = null; mode(null); });
}
