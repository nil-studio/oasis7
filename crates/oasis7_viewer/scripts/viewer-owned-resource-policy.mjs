// These proofs permit only an owned process component observation, never a total
// physical-memory, application-cache or GPU-allocation claim.
export function verifyOwnedResourceSnapshot({profile, browserPid, commandLine, pageUrls, expectedUrl, before, after, cdpProcesses}) {
  const fail = reason => { throw Error(reason); };
  if (typeof profile !== 'string' || !profile || !Number.isSafeInteger(browserPid) || browserPid <= 0) fail('owned root missing');
  if (!Array.isArray(commandLine) || commandLine.filter(x => x === '--user-data-dir=' + profile).length !== 1 || commandLine.filter(x => x.startsWith('--user-data-dir=')).length !== 1) fail('owned profile command line mismatch');
  if (pageUrls.length !== 1 || pageUrls[0] !== expectedUrl) fail('owned single page changed');
  const index = rows => {
    const result = new Map();
    for (const row of rows) {
      if (!Number.isSafeInteger(row.pid) || row.pid <= 0 || result.has(row.pid) || !Number.isSafeInteger(row.ppid) || !row.birth) fail('invalid process identity');
      result.set(row.pid, row);
    }
    return result;
  };
  const a = index(before), b = index(after);
  if (!a.has(browserPid) || !b.has(browserPid) || a.get(browserPid).birth !== b.get(browserPid).birth) fail('owned root changed');
  const descendants = table => {
    const owned = new Set([browserPid]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const row of table.values()) if (!owned.has(row.pid) && owned.has(row.ppid)) { owned.add(row.pid); changed = true; }
    }
    return owned;
  };
  const first = descendants(a), last = descendants(b);
  if (first.size !== last.size || [...first].some(pid => !last.has(pid) || a.get(pid).birth !== b.get(pid).birth || a.get(pid).ppid !== b.get(pid).ppid)) fail('owned descendants changed during observation');
  if (!cdpProcesses.some(p => p.id === browserPid && p.type === 'browser') || cdpProcesses.some(p => !last.has(p.id))) fail('CDP process outside owned descendants');
  const components = [...last].sort((x,y) => x-y).map(pid => {
    const row = b.get(pid);
    if (!Number.isSafeInteger(row.rssKiB) || row.rssKiB < 0) fail('owned RSS unavailable');
    const rssBytes = row.rssKiB * 1024;
    if (!Number.isSafeInteger(rssBytes)) fail('owned RSS exceeds exact integer range');
    return {pid, ppid:row.ppid, birth:row.birth, rssBytes, cdpType:cdpProcesses.find(p => p.id === pid)?.type ?? 'unclassified_owned_descendant'};
  });
  const rawRssSumBytes=components.reduce((sum,row)=>sum+row.rssBytes,0);
  if(!Number.isSafeInteger(rawRssSumBytes))fail('owned RSS sum exceeds exact integer range');
  return {browserPid,components,rawRssSumBytes,cdpProcessFields:[...new Set(cdpProcesses.flatMap(Object.keys))].sort(),semantics:'sum_of_owned_process_rss_shared_pages_may_repeat',gpuAllocatedBytes:null,completeApplicationCacheBytes:null};
}
