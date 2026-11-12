import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const leases = new WeakSet();
const contexts = new WeakMap();
// Only directories created by this factory may be used, including warm reopens.
export function createStartupProfile() {
  const lease = Object.freeze({ path: mkdtempSync(join(tmpdir(), 'oasis7-startup-')) });
  leases.add(lease); return lease;
}
export async function launchOwnedStartupContext(chromium, executablePath, lease) {
  if (!leases.has(lease)) throw Error('startup profile was not created by owned factory');
  const context = await chromium.launchPersistentContext(lease.path, { executablePath, chromiumSandbox: true, headless: true, viewport: { width: 1440, height: 1000 }, timeout: 15000 });
  contexts.set(context, lease);
  return context;
}
export function readOwnedStartupProfile(context) {
  const lease = contexts.get(context);
  if (!lease) throw Error('context was not created by owned startup factory');
  return lease.path;
}
