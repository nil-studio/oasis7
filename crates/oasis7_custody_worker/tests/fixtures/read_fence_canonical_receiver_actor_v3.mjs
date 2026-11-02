// Test-only SQL observation around the actual built Rust RecoveryFence.
// No guard result, signature result, body, storage result, or status is changed.
import {RecoveryFence as ActualFence} from '../../artifacts/recovery-fence/entry.mjs';
export * from '../../artifacts/recovery-fence/entry.mjs';
export class RecoveryFence extends ActualFence {
  constructor(state, env) {
    const observation = { paid_sql: 0, in_paid: false };
    const storage = state.storage;
    const sql = new Proxy(storage.sql, {get(target, key) {
      if (key === 'exec') return (...args) => {
        if (observation.in_paid) observation.paid_sql++;
        return target.exec(...args);
      };
      const value = Reflect.get(target, key, target);
      return typeof value === 'function' ? value.bind(target) : value;
    }});
    const wrappedStorage = new Proxy(storage, {get(target, key) {
      if (key === 'sql') return sql;
      const value = Reflect.get(target, key, target);
      return typeof value === 'function' ? value.bind(target) : value;
    }});
    const wrappedState = new Proxy(state, {get(target, key) {
      if (key === 'storage') return wrappedStorage;
      const value = Reflect.get(target, key, target);
      return typeof value === 'function' ? value.bind(target) : value;
    }});
    super(wrappedState, env);
    this.observation = observation; this.actualStorage = storage;
  }
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/fixture-observe') {
      const tables = this.actualStorage.sql.exec("SELECT name FROM sqlite_master WHERE type='table'").toArray().map(row => row.name);
      const once = tables.includes('read_fence_admission_v3_once') ? this.actualStorage.sql.exec('SELECT COUNT(*) AS count FROM read_fence_admission_v3_once').one().count : 0;
      return Response.json({paid_sql: this.observation.paid_sql, once});
    }
    this.observation.in_paid = path === '/internal/read-fence-read-v3';
    try { return await super.fetch(request); }
    finally { this.observation.in_paid = false; }
  }
}
export default {async fetch(request, env) {
  const url = new URL(request.url); const name = url.searchParams.get('name');
  if (!['service-fence-v1', 'fixture-noncanonical-fence-v3'].includes(name)) return new Response('fixture unknown', {status: 400});
  const id = env.FENCES.idFromName(name);
  if (url.pathname === '/identity') return Response.json({object_id: id.toString()});
  const stub = env.FENCES.get(id);
  if (url.pathname === '/observe') return stub.fetch('https://fixture.invalid/fixture-observe');
  if (url.pathname === '/legacy') return stub.fetch('https://fence.internal/internal/recovery-fence');
  if (url.pathname === '/paid') return stub.fetch(new Request('https://fence.internal/internal/read-fence-read-v3', {method: 'POST', headers: {'content-type': 'application/json'}, body: await request.text(), redirect: 'manual'}));
  return new Response('fixture unknown', {status: 404});
}};
