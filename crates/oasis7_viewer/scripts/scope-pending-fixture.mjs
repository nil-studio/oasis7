// Finite observer fixture, not signed WorldService or real grant/revocation proof.
import { randomUUID } from 'node:crypto';
const copy = value => structuredClone(value);
export function createScopePendingFixture() {
  let denied = false;
  const world = 'scope-qa-world', scope = 'scope-qa-public', cut = 'scope-qa-cut-1';
  const caps = ['world_view', 'world_view_pins_v1'], cursors = new Map();
  const bounds = { width_cm: '1000000', depth_cm: '1000000', height_cm: '100000', min_x_cm: '0', max_x_cm: '1000000', min_y_cm: '0', max_y_cm: '1000000' };
  const binding = { evidence: 'controlled_transport_fixture_not_service_proof', version: { visibility_scope: scope, projection_revision: 'scope-qa-v1', commit: { world: { world_id: world, genesis_digest: 'scope-qa-not-service-proof' }, binding: { provider_world_id: world, branch_id: 'scope-qa', reorg_generation: '0', authority_generation: '0', permission_generation: '0', governing_manifest_ref: 'scope-qa-only' } } }, viewer_epoch: '0' };
  const records = [
    ['A', '400000', '450000'], ['C', '600000', '550000'], ['B', '500000', '500000'],
  ].map(([name, x_cm, y_cm]) => {
    const id = 'scope-' + name, pos = { x_cm, y_cm, z_cm: '0' };
    return { kind: 'agent', entity_id: id, label: 'Agent ' + name, position: pos,
      render_record: { id, name: 'Agent ' + name, pos, resources: { amounts: { electricity: '100' } } },
      provenance: { geometry_source: 'authoritative', freshness: 'current', valid_at_source_cut: cut, observed_at_tick: '1', source_ref: 'controlled_scope_qa' } };
  });
  const base = generation => ({ world_id: world, scope_id: scope, source_cut: cut, reorg_epoch: '0', observed_at_tick: '1', world_bounds: copy(bounds), source_binding: copy(binding), source_frozen: false,
    request_generation: generation, relations: [], aggregates: [], events: [], unavailable_reasons: [] });
  const unknown = (q, reason) => ({ type: 'world_view', view: { ...base(q.request_generation), entities: [], completeness: 'unknown', unavailable_reasons: [reason] } });
  const snapshot = () => ({ version: 3, chunk_generation_schema_version: 1, time: 1,
    config: { space: { width_cm: 1000000, depth_cm: 1000000, height_cm: 100000 } },
    model: { agents: {}, locations: {}, agent_player_bindings: {}, agent_player_public_key_bindings: {} },
    runtime_snapshot: null, player_gameplay: null, pending_actions: [], journal_len: 0 });
  return {
    snapshot, denyReads() { denied = true; }, allowReads() { denied = false; },
    describe: () => ({ world, scope, cut, records: 3, responsePageCap: 1, capabilities: caps, deltaNegotiated: false, denied, realAuthority: false }),
    reply(message, connection) {
      if (message.type === 'hello_v2') return { type: 'hello_ack', server: 'scope-pending-controlled-qa', version: 2, min_version: 1, max_version: 2,
        world_id: world, control_profile: 'live', capabilities: caps.filter(cap => message.capabilities?.includes(cap)) };
      if (message.type === 'subscribe') return null;
      if (message.type === 'request_snapshot') return { type: 'snapshot', snapshot: snapshot() };
      if (message.type === 'request_scope_snapshot') return { type: 'scope_snapshot', request_generation: message.request_generation, snapshot: snapshot() };
      if (message.type === 'request_world_feed') return { type: 'world_feed', feed: { schema_version: 1, world_id: world, reorg_epoch: '0', status: 'unavailable', events: [], cursor: null, unavailable_reason: 'source_unavailable', snapshot_reload_required: false } };
      if (message.type !== 'request_world_view') throw Error('scope fixture refuses non-read message: ' + message.type);
      const q = message.query || {};
      if (denied) return unknown(q, 'scope_unavailable');
      if (!Number.isSafeInteger(q.page_size) || q.page_size < 1 || q.page_size > 200 || typeof q.request_generation !== 'string') return unknown(q, 'invalid_query');
      if (q.resume_cursor) throw Error('delta/resume was not negotiated');
      const key = JSON.stringify({ search: q.search ?? null, bounds: q.bounds ?? null, pinned: q.pinned_entities ?? [] });
      let rows = records.filter(row => !q.search || row.entity_id.includes(q.search) || row.label.includes(q.search));
      if (q.bounds) rows = rows.filter(row => Number(row.position.x_cm) >= Number(q.bounds.min_x_cm) && Number(row.position.x_cm) <= Number(q.bounds.max_x_cm)
        && Number(row.position.y_cm) >= Number(q.bounds.min_y_cm) && Number(row.position.y_cm) <= Number(q.bounds.max_y_cm));
      const pins = new Set((q.pinned_entities || []).filter(ref => ref.kind === 'agent').map(ref => ref.id));
      rows = [...new Map([...records.filter(row => pins.has(row.entity_id)), ...rows].map(row => [row.entity_id, row])).values()];
      let offset = 0;
      if (q.cursor) { const c = cursors.get(q.cursor); if (!c || c.connection !== connection || c.key !== key) return unknown(q, 'cursor_binding_mismatch'); offset = c.offset; }
      const next = offset + 1 < rows.length ? randomUUID() : null;
      if (next) { cursors.set(next, { connection, key, offset: offset + 1 }); while (cursors.size > 32) cursors.delete(cursors.keys().next().value); }
      // A server may return fewer than the requested maximum; the client keeps
      // its unchanged page_size=200 contract. No producer/display cap is raised.
      return { type: 'world_view', view: { ...base(q.request_generation), entities: copy(rows.slice(offset, offset + 1)),
        delivery: 'baseline', query_id: key, update_sequence: '0', next_cursor: next, completeness: next ? 'partial' : 'complete' } };
    },
  };
}
