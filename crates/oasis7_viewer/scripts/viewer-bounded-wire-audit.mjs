// A terminal observer budget, independent of application/protocol budgets.
export function createBoundedWireAudit({limitBytes,append,persistFailure,stopSource}) {
 if(!Number.isSafeInteger(limitBytes)||limitBytes<=0)throw Error('invalid wire observation limit');
 let bytes=0,rows=0,error=null;
 return {
  accepting:()=>error===null,
  failure:()=>error,
  statistics:()=>({limitBytes,bytes,rows,failed:error!==null}),
  record(row) {
   if(error)return false;
   const line=JSON.stringify(row)+'\n';append(line);bytes+=Buffer.byteLength(line);rows++;
   if(bytes>limitBytes) {
    error=Error('streamed wire byte limit exceeded; crossing row preserved; source terminal');
    try{persistFailure({limitBytes,bytes,rows,error:String(error)});}catch(cause){error.persistenceError=String(cause);}
    try{stopSource();}catch(cause){error.stopSourceError=String(cause);}
    return false;
   }
   return true;
  },
 };
}
