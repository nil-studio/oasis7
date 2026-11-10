// Read-only components. Missing process/GPU/WASM accounting stays unknown.
export async function readOwnedMemory(session) {
  const result = {jsUsedHeapBytes:null,jsTotalHeapBytes:null,domNodes:null,documents:null,listeners:null,
    ownedRssBytes:null,wasmLinearBytes:null,gpuBytes:null,capabilityReasons:{
      ownedRssBytes:'CDP process CPU information is not RSS; no process ownership/RSS interface established',
      wasmLinearBytes:'no public active-instance memory accounting interface',gpuBytes:'no per-Viewer GPU allocation interface'}};
  const collect = async (key, method, assign) => {
    try { assign(await session.page(method)); } catch (error) { result.capabilityReasons[key] = String(error.message); }
  };
  await collect('heap','Runtime.getHeapUsage',value=>{
    if (!Number.isFinite(value.usedSize)||!Number.isFinite(value.totalSize)||value.usedSize<0||value.totalSize<value.usedSize) throw Error('invalid heap component response');
    result.jsUsedHeapBytes=value.usedSize;result.jsTotalHeapBytes=value.totalSize;
  });
  await collect('dom','Memory.getDOMCounters',value=>{
    if (![value.nodes,value.documents,value.jsEventListeners].every(v=>Number.isSafeInteger(v)&&v>=0)) throw Error('invalid DOM component response');
    result.domNodes=value.nodes;result.documents=value.documents;result.listeners=value.jsEventListeners;
  });
  return result;
}
