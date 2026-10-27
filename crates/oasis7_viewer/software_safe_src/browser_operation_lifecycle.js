// Pure operation-level advice. Adapters own current authority, durable exact
// originals, locks, transport and proof validation; this grants no write right.
export const deliveryPhases = new Set(["prepared_unsent", "dispatch_may_have_started", "unknown"]);
export const terminalDisposition = (status) => ["committed", "rejected", "expired"].includes(status);
export function nextOperationAction(input) {
  if (!input.original_present || !input.scope_current) return "retain";
  if (input.proof_verified && terminalDisposition(input.disposition)) return "read_terminal";
  if (input.intent === "continue_authorized_entry" && !input.stopped && input.write_allowed) {
    if ((input.policy === "entry_first_send" && input.phase === "prepared_unsent")
      || input.policy === "private_preparation_replay") return "continue_original";
  }
  return input.read_allowed ? "query_original" : "retain";
}
export function operationObservation({ phase = "unknown", status = "unknown", verified = false,
  intent = "query_only", policy = "entry_first_send", stopped = false, scopeCurrent = true,
  readAllowed = true, writeAllowed = false } = {}) {
  const known = ["received", "pending", "unknown", "history_unavailable", "committed", "rejected", "expired"].includes(status);
  const disposition = verified && known ? status : "unknown";
  const deliveryPhase = deliveryPhases.has(phase) ? phase : "unknown";
  return { deliveryPhase, disposition, verified: verified && known,
    nextAction: nextOperationAction({ intent, policy, phase: deliveryPhase, disposition,
      proof_verified: verified && known, original_present: true, scope_current: scopeCurrent,
      read_allowed: readAllowed, write_allowed: writeAllowed, stopped }) };
}
// A late read cannot downgrade the already verified terminal for this exact
// original. Cross-original validation/CAS remains the adapter's responsibility.
export function retainTerminalObservation(previous, candidate) {
  return previous?.verified && terminalDisposition(previous.disposition) ? previous : candidate;
}
