import { BrowserGameError } from "./browser_game_client.js";
import { boundedFirstProductionSelection, directBuildSelection, sameBrowserValue } from "./browser_game_permit.js";
import { openBrowserContexts, expiredBrowserPreviewContexts } from "./browser_context_recovery.js";
const fail = (code) => { throw new BrowserGameError(code); };

export async function readBrowserActionPreview({ gameClient, recovery, permitId, confirmedFence,
 kind, selection, check = () => {} }) {
 if (typeof permitId !== "string" || !permitId.trim() || permitId.length > 180
  || !confirmedFence || !["schedule", "build"].includes(kind)) fail("action_permit_invalid");
 if (kind === "schedule" && (selection?.recipe !== "recipe.smelter.iron_ingot" || selection?.batches !== 1)) fail("action_permit_scope_mismatch");
 const operationId = `preview:${permitId}`, previewRequest = { kind, selection };
 check(); await gameClient.recover(); check();
 const current = await gameClient.freshFence(); check();
 if (!sameBrowserValue(current.fence, confirmedFence)) fail("task_fence_changed");
 await gameClient.bindFence(confirmedFence); check();
 const journal = await recovery.load(); check();
 const own = journal.operations.filter((entry) => entry.original.operationId.startsWith(`${operationId}:`));
 if (own.some((entry) => !sameBrowserValue(entry.original.runBudget?.previewRequest, previewRequest)
  || !["prepare", "prefix"].includes(entry.original.kind))) fail("action_permit_scope_mismatch");
 if (await recovery.hasUnresolved()) fail("recovery_required"); check();
 const ownPrefix = own.some((entry) => entry.original.kind === "prefix" && entry.state === "committed");
 const expiredContextRetirements = !ownPrefix ? expiredBrowserPreviewContexts(journal.operations, current) : null;
 if (!ownPrefix && await gameClient.hasUnresolved() && !expiredContextRetirements) fail("recovery_required"); check();
 const captured = await gameClient.context(operationId, { previewPermitId: permitId, previewRequest,
  ...(expiredContextRetirements ? { expiredContextRetirements } : {}) }); check();
 const control = await gameClient.freshFence(); check();
 const until = captured.prepared.valid_until_tick ?? captured.prepared.identity.valid_until_tick;
 if (until !== undefined && control.logicalTick !== undefined
  && BigInt(control.logicalTick) > BigInt(until)) fail("context_expired");
 const result = await gameClient.quote(operationId, captured, kind, selection); check();
 const quote = result.quoted.quote;
 if (!quote || quote.rejection || !quote.preview) fail("quote_rejected");
 const bounded = kind === "schedule"
  ? boundedFirstProductionSelection({ ...selection, maxConsume: quote.preview.consume, maxElectricity: quote.preview.power_required }, gameClient.agentId)
  : directBuildSelection({ site: selection.site, factory: selection.factory, factoryKind: selection.kind,
   maxConsume: quote.preview.consume, maxElectricity: quote.preview.construction_power_obligation?.electricity_amount }, gameClient.agentId);
 return { permitId, contextOperationId: operationId, confirmedFence, selection: bounded, quote: result.quoted,
  contextValidUntilTick: captured.prepared.valid_until_tick ?? captured.prepared.identity.valid_until_tick };
}

export function openBrowserPreviews(journal) {
 return openBrowserContexts(journal.operations).filter((entry) => typeof entry.original.runBudget.previewPermitId === "string" || typeof entry.original.runBudget.permitId === "string")
  .map((entry) => ({ contextOperationId: entry.original.operationId.replace(/:prefix$/, ""),
   mode: entry.original.runBudget.previewPermitId ? "preview" : "direct",
   permitId: entry.original.runBudget.previewPermitId ?? entry.original.runBudget.permitId, selection: entry.original.runBudget.previewRequest,
   context: entry.original.context, state: entry.state,
   originalStates: journal.operations.filter((other) => ["action", "outcome"].includes(other.original.kind)
    && other.original.operationId.replace(/:(action|outcome)$/, "") === entry.original.operationId.replace(/:prefix$/, "")).map((other) => other.state),
   hasOriginal: journal.operations.some((other) => ["action", "outcome"].includes(other.original.kind)
    && other.original.operationId.replace(/:(action|outcome)$/, "") === entry.original.operationId.replace(/:prefix$/, "")) }));
}

export async function endBrowserActionPreview({ gameClient, recovery, confirmedFence, contextOperationId,
 permitId, check = () => {} }) {
 if (!confirmedFence || contextOperationId !== `preview:${permitId}`) fail("action_permit_invalid");
 check(); await gameClient.recover(); check();
 const journal = await recovery.load(); check();
 if (journal.operations.some((entry) => !["committed", "rejected", "expired"].includes(entry.state))) fail("recovery_required");
 const prefix = journal.operations.find((entry) => entry.original.operationId === `${contextOperationId}:prefix`);
 if (!prefix || prefix.state !== "committed" || prefix.original.runBudget.previewPermitId !== permitId) fail("action_permit_scope_mismatch");
 if (journal.operations.some((entry) => entry.original.operationId === `${contextOperationId}:action`)) fail("original_action_requires_recovery");
 const prior = journal.operations.find((entry) => entry.original.operationId === `${contextOperationId}:outcome`);
 if (prior) {
  if (prior.state !== "committed") fail("original_result_requires_recovery");
  return { state: prior.state, contextOperationId };
 }
 const current = await gameClient.freshFence(); check();
 if (!sameBrowserValue(current.fence, confirmedFence)) fail("task_fence_changed");
 await gameClient.bindFence(confirmedFence); check();
 const until = prefix.original.context?.valid_until_tick;
 let result;
 if (until !== undefined && current.logicalTick !== undefined && BigInt(current.logicalTick) > BigInt(until)) {
  await gameClient.retireExpiredContexts({ operationId: contextOperationId }); check();
  const completed = (await recovery.load()).operations.find((entry) => entry.original.operationId === `${contextOperationId}:outcome`); check();
  result = completed?.state;
 } else {
  const captured = await gameClient.context(contextOperationId, prefix.original.runBudget); check();
  const outcome = await gameClient.outcome(contextOperationId, captured, "executor_aborted", prefix.original.runBudget); check();
  result = outcome?.outcome?.status;
 }
 if (result !== "committed") fail("original_result_requires_recovery");
 return { state: result, contextOperationId };
}
