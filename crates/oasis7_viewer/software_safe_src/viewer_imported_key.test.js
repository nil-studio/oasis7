import { describe, it, expect, vi } from "vitest";
import { createPasskeyAccountClient } from "./viewer_passkey_account_module.js";
import { webcrypto } from "node:crypto";
import { importAccountKey, nativeSigningBytes, renderImportedKey } from "./viewer_imported_key.js";
const seed = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const pub = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const encode = bytes => Buffer.from(bytes).toString("base64url");
const capability = { identity_service_id:"native-test", environment_id:"test" };
function challenge() {
  const now = Date.now(); const id = "a".repeat(43);
  const options = { ...capability, audience:"https://localhost", purpose:"login", account_context:"", operation_id:id, public_key:pub, nonce:encode(new Uint8Array(32).fill(2)), issued_at:now, expires_at:now + 120000 };
  options.signed_bytes = encode(nativeSigningBytes(options));
  return { ceremony_id:id, operation_id:id, expires_at:options.expires_at, options };
}
describe("imported account keys", () => {
  it("matches the backend protocol vector", () => {
    expect(encode(nativeSigningBytes({ ...capability, audience:"https://localhost", purpose:"login", account_context:"", operation_id:"op", public_key:"01".repeat(32), nonce:encode(new Uint8Array(32).fill(2)), issued_at:100, expires_at:120100 }))).toBe("b2FzaXM3LmlkZW50aXR5Lm5hdGl2ZS52MQABAAAAC25hdGl2ZS10ZXN0AAAABHRlc3QAAAARaHR0cHM6Ly9sb2NhbGhvc3QAAAAFbG9naW4AAAAAAAAAAm9wAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQECAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAABkAAAAAAAB1SQ");
  });
  it("signs with a real imported key and rejects mismatched pairs", async () => {
    const signer = await importAccountKey(pub, seed, webcrypto); const started = challenge();
    const signature = await signer.signChallenge(started, capability, "login", "https://localhost");
    const key = await webcrypto.subtle.importKey("raw", Buffer.from(pub,"hex"), "Ed25519", false, ["verify"]);
    expect(await webcrypto.subtle.verify("Ed25519", key, Buffer.from(signature,"base64url"), nativeSigningBytes(started.options))).toBe(true);
    await expect(importAccountKey(pub, "01".repeat(32), webcrypto)).rejects.toThrow("native_key_mismatch");
    await expect(importAccountKey(pub, "bad", webcrypto)).rejects.toThrow("native_key_invalid");
  });
  it("refuses altered bytes, services, purposes, origins and expired challenges", async () => {
    const signer = await importAccountKey(pub, seed, webcrypto);
    for (const patch of [{signed_bytes:"bad"}, {identity_service_id:"evil"}, {environment_id:"evil"}, {audience:"https://evil"}, {purpose:"bind"}, {public_key:"01".repeat(32)}, {expires_at:1}]) {
      const started = challenge(); Object.assign(started.options, patch);
      await expect(signer.signChallenge(started, capability, "login", "https://localhost")).rejects.toThrow("native_challenge_invalid");
    }
  });
  it("does not restore the signer when a login completes after clearing it", async () => {
    let resolve; const response = new Promise(done => {resolve = done;});
    const signer = {publicKey:pub, signChallenge:vi.fn(async () => "signature")};
    const fetch = vi.fn(async url => {
      const value = url.endsWith("/capabilities") ? {csrf_token:"csrf",native_key:{available:true,...capability}}
        : url.endsWith("/native-key/options") ? challenge()
        : url.endsWith("/native-key/complete") ? await response : {action_id:"a"};
      return {ok:true,json:async () => value};
    });
    const client = createPasskeyAccountClient({fetch,credentials:{}});
    const login = client.loginImportedKey(signer);
    await vi.waitFor(() => expect(fetch.mock.calls.some(([url]) => url.endsWith("/native-key/complete"))).toBe(true));
    client.clearImportedKey(); resolve({ok:true}); await login;
    await expect(client.addCredential()).rejects.toThrow("passkey_unsupported");
    expect(signer.signChallenge).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls.filter(([,init]) => init.body).every(([,init]) => !init.body.includes(seed))).toBe(true);
  });
  it("requests a new passkey proof after switching away from a rejected native proof", async () => {
    let actions = 0; let revokes = 0;
    const get = vi.fn(async () => ({id:"AQ",rawId:new Uint8Array([1]),response:{clientDataJSON:new Uint8Array([2]),authenticatorData:new Uint8Array([3]),signature:new Uint8Array([4])}}));
    const fetch = vi.fn(async url => {
      let ok = true;
      const value = url.endsWith("/capabilities") ? {csrf_token:"csrf",native_key:{available:true,...capability}}
        : url.endsWith("/actions") ? {action_id:`action-${++actions}`}
        : url.endsWith("/native-key/options") ? challenge()
        : url.endsWith("/reauth/options") ? {ceremony_id:"ceremony",options:{publicKey:{challenge:"AQ"}}}
        : url.endsWith("/revoke") && ++revokes === 1 ? (ok = false, {error_code:"last_credential_protected"})
        : url.includes("/operations/") ? {phase:"pending"} : {ok:true};
      return {ok,status:ok ? 200 : 409,json:async () => value};
    });
    const client = createPasskeyAccountClient({fetch,credentials:{get}});
    client.useImportedKey({publicKey:pub,signChallenge:async () => "signature"});
    await expect(client.revokeCredential({id:"native",version:1})).rejects.toMatchObject({code:"last_credential_protected"});
    client.clearImportedKey(); await client.revokeCredential({id:"native",version:1});
    expect(get).toHaveBeenCalledTimes(1); expect(actions).toBe(2);
  });
  it("keeps key import secondary and retains safety guidance when expanded", () => {
    const root = document.createElement("div");
    renderImportedKey(root, { account: null, busy: false, client: {}, run: vi.fn() });
    const disclosure = root.querySelector("details");
    expect(disclosure.open).toBe(false);
    expect(disclosure.querySelector("summary").textContent).toBe("使用已绑定密钥登录");
    disclosure.open = true;
    expect(disclosure.textContent).toContain("请勿输入钱包或节点私钥");
    expect(disclosure.querySelectorAll("input")).toHaveLength(2);
  });
  it("clears the password field immediately on submission", async () => {
    const root = document.createElement("div"); let pending;
    renderImportedKey(root, {account:null,busy:false,client:{},run:action => { pending = action().catch(() => {}); },onAuthenticated:() => {}});
    const inputs = root.querySelectorAll("input"); inputs[0].value = pub; inputs[1].value = "bad";
    expect(inputs[1].type).toBe("password"); root.querySelector("button").click(); expect(inputs[1].value).toBe(""); await pending;
  });
});
