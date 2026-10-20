import { describe, it, expect, vi } from "vitest";
import { accountErrorNotice, browserPasskeyOptions, serializePasskey, createPasskeyAccountClient, mountPasskeyAccount } from "./viewer_passkey_account_module.js";

describe("Passkey browser account boundary", () => {
  it("reconciles a committed self-session revoke after its session stops authenticating", async () => {
    const fetch = vi.fn(async (url) => {
      if (url.endsWith("/capabilities")) return { ok: true, json: async () => ({ csrf_token: "csrf" }) };
      if (url.endsWith("/revoke")) return { ok: false, status: 401, json: async () => ({ error_code: "session_revoked" }) };
      return { ok: true, json: async () => ({ phase: "consumed" }) };
    });
    const result = await createPasskeyAccountClient({ fetch }).revokeSession({ id: "original-session", current: true });
    expect(result).toMatchObject({ phase: "operation_committed", operation_id: "original-session" });
    expect(fetch.mock.calls.at(-1)[0]).toBe("/api/public/hosted-account/v2/operations/original-session");
  });
  it("retains an unknown self-revoke operation when receipt lookup is unavailable", async () => {
    const fetch = vi.fn(async (url) => {
      if (url.endsWith("/capabilities")) return { ok: true, json: async () => ({ csrf_token: "csrf" }) };
      throw new TypeError("network unavailable");
    });
    await expect(createPasskeyAccountClient({ fetch }).revokeSession({ id: "original-session", current: true }))
      .rejects.toMatchObject({ pending_operation_id: "original-session", next_action: "check_original_operation" });
  });
  it("decodes all browser binary fields without changing server options", () => {
    const server = { publicKey: { challenge: "AAH_", user: { id: "Ag" }, excludeCredentials: [{ id: "Aw" }] } };
    const browser = browserPasskeyOptions(server);
    expect([...browser.publicKey.challenge]).toEqual([0, 1, 255]);
    expect([...browser.publicKey.user.id]).toEqual([2]);
    expect([...browser.publicKey.excludeCredentials[0].id]).toEqual([3]);
    expect(server.publicKey.challenge).toBe("AAH_");
    expect(() => browserPasskeyOptions({ publicKey: { challenge: "a+" } })).toThrow();
  });
  it("preserves assertion signature, userHandle and clientDataJSON in standard fields", () => {
    const value = serializePasskey({ id: "AQ", type: "public-key", rawId: new Uint8Array([1]), response: {
      clientDataJSON: new Uint8Array([2]), authenticatorData: new Uint8Array([3]), signature: new Uint8Array([4]), userHandle: new Uint8Array([5]),
    } });
    expect(value).toMatchObject({ rawId: "AQ", response: { clientDataJSON: "Ag", authenticatorData: "Aw", signature: "BA", userHandle: "BQ" } });
  });
  it("sends same-origin cookies and action-specific proof before removing a credential", async () => {
    const calls = [];
    const fetch = vi.fn(async (url, init) => {
      calls.push({ url, init });
      const result = url.endsWith("/capabilities") ? { csrf_token: "csrf" }
        : url.endsWith("/actions") ? { action_id: "action-1" }
        : url.endsWith("/options") ? { ceremony_id: "ceremony-1", options: { publicKey: { challenge: "AQ", allowCredentials: [] } } }
        : { ok: true };
      return { ok: true, json: async () => result };
    });
    const get = vi.fn(async () => ({ id: "AQ", rawId: new Uint8Array([1]), type: "public-key", response: {
      clientDataJSON: new Uint8Array([2]), authenticatorData: new Uint8Array([3]), signature: new Uint8Array([4]), userHandle: null,
    } }));
    const client = createPasskeyAccountClient({ fetch, credentials: { get }, randomUUID: () => "op-1" });
    await client.capabilities();
    await client.revokeCredential({ id: "key-1", version: 2 });
    const writes = calls.filter(({ init }) => init.method === "POST");
    expect(writes.map(({ url }) => url)).toEqual([
      "/api/public/hosted-account/v2/actions",
      "/api/public/hosted-account/v2/actions/action-1/reauth/options", "/api/public/hosted-account/v2/actions/action-1/reauth/complete",
      "/api/public/hosted-account/v2/credentials/key-1/revoke",
    ]);
    expect(JSON.parse(writes[0].init.body)).toEqual({ action_type: "revoke_credential", target: "key-1" });
    expect(JSON.parse(writes[3].init.body)).toMatchObject({ action_id: "action-1", expected_version: 2, operation_id: "action-1" });
    expect(calls.every(({ init }) => init.credentials === "same-origin")).toBe(true);
    expect(calls.filter(({ init }) => init.method === "POST").every(({ init }) => init.headers["X-Oasis7-CSRF"] === "csrf")).toBe(true);
    expect(JSON.parse(writes[2].init.body).operation_id).toBe("ceremony-1");
    expect(get).toHaveBeenCalledOnce();
  });
  it("does not summon an authenticator on mount and ignores disposed refreshes", async () => {
    const root = document.createElement("section");
    let resolve;
    const pending = new Promise((done) => { resolve = done; });
    const client = { capabilities: () => pending, session: async () => ({ account: null }), register: vi.fn(), login: vi.fn() };
    const surface = mountPasskeyAccount(root, { client });
    expect(client.register).not.toHaveBeenCalled(); expect(client.login).not.toHaveBeenCalled();
    surface.dispose(); resolve({ registration_open: true });
    await surface.refresh();
    expect(root.childNodes).toHaveLength(0);
  });
  it.each(["passkey", "native_key"])("recognizes %s credentials for backup registration and email gating", async (type) => {
    const root = document.createElement("section");
    const client = {
      capabilities: async () => ({ email: { available: true } }),
      session: async () => ({ account: { display_name: "Player" } }),
      credentials: async () => ({ credentials: [{ id: "passkey-1", label: "This device", state: "active", type }] }),
      sessions: async () => ({ sessions: [] }),
      addCredential: vi.fn(),
      upgradeEmailOptions: vi.fn(),
    };
    const surface = mountPasskeyAccount(root, { client });
    await surface.refresh();

    const labels = [...root.querySelectorAll("button")].map((button) => button.textContent);
    expect(labels).toContain("添加备用通行密钥");
    expect(labels).not.toContain("验证邮箱并添加首个通行密钥");
    surface.dispose();
  });
  it("email completion consumes the original purpose-bound OTP operation in v2", async () => {
    const calls = [];
    const fetch = vi.fn(async (url, init) => {
      calls.push({ url, init });
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "current-cookie-csrf" } : { ok: true } };
    });
    const client = createPasskeyAccountClient({ fetch });
    await client.emailComplete({ otp_id: "original-otp", operation_id: "original-otp" }, "123456");
    expect(calls[1].url).toBe("/api/public/hosted-account/v2/email/login/complete");
    expect(JSON.parse(calls[1].init.body)).toEqual({ otp_id: "original-otp", otp_code: "123456", operation_id: "original-otp" });
    expect(calls[1].init.headers["X-Oasis7-CSRF"]).toBe("current-cookie-csrf");
  });
  it("unknown recovery submission retries the same fixed candidate and operation", async () => {
    const recoveryBodies = [];
    const create = vi.fn(async () => ({ id: "AQ", type: "public-key", rawId: new Uint8Array([1]), response: {
      clientDataJSON: new Uint8Array([2]), attestationObject: new Uint8Array([3]),
    } }));
    const fetch = vi.fn(async (url, init) => {
      if (url.endsWith("/recovery")) {
        recoveryBodies.push(JSON.parse(init.body));
        if (recoveryBodies.length === 1) throw new Error("connection lost after commit");
      }
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "csrf" }
        : url.endsWith("/options") ? { ceremony_id: "C".repeat(43), options: { publicKey: { challenge: "AQ", user: { id: "AQ" } } } }
          : { ok: true, operation_id: "C".repeat(43) } };
    });
    const client = createPasskeyAccountClient({ fetch, credentials: { create } });
    await expect(client.createRecovery()).rejects.toMatchObject({ code: "network_unavailable", endpoint: "/recovery" });
    await client.createRecovery();
    expect(create).toHaveBeenCalledOnce();
    expect(recoveryBodies[0].operation_id).toHaveLength(43);
    expect(recoveryBodies[0].operation_id).toBe("C".repeat(43));
    expect(recoveryBodies[1]).toEqual(recoveryBodies[0]);
  });
  it("lost registration completion reuses the original signed response and returns a safe receipt", async () => {
    let optionsCalls = 0;
    const completeBodies = [];
    const create = vi.fn(async () => ({ id: "AQ", type: "public-key", rawId: new Uint8Array([1]), response: {
      clientDataJSON: new Uint8Array([2]), attestationObject: new Uint8Array([3]),
    } }));
    const fetch = vi.fn(async (url, init) => {
      if (url.endsWith("/registration/options")) optionsCalls += 1;
      if (url.endsWith("/registration/complete")) {
        completeBodies.push(init.body);
        if (completeBodies.length === 1) throw new Error("lost committed response");
      }
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "csrf" }
        : url.endsWith("/options") ? { ceremony_id: "original-ceremony", operation_id: "original-ceremony", options: { publicKey: { challenge: "AQ", user: { id: "AQ" } } } }
          : { ok: true, phase: "operation_committed", account_retained: true, next_action: "fresh_authentication" } };
    });
    const client = createPasskeyAccountClient({ fetch, credentials: { create } });
    await expect(client.register()).rejects.toMatchObject({ pending_operation_id: "original-ceremony", next_action: "check_original_operation" });
    const receipt = await client.register();
    expect(receipt.next_action).toBe("fresh_authentication");
    expect(optionsCalls).toBe(1); expect(create).toHaveBeenCalledOnce();
    expect(completeBodies[1]).toBe(completeBodies[0]);
  });
  it("email unbinding pins the mutation receipt to the authorized action", async () => {
    let unbind;
    const fetch = vi.fn(async (url, init) => {
      if (url.endsWith("/email/unbind")) unbind = JSON.parse(init.body);
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "csrf" }
        : url.endsWith("/actions") ? { action_id: "authorized-unbind" }
          : url.endsWith("/options") ? { ceremony_id: "proof", options: { publicKey: { challenge: "AQ" } } }
            : { ok: true } };
    });
    const get = vi.fn(async () => ({ id: "AQ", type: "public-key", rawId: new Uint8Array([1]), response: {
      clientDataJSON: new Uint8Array([2]), authenticatorData: new Uint8Array([3]), signature: new Uint8Array([4]), userHandle: null,
    } }));
    const client = createPasskeyAccountClient({ fetch, credentials: { get } });
    await client.unbindEmail({ id: "email-factor", version: 7 });
    expect(unbind).toEqual({ credential_id: "email-factor", expected_version: 7, action_id: "authorized-unbind", operation_id: "authorized-unbind" });
  });
  it("loads later active credentials and sessions beyond historical first pages", async () => {
    const urls = [];
    const fetch = vi.fn(async (url) => {
      urls.push(url);
      const field = url.includes("/credentials") ? "credentials" : "sessions";
      const result = url.includes("?cursor=") ? { [field]: [{ id: `${field}-active`, state: "active" }], next_cursor: null }
        : { [field]: Array.from({ length: 100 }, (_, index) => ({ id: `${field}-history-${index}`, state: "revoked" })), next_cursor: "public cursor/+" };
      return { ok: true, json: async () => result };
    });
    const client = createPasskeyAccountClient({ fetch });
    const keys = await client.credentials(); const sessions = await client.sessions();
    expect(keys.credentials).toHaveLength(101); expect(keys.credentials[100].state).toBe("active");
    expect(sessions.sessions).toHaveLength(101); expect(sessions.sessions[100].state).toBe("active");
    expect(urls).toContain("/api/public/hosted-account/v2/credentials?cursor=public%20cursor%2F%2B");
    expect(urls).toContain("/api/public/hosted-account/v2/sessions?cursor=public%20cursor%2F%2B");
  });
  it("revoking the current session uses its stable session ID without a fresh action", async () => {
    const writes = [];
    const fetch = vi.fn(async (url, init) => {
      if (init.method === "POST") writes.push({ url, body: JSON.parse(init.body) });
      return { ok: true, json: async () => ({ ok: true, csrf_token: "csrf" }) };
    });
    const client = createPasskeyAccountClient({ fetch });
    await client.revokeSession({ id: "current-session", current: true });
    expect(writes).toEqual([{ url: "/api/public/hosted-account/v2/sessions/current-session/revoke", body: { operation_id: "current-session" } }]);
  });
  it("applicant cancellation uses the operation issued by verified recovery eligibility", async () => {
    const writes = [];
    const fetch = vi.fn(async (url, init) => {
      if (init.method === "POST") writes.push({ url, body: JSON.parse(init.body) });
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "csrf" }
        : url.endsWith("/eligibility") ? { ok: true, cancel_operation_id: "server-cancel-operation" } : { ok: true } };
    });
    const client = createPasskeyAccountClient({ fetch });
    await client.recoveryEligibility({ otp_id: "verified-otp" }, "123456", "recovery-1");
    await client.recoveryDecision({ recovery_id: "recovery-1", version: 4 }, "cancel", false);
    expect(writes[1]).toEqual({ url: "/api/public/hosted-account/v2/recovery/recovery-1/cancel",
      body: { expected_version: 4, operation_id: "server-cancel-operation" } });
    expect(writes.some(({ url }) => url.endsWith("/actions"))).toBe(false);
  });
});


describe("account login failure isolation", () => {
  const failure = (code) => Object.assign(new Error(code), { code });
  function fixture(overrides = {}) {
    return { capabilities: async () => ({ passkey: { available: true } }),
      session: async () => ({ account: { display_name: "Player" } }),
      credentials: async () => ({ credentials: [] }), sessions: async () => ({ sessions: [] }),
      clearImportedKey: vi.fn(), ...overrides };
  }
  it.each(["credentials", "sessions"])("keeps a verified session when %s fails and offers retry", async (field) => {
    const root = document.createElement("section");
    const client = fixture({ [field]: vi.fn().mockRejectedValue(failure("identity_store_unavailable")) });
    const surface = mountPasskeyAccount(root, { client });
    await expect(surface.refresh()).resolves.toBeUndefined();
    expect(root.textContent).toContain("Player");
    expect(root.textContent).toContain("重试加载");
    client[field].mockResolvedValue({ [field]: [{ id: "restored", label: "Restored key", device_label: "Restored device" }] });
    [...root.querySelectorAll("button")].find(b => b.textContent.includes("重试加载")).click();
    await vi.waitFor(() => expect(root.textContent).toContain(field === "credentials" ? "Restored key" : "Restored device"));
    surface.dispose();
  });
  it("commits session before slow management lists finish", async () => {
    let resolve;
    const lists = new Promise(done => { resolve = done; });
    const root = document.createElement("section");
    const surface = mountPasskeyAccount(root, { client: fixture({ credentials: () => lists }) });
    await vi.waitFor(() => expect(root.textContent).toContain("Player"));
    resolve({ credentials: [] }); await surface.refresh(); surface.dispose();
  });
  it.each(["session", "credentials", "sessions"])("clears authority when %s confirms session revocation", async (field) => {
    const root = document.createElement("section"); const onSessionInvalidated = vi.fn(); const client = fixture();
    const surface = mountPasskeyAccount(root, { client, onSessionInvalidated }); await surface.refresh();
    client[field] = async () => { throw failure("session_revoked"); };
    await surface.refresh();
    expect(root.textContent).not.toContain("Player");
    expect(client.clearImportedKey).toHaveBeenCalled(); expect(onSessionInvalidated).toHaveBeenCalled(); surface.dispose();
  });
  it.each(["csrf_failed", "origin_rejected", "rate_limited", "invalid_request", "clock_unhealthy", "ceremony_expired"])("shows safe diagnostic context for %s", async (code) => {
    const fetch = vi.fn(async (url) => ({ ok: !url.endsWith("/options"), status: 403,
      json: async () => url.endsWith("/capabilities") ? { csrf_token: "SECRET", passkey: { available: true } }
        : url.endsWith("/session") ? { account: null }
          : { error_code: code, diagnostic_id: "diag-123", secret: "DO_NOT_DISPLAY" } }));
    const root = document.createElement("section"); const client = createPasskeyAccountClient({ fetch, credentials: { get: vi.fn() } });
    const surface = mountPasskeyAccount(root, { client }); await surface.refresh();
    [...root.querySelectorAll("button")].find(b => b.textContent === "使用通行密钥登录").click();
    await vi.waitFor(() => expect(root.textContent).toContain(code));
    expect(root.textContent).toContain("diag-123"); expect(root.textContent).toContain("/passkeys/authentication/options");
    expect(root.textContent).not.toContain("DO_NOT_DISPLAY"); expect(root.textContent).not.toContain("SECRET"); surface.dispose();
  });
  it.each(["network", "non-json"])("retains endpoint and status for %s failures without raw response text", async (kind) => {
    const fetch = vi.fn(async () => { if (kind === "network") throw new TypeError("SECRET");
      return { ok: false, status: 502, json: async () => { throw new Error("SECRET"); } }; });
    await expect(createPasskeyAccountClient({ fetch }).session()).rejects.toMatchObject({
      code: kind === "network" ? "network_unavailable" : "account_response_invalid", endpoint: "/session",
      ...(kind === "network" ? {} : { status: 502 }) });
  });
});

describe("native signing interruption", () => {
  it.each(["cancelled", "expired", "consumed", "lost"])("reconciles %s before creating another challenge", async (outcome) => {
    let options = 0; let lookups = 0;
    const fetch = vi.fn(async (url) => {
      if (url.endsWith("/options")) options++;
      if (url.includes("/operations/")) lookups++;
      if (url.endsWith("/cancel") && outcome === "lost") throw new TypeError("lost");
      return { ok: true, json: async () => url.endsWith("/capabilities") ? { csrf_token: "csrf", native_key: { available: true } }
        : url.endsWith("/options") ? { ceremony_id: "original", operation_id: "original" }
          : { phase: outcome === "lost" ? "pending" : outcome } };
    });
    const key = { publicKey: "public", signChallenge: vi.fn().mockRejectedValue(new Error("native_challenge_invalid")) };
    const client = createPasskeyAccountClient({ fetch });
    if (outcome === "consumed") {
      await expect(client.loginImportedKey(key)).resolves.toMatchObject({ phase: "operation_committed", next_action: "fresh_authentication" });
    } else {
      await expect(client.loginImportedKey(key)).rejects.toMatchObject(outcome === "lost"
        ? { pending_operation_id: "original", next_action: "check_original_operation" } : { message: "native_challenge_invalid" });
      await client.loginImportedKey(key).catch(() => {});
      expect(options).toBe(outcome === "lost" ? 1 : 2);
      if (outcome === "lost") { expect(lookups).toBeGreaterThan(0); expect(key.signChallenge).toHaveBeenCalledOnce(); }
    }
  });
});


describe("authentication callback timing", () => {
  it("notifies login before a stalled management request and clears revoked action authority", async () => {
    const root = document.createElement("section"); let account = null; let release;
    const keys = new Promise(resolve => { release = resolve; });
    const authenticated = vi.fn(); const invalidated = vi.fn();
    const client = { capabilities: async () => ({ passkey: { available: true } }),
      session: async () => ({ account }), credentials: () => keys,
      sessions: async () => ({ sessions: [] }), clearImportedKey: vi.fn(),
      login: async () => { account = { display_name: "Player" }; return {}; } };
    const surface = mountPasskeyAccount(root, { client, onAuthenticated: authenticated, onSessionInvalidated: invalidated, onJoinWorld: vi.fn() });
    await surface.refresh();
    [...root.querySelectorAll("button")].find(b => b.textContent === "使用通行密钥登录").click();
    await vi.waitFor(() => expect(authenticated).toHaveBeenCalledWith(account));
    await vi.waitFor(() => expect(root.getAttribute("aria-busy")).toBe("false"));
    expect([...root.querySelectorAll("button")].find(b => b.textContent === "进入世界").disabled).toBe(false);
    release({ credentials: [] });
    client.logout = async () => { throw Object.assign(new Error("session_revoked"), { code: "session_revoked" }); };
    [...root.querySelectorAll("button")].find(b => b.textContent === "退出登录").click();
    await vi.waitFor(() => expect(invalidated).toHaveBeenCalled());
    expect(root.textContent).not.toContain("Player"); surface.dispose();
  });
});


it("explains recovery beyond retention without allocating another unknown challenge", async () => {
  let options = 0;
  const fetch = async url => {
    if (url.endsWith("/capabilities")) return { ok: true, json: async () => ({ native_key: { available: true } }) };
    if (url.endsWith("/options")) { options++; return { ok: true, json: async () => ({ ceremony_id: "original", operation_id: "original", expires_at: Date.now() - 700_000 }) }; }
    return { ok: false, status: 401, json: async () => ({ error_code: "authentication_failed" }) };
  };
  const client = createPasskeyAccountClient({ fetch });
  const key = { publicKey: "public", signChallenge: async () => { throw new Error("native_challenge_invalid"); } };
  await client.loginImportedKey(key).catch(() => {});
  const error = await client.loginImportedKey(key).catch(error => error);
  expect(error).toMatchObject({ pending_operation_id: "original", recovery_window_expired: true });
  expect(options).toBe(1);
  expect(accountErrorNotice(error)).toContain("请刷新页面，先确认当前会话");
  expect(accountErrorNotice(error)).toContain("结果仍未确定");
});
