"""One fixed-context, same-facts A/B experiment; never security/SARIF acceptance."""
from collections import Counter
import csv
import json
import os
from pathlib import Path
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_trait_scope_plan as plan

ARM_SECONDS = 180
MAX_DATABASE_BYTES = 2 * 1024**3
OMIT = {"results", "working", "log", "logs", ".cache", "query-cache", "common-cache", "compilation-cache"}


def facts(database):
    """Fingerprint the immutable extracted database, not query results/caches."""
    result, size = {}, 0
    for p in sorted(database.rglob("*")):
        relative = p.relative_to(database)
        if p.is_symlink():
            raise RuntimeError("symlink in database")
        if (relative.parts[0] in OMIT or relative.parts[:3] == ("db-rust", "default", "cache")):
            continue
        if not (p.is_dir() or p.is_file()):
            raise RuntimeError("nonregular database member")
        if p.is_file():
            size += p.stat().st_size
            if len(result) >= 20000 or size > MAX_DATABASE_BYTES:
                raise RuntimeError("database inventory limit")
            result[str(relative)] = runtime.digest(p)
    if not (database / "codeql-database.yml").is_file() or not any(n.startswith("db-rust/") for n in result):
        raise RuntimeError("missing extracted Rust database facts")
    return result, size


def same_results(original, candidate):
    if set(original) != set(candidate):
        raise RuntimeError("incomplete result domains")
    differences = {}
    for name in original:
        left, right = Counter(map(tuple, original[name])), Counter(map(tuple, candidate[name]))
        differences[name] = {"original_only": sum((left-right).values()),
                             "candidate_only": sum((right-left).values())}
    if any(any(v.values()) for v in differences.values()):
        raise RuntimeError("original/candidate complete results differ: " + repr(differences))
    return differences


def clean_query_timeout(exc):
    if not isinstance(exc, runtime.PhaseError) or not exc.clean_timeout:
        return False
    value = exc.result
    return (value.get("argv", [])[1:3] == ["database", "run-queries"] and
        type(value.get("pid")) is int and value["pid"] > 0 and
        value.get("pgid") == value["pid"] and type(value.get("exit")) is int and
        isinstance(value.get("started_wall"), (int, float)) and
        isinstance(value.get("elapsed"), (int, float)) and value["elapsed"] > 0)


def execute(args, env, closure, verify_library, database, texts, libp2p_root, wanted, model):
    out, work = args.output, args.work
    cli = str(args.codeql)
    baseline, size = facts(database)
    if shutil.disk_usage(work).free < max(4 * 1024**3, size*3 + 1024**3):
        raise RuntimeError("insufficient space for two independent database copies")
    if args.deadline-time.monotonic() < 600:
        raise RuntimeError("NOT_REACHED insufficient two-arm and terminal budget")
    runtime.save(out / "database-facts.json", {"sha256": baseline, "bytes": size,
        "source": model.SOURCE, "extractions": 1, "omitted_mutable_paths": sorted(OMIT | {"db-rust/default/cache"})}, out)
    before = plan.inventory(closure, args.deadline-90)
    isolated = plan.prepare_copy(args, closure, before, "successor-keyed-plan")
    copies = {}
    for label in ("original", "candidate"):
        target = work / (label + "-db")
        target.mkdir(exist_ok=False)
        for name in baseline:
            plan.check_time(args.deadline-90)
            dest = target / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(database / name, dest)
        if facts(target)[0] != baseline:
            raise RuntimeError("copied database facts differ")
        copies[label] = target
    arms = {}
    state = {label: {"status": "NOT_REACHED"} for label in copies}
    runtime.save(out / "comparison-terminal.json", state, out)
    for label, db in copies.items():
        if facts(database)[0] != baseline or facts(db)[0] != baseline:
            raise RuntimeError("database facts changed before arm")
        selected = {k: dict(v) for k,v in closure.items()}
        if label == "candidate":
            selected["codeql/rust-all"]["root"] = str(isolated)
        packs = os.pathsep.join(p["root"] for p in selected.values())
        cache, common = work / (label+"-cold-compile"), work / (label+"-cold-common")
        cache.mkdir(exist_ok=False); common.mkdir(exist_ok=False)
        options = ["--additional-packs="+packs, "--compilation-cache="+str(cache),
            "--no-default-compilation-cache", "--common-caches="+str(common), "--threads=2", "--ram=3072"]
        if args.deadline-time.monotonic() < ARM_SECONDS+135:
            raise RuntimeError("NOT_REACHED insufficient full arm and final reserve")
        end = time.monotonic()+ARM_SECONDS
        completed, tables = {}, {}
        state[label] = {"status": "RUNNING", "shared_budget_seconds": ARM_SECONDS,
            "cache_start": "empty independent DB/results/compilation/common caches; intra-arm reuse",
            "queries": {n: "NOT_REACHED" for n in texts}}
        failure = None
        try:
            for name, text in texts.items():
                state[label]["queries"][name] = "RUNNING"
                query = out / "queries" / (name+".ql")
                resolved = out / (label+"-"+name+"-library.json")
                runtime.run([cli,"resolve","library-path","--query="+str(query),"--format=json",
                    "--additional-packs="+packs],work,resolved,end,20,out,env)
                verify_library(json.loads(resolved.read_text()),query.parent,name,selected)
                log = out / (label+"-"+name+"-evaluator.jsonl")
                query_error, query_success = None, False
                try:
                    runtime.run([cli,"database","run-queries",str(db),str(query),*options,
                        "--max-disk-cache=1024","--evaluator-log","{EVALUATOR_FIFO}",
                        "--evaluator-log-minify","--tuple-counting","--verbosity=progress"],
                        args.business_source,out/(label+"-"+name+"-eval.log"),end,ARM_SECONDS,out,env,
                        runtime.DATA_LIMIT,log)
                    query_success = True
                except runtime.PhaseError as exc:
                    query_error = exc
                    raise
                finally:
                    if log.exists() and (query_success or clean_query_timeout(query_error)):
                        # Summary work is outside each equal query clock, inside the fixed job clock.
                        summary = runtime.official_summaries(cli,log,work,out,
                            min(args.deadline-90,time.monotonic()+45),env,query_error)
                        runtime.save(out/(label+"-"+name+"-summary-status.json"),summary,out)
                        if set(summary) != {"overall", "predicates"} or any(
                                v != "GENERATED_NOT_EVALUATION_COMPLETION" for v in summary.values()):
                            raise RuntimeError("official summary incomplete or unsafe; stop comparison")
                results = list((db/"results").rglob(name+".bqrs"))
                if len(results) != 1:
                    raise RuntimeError("missing/ambiguous complete BQRS")
                bqrs = out/(label+"-"+name+".bqrs")
                if results[0].stat().st_size > runtime.WRITE_CAP-runtime.output_bytes(out):
                    raise RuntimeError("complete BQRS exceeds remaining evidence cap")
                runtime.evidence_write(bqrs,results[0].read_bytes(),out)
                decoded = out/(label+"-"+name+".csv")
                runtime.run([cli,"bqrs","decode",str(bqrs),"--format=csv"],work,decoded,
                    end,20,out,env)
                table = list(csv.reader(decoded.open()))
                if not table:
                    raise RuntimeError("empty decoded table")
                if name == "Files":
                    if table[0] != ["col0","col1"]: raise RuntimeError("Files schema")
                    model.check_files(table[1:],wanted)
                elif name == "CallsCandidate":
                    if table[0] != ["col0","col1","col2","col3"]: raise RuntimeError("callee schema")
                    rows = table[1:]
                    if len(rows)!=1 or rows[0][:3] != [str(args.business_source/model.BASE/"candidate/src/adapter.rs"),"42","with_existing_identity"] or not Path(rows[0][3]).is_relative_to(libp2p_root):
                        raise RuntimeError("callee source oracle mismatch")
                else:
                    if table[0] != ["label","present"]: raise RuntimeError("witness schema")
                    completed[name] = model.validate_partition(name,text,table[1:])
                tables[name] = table
                state[label]["queries"][name] = "DECODED_VALIDATED"
            model.complete_union(completed)
            arms[label] = tables
            state[label]["status"] = "COMPLETE_VALIDATED"
        except BaseException as exc:
            failure = exc
            clean = clean_query_timeout(exc)
            state[label]["queries"][name] = "CLEAN_QUERY_TIMEOUT" if clean else "FAILED"
            state[label].update(status="CLEAN_QUERY_TIMEOUT" if clean else "FAILED",error=repr(exc)[:2048])
            if not clean: raise
        finally:
            intact = facts(db)[0] == baseline and facts(database)[0] == baseline
            state[label]["facts_unchanged"] = intact
            runtime.save(out/"comparison-terminal.json",state,out,terminal=True)
            if not intact: raise RuntimeError("database facts changed after arm") from failure
    result = {"status":"INCOMPLETE_EQUALITY_UNKNOWN","full_rust_coverage":"HOLD",
        "runtime_performance":"NOT_ESTABLISHED", "relation_counts":"inspect raw evaluator and official summaries; absent is UNKNOWN"}
    if set(arms)=={"original","candidate"}:
        result.update(status="COMPLETE_RESULTS_EQUAL",differences=same_results(arms["original"],arms["candidate"]))
    runtime.save(out/"comparison-result.json",result,out)
