import { quoteDependencyFingerprint } from "./viewer_quote_dependencies.js";
export function createProductValidationQuoteRequestModule({
  buildAuthEnvelope,
  clone,
  ensureHostedPlayerAuthAvailable,
  ensureRegisteredPlayerSession,
  getSocket,
  nextAuthNonce,
  sendJson,
  signAuthPayload,
  state,
}) {
  let requestGeneration = 0;
  async function buildAuthProof(request, auth) {
    const nonce = nextAuthNonce();
    const signingPayload = buildAuthEnvelope({
      operation: "gameplay_action",
      action_id: "quote_validate_product",
      target_agent_id: `product_id:${request.product_id}|amount:${request.amount}`,
      player_id: auth.playerId,
      public_key: auth.publicKey,
      nonce,
    });
    return {
      scheme: "ed25519",
      player_id: auth.playerId,
      public_key: auth.publicKey,
      nonce,
      signature: await signAuthPayload(signingPayload, auth),
    };
  }

  async function requestProductValidationQuote(productId, amount) {
    const generation = ++requestGeneration;
    const normalizedProductId = String(productId || "").trim();
    const amountNumber = Number(amount);
    if (!normalizedProductId || !Number.isSafeInteger(amountNumber) || amountNumber <= 0) {
      const reason = "product validation quote requires a product id and positive whole-number amount";
      state.productValidationQuoteRequest = { status: "error", error: reason };
      return { ok: false, reason };
    }
    const socket = getSocket();
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      const reason = "product validation quote requires a connected viewer websocket";
      state.productValidationQuoteRequest = { status: "error", error: reason };
      return { ok: false, reason };
    }
    try {
      await ensureHostedPlayerAuthAvailable();
      if (generation !== requestGeneration) return { ok: false, reason: "quote request superseded" };
      if (!state.auth.available) {
        const reason = state.auth.error || "product validation quote requires an active player session";
        state.productValidationQuoteRequest = { status: "error", error: reason };
        return { ok: false, reason };
      }
      const boundAgentId = String(state.auth.boundAgentId || "").trim();
      if (!boundAgentId) {
        const reason = "product validation quote requires a bound player Agent";
        state.productValidationQuoteRequest = { status: "error", error: reason };
        return { ok: false, reason };
      }
      await ensureRegisteredPlayerSession(boundAgentId);
      if (generation !== requestGeneration) return { ok: false, reason: "quote request superseded" };
      const request = {
        product_id: normalizedProductId,
        amount: amountNumber,
        player_id: state.auth.playerId,
        public_key: state.auth.publicKey,
      };
      const dependencyInputs = { product_id: normalizedProductId, amount: amountNumber };
      const dependencyFingerprint = quoteDependencyFingerprint(state, "product", dependencyInputs);
      request.auth = await buildAuthProof(request, state.auth);
      if (generation !== requestGeneration) return { ok: false, reason: "quote request superseded" };
      if (dependencyFingerprint !== quoteDependencyFingerprint(state, "product", dependencyInputs) || getSocket() !== socket) {
        state.productValidationQuoteRequest = { status: "idle", error: null };
        return { ok: false, reason: "quote inputs changed; request a fresh quote" };
      }
      state.productValidationQuote = null;
      state.productValidationQuoteRequest = { status: "pending", error: null, requestAuthNonce: request.auth.nonce,
        dependencyInputs, dependencyFingerprint };
      sendJson({ type: "quote_product_validation", request });
      return { ok: true, request: clone(request) };
    } catch (error) {
      if (generation !== requestGeneration) return { ok: false, reason: "quote request superseded" };
      const reason = `product validation quote request failed: ${String(error)}`;
      state.productValidationQuoteRequest = { status: "error", error: reason };
      return { ok: false, reason };
    }
  }

  return { requestProductValidationQuote };
}
