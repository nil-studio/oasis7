/* UI/UE design fixture only. No network, credentials, persistence or game actions. */
"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const escape = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  // Design-image camera only: integer CSS scale, shared anchors, no world mutation.
  function layoutPixelWorld() {
    const art = $("world-art");
    const width = art.clientWidth, height = art.clientHeight;
    if (art.hidden || !width || !height) return;
    const scale = Math.max(1, Math.ceil(Math.max(width / 320, height / 180)));
    const ox = Math.min(0, Math.max(width - 320 * scale, Math.floor(width / 2 - 168 * scale)));
    const oy = Math.min(0, Math.max(height - 180 * scale, Math.floor(height / 2 - 88 * scale)));
    const background = $("world-background");
    Object.assign(background.style, {left:`${ox}px`, top:`${oy}px`, width:`${320 * scale}px`, height:`${180 * scale}px`});
    art.dataset.pixelScale = String(scale);
    art.querySelectorAll("[data-world-x]").forEach((element) => {
      const x = ox + Number(element.dataset.worldX) * scale;
      const y = oy + Number(element.dataset.worldY) * scale;
      if (element.classList.contains("actor-sprite")) {
        Object.assign(element.style, {left:`${x - 8 * scale}px`, top:`${y - 15 * scale}px`, width:`${16 * scale}px`, height:`${16 * scale}px`});
      } else {
        element.style.left = `${x}px`;
        element.style.top = `${y + (element.classList.contains("place-name") ? -12 : 8)}px`;
      }
    });
  }
  const defaults = { system: "system-A", short: "short-A", long: "long-A" };
  const fields = { system: "系统 Prompt", short: "短期 Prompt", long: "长期 Prompt" };
  const fixtures = {
    entry: [["first", "首次进入"], ["return", "回访 · 历史结果"], ["identities", "观察 B / 草稿 C / 原请求 A"], ["dense", "同位置密集目标"], ["position", "位置未知"], ["renderer", "世界画面不可用"], ["entry-failed", "账号入口失败"]],
    prompt: [["noop", "空覆盖 · 清除不改变生效值"], ["override", "已有覆盖 · 清除后恢复默认"], ["indirect", "完整 profile · 间接影响"], ["stale", "草稿过期 · 保留与比较"], ["stale-read-failed", "过期后重读仍失败"], ["unknown", "原 Apply 结果未知"], ["revoked", "控制权失效 · 隐藏私有值"]],
    industry: [["blocked", "燃料不足 · 先解决阻碍"], ["ready", "报价可用 · 完整生产方案"], ["unknown", "提交结果未知"], ["query-failed", "原请求查询失败"], ["rejected", "原请求明确拒绝"], ["settled", "成果 · 摘要未发布副产物"], ["full-receipt", "成果 · 完整原件已发布"], ["return", "回访 · 历史成果"]]
  };
  let state;
  let invoker = "nav-command";
  let confirmAction = null;
  let confirmInvoker = null;

  function fresh(route, fixture) {
    const s = {
      route, fixture, entered: route !== "entry" || fixture !== "first", panel: route === "entry" ? "world" : "command",
      observed: "A", commandAgent: "A", renderer: true, control: true, notice: "", receiptKind: null, promptReceipts: [],
      chatDraft: "", goal: "", activity: "正在观察北侧加工站", chatStatus: "none", sentText: "", sentAgent: null,
      promptOpen: route === "prompt", prompt: { profile: {system:null, short:null, long:null}, draft: {system:null, short:null, long:null}, effective: {...defaults, short:"short-B"}, version:7, draftVersion:7, status:"idle", preview:false, compare:false, lookup:false, preset:null, dirty:false },
      industry: {status:"idle", fuel:2, observedFuel:2, quote:false, queryFailed:false, fullReceipt:false, history:false, readback:"settled"}
    };
    if (route === "entry") {
      if (fixture === "first" || fixture === "entry-failed") { s.panel = "account"; s.entered = false; }
      if (fixture === "return") { s.goal = "恢复北侧补给"; s.chatStatus = "confirmed"; s.sentAgent = "A"; s.sentText = s.goal; s.receiptKind = "chat"; }
      if (fixture === "identities") { s.observed = "B"; s.commandAgent = "C"; s.sentAgent = "A"; s.sentText = "检查燃料缺口"; s.chatStatus = "unknown"; s.receiptKind = "chat"; s.panel = "command"; }
      if (fixture === "dense" || fixture === "position") { s.panel = "targets"; s.observed = fixture === "position" ? "未知目标" : "B"; }
      if (fixture === "renderer") { s.renderer = false; s.panel = "targets"; }
    }
    if (route === "prompt") {
      if (fixture === "override") { s.prompt.profile.short = "short-C"; s.prompt.draft.short = "short-C"; s.prompt.effective.short = "short-C"; }
      if (fixture === "indirect") { s.prompt.draft.system = "system-D"; s.prompt.dirty = true; }
      if (fixture === "stale") { s.prompt.profile.short = "short-C2"; s.prompt.effective.short = "short-C2"; s.prompt.version = 8; s.prompt.draft.short = "short-D"; s.prompt.dirty = true; s.prompt.status = "stale"; }
      if (fixture === "stale-read-failed") { s.prompt.profile.short = "short-C"; s.prompt.effective.short = "short-C"; s.prompt.draft.short = "short-D"; s.prompt.dirty = true; s.prompt.status = "stale"; s.prompt.readFailed = true; }
      if (fixture === "unknown") { s.prompt.draft.system = "system-D"; s.prompt.dirty = true; s.prompt.status = "unknown"; s.prompt.requestId = "prompt-demo-08"; s.promptReceipts.push({id:"prompt-demo-08",agent:"A",status:"unknown"}); s.receiptKind = "prompt"; }
      if (fixture === "revoked") { s.control = false; s.prompt.draft.short = "未提交的本地示例草稿"; s.prompt.status = "revoked"; }
    }
    if (route === "industry") {
      s.industry.fuel = fixture === "blocked" ? 2 : 3; s.industry.observedFuel = s.industry.fuel; s.industry.quote = fixture !== "blocked";
      if (["unknown", "query-failed"].includes(fixture)) { s.industry.status = "unknown"; s.industry.queryFailed = fixture === "query-failed"; s.receiptKind = "industry"; }
      if (fixture === "query-failed") s.industry.readback = "unavailable";
      if (fixture === "rejected") { s.industry.status = "rejected"; s.receiptKind = "industry"; }
      if (["settled", "full-receipt", "return"].includes(fixture)) { s.industry.status = "settled"; s.industry.fullReceipt = fixture === "full-receipt"; s.industry.history = fixture === "return"; s.receiptKind = "industry"; }
    }
    return s;
  }

  const button = (action, label, primary = false, disabled = false) => `<button type="button" data-action="${action}" class="${primary ? "primary" : ""}" ${disabled ? "disabled" : ""}>${label}</button>`;
  const card = (title, body, kind = "") => `<section class="card ${kind}"><h3>${title}</h3>${body}</section>`;
  const fact = (label, value) => `<dt>${label}</dt><dd>${value}</dd>`;
  const none = (value) => value === null ? "未设置（None）" : escape(value);
  const profileSame = () => Object.keys(fields).every((key) => state.prompt.profile[key] === state.prompt.draft[key]);
  const candidate = () => Object.fromEntries(Object.keys(fields).map((key) => [key, state.prompt.draft[key] ?? defaults[key]]));
  const promptLocked = () => !state.control || ["stale", "unknown", "applied"].includes(state.prompt.status);
  const promptReceiptLabel = (status) => ({unknown:"原 Apply 结果未知",applied:"Profile 已更新 · 执行器待确认",noop:"已接受 · 没有变化",confirmed:"原 Prompt 已确认"}[status] || "原结果尚未取得");
  function recordPromptResult(status) {
    const id = `prompt-demo-${String(8 + state.promptReceipts.length).padStart(2,"0")}`;
    state.prompt.requestId = id;
    state.promptReceipts.push({id,agent:state.commandAgent,status});
    state.receiptKind = "prompt";
  }
  function promptImpact() {
    const p = state.prompt;
    if (promptLocked()) return "";
    return card("当前草稿的完整影响 · 只读", profileSame() ? "<p>候选 profile 与当前一致：accepted / no_change。不会改写当前生效值。</p>" : Object.keys(fields).map((key) => `<p>${fields[key]}：${escape(p.effective[key])} → ${escape(candidate()[key])}${p.draft[key] === p.profile[key] && p.effective[key] !== candidate()[key] ? "<br><span class='state-label'>完整 profile 重应用带来的间接变化</span>" : ""}</p>`).join(""), "soft");
  }

  function accountContent() {
    if (state.entered) return card("示例账号 A", "<p>已恢复本示例世界的会话。观察其他目标不会改变当前草稿和原请求归属。</p>") + `<div class="actions">${button("return-world", "回到世界", true)}${button("signout", "退出本地示例")}</div>`;
    return `<p class="muted">从世界中的一个目标开始，再给你的 Agent 一条高层指导。</p>` +
      (state.fixture === "entry-failed" ? card("入口暂不可用", "<p>当前会话没有建立。重试只恢复入口，不创建第二个账号。</p>", "error") : "") +
      `<div class="stack">${button("enter", "使用通行密钥进入示例", true)}${button("enter-return", "恢复已有示例会话")}</div><p class="meta">此设计原型不会打开系统通行密钥窗口。实际注册、登录与恢复沿用现有账号流程。</p>`;
  }

  function chatContent() {
    const busy = state.chatStatus === "unknown";
    return `<p class="meta">当前查看：${escape(state.observed)} · 草稿主体：${escape(state.commandAgent)}${state.sentAgent ? ` · 原请求主体：${escape(state.sentAgent)}` : ""}</p>` +
      card("当前目标", `<p>${state.goal ? escape(state.goal) : "尚未确认目标"}</p><p class="meta">${state.goal ? "来自获准的目标读回；完成情况另见世界结果。" : "发送对话后仍需请求确认与目标读回。"}</p>`) +
      card("当前活动", `<p>${escape(state.activity)}</p><p class="meta">最近一次可读观察；不是新指令的完成证明。</p>`, "soft") +
      `<label class="field" for="chat-draft"><span>给 Agent ${escape(state.commandAgent)} 的指导</span><textarea id="chat-draft" placeholder="例如：先检查燃料缺口，再规划第一批铁锭。" ${!state.control ? "disabled" : ""}>${escape(state.chatDraft)}</textarea></label><p class="meta">Enter / Shift+Enter 换行，使用按钮发送。中文输入法确认不会发送。</p>` +
      `<div class="actions">${button("chat-send", busy ? "原请求仍待确认" : "发送指导", true, !state.control || busy)}${button("chat-preset", "放入建议草稿", false, !state.control)}</div>` +
      (state.chatStatus === "rejected" ? card("这条指导没有发送", "<p>请输入内容。草稿仍在这里，目标与 Prompt 未改变。</p>", "error") : "") +
      (busy ? card("已收到，尚未确认目标", `<p>原请求主体 ${escape(state.sentAgent)} · chat-demo-01。下一步只查询这条请求，不重新发送。</p>${button("chat-query", "查询原指导结果")}`, "warn") : "") +
      (state.chatStatus === "confirmed" ? card("指导已确认", `<p>原请求的目标读回完成。当前活动仍需后续观察，不把确认当作生产完成。</p>`, "success") : "") +
      `<details id="advanced" ${state.promptOpen ? "open" : ""}><summary>高级 · Prompt 控制</summary><p class="meta">Prompt 调整不发送对话，也不是发送指导的前置条件。</p>${promptContent()}</details>`;
  }

  function promptContent() {
    const p = state.prompt;
    if (!state.control) return card("当前没有控制权限", "<p>默认值、profile、生效值、版本摘要和差异已隐藏。旧请求的迟到响应不会写入当前主体。</p>", "error") +
      `<label class="field" for="safe-draft"><span>保留的未提交本地文本</span><textarea id="safe-draft" readonly>${escape(p.draft.short ?? "")}</textarea></label><p class="meta">只保留允许留存的未提交文本；恢复权限后须重新读取并检查目标。</p>`;
    if (p.readFailed) return card("草稿已过期，重新读取仍失败", "<p>新 profile 尚未读到。先前记录不可冒充当前值；保留原 v7 草稿，等待读取恢复。</p>", "error") + `<label class="field" for="safe-draft"><span>保留的本地草稿 · 原 Agent A / v7</span><textarea id="safe-draft" readonly>${escape(p.draft.short)}</textarea></label><div class="actions">${button("prompt-read-again", "重试只读查询", true)}${button("return-world", "返回世界")}</div><p class="meta">本情境的查询持续不可用；重试结束后仍保留原草稿，不清除或应用。</p>`;
    const sourceValues = (values) => Object.keys(fields).map((key) => `${fields[key]}：${none(values[key])}`).join("<br>");
    let html = `<div class="source-grid" aria-label="Prompt 五种来源">
      <div class="source-row"><strong>1 · 初始化默认值</strong><p>${sourceValues(defaults)}</p></div>
      <div class="source-row"><strong>2 · Profile 覆盖 · v${p.version}</strong><p>${sourceValues(p.profile)}</p></div>
      <div class="source-row"><strong>3 · 当前生效值</strong><p>${sourceValues(p.effective)}</p><p class="muted">${p.effective.short === "short-B" ? "short-B 来自初始化路径；空覆盖的 no-op 不会重新应用默认值。" : "仅按匹配的执行器读回展示。"}</p></div>
      <div class="source-row"><strong>4 · 预设</strong><p>${p.preset ? "preset-P · 仅复制到草稿" : "尚未选择；不会隐式覆盖已存 profile"}</p>${button("prompt-preset", "载入预设到草稿", false, promptLocked())}</div>
      <div class="source-row"><strong>5 · 本地草稿 · 基于 v${p.draftVersion}</strong><p>修改或清除只改变下面的草稿，明确应用后才提交。</p></div>
    </div>`;
    for (const key of Object.keys(fields)) html += `<label class="field" for="prompt-${key}"><span>${fields[key]} · 草稿${p.draft[key] === null ? " / 无覆盖" : ""}</span><div class="inline"><textarea id="prompt-${key}" data-prompt-field="${key}" ${promptLocked() ? "readonly" : ""} placeholder="未设置覆盖">${escape(p.draft[key] ?? "")}</textarea>${button(`clear-${key}`, "清除覆盖", false, promptLocked())}</div></label>`;
    if (p.status === "stale") html += card("草稿已过期，编辑已保留", `<p>草稿基于 v${p.draftVersion}，当前 profile 为 v${p.version}。比较只读；重新编辑仍需明确应用。重新读取前会确认丢弃。</p><div class="actions">${button("prompt-compare", "比较差异")}${button("prompt-rebase", "基于新版本继续编辑")}${button("prompt-reload", "丢弃并重新读取")}</div>`, "warn");
    if (p.compare) html += card("只读比较", Object.keys(fields).map((key) => `<p><b>${fields[key]}</b><br>当前 profile：${none(p.profile[key])}<br>保留草稿：${none(p.draft[key])}</p>`).join(""), "soft");
    if (p.status === "unknown") html += card("原 Apply 结果未知", `<p>${escape(p.requestId)} 仍未取得终态。读取到最新 profile 不等于这条请求成功。</p>${button("prompt-latest", "读取当前 profile（只读）")}${p.lookup ? "<p>已读到 v8；原 Apply 仍为 unknown，没有重新提交。</p>" : ""}`, "warn");
    if (p.status === "applied") html += card("Profile 已更新，执行器待读回", `<p>已确认 profile v${p.version}。当前生效值保持原读面，尚未宣称全量应用完成。</p>${button("prompt-effective", "读取对应执行器结果")}`, "warn");
    if (p.status === "noop") html += card("已接受 · 没有变化", `<p>Profile 版本仍为 v${p.version}，没有 mutation 或 driver 更新。当前生效值保持原样。</p>`, "success");
    if (p.status === "confirmed") html += card("已应用并读回", "<p>当前生效值来自与本次 profile 更新相匹配的执行器结果。对话和当前目标未改变。</p>", "success");
    html += `<div id="prompt-impact" aria-live="polite">${promptImpact()}</div>`;
    html += `<div class="actions">${button("prompt-preview", "预览影响", false, promptLocked())}${button("prompt-apply", "明确应用 Prompt", true, promptLocked())}</div><p class="meta">预览不提交，也不新增权限凭据或必需 token。应用按当前权限、版本和完整 profile 校验。</p>`;
    return html;
  }

  function industryContent() {
    const p = state.industry;
    if (!state.control) return card("当前没有控制权限", "<p>原方案、成本、原结果和提交动作已隐藏。恢复会话后须重新读取。</p>", "error");
    if (p.status === "settled") return `<p class="meta">${p.history ? "回访 · 历史结果" : "原操作结果"} · 示例 Agent A / job 17 / tick 102</p>` +
      card("第一批铁锭已产出", `<p class="success-text">production_settled · production_only</p><dl class="fact-grid">${fact("消耗铁矿石", "12 iron_ore")}${fact("消耗碳燃料", "3 carbon_fuel")}${fact("产出铁锭", "9 iron_ingot")}${fact("电力需求", "24")}${fact("副产物", p.fullReceipt ? "3 slag" : "当前摘要未发布")}</dl><p class="meta">${p.fullReceipt ? "slag 来自同 job 17 的完整 RecipeCompletionReceiptV1。" : "当前 StarterIndustrialSettlementSummaryV1 无副产物字段，不能用计划值补作实际结果。"}</p>`, "success") +
      card("下一步候选 · Assembler", "<p>建造要求 10 iron_ingot；本笔产出 9，与要求相差 1。这不是当前库存余额。</p><p class='meta'>当前库存、其他材料、电力和授权仍待核验。候选不代表已解锁或可以建造；此轮只确认生产，没有交付或回款完成声明。</p>", "soft") +
      `<div class="actions">${button("industry-return", "回到世界", true)}${button("industry-original", "查看原结算依据")}</div>`;
    if (p.status === "rejected") return card("原生产请求已明确拒绝", "<p>action-demo-17 · Agent A / job 17</p><p>拒绝原因：提交时投入条件已失效。它与报价阶段的燃料不足、以及结果未知不同。</p><p>已耗材料与费用未发布，不能填零或承诺退款。没有自动重连或再次生产。</p>", "error") + `<div class="actions">${button("industry-return", "回到世界", true)}</div>`;
    const blocked = p.fuel < 3;
    return `<p class="meta">方案主体：示例 Agent A · Smelter MK1 · 3 批</p><p class="meta">工厂实例 factory-demo-01 · recipe.smelter.iron_ingot</p><p>${p.status === "unknown" ? "原结果仍未知。以下保留提交前的方案与供给读面，不代表当前库存或扣料结果。" : "先检查完整投入，再决定是否提交生产。"}</p>` +
      card(p.status === "unknown" ? "原提交方案 · 历史读面" : "完整生产方案", `<dl class="fact-grid">${fact("工厂输入账本 · 铁矿石", `需要 12 / ${p.status === "unknown" ? "提交前" : "可用"} 12 iron_ore`)}${fact("工厂输入账本 · 碳燃料", `需要 3 / ${p.status === "unknown" ? "提交前" : "可用"} ${p.fuel} carbon_fuel`)}${fact("预计主产物", "9 iron_ingot")}${fact("预计副产物", "3 slag")}${fact("配方电力需求", "24")}${fact("Catalog 基础时长", "1 tick / 不乘批次")}${fact(p.status === "unknown" ? "提交前报价时长" : "本次报价时长", p.quote ? "1 tick（示例报价值）" : "尚未取得")}${fact("费用", "未发布 / 未知")}</dl><p class="meta">每批 4 ore + 1 fuel → 3 ingot + 1 slag；电力 8。电力不附加未经定义的单位。基础时长不是完成承诺。</p>`) +
      (blocked ? card("先补足碳燃料", `<p>需要 3，当前只有 2。方案尚不能提交。查询只刷新当前可读供给与报价。</p>${button("industry-recheck", "重新检查供给与报价", true)}`, "error") : "") +
      (p.status === "unknown" ? card(p.queryFailed ? "查询暂时失败，原结果仍未知" : "已提交，等待原结果", `<p>action-demo-17 归属 Agent A / job 17。保留原请求，不自动重试生产。</p><div class="actions">${button("industry-query", "查询原生产结果", true)}</div><p class="meta">查询读取同一原请求；不是新报价或第二次提交。查询服务恢复后可再次只读查询。</p>`, "warn") : `<div class="actions">${button("industry-preview", "重新读取报价")}${button("industry-submit", "明确提交生产", true, blocked || !p.quote)}</div><p class="meta">报价与预览不创建回执；提交产生独立的原操作身份。</p>`);
  }

  function targetsContent() {
    const unknown = state.fixture === "position";
    return `<p class="muted">查看目标只改变观察对象，不会重新绑定草稿或原请求。</p>` +
      (state.fixture === "dense" ? card("同位置 · 3 个候选", "<p>共享同一已知位置，按名称和身份选择。不把标记随机散开。</p>", "soft") : "") +
      ["A","B","C"].map((id) => `<div class="target-row"><div><strong>示例 Agent ${id}</strong><p>${id === "B" ? "仅观察" : "本示例可指导"} · ${state.fixture === "dense" ? "共享位置：北侧加工站" : "位置：北侧补给区"}</p></div><button type="button" data-action="observe-${id}">查看 ${id}</button></div>`).join("") +
      `<div class="target-row"><div><strong>位置未知的目标</strong><p>没有获准坐标，不显示地图标记。</p></div>${button("observe-unknown", "文字查看")}</div>` +
      (unknown || state.observed === "未知目标" ? card("位置未知", "<p>当前只提供文字状态，没有定位、跟随或推测坐标操作。</p>", "warn") : card(`正在查看 ${escape(state.observed)}`, `<p>${state.observed === "B" ? "你可以观察该目标，不能向它提交指令。" : "选中目标不会自动改写既有草稿。"}</p>${state.observed !== "B" ? button("use-observed", "将该目标用于新草稿") : ""}`, "soft"));
  }

  function renderNext() {
    let title = "给 Agent 第一条指导", body = "提出高层建议，等待确认，再观察它的行动。", action = "open-command", label = "指导 Agent";
    if (!state.entered) { title = "从一个世界开始"; body = "进入或恢复会话，保留清晰的操作归属。"; action = "open-account"; label = "进入示例世界"; }
    else if (state.route === "prompt") { title = "检查 Prompt 的实际影响"; body = "区分默认值、覆盖、生效值和本地草稿。"; label = "打开高级 Prompt"; }
    else if (state.route === "industry") { title = state.industry.status === "settled" ? "第一批成果已可回看" : state.industry.status === "unknown" ? "核对原生产请求" : state.industry.status === "rejected" ? "先理解原请求的拒绝原因" : state.industry.fuel < 3 ? "先补足首批生产投入" : "核对后提交首批生产"; body = state.industry.status === "settled" ? "本笔 9 枚铁锭已确认；下一步仍需核对当前库存和条件。" : state.industry.status === "unknown" ? "原结果未知，保留请求身份并只读查询。" : "3 批完整配方，投入和阻碍一起可见。"; label = state.industry.status === "settled" ? "查看历史成果" : state.industry.status === "unknown" ? "查看原请求" : "查看生产方案"; }
    else if (state.goal) { title = state.goal; body = "目标已确认；当前活动与世界成果分别读取。"; label = "查看 Agent 状态"; }
    $("next-move").innerHTML = `<p class="eyebrow">下一步 · NEXT MOVE</p><h2 id="next-title">${escape(title)}</h2><p>${escape(body)}</p><button id="next-action" type="button" class="primary" data-action="${action}">${label} <span aria-hidden="true">↗</span></button>`;
  }

  function renderReceipt() {
    const r = $("receipt");
    r.hidden = !state.receiptKind || !state.entered;
    $("stage").classList.toggle("has-receipt", !r.hidden);
    if (r.hidden) return;
    let id, title, description, action;
    if (state.receiptKind === "chat") { id = `chat-demo-01 · Agent ${state.sentAgent}`; title = state.chatStatus === "confirmed" ? "原指导已确认" : "原指导 · 结果待确认"; description = "世界动态不代替这条请求的结果。"; action = state.chatStatus === "unknown" ? button("chat-query", "查询原指导结果") : button("open-command", "查看当前目标"); }
    if (state.receiptKind === "prompt") { const original = state.promptReceipts.at(-1); id = original ? `${original.id} · Agent ${original.agent}` : "尚无 Prompt 原请求"; title = !state.control ? "原请求信息暂不可读" : promptReceiptLabel(original?.status); description = "Profile 读取与原请求终态分别展示。"; action = button("prompt-history", `查看原请求（${state.promptReceipts.length}）`); }
    if (state.receiptKind === "industry") { id = "action-demo-17 · Agent A / job 17"; title = state.industry.status === "settled" ? "原生产已结算 · 9 iron_ingot" : state.industry.status === "rejected" ? "原生产已明确拒绝" : "原生产 · 结果未知"; description = state.industry.history ? "恢复历史摘要，没有发起新生产。" : "仅关联这条原操作，不被新报价覆盖。"; action = state.industry.status === "unknown" ? button("industry-query", "查询原生产结果") : button("open-command", "查看原结果"); }
    r.innerHTML = `<p class="receipt-id">${escape(id)}</p><strong>${escape(title)}</strong><p>${escape(description)}</p>${action}`;
    const receiptAction = r.querySelector("button"); if (receiptAction) receiptAction.id = "receipt-action";
  }

  function render(focusId) {
    const focused = document.activeElement;
    const oldId = focusId || focused?.id;
    const oldAction = focused?.dataset?.action;
    $("stage").classList.toggle("panel-open", state.panel !== "world");
    $("stage").classList.toggle("renderer-unavailable", !state.renderer);
    $("renderer-message").hidden = state.renderer;
    $("world-art").hidden = !state.renderer;
    const dense = state.fixture === "dense";
    document.querySelectorAll(".actor").forEach((marker) => {
      const id = marker.classList.contains("actor-a") ? "A" : marker.classList.contains("actor-b") ? "B" : "C";
      marker.hidden = dense && id !== "B";
      marker.textContent = dense ? "3" : id;
      marker.classList.toggle("actor-cluster", dense);
      document.querySelector(`[data-actor-sprite="${id}"]`).hidden = marker.hidden || dense;
      marker.style.outline = state.observed === id || dense ? "3px solid var(--gold)" : "none";
      marker.style.outlineOffset = "5px";
    });
    $("account-button").textContent = state.entered ? "示例账号 A · 已进入" : "示例账号 · 未进入";
    $("world-context").innerHTML = `<span class="badge">查看 ${escape(state.observed)}</span><span class="badge">草稿 ${escape(state.commandAgent)}</span>${state.sentAgent ? `<span class="badge">原请求 ${escape(state.sentAgent)}</span>` : ""}`;
    $("feed-text").textContent = state.renderer ? "世界动态 · 加工站等待补给" : "文字状态可用 · 世界动态不是原操作回执";
    $("notice").textContent = state.notice;
    renderNext(); renderReceipt();
    $("drawer").hidden = state.panel === "world";
    document.querySelectorAll("[data-panel]").forEach((el) => el.setAttribute("aria-pressed", String(el.dataset.panel === state.panel)));
    if (state.panel !== "world") {
      $("drawer-kicker").textContent = state.panel === "account" ? "ACCOUNT / ENTRY" : state.panel.toUpperCase();
      $("drawer-title").textContent = state.panel === "account" ? "进入与回访" : state.panel === "targets" ? "目标与位置" : state.panel === "prompt-history" ? "Prompt 原请求记录" : state.route === "industry" ? "首局生产" : state.route === "prompt" ? `高级 Prompt · Agent ${state.commandAgent}` : `指导 Agent ${state.commandAgent}`;
      $("drawer-content").innerHTML = state.panel === "account" ? accountContent() : state.panel === "targets" ? targetsContent() : state.panel === "prompt-history" ? state.promptReceipts.map((original) => card(`${escape(original.id)} · Agent ${escape(original.agent)}`, `<p>${promptReceiptLabel(original.status)}</p><p class="meta">新草稿、新主体或最新 profile 不裁决这条原请求。</p>`)).join("") : state.route === "industry" ? industryContent() : state.route === "prompt" ? promptContent() : chatContent();
    }
    layoutPixelWorld();
    if (oldId && $(oldId)) $(oldId).focus({preventScroll:true});
    else if (oldAction && state.panel !== "world") {
      const replacement = Array.from($("drawer").querySelectorAll("[data-action]")).find((el) => el.dataset.action === oldAction && !el.disabled);
      (replacement || $("drawer-title")).focus({preventScroll:true});
    }
  }

  function openPanel(panel) {
    invoker = document.activeElement?.id || `nav-${panel}`;
    state.panel = panel; state.notice = ""; render(panel === "world" ? "nav-world" : "drawer-title");
  }
  function closePanel() { if (state.route === "industry" && state.panel === "command" && state.industry.status === "settled") state.industry.history = true; state.panel = "world"; state.notice = ""; render(invoker); }
  function confirm(title, description, accept, callback) {
    confirmInvoker = document.activeElement;
    $("confirm-title").textContent = title; $("confirm-description").textContent = description; $("accept-confirm").textContent = accept;
    confirmAction = callback; $("confirm-dialog").showModal(); $("cancel-confirm").focus();
  }
  function changeDraft(key, value) { state.prompt.draft[key] = value; state.prompt.preview = false; state.prompt.dirty = true; if (["noop", "confirmed"].includes(state.prompt.status)) state.prompt.status = "idle"; }

  function act(action) {
    state.notice = "";
    const p = state.prompt, ind = state.industry;
    if (action === "open-command") return openPanel(state.entered ? "command" : "account");
    if (action === "open-account") return openPanel("account");
    if (action === "prompt-history" && state.control) return openPanel("prompt-history");
    if (action === "return-world" || action === "industry-return") { if (action === "industry-return") ind.history = true; return closePanel(); }
    if (action === "enter" || action === "enter-return") { state = fresh(state.route, fixtures[state.route][0][0]); state.entered = true; state.control = true; if (action === "enter-return" && state.route === "entry") { state.goal = "恢复北侧补给"; state.chatStatus = "confirmed"; state.sentAgent = "A"; state.receiptKind = "chat"; } state.panel = "world"; state.notice = "新示例会话已建立；旧会话私有读面不复用。没有创建真实账号或提交游戏操作。"; return render("next-action"); }
    if (action === "signout") { const route = state.route; state = fresh(route, fixtures[route][0][0]); state.entered = false; state.control = false; state.panel = "account"; state.notice = "本地示例会话已退出，私有读面和旧请求已清除。"; return render("drawer-title"); }
    if (action === "retry-renderer") { state.renderer = true; state.notice = "示例重试完成，世界示意画面恢复。"; }
    if (action.startsWith("observe-")) { state.observed = action.slice(8) === "unknown" ? "未知目标" : action.slice(8); state.notice = "观察对象已切换；草稿和原请求主体保持不变。"; }
    if (action === "use-observed" && ["A","C"].includes(state.observed)) {
      const change = () => { state.commandAgent = state.observed; state.chatDraft = ""; state.goal = ""; state.prompt = fresh("entry", "first").prompt; state.promptOpen = false; state.notice = "新的空白草稿已绑定该目标，原请求仍归属原主体。"; render("drawer-title"); };
      if (state.chatDraft || state.prompt.dirty) return confirm("切换草稿主体？", "将丢弃当前未发送的指导和未提交 Prompt 草稿。原请求及其状态仍归属原主体。", "切换并新建草稿", change);
      change(); return;
    }
    if (action === "chat-preset") {
      const load = () => { state.chatDraft = "先检查燃料缺口，再规划第一批铁锭。"; render("chat-draft"); };
      if (state.chatDraft) return confirm("替换未发送的指导？", "建议只放入本地草稿，不发送给 Agent。", "替换草稿", load);
      load(); return;
    }
    if (action === "chat-send" && state.control && state.chatStatus !== "unknown") {
      if (!state.chatDraft.trim()) state.chatStatus = "rejected";
      else { state.sentText = state.chatDraft; state.sentAgent = state.commandAgent; state.chatStatus = "unknown"; state.receiptKind = "chat"; state.notice = "示例已接收指导；当前目标尚未更新。"; }
    }
    if (action === "chat-query" && state.chatStatus === "unknown") { state.chatStatus = "confirmed"; if (state.commandAgent === state.sentAgent) state.goal = state.sentText; state.notice = `原请求 ${state.sentAgent} 的指导已确认；其他主体的当前目标不被覆盖。`; }
    if (action.startsWith("clear-") && !promptLocked()) { changeDraft(action.slice(6), null); state.notice = "仅清除了本地覆盖草稿，尚未提交。"; }
    if (action === "prompt-preset" && !promptLocked()) {
      const load = () => { p.draft = {system:"preset-P / system",short:"preset-P / short",long:"preset-P / long"}; p.preset = "P"; p.dirty = true; p.preview = false; p.status = "idle"; render("prompt-system"); };
      if (p.dirty) return confirm("用预设替换本地草稿？", "现有未提交编辑将被替换。profile、生效值和对话不会改变。", "载入预设", load);
      load(); return;
    }
    if (action === "prompt-preview" && !promptLocked()) { p.preview = true; state.notice = "完整影响已按当前草稿刷新，未提交任何更改。"; }
    if (action === "prompt-apply" && !promptLocked()) {
      if (p.draftVersion !== p.version) p.status = "stale";
      else if (profileSame()) { p.status = "noop"; p.dirty = false; recordPromptResult("noop"); }
      else { p.profile = {...p.draft}; p.version += 1; p.draftVersion = p.version; p.status = "applied"; p.dirty = false; recordPromptResult("applied"); }
    }
    if (action === "prompt-effective" && state.control && p.status === "applied") { p.effective = candidate(); p.status = "confirmed"; p.preview = false; const original = state.promptReceipts.find((r) => r.id === p.requestId && r.agent === state.commandAgent); if (original) original.status = "confirmed"; }
    if (action === "prompt-compare" && state.control) p.compare = true;
    if (action === "prompt-rebase" && state.control && p.status === "stale") { p.draftVersion = p.version; p.status = "idle"; p.compare = false; state.notice = "草稿已保留并采用新基准，尚未应用。"; }
    if (action === "prompt-reload" && state.control) return confirm("丢弃草稿并重新读取？", "未提交的编辑会丢失。重新读取不会自动应用 Prompt。", "丢弃并重新读取", () => { p.draft = {...p.profile}; p.draftVersion = p.version; p.status = "idle"; p.compare = false; p.dirty = false; p.preview = false; state.notice = "已重新读取，尚未提交任何修改。"; render("drawer-title"); });
    if (action === "prompt-latest" && state.control && p.status === "unknown") { p.version = 8; p.profile.system = "system-D"; p.lookup = true; }
    if (action === "prompt-read-again" && state.control && p.readFailed) state.notice = "重读已结束，但仍不可用。原过期状态和 short-D 草稿保持，未应用。";
    if (action === "industry-preview" && state.control && ind.status === "idle") { ind.quote = ind.fuel >= 3; state.notice = ind.quote ? "已只读刷新示例报价，未创建操作回执。" : "报价仍被投入条件阻塞，没有成功 preview，实际时长保持未知。"; }
    if (action === "industry-recheck" && state.control && ind.status === "idle") { ind.fuel = ind.observedFuel; ind.quote = ind.fuel >= 3; state.notice = ind.fuel >= 3 ? "外部供给读回显示燃料为 3。只读查询没有补购或提交。" : "供给仍为 2，缺少 1。只读重查没有补料或提交。"; }
    if (action === "industry-submit" && state.control && ind.fuel >= 3 && ind.quote && ind.status === "idle") { ind.status = "unknown"; state.receiptKind = "industry"; state.notice = "原请求已建立；尚不能宣称生产成功。"; }
    if (action === "industry-query" && state.control && ind.status === "unknown") { if (ind.readback === "unavailable") { ind.queryFailed = true; state.notice = "查询已结束，服务仍不可用。原结果保持 unknown，没有第二次提交。"; } else if (ind.readback === "mismatch") { state.notice = "读到了另一个 job 的结果，不能归给原 job 17；原结果仍未知。"; } else { ind.status = ind.readback; ind.queryFailed = false; state.notice = ind.status === "settled" ? "同一 owner A / factory-demo-01 / recipe.smelter.iron_ingot / job 17 的原结果已读回。副产物仍以实际发布字段为准。" : "原请求已明确拒绝；没有自动再次生产。"; } }
    if (action === "industry-original") { state.notice = ind.fullReceipt ? "同 job 17 的完整 RecipeCompletionReceiptV1：9 iron_ingot、3 slag。" : "StarterIndustrialSettlementSummaryV1：消耗 12 ore / 3 fuel，产出 9 ingot，电力 24；没有副产物字段。"; }
    render();
  }

  function reset(route, fixture) {
    state = fresh(route, fixture);
    $("fixture").innerHTML = fixtures[route].map(([value,label]) => `<option value="${value}" ${value === fixture ? "selected" : ""}>${label}</option>`).join("");
    $("readback-label").hidden = route !== "industry";
    $("readback").value = state.industry.readback;
    $("supply-label").hidden = route !== "industry";
    $("supply").value = String(state.industry.observedFuel);
    document.querySelectorAll("[data-route]").forEach((el) => { if (el.dataset.route === route) el.setAttribute("aria-current", "page"); else el.removeAttribute("aria-current"); });
    render();
  }
  function routeFromHash() { const hash = location.hash.slice(1); if (state && !["entry","prompt","industry"].includes(hash)) return; const route = ["entry","prompt","industry"].includes(hash) ? hash : "entry"; reset(route, fixtures[route][0][0]); }
  document.addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target || target.disabled) return;
    if (target.dataset.panel) return target.dataset.panel === "world" ? closePanel() : openPanel(state.entered ? target.dataset.panel : "account");
    if (target.dataset.action) act(target.dataset.action);
  });
  $("close-drawer").addEventListener("click", closePanel);
  $("account-button").addEventListener("click", () => openPanel("account"));
  $("fixture").addEventListener("change", (event) => reset(state.route, event.target.value));
  $("readback").addEventListener("change", (event) => { state.industry.readback = event.target.value; });
  $("supply").addEventListener("change", (event) => { state.industry.observedFuel = Number(event.target.value); });
  $("skip-to-stage").addEventListener("click", (event) => { event.preventDefault(); $("stage").focus(); });
  $("text-size").addEventListener("click", () => { const large = document.documentElement.classList.toggle("text-large"); $("text-size").setAttribute("aria-pressed", String(large)); layoutPixelWorld(); });
  $("drawer-content").addEventListener("input", (event) => {
    if (event.target.id === "chat-draft") state.chatDraft = event.target.value;
    if (event.target.dataset.promptField && !promptLocked()) { changeDraft(event.target.dataset.promptField, event.target.value.trim() || null); if ($("prompt-impact")) $("prompt-impact").innerHTML = promptImpact(); }
  });
  $("drawer-content").addEventListener("toggle", (event) => { if (event.target.id === "advanced") state.promptOpen = event.target.open; }, true);
  document.addEventListener("keydown", (event) => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === "Escape" && !$("confirm-dialog").open && state.panel !== "world") { event.preventDefault(); closePanel(); }
  });
  $("cancel-confirm").addEventListener("click", () => $("confirm-dialog").close());
  $("accept-confirm").addEventListener("click", () => { const action = confirmAction; confirmAction = null; $("confirm-dialog").close(); action?.(); });
  $("confirm-dialog").addEventListener("close", () => { confirmAction = null; if (confirmInvoker?.isConnected) confirmInvoker.focus(); });
  window.addEventListener("resize", layoutPixelWorld);
  if (typeof ResizeObserver !== "undefined") new ResizeObserver(layoutPixelWorld).observe($("stage"));
  window.addEventListener("hashchange", routeFromHash);
  routeFromHash();
})();
