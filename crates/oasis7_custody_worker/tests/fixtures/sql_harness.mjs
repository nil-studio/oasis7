// Local workerd fixture; never referenced by deployment configuration.
import { DurableObject } from 'cloudflare:workers';
import { initializeTextRecord, updateTextRecord } from '../../platform/sql.mjs';
export class SqlHarness extends DurableObject {
  constructor(ctx, env) { super(ctx, env); this.ctx = ctx; }
  async fetch(request) {
    const url = new URL(request.url);
    const initial = '{"epoch":"9007199254740993","label":"测试"}';
    initializeTextRecord(this.ctx, 'state', initial);
    if (url.pathname === '/rollback') {
      try {
        updateTextRecord(this.ctx, 'state', old => {
          this.ctx.storage.sql.exec('UPDATE custody_records SET payload = ? WHERE id = ?', '{"epoch":"18446744073709551615"}', 'state');
          this.ctx.storage.sql.exec('INSERT INTO custody_records (id, payload) VALUES (?, ?)', 'audit', 'must-rollback');
          throw new Error('intentional mid-transaction failure');
        });
      } catch (error) {
        if (error.message !== 'intentional mid-transaction failure') throw error;
      }
    } else if (url.pathname === '/advance') {
      updateTextRecord(this.ctx, 'state', () => '{"epoch":"18446744073709551615","label":"测试"}');
    }
    const payload = this.ctx.storage.sql.exec('SELECT payload FROM custody_records WHERE id = ?', 'state').one().payload;
    const audit = this.ctx.storage.sql.exec('SELECT payload FROM custody_records WHERE id = ?', 'audit').toArray();
    return Response.json({ payload, audit });
  }
}
export default { fetch() { return new Response(null, { status: 404 }); } };
