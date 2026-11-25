const HANDOFF_SCHEMA = "oasis7.viewer.race-handoff/v1";
const CHANNEL_PREFIX = "oasis7.viewer.race-handoff.v1";
const DEFAULT_TTL_MS = 30_000;

function isEnabledFlag(searchParams, name) {
  const value = String(searchParams.get(name) || "").trim().toLowerCase();
  return value === "1" || value === "true" || value === "yes" || value === "on";
}

function isLoopbackHostname(hostname) {
  const host = String(hostname || "").trim().toLowerCase().replace(/^\[|\]$/g, "");
  return host === "localhost"
    || host === "::1"
    || /^127(?:\.\d{1,3}){3}$/.test(host);
}

function handoffError(code) {
  const error = new Error(`browser race handoff ${code}`);
  error.code = code;
  return error;
}

function asPositiveTtl(value) {
  const ttl = Number(value);
  return Number.isFinite(ttl) && ttl > 0 ? ttl : DEFAULT_TTL_MS;
}

function randomToken(cryptoRef) {
  if (typeof cryptoRef?.randomUUID === "function") {
    return String(cryptoRef.randomUUID());
  }
  if (typeof cryptoRef?.getRandomValues === "function") {
    const bytes = new Uint8Array(24);
    cryptoRef.getRandomValues(bytes);
    return Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  }
  throw handoffError("randomness_unavailable");
}

function closeChannel(channel) {
  try {
    channel?.close?.();
  } catch (_) {
  }
}

export function createViewerBrowserRaceHandoffModule({
  BroadcastChannelImpl = globalThis.BroadcastChannel,
  clearTimeoutImpl = globalThis.clearTimeout,
  cryptoRef = globalThis.crypto,
  locationRef = globalThis.location,
  now = () => Date.now(),
  setTimeoutImpl = globalThis.setTimeout,
} = {}) {
  const locationOrigin = String(locationRef?.origin || "").trim();
  const searchParams = new URLSearchParams(String(locationRef?.search || ""));
  const enabled = Boolean(
    locationOrigin
      && isLoopbackHostname(locationRef?.hostname)
      && isEnabledFlag(searchParams, "test_api")
      && isEnabledFlag(searchParams, "hosted_test_login")
      && typeof BroadcastChannelImpl === "function"
      && (typeof cryptoRef?.randomUUID === "function" || typeof cryptoRef?.getRandomValues === "function"),
  );
  const activeEntries = new Set();
  const consumedDescriptors = new Set();
  let disposed = false;

  function requireEnabled() {
    if (disposed) {
      throw handoffError("disposed");
    }
    if (!enabled) {
      throw handoffError("disabled");
    }
  }

  function makeDescriptor(ttlMs) {
    const nonce = randomToken(cryptoRef);
    const channelToken = randomToken(cryptoRef);
    const expiresAt = Number(now()) + asPositiveTtl(ttlMs);
    return Object.freeze({
      channelName: `${CHANNEL_PREFIX}:${channelToken}`,
      expiresAt,
      nonce,
      origin: locationOrigin,
      schema: HANDOFF_SCHEMA,
    });
  }

  function validateDescriptor(descriptor) {
    if (!descriptor || descriptor.schema !== HANDOFF_SCHEMA) {
      throw handoffError("schema_mismatch");
    }
    if (descriptor.origin !== locationOrigin) {
      throw handoffError("origin_mismatch");
    }
    if (typeof descriptor.channelName !== "string" || !descriptor.channelName.startsWith(`${CHANNEL_PREFIX}:`)) {
      throw handoffError("channel_mismatch");
    }
    if (typeof descriptor.nonce !== "string" || descriptor.nonce.length < 16) {
      throw handoffError("nonce_mismatch");
    }
    if (!Number.isFinite(Number(descriptor.expiresAt))) {
      throw handoffError("expiry_invalid");
    }
    if (Number(now()) >= Number(descriptor.expiresAt)) {
      throw handoffError("expired");
    }
  }

  function makeEnvelope(descriptor, type, fields = {}) {
    return {
      channelName: descriptor.channelName,
      expiresAt: descriptor.expiresAt,
      nonce: descriptor.nonce,
      origin: locationOrigin,
      schema: HANDOFF_SCHEMA,
      type,
      ...fields,
    };
  }

  function settleEntry(entry, kind, value) {
    if (entry.settled) {
      return;
    }
    entry.settled = true;
    activeEntries.delete(entry);
    if (entry.timer != null && typeof clearTimeoutImpl === "function") {
      clearTimeoutImpl(entry.timer);
    }
    if (kind === "resolve") {
      entry.resolve(value);
    } else {
      entry.reject(value);
    }
  }

  function rejectEntry(entry, code) {
    settleEntry(entry, "reject", handoffError(code));
    closeChannel(entry.channel);
  }

  function scheduleExpiry(entry, delayMs) {
    if (typeof setTimeoutImpl !== "function") {
      return;
    }
    entry.timer = setTimeoutImpl(() => {
      if (!entry.settled) {
        entry.consumed = true;
        entry.keyMaterial = null;
        rejectEntry(entry, "expired");
      }
    }, Math.max(0, delayMs));
  }

  function offerKeyMaterial({ privateKey, publicKey, releaseToken, playerId, sessionEpoch, bindingEpoch, boundAgentId, authorityEpoch, ttlMs } = {}) {
    requireEnabled();
    if (!String(privateKey || "").trim() || !String(publicKey || "").trim() || !String(releaseToken || "").trim() || !String(playerId || "").trim() || sessionEpoch == null) {
      throw handoffError("key_material_missing");
    }
    const descriptor = makeDescriptor(ttlMs);
    const channel = new BroadcastChannelImpl(descriptor.channelName);
    const entry = {
      channel,
      consumed: false,
      descriptor,
      disposed: false,
      keyMaterial: {
        privateKey: String(privateKey),
        publicKey: String(publicKey),
        releaseToken: String(releaseToken),
        playerId: String(playerId),
        sessionEpoch: Number(sessionEpoch),
        bindingEpoch: bindingEpoch == null ? null : Number(bindingEpoch),
        boundAgentId: String(boundAgentId || "") || null,
        authorityEpoch: String(authorityEpoch || "") || null,
      },
      settled: false,
      timer: null,
    };
    let resolveClaim;
    let rejectClaim;
    const claimed = new Promise((resolve, reject) => {
      resolveClaim = resolve;
      rejectClaim = reject;
    });
    // An offer may legitimately expire without a caller awaiting the optional
    // claim receipt. Mark that rejection as observed while preserving the
    // original promise for callers that do await `waitForClaim()`.
    void claimed.catch(() => {});
    entry.resolve = resolveClaim;
    entry.reject = rejectClaim;
    activeEntries.add(entry);
    channel.onmessage = ({ data }) => {
      if (entry.disposed || !data || data.schema !== HANDOFF_SCHEMA || data.type !== "claim") {
        return;
      }
      const rejectClaimMessage = (code) => {
        try {
          channel.postMessage(makeEnvelope(descriptor, "reject", {
            claimNonce: data.claimNonce || null,
            code,
          }));
        } catch (_) {
        }
      };
      if (data.origin !== locationOrigin) {
        rejectClaimMessage("origin_mismatch");
        return;
      }
      if (data.channelName !== descriptor.channelName) {
        rejectClaimMessage("channel_mismatch");
        return;
      }
      if (data.nonce !== descriptor.nonce) {
        rejectClaimMessage("nonce_mismatch");
        return;
      }
      if (Number(now()) >= Number(descriptor.expiresAt)) {
        entry.consumed = true;
        entry.keyMaterial = null;
        rejectClaimMessage("expired");
        rejectEntry(entry, "expired");
        return;
      }
      if (entry.consumed) {
        rejectClaimMessage("replay");
        return;
      }
      if (typeof data.claimNonce !== "string" || data.claimNonce.length < 16) {
        rejectClaimMessage("claim_nonce_invalid");
        return;
      }
      entry.consumed = true;
      const keyMaterial = entry.keyMaterial;
      entry.keyMaterial = null;
      channel.postMessage(makeEnvelope(descriptor, "offer", {
        claimNonce: data.claimNonce,
        keyMaterial,
      }));
      settleEntry(entry, "resolve", { claimNonce: data.claimNonce });
      closeChannel(channel);
    };
    scheduleExpiry(entry, Number(descriptor.expiresAt) - Number(now()));
    return {
      descriptor,
      get disposed() {
        return entry.disposed;
      },
      dispose() {
        if (entry.disposed) {
          return;
        }
        entry.disposed = true;
        entry.keyMaterial = null;
        rejectEntry(entry, "disposed");
      },
      waitForClaim() {
        return claimed;
      },
    };
  }

  function claimOffer(descriptor) {
    if (disposed) {
      return Promise.reject(handoffError("disposed"));
    }
    try {
      requireEnabled();
      validateDescriptor(descriptor);
    } catch (error) {
      return Promise.reject(error);
    }
    const descriptorKey = `${descriptor.origin}|${descriptor.channelName}|${descriptor.nonce}`;
    if (consumedDescriptors.has(descriptorKey)) {
      return Promise.reject(handoffError("replay"));
    }
    const channel = new BroadcastChannelImpl(descriptor.channelName);
    const claimNonce = randomToken(cryptoRef);
    const entry = {
      channel,
      disposed: false,
      reject: null,
      resolve: null,
      settled: false,
      timer: null,
    };
    const claimed = new Promise((resolve, reject) => {
      entry.resolve = resolve;
      entry.reject = reject;
    });
    activeEntries.add(entry);
    channel.onmessage = ({ data }) => {
      if (entry.disposed || !data || data.schema !== HANDOFF_SCHEMA) {
        return;
      }
      if (data.origin !== locationOrigin || data.channelName !== descriptor.channelName || data.nonce !== descriptor.nonce) {
        return;
      }
      if (data.claimNonce !== claimNonce) {
        return;
      }
      if (data.type === "reject") {
        rejectEntry(entry, data.code || "rejected");
        return;
      }
      if (data.type !== "offer" || !data.keyMaterial) {
        rejectEntry(entry, "key_material_missing");
        return;
      }
      if (Number(now()) >= Number(descriptor.expiresAt)) {
        rejectEntry(entry, "expired");
        return;
      }
      const keyMaterial = {
        privateKey: String(data.keyMaterial.privateKey || ""),
        publicKey: String(data.keyMaterial.publicKey || ""),
        releaseToken: String(data.keyMaterial.releaseToken || ""),
        playerId: String(data.keyMaterial.playerId || ""),
        sessionEpoch: data.keyMaterial.sessionEpoch == null ? null : Number(data.keyMaterial.sessionEpoch),
        bindingEpoch: data.keyMaterial.bindingEpoch == null ? null : Number(data.keyMaterial.bindingEpoch),
        boundAgentId: String(data.keyMaterial.boundAgentId || "") || null,
        authorityEpoch: String(data.keyMaterial.authorityEpoch || "") || null,
      };
      if (!keyMaterial.privateKey || !keyMaterial.publicKey || !keyMaterial.releaseToken || !keyMaterial.playerId || keyMaterial.sessionEpoch == null) {
        rejectEntry(entry, "key_material_missing");
        return;
      }
      consumedDescriptors.add(descriptorKey);
      settleEntry(entry, "resolve", keyMaterial);
      closeChannel(channel);
    };
    scheduleExpiry(entry, Number(descriptor.expiresAt) - Number(now()));
    channel.postMessage(makeEnvelope(descriptor, "claim", { claimNonce }));
    return claimed;
  }

  function dispose() {
    if (disposed) {
      return;
    }
    disposed = true;
    for (const entry of [...activeEntries]) {
      entry.disposed = true;
      entry.keyMaterial = null;
      rejectEntry(entry, "disposed");
    }
    activeEntries.clear();
    consumedDescriptors.clear();
  }

  return {
    enabled,
    claimOffer,
    dispose,
    offerKeyMaterial,
  };
}
