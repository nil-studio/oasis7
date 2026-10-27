// Explicit isolated authority for positive component integration only. Never deployed.
import { WorkerEntrypoint } from 'cloudflare:workers';
let disabled = false;
let delayOnce = false;
export default { fetch(request) {
  if (new URL(request.url).pathname === '/freeze') disabled = true;
  if (new URL(request.url).pathname === '/slow-once') delayOnce = true;
  return Response.json({fixture:true, disabled});
} };
export class RecoveryFenceReader extends WorkerEntrypoint {
  async read() {
    if (delayOnce) { delayOnce = false; await new Promise(resolve => setTimeout(resolve, 5200)); }
    return JSON.stringify({epoch:'1',disabled});
  }
}
