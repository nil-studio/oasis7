"""The fixed Files -> PeerAst prefix, checked after each query; no A/B acceptance."""
import csv
import json
import os
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_successor_evaluation as inventory

SEQUENCE = ("Files", "PeerAst")


def spawned(result):
    return (isinstance(result,dict) and type(result.get("pid")) is int and result["pid"]>0 and
        result.get("pgid")==result["pid"] and isinstance(result.get("started_wall"),(int,float)))


def quiescent(result):
    if result.get("cleanup_errors") or result.get("evidence_errors"):
        return False
    return (result.get("pid") is None or
        (spawned(result) and type(result.get("exit")) is int))


def execute(args, env, closure, verify_library, database, texts, libp2p_root, wanted, model):
    if tuple(texts) != SEQUENCE:
        raise RuntimeError("inventory prefix must be exactly Files then PeerAst")
    out, work, cli = args.output, args.work, str(args.codeql)
    baseline, size = inventory.facts(database,deadline=args.deadline-90)
    if args.deadline-time.monotonic() < 355 or shutil.disk_usage(work).free < max(4*1024**3, size*2+1024**3):
        raise RuntimeError("NOT_REACHED prefix preparation budget/space insufficient")
    runtime.save(out/"database-facts.json", {"sha256":baseline,"bytes":size,"source":model.SOURCE,
        "extractions":1,"omitted_mutable_paths":sorted(inventory.OMIT|{"db-rust/default/cache"})},out)
    db = work/"peer-ast-inventory-db"
    if db.resolve()==database.resolve():
        raise RuntimeError("prefix copy aliases extraction database")
    db.mkdir(exist_ok=False)
    for name in baseline:
        if time.monotonic() >= args.deadline-90:
            raise RuntimeError("NOT_REACHED prefix copy deadline exhausted")
        target=db/name;target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(database/name,target)
    intact, before = inventory.after_facts(database,db,baseline,out,"before",deadline=args.deadline-315)
    state={"sequence":list(SEQUENCE),"full_rust_coverage":"HOLD","candidate_queries":0,
        "facts_before":before,"queries":{n:{"status":"NOT_REACHED"} for n in SEQUENCE}}
    runtime.save(out/"prefix-terminal.json",state,out)
    if not intact:
        raise RuntimeError("prefix copied facts changed or unknown")
    packs=os.pathsep.join(p["root"] for p in closure.values())
    cache, common=work/"prefix-cold-compile",work/"prefix-cold-common"
    cache.mkdir(exist_ok=False);common.mkdir(exist_ok=False)
    options=["--additional-packs="+packs,"--compilation-cache="+str(cache),
        "--no-default-compilation-cache","--common-caches="+str(common),"--threads=2","--ram=3072"]
    for name in SEQUENCE:
        query=out/"queries"/(name+".ql")
        resolved=out/("prefix-"+name+"-library.json")
        runtime.run([cli,"resolve","library-path","--query="+str(query),"--format=json",
            "--additional-packs="+packs],work,resolved,args.deadline-315,20,out,env)
        verify_library(json.loads(resolved.read_text()),query.parent,name,closure)
    if args.deadline-time.monotonic() < 315:
        raise RuntimeError("NOT_REACHED prefix query/final reserve insufficient")
    end=time.monotonic()+180
    state["shared_query_monotonic_end"]=end
    state["query_window_includes_intermediate_snapshots_and_summaries"]=True
    summary_remaining=45.0
    for name in SEQUENCE:
        phase=state["queries"][name]
        query=out/"queries"/(name+".ql")
        evaluator=out/("prefix-"+name+"-evaluator.jsonl")
        original_cap=None;attempted=False;failure=None;query_error=None;safe_boundary=True
        try:
            if time.monotonic() >= end:
                raise RuntimeError("NOT_REACHED shared prefix clock exhausted")
            original_cap=inventory.reserve_after_maps(out,baseline)
            runtime.evidence_write(out/("prefix-"+name+"-space-reservation.json"),
                (out/"inventory-space-reservation.json").read_bytes(),out)
            try:
                receipt=runtime.run([cli,"database","run-queries",str(db),str(query),*options,"--max-disk-cache=1024",
                "--evaluator-log","{EVALUATOR_FIFO}","--evaluator-log-minify","--tuple-counting","--verbosity=progress"],
                args.business_source,out/("prefix-"+name+"-eval.log"),end,180,out,env,runtime.DATA_LIMIT,evaluator)
            except runtime.PhaseError as exc:
                attempted=spawned(exc.result);safe_boundary=quiescent(exc.result)
                raise
            attempted=spawned(receipt);safe_boundary=quiescent(receipt) if isinstance(receipt,dict) else False
            if not attempted or not safe_boundary:
                raise RuntimeError("prefix query lacks clean spawned process receipt")
            results=list((db/"results").rglob(name+".bqrs"))
            if len(results)!=1 or results[0].stat().st_size > runtime.WRITE_CAP-runtime.output_bytes(out):
                raise RuntimeError("missing/ambiguous or over-budget prefix BQRS")
            bqrs=out/("prefix-"+name+".bqrs")
            runtime.evidence_write(bqrs,results[0].read_bytes(),out)
            decoded=out/("prefix-"+name+".csv")
            runtime.run([cli,"bqrs","decode",str(bqrs),"--format=csv"],work,decoded,end,20,out,env)
            with decoded.open() as stream:table=list(csv.reader(stream))
            expected=["col0","col1"] if name=="Files" else ["label","present"]
            if not table or table[0]!=expected:
                raise RuntimeError("prefix CSV schema mismatch")
            if name=="Files":model.check_files(table[1:],wanted)
            else:phase["metrics"]=model.validate_partition(name,texts[name],table[1:])
            phase["status"]="DECODED_VALIDATED"
        except BaseException as exc:
            failure=exc
            if isinstance(exc,runtime.PhaseError):
                query_error=exc
                safe_boundary=safe_boundary and quiescent(exc.result)
            phase.update(status=("CLEAN_QUERY_TIMEOUT" if inventory.clean_query_timeout(exc) else
                "FAILED" if attempted else "NOT_REACHED"),error=repr(exc)[:2048])
        finally:
            if attempted and safe_boundary and (failure is None or inventory.clean_query_timeout(failure)):
                started=time.monotonic()
                try:
                    if not evaluator.is_file():raise RuntimeError("prefix evaluator log missing")
                    summary={}
                    for fmt in ("overall","predicates"):
                        runtime.run([cli,"generate","log-summary","--format="+fmt,"--minify-output",str(evaluator)],
                            work,out/(evaluator.stem+"-summary-"+fmt+".jsonl"),
                            min(end,args.deadline-90,started+summary_remaining),20,out,env)
                        summary[fmt]="GENERATED_NOT_EVALUATION_COMPLETION"
                    runtime.save(out/("prefix-"+name+"-summary-status.json"),summary,out)
                except BaseException as exc:
                    phase["summary_error"]=repr(exc)[:1024]
                    phase["pre_summary_status"]=phase["status"];phase["status"]="FAILED_SUMMARIES"
                    if isinstance(exc,runtime.PhaseError):safe_boundary=safe_boundary and quiescent(exc.result)
                    if failure is None:failure=exc
                summary_remaining=max(0,summary_remaining-(time.monotonic()-started))
            if original_cap is not None:runtime.WRITE_CAP=original_cap
            if attempted and safe_boundary:
                intact,detail=inventory.after_facts(database,db,baseline,out,"after-"+name,
                    deadline=(end if name=="Files" and failure is None else args.deadline-20))
                phase["facts_after"]=detail
                phase["facts_unchanged"]=intact
                if not intact:
                    phase["pre_fact_status"]=phase["status"];phase["status"]="FAILED_FACTS_AFTER"
                    if failure is None:failure=RuntimeError("prefix database inventory changed or unknown after "+name)
            elif attempted:
                phase["facts_after_status"]="UNKNOWN_UNSAFE_PROCESS_OR_EVIDENCE_BOUNDARY"
                phase["facts_after"]={side:{"status":"UNKNOWN","details_complete":False} for side in ("source","arm")}
            else:phase["facts_after_status"]="NOT_REACHED_NO_QUERY"
            state["summary_seconds_remaining"]=summary_remaining
            runtime.save(out/"prefix-terminal.json",state,out,terminal=True)
        if failure is not None:raise failure
    runtime.save(out/"prefix-result.json",{"status":"PREFIX_TRACKED_FACTS_UNCHANGED",
        "sequence":list(SEQUENCE),"full_rust_coverage":"HOLD","candidate_queries":0},out,terminal=True)
