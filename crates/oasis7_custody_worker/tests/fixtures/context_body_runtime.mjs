// Temporary test actors only. All ordinary storage requests reach actual Rust.
import deployed, {ContextBody as ActualContextBody} from '../../artifacts/identity/entry.mjs';
export default deployed;
function bound(target, name) {
  const value = Reflect.get(target, name, target);
  return typeof value === 'function' && name !== 'constructor' ? value.bind(target) : value;
}

export class FixtureContextBody extends ActualContextBody {
  constructor(state, env) {
    const control = {failUpdate: false, updateFaults: 0}, storage = state.storage;
    const sql = new Proxy(storage.sql, {get(target, name) {
      if (name !== 'exec') return bound(target, name);
      return (...args) => {
        const selected = control.failUpdate && typeof args[0] === 'string'
          && args[0].startsWith('UPDATE custody_records SET payload') && args[2] === 'custody-context-body-v1';
        if (!selected) return target.exec(...args);
        if (!JSON.parse(args[1]).envelope || target.exec('SELECT COUNT(*) AS n FROM custody_audit').one().n !== 1) {
          throw new Error('fixed context UPDATE fault prerequisite failed');
        }
        try { return target.exec(...args); }
        catch (error) {
          // Observe the actual SQLite constraint error; never fabricate or swallow it.
          if (String(error.message).includes('CHECK constraint failed')) control.updateFaults++;
          throw error;
        }
      };
    }});
    const wrappedStorage = new Proxy(storage, {get(target, name) { return name === 'sql' ? sql : bound(target, name); }});
    const wrappedState = new Proxy(state, {get(target, name) { return name === 'storage' ? wrappedStorage : bound(target, name); }});
    super(wrappedState, env);
    this.fixtureState = wrappedState;
    this.fixtureEnv = env;
    this.fixtureControl = control;
  }
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/fixture/update-fault-count') return Response.json({reached: this.fixtureControl.updateFaults});
    if (path === '/fixture/inventory') {
      const snapshot = this.fixtureState.storage.transactionSync(() => {
        const sql = this.fixtureState.storage.sql;
        const tables = sql.exec("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").toArray().map(row => row.name);
        const rows = tables.includes('custody_records') ? sql.exec('SELECT payload FROM custody_records ORDER BY id').toArray() : [];
        const audit = tables.includes('custody_audit') ? sql.exec('SELECT COUNT(*) AS n FROM custody_audit').one().n : 0;
        return {tables, rows, audit};
      });
      const payload_digests = await Promise.all(snapshot.rows.map(async row => {
        const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(row.payload));
        return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
      }));
      // Only synthetic ciphertext is present, and no record/body is returned.
      return Response.json({tables: snapshot.tables, rows: snapshot.rows.length, audit: snapshot.audit,
        payload_lengths: snapshot.rows.map(row => row.payload.length), payload_digests});
    }
    if (path === '/fixture/mutate') {
      const {mode, record} = await request.json();
      this.fixtureState.storage.transactionSync(() => {
        const sql = this.fixtureState.storage.sql;
        switch (mode) {
          case 'fail_update':
            // Real SQLite UPDATE failure after the Rust callback and audit INSERT.
            sql.exec(`CREATE TABLE custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL CHECK(instr(payload, '"envelope"') = 0))`);
            this.fixtureControl.failUpdate = true;
            break;
          case 'extra_row':
            sql.exec("INSERT INTO custody_records(id,payload) VALUES('fixture-extra','{}')");
            break;
          case 'unknown_table':
            sql.exec('CREATE TABLE fixture_unknown (value TEXT)');
            break;
          case 'noncanonical': {
            const row = sql.exec("SELECT payload FROM custody_records WHERE id='custody-context-body-v1'").one();
            sql.exec("UPDATE custody_records SET payload=? WHERE id='custody-context-body-v1'", JSON.stringify(JSON.parse(row.payload), null, 2));
            break;
          }
          case 'historical_expired':
            sql.exec('CREATE TABLE custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)');
            sql.exec("INSERT INTO custody_records(id,payload) VALUES('custody-context-body-v1',?)", record);
            break;
          default: throw new Error('unknown fixed context fixture mode');
        }
      });
      return new Response(null, {status: 204});
    }
    const response = await super.fetch(request);
    if (this.fixtureEnv.FIXTURE_DROP_CONTEXT_REPLY === 'true' && path === '/internal/put' && response.status === 200) {
      await response.arrayBuffer();
      throw new Error('fixture context reply lost after actual store commit');
    }
    return response;
  }
}
