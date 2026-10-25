export class BrowserGameStorageError extends Error {
  constructor(code) { super(code); this.name = "BrowserGameStorageError"; this.code = code; }
}
export const gameStorageFail = (code) => { throw new BrowserGameStorageError(code); };

// A transaction only runs synchronous local mutations. No network or crypto await
// is allowed inside it; IDB commit must succeed before its result is published.
export function createBrowserGameDatabase(indexedDB = globalThis.indexedDB) {
  let opening;
  const open = () => {
    if (!indexedDB?.open) return Promise.reject(new BrowserGameStorageError("indexeddb_unavailable"));
    if (!opening) opening = new Promise((resolve, reject) => {
      let request;
      try { request = indexedDB.open("oasis7-browser-game-v1", 1); }
      catch { reject(new BrowserGameStorageError("indexeddb_unavailable")); return; }
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains("records")) request.result.createObjectStore("records");
      };
      request.onsuccess = () => { const db = request.result;
        db.onversionchange = () => { db.close(); opening = undefined; };
        resolve(db);
      };
      request.onerror = () => reject(new BrowserGameStorageError("indexeddb_unavailable"));
      request.onblocked = () => reject(new BrowserGameStorageError("indexeddb_blocked"));
    });
    return opening;
  };
  const transaction = async (key, mutate) => {
    const db = await open();
    return new Promise((resolve, reject) => {
      let tx; let result; let failure;
      try {
        tx = db.transaction("records", mutate ? "readwrite" : "readonly");
        const store = tx.objectStore("records");
        const request = store.get(key);
        request.onsuccess = () => {
          try {
            result = mutate ? mutate(request.result) : request.result;
            if (mutate) store.put(result, key);
          } catch (error) { failure = error; tx.abort(); }
        };
        tx.oncomplete = () => resolve(result);
        tx.onabort = tx.onerror = () => reject(failure instanceof BrowserGameStorageError
          ? failure : new BrowserGameStorageError("indexeddb_transaction_failed"));
      } catch { reject(new BrowserGameStorageError("indexeddb_transaction_failed")); }
    });
  };
  return { get: (key) => transaction(key), update: (key, mutate) => transaction(key, mutate) };
}

export function browserIdentityScope(scope) {
  const fields = [scope?.worldId, scope?.genesisDigest, scope?.accountId, scope?.agentId];
  if (fields.some((value) => typeof value !== "string" || !value.trim() || value.length > 512)) {
    gameStorageFail("identity_scope_invalid");
  }
  return JSON.stringify(fields);
}
const hex = (bytes) => Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");

export function createBrowserExecutorIdentity({ indexedDB, storage,
  cryptoImpl = globalThis.crypto } = {}) {
  const database = storage ?? createBrowserGameDatabase(indexedDB);
  async function validate(record, scopeKey) {
    if (record?.schema !== 1 || record.scopeKey !== scopeKey
      || !/^[0-9a-f]{64}$/.test(record.publicKeyHex)
      || record.privateKey?.type !== "private" || record.privateKey.extractable !== false
      || record.privateKey.algorithm?.name !== "Ed25519"
      || record.privateKey.usages?.length !== 1 || record.privateKey.usages[0] !== "sign") {
      gameStorageFail("executor_identity_corrupt");
    }
    // A persisted public/private pair must still correspond after reload.
    try {
      const publicBytes = Uint8Array.from(record.publicKeyHex.match(/../g), (part) => parseInt(part, 16));
      const publicKey = await cryptoImpl.subtle.importKey("raw", publicBytes, "Ed25519", false, ["verify"]);
      const challenge = new TextEncoder().encode(`oasis7/browser-executor-storage/v1:${scopeKey}`);
      const signature = await cryptoImpl.subtle.sign("Ed25519", record.privateKey, challenge);
      if (!await cryptoImpl.subtle.verify("Ed25519", publicKey, signature, challenge)) gameStorageFail("executor_identity_corrupt");
    } catch { gameStorageFail("executor_identity_corrupt"); }
    return Object.freeze({ publicKeyHex: record.publicKeyHex,
      async sign(bytes) {
        if (!(bytes instanceof Uint8Array)) gameStorageFail("signing_bytes_invalid");
        try { return new Uint8Array(await cryptoImpl.subtle.sign("Ed25519", record.privateKey, bytes)); }
        catch { gameStorageFail("executor_signing_failed"); }
      } });
  }
  return {
    async getOrCreate(scope) {
      const scopeKey = browserIdentityScope(scope);
      if (!cryptoImpl?.subtle) gameStorageFail("webcrypto_unavailable");
      const key = `executor:${scopeKey}`;
      const existing = await database.get(key);
      if (existing !== undefined) return validate(existing, scopeKey);
      let candidate;
      try {
        const pair = await cryptoImpl.subtle.generateKey("Ed25519", false, ["sign", "verify"]);
        candidate = { schema: 1, scopeKey, privateKey: pair.privateKey,
          publicKeyHex: hex(new Uint8Array(await cryptoImpl.subtle.exportKey("raw", pair.publicKey))) };
      } catch { gameStorageFail("executor_crypto_unsupported"); }
      const winner = await database.update(key, (stored) => stored === undefined ? candidate : stored);
      return validate(winner, scopeKey);
    },
  };
}
