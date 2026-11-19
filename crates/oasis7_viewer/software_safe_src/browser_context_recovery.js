import { sameBrowserValue } from "./browser_game_permit.js";
const terminal = (state) => ["committed", "rejected", "expired"].includes(state);
const sameTaskScope = (left, right) => ["world_id", "genesis_digest", "agent_id", "task_id"]
 .every((field) => typeof left?.[field] === "string" && left[field].trim().length > 0 && left[field] === right?.[field]);
export function openBrowserContexts(operations) {
 return operations.filter((entry) => entry.original.kind === "prefix" && entry.state === "committed"
  && !operations.some((other) => {
   if (other.state !== "committed") return false;
   const operationId = entry.original.operationId.replace(/:prefix$/, "");
   if (["action", "outcome"].includes(other.original.kind)) return other.original.operationId.replace(/:(action|outcome)$/, "") === operationId;
   // Runtime atomically retires an expired Context or a previous fence of the
   // same task when this explicitly confirmed successor prefix commits.
   return other.original.kind === "prefix" && other !== entry
    && other.original.runBudget?.expiredContextRetirements?.includes(operationId)
    && (sameBrowserValue(other.original.context?.fence, entry.original.context?.fence)
     || sameTaskScope(other.original.context?.fence, entry.original.context?.fence));
  }));
}
export function expiredBrowserPreviewContexts(operations, control) {
 if (operations.some((entry) => !terminal(entry.state)) || control?.logicalTick === undefined) return null;
 const open = openBrowserContexts(operations);
 if (!open.length) return null;
 for (const entry of open) {
  const context = entry.original.context;
  if ((!entry.original.runBudget?.previewPermitId && !entry.original.runBudget?.permitId) || !context?.fence || context.valid_until_tick === undefined) return null;
  if (sameBrowserValue(context.fence, control.fence)) {
   try { if (BigInt(control.logicalTick) <= BigInt(context.valid_until_tick)) return null; } catch { return null; }
  } else if (!sameTaskScope(context.fence, control.fence)) return null;
 }
 return open.map((entry) => entry.original.operationId.replace(/:prefix$/, "")).sort();
}
