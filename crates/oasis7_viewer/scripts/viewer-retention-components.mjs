// Component comparisons never equate encoded JSON, heap, linear memory and RSS.
export function compareRetentionComponent(initial, final) {
  const limit = 0.10;
  if (!Number.isFinite(initial) || !Number.isFinite(final) || initial < 0 || final < 0) return { status: 'unknown', reason: 'component_unavailable' };
  if (initial === 0) return final === 0 ? { status: 'within_budget', growth: 0 } : { status: 'unknown', reason: 'zero_initial_component', initial, final };
  const growth = (final - initial) / initial;
  return { status: growth <= limit ? 'within_budget' : 'over_budget', initial, final, growth, limit };
}
export function numericRetentionRecord(input) {
  const allowed = new Set(['timeMs','phase','scopeSequence','generation','viewJsonBytes','viewObjects','viewArrays','pagingJsonBytes','pagingObjects','pagingArrays','pagingActive','pagingEntities','pagingRelations','pagingAggregates','pagingEvents','pagingCursors','observationMs','jsUsedHeapBytes','jsTotalHeapBytes','domNodes','documents','listeners','wasmLinearBytes','rendererBytes','gpuBytes','ownedRssBytes']);
  for (const [key, value] of Object.entries(input)) {
    if (!allowed.has(key)) throw Error('unrecognized retention field');
    if (key === 'phase') { if (!['warmup','home','away','filtered','restored'].includes(value)) throw Error('unknown retention phase'); }
    else if (key === 'pagingActive') { if (typeof value !== 'boolean') throw Error('paging active must be boolean'); }
    else if (value !== null && (typeof value !== 'number' || !Number.isFinite(value) || value < 0)) throw Error('retention field must be nonnegative numeric or unknown');
  }
  return { ...input };
}
