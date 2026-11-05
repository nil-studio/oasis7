import {readFileSync} from 'node:fs';
import {dirname, resolve, basename} from 'node:path';
import {fileURLToPath} from 'node:url';
// Actual workerd structured-log capture only; no synthetic RPC or counting service.
// Installed Miniflare handleStructuredLogs receives {timestamp, level, message}.
const prefix="oasis7.physical-edge.v1 ";
const ports=new Set([
  "source.verify.service","native.call",
  "owner.permit.service","owner.permit.account","directory.actor",
  "fence.service","fence.actor","identity.driver.service","identity.manifest.actor",
  "private.cookie.before","private.cookie.after","private.csrf.before","private.csrf.after",
  "owner.factory.service","owner.vault.actor","world.submit","world.lookup",
]);
const stages=new Set(["source","create_task","prepare_goal","publish_goal","delegation","bind","final_observation","reconcile"]);
const phases=new Set(["start","success","unknown","rejected"]);
export function ownerSetupPhysicalTrace() {
  const events=[];let invalid=0;
  return {
    capture(log) {
      if(typeof log?.message!=="string" || !log.message.startsWith(prefix)) return false;
      const parts=log.message.slice(prefix.length).split(" ");
      if(events.length>=4096 || parts.length!==3 || !ports.has(parts[0]) || !stages.has(parts[1]) || !phases.has(parts[2]) || !Number.isFinite(log.timestamp)) {invalid++;return true;}
      events.push(Object.freeze({port:parts[0],stage:parts[1],phase:parts[2],timestamp:log.timestamp}));
      return true;
    },
    snapshot() {if(invalid)throw new Error("invalid fixed physical trace event");return events.map(v=>({...v}));},
    countStarts(stage) {if(invalid)throw new Error("invalid fixed physical trace event");return events.filter(v=>v.phase==="start" && (stage===undefined||v.stage===stage)).length;},
  };
}

// Preserve module identities so existing real-role wrapper imports still resolve.
// Separate trace artifacts supply the actual bytes; normal artifacts are untouched.
export function configureOwnerSetupPhysicalTrace(raw, trace) {
  const root=resolve(dirname(fileURLToPath(import.meta.url)),"../..");
  const roles=new Map([["owner","owner"],["identity","identity"],["fence","recovery-fence"]]);
  const replaced=new Set();
  const workers=raw.workers.map(worker=>{
    const role=roles.get(worker.name);if(!role)return worker;
    let count=0;
    const modules=worker.modules.map(module=>{
      if(!module.path?.startsWith(resolve(root,"artifacts",role)+"/"))return module;
      const file=basename(module.path);
      if(!["entry.mjs","index.js","index_bg.wasm"].includes(file))throw new Error("unexpected physical trace module");
      count++;
      const bytes=readFileSync(resolve(root,"artifacts","owner-setup-trace-"+role,file));
      return {...module,contents:module.type==="CompiledWasm"?bytes:bytes.toString("utf8")};
    });
    if(count!==3)throw new Error("physical trace role requires three actual modules");
    replaced.add(worker.name);return {...worker,modules};
  });
  if(replaced.size!==3)throw new Error("physical trace requires real Owner, Identity and Fence");
  const prior=raw.handleStructuredLogs;
  return {...raw,workers,handleStructuredLogs(log){
    if(trace.capture(log))return;
    if(prior){prior(log);return;}
    if(log.level==="error"||log.level==="warn")console.error(log.message);
    else console.log(log.message);
  }};
}
export function physicalStartCounts(events) {
  const counts={};
  for(const event of events)if(event.phase==="start")counts[event.port]=(counts[event.port]??0)+1;
  return counts;
}
