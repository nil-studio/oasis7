// Short-lived world key custody only; account authentication remains HttpOnly.
const storageKey = "oasis7.hosted-world-recovery.v1";
const bytes = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
const base64 = value => btoa(String.fromCharCode(...new Uint8Array(value)));
function database() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("oasis7-world-recovery-v1", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("keys");
    request.onsuccess = () => resolve(request.result);
    request.onerror = request.onblocked = () => reject(new Error("world_recovery_unavailable"));
  });
}
async function keyOperation(id, operation, value) {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction("keys", operation === "get" ? "readonly" : "readwrite");
      const store = transaction.objectStore("keys");
      const request = operation === "get" ? store.get(id) : operation === "put" ? store.put(value, id) : store.delete(id);
      transaction.oncomplete = () => resolve(request.result);
      transaction.onerror = transaction.onabort = () => reject(new Error("world_recovery_unavailable"));
    });
  } finally { db.close(); }
}
export function createHostedWorldRecovery({ keyStore = keyOperation } = {}) {
  let heldId = null, unlock = null, generation = 0;
  let operations = Promise.resolve();
  function schedule(operation) {
    const task = operations.then(operation);
    operations = task.catch(() => {});
    return task;
  }
  const supported = () => Boolean(globalThis.indexedDB && globalThis.navigator?.locks && globalThis.crypto?.subtle);
  async function acquire(id) {
    if (heldId === id) return;
    if (heldId) throw new Error("world_recovery_context_changed");
    await new Promise((resolve, reject) => {
      navigator.locks.request(`oasis7-world-recovery:${id}`, { ifAvailable: true }, async lock => {
        if (!lock) { reject(Object.assign(new Error("world_recovery_context_in_use"), { code: "world_recovery_context_in_use" })); return; }
        heldId = id;
        await new Promise(release => { unlock = release; resolve(); });
      }).catch(reject);
    });
  }
  async function load(accountId) {
    if (!supported()) return null;
    const stored = sessionStorage.getItem(storageKey);
    if (!stored) return null;
    let capsule;
    try { capsule = JSON.parse(stored); } catch { throw new Error("world_recovery_unavailable"); }
    await acquire(capsule.id);
    if (capsule.accountId !== accountId) { generation++; await clear(); return null; }
    try {
      const key = await keyStore(capsule.id, "get");
      if (!key || key.extractable) throw new Error("world_recovery_unavailable");
      const plaintext = await crypto.subtle.decrypt({ name: "AES-GCM", iv: bytes(capsule.iv), additionalData: new TextEncoder().encode(accountId) }, key, bytes(capsule.ciphertext));
      const candidate = JSON.parse(new TextDecoder().decode(plaintext));
      if (candidate.accountId !== accountId || !candidate.attempt || !candidate.keypair?.privateKey) throw new Error("world_recovery_unavailable");
      return candidate;
    } catch { throw new Error("world_recovery_unavailable"); }
  }
  async function save(candidate, revision) {
    if (generation !== revision) return false;
    if (!supported()) return false;
    const id = heldId || crypto.randomUUID();
    await acquire(id);
    let key = await keyStore(id, "get");
    if (!key) { key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]); await keyStore(id, "put", key); }
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: new TextEncoder().encode(candidate.accountId) }, key, new TextEncoder().encode(JSON.stringify(candidate)));
    if (generation !== revision || heldId !== id) return false;
    sessionStorage.setItem(storageKey, JSON.stringify({ id, accountId: candidate.accountId, iv: base64(iv), ciphertext: base64(ciphertext) }));
    return true;
  }
  async function clear() {
    try {
      const stored = sessionStorage.getItem(storageKey);
      sessionStorage.removeItem(storageKey);
      let capsule = null;
      try { capsule = JSON.parse(stored || "null"); } catch { /* Explicit logout clears corrupt capsules too. */ }
      const id = heldId || capsule?.id;
      if (id && supported()) { await acquire(id); await keyStore(id, "delete"); }
    } finally { detach(); }
  }
  function detach() { unlock?.(); unlock = null; heldId = null; }
  globalThis.window?.addEventListener("pagehide", event => { if (!event.persisted) { generation++; detach(); } });
  return {
    load: accountId => schedule(() => load(accountId)),
    save: candidate => { const revision = generation; return schedule(() => save(candidate, revision)); },
    clear: () => { generation++; return schedule(clear); },
  };
}
