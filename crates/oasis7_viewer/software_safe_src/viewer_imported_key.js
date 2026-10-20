// Imported account keys stay in this page's memory, without the gameplay key cache.
const encoder = new TextEncoder();
const prefix = Uint8Array.from([48,46,2,1,0,48,5,6,3,43,101,112,4,34,4,32]);
function hex(value) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/i.test(value)) throw new Error("native_key_invalid");
  return Uint8Array.from(value.match(/../g), byte => parseInt(byte, 16));
}
function decode(value) {
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]+$/.test(value)) throw new Error("native_challenge_invalid");
  return Uint8Array.from(atob(value.replace(/-/g, "+").replace(/_/g, "/")), c => c.charCodeAt(0));
}
function encode(value) { return btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, ""); }
export function nativeSigningBytes(options) {
  const chunks = [encoder.encode("oasis7.identity.native.v1\0"), Uint8Array.of(1)];
  for (const value of [options.identity_service_id, options.environment_id, options.audience, options.purpose, options.account_context, options.operation_id]) {
    if (typeof value !== "string" || value.length > 4096) throw new Error("native_challenge_invalid");
    const bytes = encoder.encode(value); const size = new Uint8Array(4); new DataView(size.buffer).setUint32(0, bytes.length);
    chunks.push(size, bytes);
  }
  const nonce = decode(options.nonce); if (nonce.length !== 32) throw new Error("native_challenge_invalid");
  chunks.push(hex(options.public_key), nonce);
  for (const value of [options.issued_at, options.expires_at]) {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error("native_challenge_invalid");
    const bytes = new Uint8Array(8); new DataView(bytes.buffer).setBigUint64(0, BigInt(value)); chunks.push(bytes);
  }
  const bytes = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0)); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}
export async function importAccountKey(publicHex, privateHex, crypto = globalThis.crypto) {
  const publicBytes = hex(publicHex.trim()); const seed = hex(privateHex.trim()); privateHex = "";
  if (!crypto?.subtle) throw new Error("native_key_unsupported");
  const pkcs8 = new Uint8Array(prefix.length + seed.length); pkcs8.set(prefix); pkcs8.set(seed, prefix.length);
  let key;
  try { key = await crypto.subtle.importKey("pkcs8", pkcs8, "Ed25519", false, ["sign"]); }
  catch { throw new Error("native_key_unsupported"); }
  finally { seed.fill(0); pkcs8.fill(0); }
  const verify = await crypto.subtle.importKey("raw", publicBytes, "Ed25519", false, ["verify"]);
  const proof = crypto.getRandomValues(new Uint8Array(32));
  if (!await crypto.subtle.verify("Ed25519", verify, await crypto.subtle.sign("Ed25519", key, proof), proof)) throw new Error("native_key_mismatch");
  return Object.freeze({ publicKey: publicHex.trim().toLowerCase(), async signChallenge(started, capability, intent, origin = globalThis.location.origin) {
    const o = started.options;
    if (!o || o.identity_service_id !== capability.identity_service_id || o.environment_id !== capability.environment_id || o.audience !== origin || o.purpose !== intent || o.public_key !== this.publicKey || o.operation_id !== started.ceremony_id || started.operation_id !== started.ceremony_id || !/^[A-Za-z0-9_-]{43}$/.test(started.ceremony_id) || (intent === "login" ? o.account_context !== "" : !o.account_context) || o.expires_at !== started.expires_at || o.expires_at <= Date.now() || o.issued_at > Date.now() + 5000 || o.expires_at - o.issued_at !== 120000) throw new Error("native_challenge_invalid");
    const bytes = nativeSigningBytes(o);
    if (encode(bytes) !== o.signed_bytes) throw new Error("native_challenge_invalid");
    return encode(await crypto.subtle.sign("Ed25519", key, bytes));
  } });
}

export function renderImportedKey(root, { account, busy, client, run, onAuthenticated, isActive = () => true }) {
  const disclosure = document.createElement("details");
  const summary = document.createElement("summary"); summary.textContent = account ? "管理账号密钥" : "使用已绑定密钥登录"; disclosure.append(summary);
  const section = document.createElement("fieldset"); section.disabled = busy; section.className = "stack";
  const legend = document.createElement("legend"); legend.textContent = account ? "导入账号密钥" : "导入密钥登录"; section.append(legend);
  const hint = document.createElement("p"); hint.textContent = "仅支持已绑定的 Ed25519 账号密钥。私钥仅用于本页内存签名，退出或关闭页面后需重新导入。请勿输入钱包或节点私钥；此方式不具备通行密钥的抗钓鱼保护。"; section.append(hint);
  function input(label, type) { const row = document.createElement("label"); row.textContent = label; row.className = "stack"; const node = document.createElement("input"); node.type = type; node.autocomplete = "off"; node.spellcheck = false; node.maxLength = 64; node.placeholder = "64 位十六进制"; row.append(node); section.append(row); return node; }
  const pub = input("公钥", "text"); const secret = input("私钥种子", "password");
  function action(label, operation) {
    const button = document.createElement("button"); button.type = "button"; button.textContent = label;
    button.addEventListener("click", () => {
      const publicHex = pub.value; let privateHex = secret.value; secret.value = ""; pub.value = "";
      // Import starts only on the explicit click; do not retain plaintext in render state.
      const signer = importAccountKey(publicHex, privateHex); privateHex = "";
      run(async () => { let key;
        try { key = await signer; } catch (error) { error.stage = "本地密钥导入"; throw error; }
        if (isActive()) await operation(key); });
    }); section.append(button);
  }
  if (account) {
    action("绑定这把密钥到当前账号", key => client.bindImportedKey(key));
    action("加载已绑定密钥用于安全确认", key => client.useImportedKey(key));
  } else action("使用导入密钥登录", async key => { await client.loginImportedKey(key); await onAuthenticated(); });
  disclosure.append(section); root.append(disclosure);
}
