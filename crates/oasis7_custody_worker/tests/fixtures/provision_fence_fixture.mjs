// Explicit isolated authority for positive component integration only. Never deployed.
import { WorkerEntrypoint } from 'cloudflare:workers';
let disabled = false;
export default { fetch(request) {
  if (new URL(request.url).pathname === '/freeze') disabled = true;
  return Response.json({fixture:true, disabled});
} };
export class RecoveryFenceReader extends WorkerEntrypoint {
  read() { return JSON.stringify({epoch:'1',disabled}); }
}
