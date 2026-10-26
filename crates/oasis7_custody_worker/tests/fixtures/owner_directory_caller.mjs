// Component-only caller: supplied sessions are synthetic trusted adapter input,
// not evidence of browser/native authentication. No deployment imports this file.
import { AccountAuthority, ManagerAssociations, OwnerManagement, OwnerRevocations } from '../../artifacts/owner/entry.mjs';
export { ManagerAssociations, OwnerManagement, OwnerRevocations };
export class AccountHarness extends AccountAuthority {
  constructor(ctx, env) { super(ctx, env); this.ctx = ctx; }
  async fetch(request) {
    const url = new URL(request.url);
    if (!url.pathname.startsWith('/fixture/')) {
      if (url.pathname === '/internal/draft') {
        const value = await request.clone().json();
        if (value.entry.client_request_id.startsWith('fixture-pending-') && !this.allowPendingDraft)
          return new Response('component fixture delays first initialization', { status: 503 });
      }
      if (url.pathname === '/internal/draft' && this.pauseNextDraft) {
        this.pauseNextDraft = false;
        this.draftPaused = true;
        await new Promise(resolve => { this.resumeDraft = resolve; });
        this.draftPaused = false;
      }
      return super.fetch(request);
    }
    if (url.pathname === '/fixture/allow-pending-draft') { this.allowPendingDraft = true; return new Response(null, { status: 204 }); }
    if (url.pathname === '/fixture/pause-next-draft') { this.pauseNextDraft = true; return new Response(null, { status: 204 }); }
    if (url.pathname === '/fixture/draft-paused') return Response.json({ paused: Boolean(this.draftPaused) });
    if (url.pathname === '/fixture/resume-draft') { this.resumeDraft?.(); return new Response(null, { status: 204 }); }
    const sql = this.ctx.storage.sql;
    if (url.pathname === '/fixture/inspect') {
      let records = [], audit = [];
      try { records = sql.exec('SELECT id, payload FROM custody_records ORDER BY id').toArray(); } catch {}
      try { audit = sql.exec('SELECT record_id, sequence, payload FROM custody_audit ORDER BY record_id, sequence').toArray(); } catch {}
      return Response.json({ records, audit });
    }
    sql.exec('CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)');
    if (url.pathname === '/fixture/unknown-row')
      sql.exec('INSERT INTO custody_records (id, payload) VALUES (?, ?)', 'owner-approval-admission-v1', '{"unrecognized":"must preserve"}');
    else if (url.pathname === '/fixture/unknown-table') sql.exec('CREATE TABLE future_authorizations (payload TEXT)');
    else if (url.pathname === '/fixture/unknown-trigger')
      sql.exec('CREATE TRIGGER future_writer AFTER INSERT ON custody_records BEGIN SELECT 1; END');
    else if (url.pathname === '/fixture/rollback-trigger')
      sql.exec("CREATE TRIGGER fail_update BEFORE UPDATE ON custody_records BEGIN SELECT RAISE(ABORT, 'fixture rollback'); END");
    else if (url.pathname === '/fixture/drop-rollback-trigger') sql.exec('DROP TRIGGER fail_update');
    else if (url.pathname === '/fixture/future-draft') {
      const row = sql.exec('SELECT payload FROM custody_records WHERE id = ?', 'custody-account-draft-v1').one();
      const value = JSON.parse(row.payload); value.schema_version = 2;
      sql.exec('UPDATE custody_records SET payload = ? WHERE id = ?', JSON.stringify(value), 'custody-account-draft-v1');
    } else return new Response(null, { status: 404 });
    return new Response(null, { status: 204 });
  }
}
export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const raw = await request.text();
    try {
      let result;
      if (url.pathname === '/default-source-rpc') {
        try { await env.DEFAULT_OWNER.owner_close_source_event(raw); return Response.json({ missing_rpc: false }); }
        catch (error) {
          const message = String(error);
          const missing = message.includes('owner_close_source_event') && /does not implement|not a function|not implemented/i.test(message);
          return Response.json({ missing_rpc: missing }, { status: 409 });
        }
      }
      else if (url.pathname === '/source-event') result = await env.REVOCATIONS.closeSourceEvent(raw);
      else if (url.pathname === '/default-rpc') result = await env.DEFAULT_OWNER.owner_reserve_draft(raw);
      else if (url.pathname === '/reserve') result = await env.MANAGEMENT.reserveDraft(raw);
      else if (url.pathname === '/read') result = await env.MANAGEMENT.getDraft(raw);
      else if (url.pathname === '/freeze-manager') result = await env.MANAGEMENT.freezeManager(raw);
      else return new Response(null, { status: 404 });
      return result instanceof Response ? result : new Response(result, { headers: { 'content-type': 'application/json' } });
    } catch { return new Response('private component RPC rejected', { status: 409 }); }
    return new Response(null, { status: 404 });
  },
};
