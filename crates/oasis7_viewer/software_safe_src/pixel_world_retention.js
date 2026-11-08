// Logical JSON encoding and unique object counts, never a heap-size estimate.
// Walks references without cloning/stringifying a DTO or exposing its fields.
export function measureRetainedJson(roots) {
  const unique = new Set(), active = new Set(), encoder = new TextEncoder();
  let objects = 0, arrays = 0, nodes = 0;
  const bytes = (value, depth = 0) => {
    if (depth > 64) throw Error('retention observation depth budget exceeded');
    if (++nodes > 524288) throw Error('retention observation node budget exceeded');
    if (value === null) return 4;
    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return encoder.encode(JSON.stringify(value)).length;
    if (value === undefined) return 0;
    if (typeof value !== 'object') throw Error('retention observation requires JSON values');
    if (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) throw Error('retention observation requires JSON objects');
    if (active.has(value)) throw Error('retention observation cycle');
    if (!unique.has(value)) { unique.add(value); Array.isArray(value) ? arrays++ : objects++; }
    active.add(value); let size;
    if (Array.isArray(value)) { size = 2 + Math.max(0, value.length - 1); for (const item of value) size += item === undefined ? 4 : bytes(item, depth + 1); }
    else { const keys = Object.keys(value).filter(key => value[key] !== undefined); size = 2 + Math.max(0, keys.length - 1); for (const key of keys) size += encoder.encode(JSON.stringify(key)).length + 1 + bytes(value[key], depth + 1); }
    active.delete(value); return size;
  };
  const logicalJsonBytes = roots.reduce((sum, value) => sum + bytes(value), 0);
  return { logicalJsonBytes, uniqueObjects: objects, uniqueArrays: arrays, visitedNodes: nodes, heapBytes: null };
}
