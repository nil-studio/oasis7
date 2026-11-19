import { BrowserGameError } from "./browser_game_client.js";
import { assertBrowserPermitAction, assertDirectTransferLocation, sameBrowserValue, validateBrowserActionSelection } from "./browser_game_permit.js";
import { openBrowserContexts, expiredBrowserPreviewContexts } from "./browser_context_recovery.js";
const fail = (code) => { throw new BrowserGameError(code); };

// One explicitly confirmed player operation. It shares the original journal
// with the Agent, but never acquires a model credential or claims Agent work.
export async function executeBrowserDirectAction({ gameClient, recovery, confirmedFence, permitId,
 selection, contextOperationId, check = () => {}, onState = () => {}, onContextOperation = () => {} }) {
 const selected = validateBrowserActionSelection(selection);
 if (typeof permitId !== "string" || !permitId.trim() || permitId.length > 180
  || selected.agentId !== gameClient.agentId || !confirmedFence) fail("action_permit_invalid");
 check(); await gameClient.recover(); check();
 const current = await gameClient.freshFence(); check();
 if (!sameBrowserValue(current.fence, confirmedFence)) fail("task_fence_changed");
 await gameClient.bindFence(confirmedFence); check();
 const permit = await recovery.initializeActionPermit(permitId, { fence: confirmedFence, mode: "direct", selection: selected }); check();
 const result = async () => {
  const saved = await recovery.getActionPermit(permitId); check();
  const original = (await recovery.load()).operations.find((entry) => entry.original.operationId === saved.originalOperationId); check();
  if (!original) fail("action_permit_corrupt");
  const terminal = ["committed", "rejected", "expired"].includes(original.state);
  const value = { phase: terminal ? "settled" : "paused", code: terminal ? `original_action_${original.state}` : "original_result_requires_recovery",
   originalOperationId: saved.originalOperationId, submissionState: original.state, agentDelegated: false };
  onState(value); return value;
 };
 if (permit.originalOperationId) return result();
 let operationId = contextOperationId ?? `direct:${permitId}`;
 if (contextOperationId && !contextOperationId.startsWith("preview:")) fail("action_permit_invalid");
 const journal = await recovery.load(); check();
 if (await recovery.hasUnresolved()) fail("recovery_required");
 check();
 const priorPrefixes = journal.operations.filter((entry) => entry.original.kind === "prefix" && entry.original.runBudget?.permitId === permitId);
 if (!contextOperationId) {
  const open = openBrowserContexts(journal.operations).filter((entry) => entry.original.runBudget?.permitId === permitId);
  if (open.length > 1) fail("recovery_required");
  if (open.length) operationId = open[0].original.operationId.replace(/:prefix$/, "");
  else if (priorPrefixes.length) operationId = `direct:${permitId}:retry:${priorPrefixes.length}`;
 }
 let ownPrefix = journal.operations.some((entry) => entry.original.operationId === `${operationId}:prefix` && entry.state === "committed");
 const expiredContextRetirements = expiredBrowserPreviewContexts(journal.operations, current);
 if (ownPrefix && expiredContextRetirements?.includes(operationId) && !contextOperationId) {
  const attempt = priorPrefixes.length;
  operationId = `direct:${permitId}:retry:${attempt}`; ownPrefix = false;
 }
 if (!ownPrefix && await gameClient.hasUnresolved() && !expiredContextRetirements) fail("recovery_required");
 check(); onState({ phase: "direct_running", code: null, agentDelegated: false });
 onContextOperation(operationId); check();
 let captured = await gameClient.context(operationId, { permitId,
  ...(!ownPrefix && expiredContextRetirements ? { expiredContextRetirements } : {}) }); check();
 const control = await gameClient.freshFence(); check();
 const until = captured.prepared.valid_until_tick ?? captured.prepared.identity.valid_until_tick;
 if (until !== undefined && control.logicalTick !== undefined
  && BigInt(control.logicalTick) > BigInt(until)) fail("context_expired");
 let action, preview;
 if (selected.kind === "direct_action") {
  action = selected.action;
  if (action.type === "TransferMaterial") {
   const observation = await gameClient.publicView(); check();
   assertDirectTransferLocation(action, observation.view?.state ?? observation.view?.observation?.state);
   captured = await gameClient.readContext(operationId, captured.prepared.identity); check();
  }
 }
 else {
  const build = selected.kind === "direct_build";
  const quoted = await gameClient.quote(operationId, captured, build ? "build" : "schedule", build
   ? { factory: selected.factory, site: selected.site, kind: selected.factoryKind }
   : { factory: selected.factory, recipe: selected.recipe, batches: 1 }); check();
  if (!quoted.quoted.quote || quoted.quoted.quote.rejection) fail("quote_rejected");
  captured = quoted.captured; action = quoted.quoted.quote.action; preview = quoted.quoted.quote.preview;
 }
 assertBrowserPermitAction(selected, action, preview); check();
 await gameClient.action(operationId, captured, action, { permitId, ...(preview ? { quotePreview: preview } : {}) }); check();
 return result();
}
