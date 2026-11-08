// A terminal observer budget, independent of application/protocol budgets.
export function createBoundedWireAudit({limitBytes,append,persistFailure,stopSource}) {
 if(!Number.isSafeInteger(limitBytes)||limitBytes<=0)throw Error('invalid wire observation limit');
 let bytes=0,rows=0,error=null,failed=false;
 const fail=(cause,rowPreserved)=>{
  failed=true;error=cause;
  try{persistFailure({limitBytes,bytes,rows,rowPreserved,error:String(cause)});}catch(secondary){try{error.persistenceError=String(secondary);}catch{}}
  try{stopSource();}catch(secondary){try{error.stopSourceError=String(secondary);}catch{}}
  return false;
 };
 return {
  accepting:()=>!failed,
  failure:()=>error,
  statistics:()=>({limitBytes,bytes,rows,failed}),
  record(row) {
   if(failed)return false;
   const line=JSON.stringify(row)+'\n';
   try{append(line);}catch(cause){return fail(cause,false);}
   bytes+=Buffer.byteLength(line);rows++;
   if(bytes>limitBytes)return fail(Error('streamed wire byte limit exceeded; crossing row preserved; source terminal'),true);
   return true;
  },
 };
}
