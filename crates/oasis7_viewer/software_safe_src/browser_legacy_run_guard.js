import { createViewerModelConnectionClient } from "./viewer_model_connection_client.js";
export class LegacyRunGuardError extends Error {
 constructor(code) { super(code); this.name = "LegacyRunGuardError"; this.code = code; }
}
const reject = (code) => { throw new LegacyRunGuardError(code); };
export async function assertNoLegacyRun({ client = createViewerModelConnectionClient() } = {}) {
 try {
  const cap = await client.capabilities();
  if (cap?.ok !== true) reject("legacy_state_unknown");
  // A missing/unreachable endpoint is not evidence of a disabled deployment.
  if (cap.enabled === false && cap.reason === "deployment_unavailable") return;
  if (cap.enabled !== true || cap.authenticated !== true) reject("legacy_state_unknown");
  if (cap.legacy_execution_policy !== "management_only") reject("legacy_migration_required");
  // This startup policy forbids legacy workers for every session. The list is
  // still checked for inconsistent local projections; a session list alone
  // never proves that another session has no worker.
  const connections = await client.list();
  if (!Array.isArray(connections)) reject("legacy_state_unknown");
  for (const item of connections) {
   if (item?.run_state === "running" || item?.execution?.phase === "running") reject("legacy_run_active");
   if (!item || !["stopped", "idle", "unavailable"].includes(item.run_state)
    || !["checking", "ready", "blocked", "revoked", "expired"].includes(item.state)
    || (item.execution != null && !["finished", "stopped"].includes(item.execution.phase))) reject("legacy_state_unknown");
  }
 } catch (error) {
  if (error instanceof LegacyRunGuardError) throw error;
  reject("legacy_state_unknown");
 }
}
