import { renderAccountEmailRecovery } from "./viewer_account_email_recovery.js";
import { renderImportedKey } from "./viewer_imported_key.js";
const API = "/api/public/hosted-account/v2";

export function decodePasskeyBytes(encoded) {
  if (typeof encoded !== "string" || !/^[A-Za-z0-9_-]*$/.test(encoded)) throw new Error("invalid_passkey_encoding");
  const binary = atob(encoded.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

export function encodePasskeyBytes(bytes) {
  return btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function browserPasskeyOptions(options) {
  const result = structuredClone(options);
  const key = result.publicKey;
  if (!key) throw new Error("invalid_passkey_options");
  key.challenge = decodePasskeyBytes(key.challenge);
  if (key.user) key.user.id = decodePasskeyBytes(key.user.id);
  for (const list of [key.excludeCredentials, key.allowCredentials]) {
    for (const credential of list || []) credential.id = decodePasskeyBytes(credential.id);
  }
  return result;
}

export function serializePasskey(credential) {
  const response = { clientDataJSON: encodePasskeyBytes(credential.response.clientDataJSON) };
  if (credential.response.attestationObject) {
    response.attestationObject = encodePasskeyBytes(credential.response.attestationObject);
    if (typeof credential.response.getTransports === "function") response.transports = credential.response.getTransports();
  } else {
    response.authenticatorData = encodePasskeyBytes(credential.response.authenticatorData);
    response.signature = encodePasskeyBytes(credential.response.signature);
    response.userHandle = credential.response.userHandle ? encodePasskeyBytes(credential.response.userHandle) : null;
  }
  return { id: credential.id, rawId: encodePasskeyBytes(credential.rawId), type: credential.type,
    response, extensions: credential.getClientExtensionResults?.() || {} };
}

export function createPasskeyAccountClient({ fetch: fetchImpl = globalThis.fetch,
  credentials = globalThis.navigator?.credentials } = {}) {
  let csrf = null;
  let importedSigner = null;
  let signerGeneration = 0;
  function clearImportedKey() { importedSigner = null; signerGeneration += 1; pendingActions.clear(); }
  let recoveryOperation = null;
  let recoveryCancelOperation = null;
  const pendingCeremonies = new Map();
  const pendingActions = new Map();
  async function request(path, body, method = body === undefined ? "GET" : "POST") {
    if (body !== undefined) { const capabilities = await request("/capabilities"); csrf = capabilities.csrf_token; }
    const headers = { Accept: "application/json" };
    if (body !== undefined) { headers["Content-Type"] = "application/json"; headers["X-Oasis7-CSRF"] = csrf || ""; }
    let response;
    try { response = await fetchImpl(`${API}${path}`, { method, headers, credentials: "same-origin", cache: "no-store",
      ...(body === undefined ? {} : { body: JSON.stringify(body) }) }); }
    catch { throw Object.assign(new Error("network_unavailable"), { code: "network_unavailable", endpoint: path }); }
    let result;
    try { result = await response.json(); }
    catch { throw Object.assign(new Error("account_response_invalid"), { code: "account_response_invalid", status: response.status, endpoint: path }); }
    if (!result || typeof result !== "object" || Array.isArray(result)) throw Object.assign(new Error("account_response_invalid"), { code: "account_response_invalid", status: response.status, endpoint: path });
    if (!response.ok || result.ok === false || result.phase === "operation_pending" || result.error_code === "operation_pending") {
      const error = new Error(result.error_code || "account_request_failed");
      error.code = result.error_code || (result.phase === "operation_pending" ? "operation_pending" : undefined);
      error.payload = result; error.status = response.status; error.endpoint = path; error.diagnostic_id = result.diagnostic_id; throw error;
    }
    return result;
  }
  async function ceremony(path, body, kind) {
    if (!credentials?.[kind]) throw new Error("passkey_unsupported");
    const key = `${path}:${JSON.stringify(body)}`;
    let pending = pendingCeremonies.get(key);
    if (!pending) {
      pending = { started: await request(`${path}/options`, body), response: null };
      pendingCeremonies.set(key, pending);
    }
    const started = pending.started;
    if (!pending.response) {
      try {
        const credential = await credentials[kind](browserPasskeyOptions(started.options));
        if (!credential) throw new Error("passkey_cancelled");
        pending.response = serializePasskey(credential);
      } catch (error) {
        error.stage = "系统通行密钥验证";
        // Explicit cancellation is authority-confirmed before discarding state.
        // A lost cancellation response leaves the original ceremony available.
        try {
          const cancelled = await request(`/ceremonies/${encodeURIComponent(started.ceremony_id)}/cancel`, {});
          if (["cancelled", "expired", "exhausted"].includes(cancelled.phase)) pendingCeremonies.delete(key);
          else { error.pending_operation_id = started.operation_id || started.ceremony_id; error.next_action = "check_original_operation"; }
        }
        catch { /* Preserve pending state; never start a second registration. */ }
        throw error;
      }
    }
    pending.payload ||= { ceremony_id: started.ceremony_id,
      response: pending.response, operation_id: started.operation_id || started.ceremony_id, ...body };
    return finishCeremony(path, key, pending);
  }
  async function finishCeremony(path, key, pending) {
    const started = pending.started;
    try {
      const result = await request(`${path}/complete`, pending.payload);
      pendingCeremonies.delete(key);
      return { ...result, operation_id: result.operation_id || started.operation_id || started.ceremony_id };
    } catch (error) {
      const operationId = started.operation_id || started.ceremony_id;
      if ([400, 401, 403, 409, 410].includes(error.status)) {
        try {
          const status = await request(`/operations/${encodeURIComponent(operationId)}`);
          if (["consumed", "committed", "operation_committed"].includes(status.phase || status.state)) {
            pendingCeremonies.delete(key);
            return { ...status, operation_id: operationId, phase: "operation_committed", account_retained: true, next_action: "fresh_authentication" };
          }
          if (["expired", "cancelled", "exhausted"].includes(status.phase || status.state)) pendingCeremonies.delete(key);
          else if (status.phase === "pending" && [400, 401].includes(error.status)) {
            // Cancel atomically after a failed proof. A concurrent successful
            // completion must return its committed receipt instead of a new key.
            const cancelled = await request(`/ceremonies/${encodeURIComponent(started.ceremony_id)}/cancel`, {});
            if (["consumed", "committed", "operation_committed"].includes(cancelled.phase || cancelled.state)) {
              pendingCeremonies.delete(key);
              return { ...cancelled, operation_id: operationId, phase: "operation_committed", account_retained: true, next_action: "fresh_authentication" };
            }
            if (["cancelled", "ceremony_cancelled"].includes(cancelled.phase || cancelled.state)) pendingCeremonies.delete(key);
          }
        } catch { /* Unknown authority outcome: retain the original response. */ }
      }
      if (pendingCeremonies.has(key)) {
        error.pending_operation_id = operationId;
        error.next_action = "check_original_operation";
      }
      throw error;
    }
  }
  async function authorize(actionType, target, extra = {}) {
    const key = JSON.stringify({ actionType, target, extra });
    let action = pendingActions.get(key);
    if (!action) { action = await request("/actions", { action_type: actionType, target, ...extra }); pendingActions.set(key, action); }
    if (!action.authorized) {
      if (importedSigner) await nativeCeremony(importedSigner, "reauth", action.action_id);
      else await ceremony(`/actions/${encodeURIComponent(action.action_id)}/reauth`, {}, "get");
      action.authorized = true;
    }
    return action.action_id;
  }
  async function nativeCeremony(signer, intent, action_id) {
    const capability = (await request("/capabilities")).native_key;
    if (!capability?.available) throw new Error("native_key_unsupported");
    const key = `native:${intent}:${signer.publicKey}:${action_id || ""}`;
    let pending = pendingCeremonies.get(key);
    if (pending && !pending.payload) {
      // A lost cancellation must be reconciled before signing or allocating again.
      try {
        const status = await request(`/operations/${encodeURIComponent(pending.started.operation_id || pending.started.ceremony_id)}`);
        if (["consumed", "committed", "operation_committed"].includes(status.phase || status.state)) {
          pendingCeremonies.delete(key);
          return { ...status, operation_id: pending.started.operation_id || pending.started.ceremony_id, phase: "operation_committed", next_action: "fresh_authentication" };
        }
        if (["cancelled", "expired", "exhausted"].includes(status.phase || status.state)) {
          pendingCeremonies.delete(key); pending = null;
        } else {
          const result = await cancelUnsigned();
          if (["consumed", "committed", "operation_committed"].includes(result.phase || result.state)) {
            return { ...result, operation_id: pending.started.operation_id || pending.started.ceremony_id, phase: "operation_committed", next_action: "fresh_authentication" };
          }
        }
      } catch (error) {
        if (!pendingCeremonies.has(key)) throw error;
        throw unresolved(error);
      }
      if (pendingCeremonies.has(key)) throw unresolved(new Error("operation_pending"));
      pending = null;
    }
    function unresolved(error) {
      error.pending_operation_id = pending.started.operation_id || pending.started.ceremony_id;
      error.next_action = "check_original_operation";
      error.stage = "核对原登录挑战";
      error.recovery_window_expired = Number.isSafeInteger(pending.started.expires_at) && Date.now() >= pending.started.expires_at + 600_000;
      return error;
    }
    async function cancelUnsigned() {
      const result = await request(`/ceremonies/${encodeURIComponent(pending.started.ceremony_id)}/cancel`, {});
      const phase = result.phase || result.state;
      if (["cancelled", "expired", "exhausted", "consumed", "committed", "operation_committed"].includes(phase)) pendingCeremonies.delete(key);
      return result;
    }
    if (!pending) {
      const started = await request("/native-key/options", { intent, public_key: signer.publicKey, action_id });
      pending = { started, payload: null };
      pendingCeremonies.set(key, pending);
      try {
        const signature = await signer.signChallenge(started, capability, intent);
        pending.payload = { ceremony_id: started.ceremony_id, operation_id: started.operation_id, signature };
      } catch (error) {
        error.stage = "本地密钥签名";
        try {
          const result = await cancelUnsigned();
          if (["consumed", "committed", "operation_committed"].includes(result.phase || result.state)) {
            return { ...result, operation_id: pending.started.operation_id || pending.started.ceremony_id, phase: "operation_committed", next_action: "fresh_authentication" };
          }
        } catch { /* Keep the original challenge when cancellation is unknown. */ }
        if (pendingCeremonies.has(key)) throw unresolved(error);
        throw error;
      }
    }
    return finishCeremony("/native-key", key, pending);
  }
  function forgetAction(actionId) {
    for (const [key, action] of pendingActions) if (action.action_id === actionId) pendingActions.delete(key);
  }
  async function mutation(path, body) {
    try { return await request(path, body); }
    catch (error) {
      const id = body.operation_id;
      if (!id) throw error;
      try {
        const result = await request(`/operations/${encodeURIComponent(id)}`);
        if (["committed", "consumed", "operation_committed"].includes(result.phase || result.state)) return { ...result, operation_id: id, phase: "operation_committed" };
      } catch { /* Keep the original operation available for reconciliation. */ }
      if (!error.status || error.status >= 500 || error.status === 202 || error.status === 401) {
        error.pending_operation_id = id;
        error.next_action = "check_original_operation";
      }
      throw error;
    }
  }
  async function listAll(path, field) {
    const items = [];
    const seen = new Set();
    let cursor = null;
    let summary;
    for (let page = 0; page < 500; page += 1) {
      const result = await request(`${path}${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`);
      summary ||= result;
      if (!Array.isArray(result[field]) || items.length + result[field].length > 50_000) throw new Error("account_list_limit");
      items.push(...result[field]);
      cursor = result.next_cursor;
      if (!cursor) return { ...summary, [field]: items, next_cursor: null };
      if (typeof cursor !== "string" || seen.has(cursor)) throw new Error("account_list_limit");
      seen.add(cursor);
    }
    throw new Error("account_list_limit");
  }
  return {
    clearImportedKey,
    async capabilities() { const result = await request("/capabilities"); csrf = result.csrf_token; return result; },
    session: () => request("/session"),
    operation: (id) => request(`/operations/${encodeURIComponent(id)}`),
    credentials: () => listAll("/credentials", "credentials"),
    sessions: () => listAll("/sessions", "sessions"),
    register: () => ceremony("/passkeys/registration", { intent: "new_account" }, "create"),
    login: () => { clearImportedKey(); pendingActions.clear(); return ceremony("/passkeys/authentication", {}, "get"); },
    async addCredential() { const actionId = await authorize("bind");
      const result = await ceremony("/passkeys/registration", { intent: "bind", action_id: actionId }, "create");
      forgetAction(actionId); return result; },
    async revokeCredential(credential) { const actionId = await authorize("revoke_credential", credential.id);
      const result = await mutation(`/credentials/${encodeURIComponent(credential.id)}/revoke`,
        { action_id: actionId, expected_version: credential.version, operation_id: actionId }); forgetAction(actionId); return result; },
    async revokeSession(session) { const actionId = session.current ? undefined : await authorize("revoke_session", session.id);
      const result = await mutation(`/sessions/${encodeURIComponent(session.id)}/revoke`, { action_id: actionId, operation_id: actionId || session.id }); forgetAction(actionId); return result; },
    async loginImportedKey(key) { const generation = signerGeneration; const result = await nativeCeremony(key, "login"); if (generation === signerGeneration) importedSigner = key; return result; },
    async bindImportedKey(key) { const generation = signerGeneration; const action = await authorize("bind"); const result = await nativeCeremony(key, "bind", action); forgetAction(action); if (generation === signerGeneration) importedSigner = key; return result; },
    useImportedKey(key) { clearImportedKey(); importedSigner = key; },
    async logout() { clearImportedKey(); pendingActions.clear(); return request("/logout", {}); },
    joinWorld: (payload) => request("/world/join", payload),
    refreshWorld: (payload) => request("/world/refresh", payload),
    releaseWorld: (payload) => request("/world/release", payload),
    emailOptions: (email) => request("/email/login/options", { email }),
    emailComplete: (otp, code) => request("/email/login/complete", { otp_id: otp.otp_id, otp_code: code, operation_id: otp.operation_id || otp.otp_id }),
    async emailBindingOptions(email) { const action_id = await authorize("bind_email", undefined, { email });
      return { ...(await request("/email/binding/options", { email, action_id })), action_id }; },
    async emailBindingComplete(otp, code) { const result = await mutation("/email/binding/complete", { otp_id: otp.otp_id, otp_code: code, action_id: otp.action_id, operation_id: otp.operation_id || otp.otp_id });
      forgetAction(otp.action_id); return result; },
    async unbindEmail(credential) { const action_id = await authorize("unbind_email", credential.id);
      const result = await mutation("/email/unbind", { credential_id: credential.id, expected_version: credential.version, action_id, operation_id: action_id }); forgetAction(action_id); return result; },
    async upgradeEmailOptions() { const action = await request("/actions", { action_type: "bind" });
      return { ...(await request(`/actions/${encodeURIComponent(action.action_id)}/email/options`, {})), action_id: action.action_id }; },
    async upgradeEmailComplete(otp, code) {
      if (!otp.authorized) {
        await request(`/actions/${encodeURIComponent(otp.action_id)}/email/complete`, { otp_id: otp.otp_id, otp_code: code }); otp.authorized = true;
      }
      return ceremony("/passkeys/registration", { intent: "bind", action_id: otp.action_id }, "create");
    },
    recoveryEmailOptions: (email, recovery_id) => request("/recovery/eligibility/options", { email, recovery_id }),
    async recoveryEligibility(otp, code, recovery_id) {
      const result = await request("/recovery/eligibility", { otp_id: otp.otp_id, otp_code: code, recovery_id });
      recoveryCancelOperation = result.cancel_operation_id || null;
      return result;
    },
    async createRecovery() {
      if (!recoveryOperation) {
        const candidate = await ceremony("/recovery/candidate", {}, "create");
        if (typeof candidate.operation_id !== "string" || candidate.operation_id.length !== 43) throw new Error("invalid_recovery_operation");
        recoveryOperation = candidate.operation_id;
      }
      return request("/recovery", { operation_id: recoveryOperation });
    },
    recoveryStatus: (id) => request(`/recovery/${encodeURIComponent(id)}`),
    async recoveryDecision(recovery, decision, authenticated) {
      const action_id = authenticated ? await authorize(`recovery_${decision}`, `${recovery.recovery_id}:${recovery.candidate_digest}:${recovery.version}`) : undefined;
      const operation_id = authenticated ? action_id : recoveryCancelOperation;
      if (!operation_id) throw new Error("recovery_reverification_required");
      const result = await request(`/recovery/${encodeURIComponent(recovery.recovery_id)}/${decision}`, { action_id, expected_version: recovery.version, operation_id }); forgetAction(action_id); return result;
    },
    recoveryCompletionOptions: (id) => request(`/recovery/${encodeURIComponent(id)}/completion/options`, {}),
    async recoveryComplete(id, started, code) {
      const key = `recovery-complete:${id}:${started.ceremony_id}`;
      let pending = pendingCeremonies.get(key);
      if (!pending) {
        pending = { started, response: null };
        pendingCeremonies.set(key, pending);
      }
      if (!pending.response) {
        const credential = await credentials.get(browserPasskeyOptions(started.options));
        if (!credential) throw new Error("passkey_cancelled");
        pending.response = serializePasskey(credential);
        pending.payload = { ceremony_id: started.ceremony_id, otp_id: started.otp_id, otp_code: code,
          response: pending.response, operation_id: started.operation_id || started.ceremony_id };
      }
      return finishCeremony(`/recovery/${encodeURIComponent(id)}`, key, pending);
    },
  };
}

const messages = {
  csrf_failed: "安全校验已失效，请刷新页面后重试。",
  origin_rejected: "当前页面来源不被账号服务接受，请从正式入口打开；若仍失败请联系维护者。",
  rate_limited: "请求过于频繁，请稍候再试。",
  invalid_request: "请求格式不符合账号服务要求，请检查输入或刷新页面。",
  clock_unhealthy: "账号服务时钟异常，请稍后重试并联系维护者。",
  ceremony_expired: "认证挑战已过期，请重新发起认证。",
  network_unavailable: "无法连接账号服务，请检查网络后重试。",
  account_response_invalid: "账号服务返回了无法解析的响应，请稍后重试并提供诊断信息。",
  session_expired: "登录会话已过期，请重新登录。",
  session_revoked: "登录会话已退出，请重新登录。",
  account_frozen: "账号已冻结，请使用受支持的恢复或支持入口。",
  native_key_invalid: "请输入各 64 位十六进制的 Ed25519 公钥和私钥种子。",
  native_key_mismatch: "公钥与私钥不匹配，请检查输入。",
  native_key_unsupported: "当前环境或浏览器不支持导入密钥，请使用通行密钥。",
  native_challenge_invalid: "登录挑战与当前服务不一致或已过期，请刷新重试。",
  authentication_failed: "认证失败。请检查密钥是否已绑定且未撤销；未绑定的密钥不会创建账号。",
  credential_conflict: "这把公钥已登记，无法重复绑定或转移到其他账号。",
  NotAllowedError: "操作已取消或超时。账号不会因此丢失，请重新点击。",
  passkey_unsupported: "当前浏览器无法使用通行密钥，请使用支持的浏览器、外部安全密钥或邮箱入口。",
  last_credential_protected: "请用另一把已登记的公钥凭证确认移除，并至少保留一把有效公钥。",
  reauth_required: "请使用已登记的公钥凭证确认这次操作。",
  state_changed: "账号状态已经变化，请刷新后重新确认。",
  registration_closed: "当前暂不开放新账号；已有账号仍可登录。",
  recovery_not_ready: "恢复保护期尚未结束，或通知仍待确认。请核对申请状态与最早完成时间。",
  recovery_expired: "恢复申请已经到期，请重新验证已绑定邮箱。",
  email_unavailable: "邮箱服务暂时不可用；通行密钥登录仍可继续使用。",
  recovery_unavailable: "当前环境未开放邮箱恢复，请使用其他已登记的登录方式。",
  identity_store_unavailable: "账号服务暂时不可用，请稍后重试。",
};

export function accountErrorNotice(error, stage = "账号请求") {
  const code = error.code || error.message;
  const message = messages[code] || messages[error.name] || "操作未完成，请重试或选择其他已登记的登录方式。";
  const safe = value => typeof value === "string" && /^[A-Za-z0-9_-]{1,96}$/.test(value) ? value : null;
  // Never render raw exception/response text, request bodies, query strings or tokens.
  const endpoint = typeof error.endpoint === "string" ? {
    "/capabilities": "能力检查", "/session": "会话确认", "/credentials": "登录方式加载", "/sessions": "会话列表加载",
    "/native-key/options": "创建密钥挑战", "/native-key/complete": "提交密钥认证",
    "/passkeys/authentication/options": "创建通行密钥挑战", "/passkeys/authentication/complete": "提交通行密钥认证",
  }[error.endpoint] : null;
  const details = [safe(code), Number.isInteger(error.status) ? `HTTP ${error.status}` : null,
    safe(error.diagnostic_id) ? `诊断编号 ${error.diagnostic_id}` : null].filter(Boolean);
  const recovery = error.recovery_window_expired ? " 原操作已超出查询保留窗口，结果仍未确定。请刷新页面，先确认当前会话；若未登录，再使用已登记方式重新认证。刷新不会撤销可能已提交的原操作。" : "";
  return `${error.stage || endpoint || stage}：${message}${endpoint ? `（${error.endpoint}）` : ""}${details.length ? ` [${details.join("；")}]` : ""}${recovery}`;
}
const invalidSession = error => ["session_expired", "session_revoked", "authentication_failed", "account_frozen"].includes(error.code);

/** No authenticator UI is opened until an explicit button click. No bearer is persisted. */
export function mountPasskeyAccount(root, { client = createPasskeyAccountClient(), onAuthenticated = () => {},
  onJoinWorld, onSessionInvalidated = () => {} } = {}) {
  let disposed = false;
  let busy = false;
  let generation = 0;
  let account = null;
  let capabilities = {};
  let credentialList = [];
  let sessionList = [];
  let notice = "";
  let managementErrors = {};
  let credentialsLoaded = false;
  let takeover = null;
  let pendingOperation = null;
  const emailRecoveryState = {};
  function element(tag, text) { const node = document.createElement(tag); if (text) node.textContent = text; if (tag === "p") node.style.overflowWrap = "anywhere"; return node; }
  function button(text, action, disabled = false) {
    const node = element("button", text); node.type = "button"; node.disabled = busy || disabled;
    node.addEventListener("click", () => run(action)); return node;
  }
  async function run(action) {
    if (busy || disposed) return;
    busy = true; notice = ""; render();
    try { await action(); await refresh(); }
    catch (error) {
      if (["session_expired", "session_revoked", "account_frozen"].includes(error.code)) invalidate();
      if (error.code === "control_elsewhere") {
        takeover = error.payload;
        notice = "另一设备仍控制此世界。你可以保留原设备控制，或明确选择在此设备继续。";
      } else if (error.next_action === "check_original_operation" || error.payload?.next_action === "check_original_operation" || error.code === "operation_pending") {
        pendingOperation = error.pending_operation_id || error.payload?.operation_id;
        notice = "原操作结果尚未确定，请核对原操作进度；不要重新创建账号或更换恢复候选。";
      } else notice = accountErrorNotice(error);
      if (pendingOperation) notice += ` ${accountErrorNotice(error)}`;
    }
    finally { busy = false; render(); }
  }
  function render() {
    if (disposed) return;
    root.replaceChildren(); root.setAttribute("aria-busy", String(busy));
    root.append(element("h2", account ? "账号与安全" : "进入 Oasis7"));
    const status = element("p", notice || (account ? "账号已登录。进入世界后才会申请玩家资格和控制会话。" : "使用通行密钥，通过指纹、面容或设备解锁创建账号，无需填写邮箱。"));
    status.setAttribute("role", "status"); root.append(status);
    if (pendingOperation) {
      root.append(element("p", `待核对操作：${pendingOperation}`));
      root.append(button("重新确认当前登录会话", () => refresh(true)));
      root.append(button("核对原操作进度", async () => {
        const result = await client.operation(pendingOperation);
        if (["consumed", "committed", "operation_committed"].includes(result.phase || result.state)) {
          pendingOperation = null; notice = "原操作已完成。若当前没有登录会话，请使用已登记的登录方式重新登录。";
        } else if (["cancelled", "expired", "exhausted"].includes(result.phase || result.state)) {
          pendingOperation = null; notice = "原操作已结束，可以重新发起认证。";
        }
      }));
    }
    if (!account) {
      root.append(button("免邮箱创建账号", async () => {
        const result = await client.register();
        if (result.next_action === "fresh_authentication" || result.phase === "operation_committed") {
          pendingOperation = null; notice = "账号已创建。请使用刚登记的通行密钥重新登录，再申请进入世界。";
        }
        await refresh(true);
      }, !capabilities.registration_enabled || !capabilities.passkey?.available));
      root.append(button("使用通行密钥登录", async () => {
        const result = await client.login();
        if (result.next_action === "fresh_authentication" || result.phase === "operation_committed") {
          pendingOperation = null; notice = "原登录操作已完成，但当前浏览器没有登录会话。请重新进行通行密钥认证。";
        }
        await refresh(true);
      }, !capabilities.passkey?.available));
      if (capabilities.native_key?.available) renderImportedKey(root, { account, busy, client, run, isActive: () => !disposed, onAuthenticated: () => refresh(true) });
      renderAccountEmailRecovery(root, { state: emailRecoveryState, account, credentials: credentialList, capabilities, client, run, render });
      return;
    }
    root.append(element("p", account.display_name || "玩家"));
    if (onJoinWorld) root.append(button("进入世界", () => onJoinWorld(account)));
    if (takeover && onJoinWorld) {
      root.append(button("确认在此设备继续，替换原世界控制", async () => {
        await onJoinWorld(account, { takeover: true, expectedEpoch: takeover.world_session_epoch ?? takeover.expected_epoch }); takeover = null;
      }));
      root.append(button("保持原设备控制", () => { takeover = null; }));
    }
    for (const [field, error] of Object.entries(managementErrors)) {
      root.append(element("p", accountErrorNotice(error, field === "credentials" ? "登录方式加载" : "会话列表加载")));
      root.append(button(field === "credentials" ? "重试加载登录方式" : "重试加载会话列表", () => loadManagement(generation, [field])));
    }
    root.append(element("h3", "登录方式"));
    root.append(element("p", "一个通行密钥可能同步到多个设备；下方账号会话才表示各次登录。"));
    for (const credential of credentialList) {
      const row = element("div"); row.append(element("span", credential.label || "通行密钥"));
      row.append(button("移除", () => credential.type === "email" ? client.unbindEmail(credential) : client.revokeCredential(credential))); root.append(row);
    }
    if (credentialList.some((key) => key.state === "active" && ["passkey", "native_key"].includes(key.type))) root.append(button("添加备用通行密钥", () => client.addCredential()));
    if (capabilities.native_key?.available) {
      renderImportedKey(root, { account, busy, client, run, isActive: () => !disposed });
      if (credentialList.some(key => key.state === "active" && key.type === "passkey")) root.append(button("改用通行密钥确认安全操作", () => client.clearImportedKey()));
    }
    if (credentialsLoaded) renderAccountEmailRecovery(root, { state: emailRecoveryState, account, credentials: credentialList, capabilities, client, run, render });
    root.append(element("h3", "已登录会话"));
    for (const session of sessionList) {
      const row = element("div"); row.append(element("span", session.current ? "当前会话" : (session.device_label || "未知设备")));
      row.append(button(session.current ? "退出当前会话" : "退出该会话", () => client.revokeSession(session))); root.append(row);
    }
    root.append(button("退出登录", async () => { await client.logout(); account = null; onSessionInvalidated(); }));
  }
  function invalidate() {
    const hadAccount = account;
    account = null; credentialList = []; sessionList = []; managementErrors = {}; credentialsLoaded = false;
    client.clearImportedKey?.();
    if (hadAccount) onSessionInvalidated();
  }
  async function loadManagement(revision, fields = ["credentials", "sessions"]) {
    await Promise.all(fields.map(async field => {
      try {
        const result = await client[field]();
        if (disposed || revision !== generation || !account) return;
        if (field === "credentials") { credentialList = result.credentials || []; credentialsLoaded = true; }
        else sessionList = result.sessions || [];
        delete managementErrors[field];
      } catch (error) {
        if (disposed || revision !== generation || !account) return;
        if (invalidSession(error)) { invalidate(); notice = accountErrorNotice(error); }
        else {
          if (field === "credentials") { credentialList = []; credentialsLoaded = false; } else sessionList = [];
          managementErrors[field] = error;
        }
      }
      render();
    }));
  }
  async function refresh(notifyAuthenticated = false) {
    const revision = ++generation;
    // Capability failures cannot hide an otherwise valid session.
    const capabilityResult = client.capabilities().then(value => ({ value }), error => ({ error }));
    let nextAccount;
    try { const session = await client.session(); nextAccount = session.account || null; }
    catch (error) {
      if (!invalidSession(error)) throw error;
      nextAccount = null;
      if (!disposed && revision === generation) notice = accountErrorNotice(error, "会话确认");
    }
    if (disposed || revision !== generation) return;
    if (!nextAccount) invalidate();
    else { account = nextAccount; credentialList = []; sessionList = []; managementErrors = {}; credentialsLoaded = false; }
    render();
    if (notifyAuthenticated && account) onAuthenticated(account);
    if (account) void loadManagement(revision);
    const result = await capabilityResult;
    if (disposed || revision !== generation) return;
    if (result.value) capabilities = result.value;
    else notice = accountErrorNotice(result.error, "能力检查");
    render();
  }
  render();
  refresh().catch(error => { if (!disposed) { notice = accountErrorNotice(error, "会话确认"); render(); } });
  return { refresh, dispose() { client.clearImportedKey?.(); disposed = true; ++generation; root.replaceChildren(); } };
}
