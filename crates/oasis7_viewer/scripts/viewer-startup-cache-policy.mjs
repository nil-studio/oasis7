// This is a browser-profile cache capability check, never a startup budget oracle.
export function verifyStartupCacheCases(cases) {
  if (cases.length !== 3) throw Error('expected cold/warm/cold capability cases');
  const [cold, warm, independent] = cases;
  if (cold.profile === independent.profile || cold.profile !== warm.profile) throw Error('profile isolation mismatch');
  for (const row of cases) {
    if (!row.wasmCompiled || row.responses.length !== 1) throw Error('missing actual WASM response/compile');
    if (row.responses[0].status !== 200) throw Error('unexpected WASM response');
  }
  for (const row of [cold, independent]) {
    if (row.networkRequests !== 1 || row.responses[0].fromDiskCache || row.responses[0].fromServiceWorker) throw Error('fresh profile did not prove network fetch');
  }
  if (warm.networkRequests !== 0 || !warm.responses[0].fromDiskCache || warm.responses[0].fromServiceWorker) throw Error('warm profile did not prove disk cache reuse');
  return { status: 'cache_capability_only', wasmCompiledCodeCache: 'unknown', osPageCache: 'not isolated', startupBudget: 'unmeasured' };
}
