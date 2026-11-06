/* UI/UE design fixture only. No network, credentials, persistence or game actions. */
"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const escape = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  // Presentation is independent of fresh() and never writes business state.
  const presentation = {locale:"zh-CN", copyMode:"normal"};
  const t = (key, values) => window.Oasis7PrototypeCopy.text(key, presentation.locale, presentation.copyMode, values);
  const observedLabel = () => state.observed === "未知目标" ? t("targets.unknownIdentity") : state.observed;
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
  const fields = { system: "field.system", short: "field.short", long: "field.long" };
  const fixtures = {
    entry: [["first", "首次进入"], ["return", "回访 · 历史结果"], ["identities", "观察 B / 草稿 C / 原请求 A"], ["dense", "同位置密集目标"], ["position", "位置未知"], ["renderer", "世界画面不可用"], ["entry-failed", "账号入口失败"]],
    prompt: [["noop", "空覆盖 · 清除不改变生效值"], ["override", "已有覆盖 · 清除后恢复默认"], ["indirect", "完整 profile · 间接影响"], ["stale", "草稿过期 · 保留与比较"], ["stale-read-failed", "过期后重读仍失败"], ["unknown", "原 Apply 结果未知"], ["revoked", "控制权失效 · 隐藏私有值"]],
    industry: [["blocked", "燃料不足 · 先解决阻碍"], ["ready", "报价可用 · 完整生产方案"], ["unknown", "提交结果未知"], ["query-failed", "原请求查询失败"], ["rejected", "原请求明确拒绝"], ["settled", "成果 · 摘要未发布副产物"], ["full-receipt", "成果 · 完整原件已发布"], ["return", "回访 · 历史成果"]]
  };
  let state;
  let invoker = "nav-command";
  let confirmAction = null;
  let confirmInvoker = null;
  let confirmCopy = ["confirm.reloadTitle", "confirm.initialHelp", "prompt.reload"];

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
  const none = (value) => value === null ? t("value.none") : escape(value);
  const profileSame = () => Object.keys(fields).every((key) => state.prompt.profile[key] === state.prompt.draft[key]);
  const candidate = () => Object.fromEntries(Object.keys(fields).map((key) => [key, state.prompt.draft[key] ?? defaults[key]]));
  const promptLocked = () => !state.control || ["stale", "unknown", "applied"].includes(state.prompt.status);
  const promptReceiptLabel = (status) => t({unknown:"prompt.unknown",applied:"prompt.receiptApplied",noop:"prompt.noop",confirmed:"prompt.receiptConfirmed"}[status] || "prompt.receiptPending");
  function recordPromptResult(status) {
    const id = `prompt-demo-${String(8 + state.promptReceipts.length).padStart(2,"0")}`;
    state.prompt.requestId = id;
    state.promptReceipts.push({id,agent:state.commandAgent,status});
    state.receiptKind = "prompt";
  }
  function promptImpact() {
    const p = state.prompt;
    if (promptLocked()) return "";
    return card(t("prompt.impact"), profileSame() ? `<p>${t("prompt.noChangeImpact")}</p>` : Object.keys(fields).map((key) => `<p>${t(fields[key])}: ${escape(p.effective[key])} → ${escape(candidate()[key])}${p.draft[key] === p.profile[key] && p.effective[key] !== candidate()[key] ? `<br><span class="state-label">${t("prompt.indirect")}</span>` : ""}</p>`).join(""), "soft");
  }

  function accountContent() {
    if (state.entered) return card(t("account.title"), `<p>${t("account.restored")}</p>`) + `<div class="actions">${button("return-world", t("account.return"), true)}${button("signout", t("account.signout"))}</div>`;
    return `<p class="muted">${t("account.intro")}</p>` +
      (state.fixture === "entry-failed" ? card(t("account.unavailable"), `<p>${t("account.failed")}</p>`, "error") : "") +
      `<div class="stack">${button("enter", t("account.enter"), true)}${button("enter-return", t("account.restore"))}</div><p class="meta">${t("account.note")}</p>`;
  }

  function chatContent() {
    const busy = state.chatStatus === "unknown";
    return `<p class="meta">${t("chat.context", {observed:escape(observedLabel()), agent:escape(state.commandAgent)})}${state.sentAgent ? t("chat.original", {agent:escape(state.sentAgent)}) : ""}</p>` +
      card(t("chat.goal"), `<p>${state.goal ? escape(state.goal) : t("chat.noGoal")}</p><p class="meta">${t(state.goal ? "chat.goalRead" : "chat.goalPending")}</p>`) +
      card(t("chat.activity"), `<p>${t("world.activity")}</p><p class="meta">${t("chat.activityHelp")}</p>`, "soft") +
      `<label class="field" for="chat-draft"><span>${t("chat.field", {agent:escape(state.commandAgent)})}</span><textarea id="chat-draft" placeholder="${escape(t("chat.placeholder"))}" ${!state.control ? "disabled" : ""}>${escape(state.chatDraft)}</textarea></label><p class="meta">${t("chat.inputHelp")}</p>` +
      `<div class="actions">${button("chat-send", t(busy ? "chat.waiting" : "chat.send"), true, !state.control || busy)}${button("chat-preset", t("chat.preset"), false, !state.control)}</div>` +
      (state.chatStatus === "rejected" ? card(t("chat.notSent"), `<p>${t("chat.empty")}</p>`, "error") : "") +
      (busy ? card(t("chat.received"), `<p>${t("chat.queryHelp", {agent:escape(state.sentAgent)})}</p>${button("chat-query", t("chat.query"))}`, "warn") : "") +
      (state.chatStatus === "confirmed" ? card(t("chat.confirmed"), `<p>${t("chat.confirmedHelp")}</p>`, "success") : "") +
      `<details id="advanced" ${state.promptOpen ? "open" : ""}><summary>${t("chat.advanced")}</summary><p class="meta">${t("chat.advancedHelp")}</p>${promptContent()}</details>`;
  }

  function promptContent() {
    const p = state.prompt;
    if (!state.control) return card(t("prompt.noControl"), `<p>${t("prompt.privateHidden")}</p>`, "error") +
      `<label class="field" for="safe-draft"><span>${t("prompt.localText")}</span><textarea id="safe-draft" readonly>${escape(p.draft.short ?? "")}</textarea></label><p class="meta">${t("prompt.retainedHelp")}</p>`;
    if (p.readFailed) return card(t("prompt.readFailed"), `<p>${t("prompt.readFailedHelp")}</p>`, "error") + `<label class="field" for="safe-draft"><span>${t("prompt.originalDraft")}</span><textarea id="safe-draft" readonly>${escape(p.draft.short)}</textarea></label><div class="actions">${button("prompt-read-again", t("prompt.retryRead"), true)}${button("return-world", t("prompt.return"))}</div><p class="meta">${t("prompt.readStillUnavailable")}</p>`;
    const sourceValues = (values) => Object.keys(fields).map((key) => `${t(fields[key])}: ${none(values[key])}`).join("<br>");
    // Deliberately omit one translation in review mode; keep its source language explicit.
    const draftHelp = presentation.copyMode === "missing"
      ? `<span lang="zh-CN">${escape(window.Oasis7PrototypeCopy.text("prompt.draftHelp", "zh-CN"))}</span> <span class="translation-note">${t("copy.missingNote")}</span>`
      : t("prompt.draftHelp");
    let html = `<div class="source-grid" aria-label="${escape(t("prompt.sources"))}">
      <div class="source-row"><strong>${t("prompt.defaults")}</strong><p>${sourceValues(defaults)}</p></div>
      <div class="source-row"><strong>${t("prompt.profile", {version:p.version})}</strong><p>${sourceValues(p.profile)}</p></div>
      <div class="source-row"><strong>${t("prompt.effective")}</strong><p>${sourceValues(p.effective)}</p><p class="muted">${t(p.effective.short === "short-B" ? "prompt.initialPath" : "prompt.matchedRead")}</p></div>
      <div class="source-row"><strong>${t("prompt.preset")}</strong><p>${t(p.preset ? "prompt.presetCopied" : "prompt.noPreset")}</p>${button("prompt-preset", t("prompt.loadPreset"), false, promptLocked())}</div>
      <div class="source-row"><strong>${t("prompt.draftSource", {version:p.draftVersion})}</strong><p id="prompt-source-help">${draftHelp}</p></div>
    </div>`;
    for (const key of Object.keys(fields)) html += `<label class="field" for="prompt-${key}"><span>${t("prompt.fieldDraft", {field:t(fields[key]), suffix:p.draft[key] === null ? t("prompt.noOverride") : ""})}</span><div class="inline"><textarea id="prompt-${key}" data-prompt-field="${key}" ${promptLocked() ? "readonly" : ""} placeholder="${escape(t("prompt.placeholder"))}">${escape(p.draft[key] ?? "")}</textarea>${button(`clear-${key}`, t("prompt.clear"), false, promptLocked())}</div></label>`;
    if (p.status === "stale") html += card(t("prompt.stale"), `<p>${t("prompt.staleHelp", {draft:p.draftVersion, current:p.version})}</p><div class="actions">${button("prompt-compare", t("prompt.compare"))}${button("prompt-rebase", t("prompt.rebase"))}${button("prompt-reload", t("prompt.reload"))}</div>`, "warn");
    if (p.compare) html += card(t("prompt.comparison"), Object.keys(fields).map((key) => `<p><b>${t(fields[key])}</b><br>${t("prompt.currentProfile")}${none(p.profile[key])}<br>${t("prompt.retainedDraft")}${none(p.draft[key])}</p>`).join(""), "soft");
    if (p.status === "unknown") html += card(t("prompt.unknown"), `<p>${t("prompt.unknownHelp", {id:escape(p.requestId)})}</p>${button("prompt-latest", t("prompt.latest"))}${p.lookup ? `<p>${t("prompt.latestRead")}</p>` : ""}`, "warn");
    if (p.status === "applied") html += card(t("prompt.applied"), `<p>${t("prompt.appliedHelp", {version:p.version})}</p>${button("prompt-effective", t("prompt.readEffective"))}`, "warn");
    if (p.status === "noop") html += card(t("prompt.noop"), `<p>${t("prompt.noopHelp", {version:p.version})}</p>`, "success");
    if (p.status === "confirmed") html += card(t("prompt.confirmed"), `<p>${t("prompt.confirmedHelp")}</p>`, "success");
    html += `<div id="prompt-impact" aria-live="polite">${promptImpact()}</div>`;
    html += `<div class="actions">${button("prompt-preview", t("prompt.preview"), false, promptLocked())}${button("prompt-apply", t("prompt.apply"), true, promptLocked())}</div><p class="meta">${t("prompt.applyHelp")}</p>`;
    return html;
  }

  function industryContent() {
    const p = state.industry;
    if (!state.control) return card(t("prompt.noControl"), `<p>${t("industry.privateHidden")}</p>`, "error");
    if (p.status === "settled") return `<p class="meta">${t(p.history ? "industry.history" : "industry.original")} · ${t("industry.owner")}</p>` +
      card(t("industry.produced"), `<p class="success-text">production_settled · production_only</p><dl class="fact-grid">${fact(t("industry.oreUsed"), "12 iron_ore")}${fact(t("industry.fuelUsed"), "3 carbon_fuel")}${fact(t("industry.ingots"), "9 iron_ingot")}${fact(t("industry.power"), "24")}${fact(t("industry.byproduct"), p.fullReceipt ? "3 slag" : t("industry.unpublished"))}</dl><p class="meta">${t(p.fullReceipt ? "industry.fullSource" : "industry.summarySource")}</p>`, "success") +
      card(t("industry.nextCandidate"), `<p>${t("industry.gap")}</p><p class="meta">${t("industry.candidateHelp")}</p>`, "soft") +
      `<div class="actions">${button("industry-return", t("account.return"), true)}${button("industry-original", t("industry.source"))}</div>`;
    if (p.status === "rejected") return card(t("industry.rejected"), `<p>action-demo-17 · Agent A / job 17</p><p>${t("industry.rejectionReason")}</p><p>${t("industry.rejectionHelp")}</p>`, "error") + `<div class="actions">${button("industry-return", t("account.return"), true)}</div>`;
    const blocked = p.fuel < 3;
    const supply = (required, amount, material) => t("industry.requirement", {required, amount, material, time:t(p.status === "unknown" ? "industry.before" : "industry.available")});
    return `<p class="meta">${t("industry.planOwner")}</p><p class="meta">${t("industry.factory")}</p><p>${t(p.status === "unknown" ? "industry.unknownPlan" : "industry.planHelp")}</p>` +
      card(t(p.status === "unknown" ? "industry.historicalPlan" : "industry.plan"), `<dl class="fact-grid">${fact(t("industry.inputOre"), supply(12,12,"iron_ore"))}${fact(t("industry.inputFuel"), supply(3,p.fuel,"carbon_fuel"))}${fact(t("industry.expectedMain"), "9 iron_ingot")}${fact(t("industry.expectedByproduct"), "3 slag")}${fact(t("industry.recipePower"), "24")}${fact(t("industry.baseDuration"), t("industry.baseTicks"))}${fact(t(p.status === "unknown" ? "industry.oldQuoteDuration" : "industry.quoteDuration"), t(p.quote ? "industry.quotedTicks" : "industry.noQuote"))}${fact(t("industry.fees"), t("industry.unknownFees"))}</dl><p class="meta">${t("industry.recipeHelp")}</p>`) +
      (blocked ? card(t("industry.shortage"), `<p>${t("industry.shortageHelp")}</p>${button("industry-recheck", t("industry.recheck"), true)}`, "error") : "") +
      (p.status === "unknown" ? card(t(p.queryFailed ? "industry.queryFailed" : "industry.waiting"), `<p>${t("industry.queryHelp")}</p><div class="actions">${button("industry-query", t("industry.query"), true)}</div><p class="meta">${t("industry.queryNote")}</p>`, "warn") : `<div class="actions">${button("industry-preview", t("industry.preview"))}${button("industry-submit", t("industry.submit"), true, blocked || !p.quote)}</div><p class="meta">${t("industry.submitHelp")}</p>`);
  }

  function targetsContent() {
    const canGuide = (id) => state.control && ["A","C"].includes(id);
    return `<p class="muted">${t("targets.intro")}</p>` +
      (state.fixture === "dense" ? card(t("targets.cluster"), `<p>${t("targets.clusterHelp")}</p>`, "soft") : "") +
      ["A","B","C"].map((id) => `<div class="target-row"><div><strong>${t("targets.name", {agent:id})}</strong><p>${t(canGuide(id) ? "targets.guideable" : "targets.observeOnly")} · ${t(state.fixture === "dense" ? "targets.sharedPosition" : "targets.position")}</p></div>${button(`observe-${id}`, t("targets.view", {agent:id}))}</div>`).join("") +
      `<div class="target-row"><div><strong>${t("targets.unknownName")}</strong><p>${t("targets.noCoordinates")}</p></div>${button("observe-unknown", t("targets.textView"))}</div>` +
      (state.observed === "未知目标" ? card(t("targets.unknown"), `<p>${t("targets.unknownHelp")}</p>`, "warn") : card(t("targets.viewing", {agent:escape(state.observed)}), `<p>${t(canGuide(state.observed) ? "targets.draftHelp" : "targets.readOnlyHelp")}</p>${canGuide(state.observed) ? button("use-observed", t("targets.use")) : ""}`, "soft"));
  }

  function renderNext() {
    let title = t("next.first"), body = "next.firstHelp", action = "open-command", label = "next.guide";
    if (!state.entered) { title = t("next.start"); body = "next.startHelp"; action = "open-account"; label = "next.enter"; }
    else if (state.route === "prompt") { title = t("next.prompt"); body = "next.promptHelp"; label = "next.openPrompt"; }
    else if (state.route === "industry") { title = t(state.industry.status === "settled" ? "next.settled" : state.industry.status === "unknown" ? "next.unknown" : state.industry.status === "rejected" ? "next.rejected" : state.industry.fuel < 3 ? "next.blocked" : "next.ready"); body = state.industry.status === "settled" ? "next.settledHelp" : state.industry.status === "unknown" ? "next.unknownHelp" : "next.planHelp"; label = state.industry.status === "settled" ? "next.history" : state.industry.status === "unknown" ? "next.original" : "next.plan"; }
    else if (state.goal) { title = state.goal; body = "next.goalHelp"; label = "next.agent"; }
    $("next-move").innerHTML = `<p class="eyebrow">${t("next.eyebrow")}</p><h2 id="next-title">${escape(title)}</h2><p>${t(body)}</p><button id="next-action" type="button" class="primary" data-action="${action}">${t(label)} <span aria-hidden="true">↗</span></button>`;
  }

  function renderReceipt() {
    const r = $("receipt");
    r.hidden = !state.receiptKind || !state.entered;
    $("stage").classList.toggle("has-receipt", !r.hidden);
    if (r.hidden) return;
    let id, title, description, action;
    if (state.receiptKind === "chat") { id = `chat-demo-01 · Agent ${state.sentAgent}`; title = t(state.chatStatus === "confirmed" ? "receipt.chatConfirmed" : "receipt.chatUnknown"); description = t("receipt.chatHelp"); action = state.chatStatus === "unknown" ? button("chat-query", t("chat.query")) : button("open-command", t("receipt.goal")); }
    if (state.receiptKind === "prompt") { const original = state.promptReceipts.at(-1); id = original ? `${original.id} · Agent ${original.agent}` : t("receipt.noPrompt"); title = !state.control ? t("receipt.private") : promptReceiptLabel(original?.status); description = t("receipt.promptHelp"); action = button("prompt-history", t("receipt.history", {count:state.promptReceipts.length})); }
    if (state.receiptKind === "industry") { id = "action-demo-17 · Agent A / job 17"; title = t(state.industry.status === "settled" ? "receipt.settled" : state.industry.status === "rejected" ? "receipt.rejected" : "receipt.unknown"); description = t(state.industry.history ? "receipt.historical" : "receipt.originalHelp"); action = state.industry.status === "unknown" ? button("industry-query", t("industry.query")) : button("open-command", t("receipt.result")); }
    r.innerHTML = `<p class="receipt-id">${escape(id)}</p><strong>${escape(title)}</strong><p>${escape(description)}</p>${action}`;
    const receiptAction = r.querySelector("button"); if (receiptAction) receiptAction.id = "receipt-action";
  }

  function render(focusId) {
    const focused = document.activeElement;
    const oldId = focusId || focused?.id;
    const oldAction = focused?.dataset?.action;
    syncReviewControls();
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
      const selected = dense ? ["A","B","C"].includes(state.observed) : state.observed === id;
      marker.style.outline = selected ? "3px solid var(--gold)" : "none";
      marker.style.outlineOffset = "5px";
    });
    $("account-button").textContent = t(state.entered ? "account.entered" : "account.signedOut");
    $("world-context").innerHTML = `<span class="badge">${t("world.viewing", {agent:escape(observedLabel())})}</span><span class="badge">${t("world.draft", {agent:escape(state.commandAgent)})}</span>${state.sentAgent ? `<span class="badge">${t("world.original", {agent:escape(state.sentAgent)})}</span>` : ""}`;
    $("feed-text").textContent = t(state.renderer ? "world.feed" : "world.textFeed");
    $("notice").textContent = state.notice ? t(state.notice, state.noticeValues) : "";
    renderNext(); renderReceipt();
    $("drawer").hidden = state.panel === "world";
    document.querySelectorAll("[data-panel]").forEach((el) => el.setAttribute("aria-pressed", String(el.dataset.panel === state.panel)));
    if (state.panel !== "world") {
      $("drawer-kicker").textContent = state.panel === "account" ? "ACCOUNT / ENTRY" : state.panel.toUpperCase();
      $("drawer-title").textContent = t(state.panel === "account" ? "panel.account" : state.panel === "targets" ? "panel.targets" : state.panel === "prompt-history" ? "panel.history" : state.route === "industry" ? "panel.industry" : state.route === "prompt" ? "panel.prompt" : "panel.command", {agent:state.commandAgent});
      $("drawer-content").innerHTML = state.panel === "account" ? accountContent() : state.panel === "targets" ? targetsContent() : state.panel === "prompt-history" ? state.promptReceipts.map((original) => card(`${escape(original.id)} · Agent ${escape(original.agent)}`, `<p>${promptReceiptLabel(original.status)}</p><p class="meta">${t("prompt.historyHelp")}</p>`)).join("") : state.route === "industry" ? industryContent() : state.route === "prompt" ? promptContent() : chatContent();
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
    confirmCopy = [title, description, accept];
    renderConfirmCopy();
    confirmAction = callback; $("confirm-dialog").showModal(); $("cancel-confirm").focus();
  }
  function renderConfirmCopy() {
    ["confirm-title", "confirm-description", "accept-confirm"].forEach((id, index) => { $(id).textContent = t(confirmCopy[index]); });
  }
  function changeDraft(key, value) { state.prompt.draft[key] = value; state.prompt.preview = false; state.prompt.dirty = true; if (["noop", "confirmed"].includes(state.prompt.status)) state.prompt.status = "idle"; }

  function act(action) {
    state.notice = "";
    const p = state.prompt, ind = state.industry;
    if (action === "open-command") return openPanel(state.entered ? "command" : "account");
    if (action === "open-account") return openPanel("account");
    if (action === "prompt-history" && state.control) return openPanel("prompt-history");
    if (action === "return-world" || action === "industry-return") { if (action === "industry-return") ind.history = true; return closePanel(); }
    if (action === "enter" || action === "enter-return") { const fixture = action === "enter-return" && state.route === "entry" ? "return" : fixtures[state.route][0][0]; state = fresh(state.route, fixture); state.entered = true; state.control = true; state.panel = "world"; state.notice = "notice.enter"; return render("next-action"); }
    if (action === "signout") { const route = state.route; state = fresh(route, fixtures[route][0][0]); state.entered = false; state.control = false; state.panel = "account"; state.notice = "notice.signout"; return render("drawer-title"); }
    if (action === "retry-renderer") { state.renderer = true; state.notice = "notice.renderer"; }
    if (action.startsWith("observe-")) { state.observed = action.slice(8) === "unknown" ? "未知目标" : action.slice(8); state.notice = "notice.observe"; }
    if (action === "use-observed" && state.control && ["A","C"].includes(state.observed)) {
      const change = () => { state.commandAgent = state.observed; state.chatDraft = ""; state.goal = ""; state.prompt = fresh("entry", "first").prompt; state.promptOpen = false; state.notice = "notice.draftOwner"; render("drawer-title"); };
      if (state.chatDraft || state.prompt.dirty) return confirm("confirm.switchTitle", "confirm.switchHelp", "confirm.switchAccept", change);
      change(); return;
    }
    if (action === "chat-preset") {
      const load = () => { state.chatDraft = "先检查燃料缺口，再规划第一批铁锭。"; render("chat-draft"); };
      if (state.chatDraft) return confirm("confirm.chatTitle", "confirm.chatHelp", "confirm.chatAccept", load);
      load(); return;
    }
    if (action === "chat-send" && state.control && state.chatStatus !== "unknown") {
      if (!state.chatDraft.trim()) state.chatStatus = "rejected";
      else { state.sentText = state.chatDraft; state.sentAgent = state.commandAgent; state.chatStatus = "unknown"; state.receiptKind = "chat"; state.notice = "notice.chatReceived"; }
    }
    if (action === "chat-query" && state.chatStatus === "unknown") { state.chatStatus = "confirmed"; if (state.commandAgent === state.sentAgent) state.goal = state.sentText; state.notice = "notice.chatConfirmed"; state.noticeValues = {agent:state.sentAgent}; }
    if (action.startsWith("clear-") && !promptLocked()) { changeDraft(action.slice(6), null); state.notice = "notice.clear"; }
    if (action === "prompt-preset" && !promptLocked()) {
      const load = () => { p.draft = {system:"preset-P / system",short:"preset-P / short",long:"preset-P / long"}; p.preset = "P"; p.dirty = true; p.preview = false; p.status = "idle"; render("prompt-system"); };
      if (p.dirty) return confirm("confirm.presetTitle", "confirm.presetHelp", "confirm.presetAccept", load);
      load(); return;
    }
    if (action === "prompt-preview" && !promptLocked()) { p.preview = true; state.notice = "notice.preview"; }
    if (action === "prompt-apply" && !promptLocked()) {
      if (p.draftVersion !== p.version) p.status = "stale";
      else if (profileSame()) { p.status = "noop"; p.dirty = false; recordPromptResult("noop"); }
      else { p.profile = {...p.draft}; p.version += 1; p.draftVersion = p.version; p.status = "applied"; p.dirty = false; recordPromptResult("applied"); }
    }
    if (action === "prompt-effective" && state.control && p.status === "applied") { p.effective = candidate(); p.status = "confirmed"; p.preview = false; const original = state.promptReceipts.find((r) => r.id === p.requestId && r.agent === state.commandAgent); if (original) original.status = "confirmed"; }
    if (action === "prompt-compare" && state.control) p.compare = true;
    if (action === "prompt-rebase" && state.control && p.status === "stale") { p.draftVersion = p.version; p.status = "idle"; p.compare = false; state.notice = "notice.rebase"; }
    if (action === "prompt-reload" && state.control) return confirm("confirm.reloadTitle", "confirm.reloadHelp", "prompt.reload", () => { p.draft = {...p.profile}; p.draftVersion = p.version; p.status = "idle"; p.compare = false; p.dirty = false; p.preview = false; state.notice = "notice.reload"; render("drawer-title"); });
    if (action === "prompt-latest" && state.control && p.status === "unknown") { p.version = 8; p.profile.system = "system-D"; p.lookup = true; }
    if (action === "prompt-read-again" && state.control && p.readFailed) state.notice = "notice.readFailed";
    if (action === "industry-preview" && state.control && ind.status === "idle") { ind.quote = ind.fuel >= 3; state.notice = ind.quote ? "notice.quote" : "notice.quoteBlocked"; }
    if (action === "industry-recheck" && state.control && ind.status === "idle") { ind.fuel = ind.observedFuel; ind.quote = ind.fuel >= 3; state.notice = ind.fuel >= 3 ? "notice.supplyReady" : "notice.supplyBlocked"; }
    if (action === "industry-submit" && state.control && ind.fuel >= 3 && ind.quote && ind.status === "idle") { ind.status = "unknown"; state.receiptKind = "industry"; state.notice = "notice.productionSubmitted"; }
    if (action === "industry-query" && state.control && ind.status === "unknown") { if (ind.readback === "unavailable") { ind.queryFailed = true; state.notice = "notice.queryUnavailable"; } else if (ind.readback === "mismatch") { state.notice = "notice.queryMismatch"; } else { ind.status = ind.readback; ind.queryFailed = false; state.notice = ind.status === "settled" ? "notice.settled" : "notice.rejected"; } }
    if (action === "industry-original") { state.notice = ind.fullReceipt ? "notice.fullReceipt" : "notice.summaryReceipt"; }
    render();
  }

  function syncReviewControls() {
    document.documentElement.lang = presentation.locale;
    // Only explicit, author-owned copy nodes are localized. Never walk or replace user text.
    document.querySelectorAll("[data-i18n]").forEach((element) => { element.textContent = t(element.dataset.i18n); });
    document.querySelectorAll("[data-i18n-aria-label]").forEach((element) => { element.setAttribute("aria-label", t(element.dataset.i18nAriaLabel)); });
    $("locale").value = presentation.locale;
    $("copy-mode").value = presentation.copyMode;
    $("fixture").innerHTML = fixtures[state.route].map(([value]) => `<option value="${value}" ${value === state.fixture ? "selected" : ""}>${t(`fixture.${state.route}.${value}`)}</option>`).join("");
    $("readback-label").hidden = state.route !== "industry";
    $("readback").value = state.industry.readback;
    $("supply-label").hidden = state.route !== "industry";
    $("supply").value = String(state.industry.observedFuel);
    document.querySelectorAll("[data-route]").forEach((el) => { if (el.dataset.route === state.route) el.setAttribute("aria-current", "page"); else el.removeAttribute("aria-current"); });
    renderConfirmCopy();
  }
  function updatePresentation(key, value) {
    const choices = key === "locale" ? ["zh-CN", "en"] : ["normal", "long", "missing"];
    if (!choices.includes(value)) return;
    // Preserve exact edit buffers, including whitespace, independently of Prompt's normalized candidate.
    const buffers = Array.from($("drawer-content").querySelectorAll("textarea"), (field) => ({id:field.id, value:field.value, start:field.selectionStart, end:field.selectionEnd, direction:field.selectionDirection, scroll:field.scrollTop}));
    const scrollTop = $("drawer-content").scrollTop;
    presentation[key] = value;
    render();
    buffers.forEach((buffer) => {
      const field = $(buffer.id);
      if (!field) return;
      field.value = buffer.value;
      field.setSelectionRange(buffer.start, buffer.end, buffer.direction);
      field.scrollTop = buffer.scroll;
    });
    $("drawer-content").scrollTop = scrollTop;
  }
  function reset(route, fixture) {
    state = fresh(route, fixture);
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
  $("locale").addEventListener("change", (event) => updatePresentation("locale", event.target.value));
  $("copy-mode").addEventListener("change", (event) => updatePresentation("copyMode", event.target.value));
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
