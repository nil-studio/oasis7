"""Fixed-context inventory diagnostics and A/B; never security/SARIF acceptance."""
from collections import Counter
import csv
import hashlib
import json
import os
from pathlib import Path
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_trait_scope_plan as plan

ARM_SECONDS = 180
MAX_DATABASE_BYTES = 2 * 1024**3
MAX_INVENTORY_MAP_BYTES = 4 * 1024**2
AFTER_MAP_RESERVE = 2 * MAX_INVENTORY_MAP_BYTES + 4 * runtime.MAX_PROCESS_RECEIPT
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


def inventory_bytes(value):
    encoded = (json.dumps(value,indent=2)+'\n').encode()
    if len(encoded) > MAX_INVENTORY_MAP_BYTES:
        raise RuntimeError("complete database inventory exceeds 4MiB bound")
    return encoded


def reserve_after_maps(output, baseline):
    """Constrain query output while leaving both complete after maps writable."""
    original = runtime.WRITE_CAP
    limit = original-AFTER_MAP_RESERVE
    current = runtime.output_bytes(output)
    if limit-current < 1024**2:
        raise RuntimeError("NOT_REACHED insufficient reserved inventory/query evidence space")
    runtime.save(output/'inventory-space-reservation.json', {
        "baseline_inventory_bytes": len(inventory_bytes(baseline)),
        "maximum_after_map_bytes_each": MAX_INVENTORY_MAP_BYTES,
        "after_maps_and_four_process_receipts_bytes": AFTER_MAP_RESERVE,
        "query_write_cap_bytes": limit,
        "final_receipts_reserved_bytes": runtime.CAP-original,
        "query_output_remaining_before_this_receipt_bytes": limit-current},output)
    if limit-runtime.output_bytes(output) < 1024**2:
        raise RuntimeError("NOT_REACHED insufficient query evidence after reservation receipt")
    runtime.WRITE_CAP = limit
    return original


def after_facts(source, arm, baseline, output=None, prefix="after"):
    """Report both inventories even if one changed; do not waive any difference."""
    details = {}
    for label, database in (("source", source), ("arm", arm)):
        try:
            current, size = facts(database)
            added = sorted(set(current)-set(baseline))
            removed = sorted(set(baseline)-set(current))
            changed = sorted(n for n in set(current)&set(baseline) if current[n] != baseline[n])
            record = {"status": "UNCHANGED" if current == baseline else "CHANGED",
                "files": len(current), "bytes": size,
                "inventory_sha256": hashlib.sha256(json.dumps(current,sort_keys=True).encode()).hexdigest(),
                "added_count": len(added), "removed_count": len(removed), "changed_count": len(changed),
                "details_complete": True}
            for category, names in (("added", added), ("removed", removed), ("changed", changed)):
                record[category] = [{"path": n[:256],
                    "path_sha256": hashlib.sha256(n.encode()).hexdigest(),
                    "before_sha256": baseline.get(n), "after_sha256": current.get(n)} for n in names[:4]]
                if len(names)>4 or any(len(n)>256 for n in names):
                    record["details_complete"] = False
            if output is not None:
                inventory_path = output / (prefix+"-"+label+"-inventory.json")
                runtime.evidence_write(inventory_path,inventory_bytes(current),output)
                record["inventory_file"] = inventory_path.name
            details[label] = record
        except Exception as exc:
            details[label] = {"status": "UNKNOWN", "error": repr(exc)[:1024], "details_complete": False}
    return all(v["status"] == "UNCHANGED" for v in details.values()), details


def clean_query_timeout(exc):
    if not isinstance(exc, runtime.PhaseError) or not exc.clean_timeout:
        return False
    value = exc.result
    return (value.get("argv", [])[1:3] == ["database", "run-queries"] and
        type(value.get("pid")) is int and value["pid"] > 0 and
        value.get("pgid") == value["pid"] and type(value.get("exit")) is int and
        isinstance(value.get("started_wall"), (int, float)) and
        isinstance(value.get("elapsed"), (int, float)) and value["elapsed"] > 0)


def execute(args, env, closure, verify_library, database, texts, libp2p_root, wanted, model, files_only=False):
    out, work = args.output, args.work
    cli = str(args.codeql)
    if files_only and tuple(texts) != ("Files",):
        raise RuntimeError("Files inventory mode requires exactly one Files query")
    baseline, size = facts(database)
    if shutil.disk_usage(work).free < max(4 * 1024**3, size*3 + 1024**3):
        raise RuntimeError("insufficient space for two independent database copies")
    initial_required = 335 if files_only else 540+20*len(texts)
    if args.deadline-time.monotonic() < initial_required:
        raise RuntimeError("NOT_REACHED insufficient two-arm and terminal budget")
    runtime.save(out / "database-facts.json", {"sha256": baseline, "bytes": size,
        "source": model.SOURCE, "extractions": 1, "omitted_mutable_paths": sorted(OMIT | {"db-rust/default/cache"})}, out)
    isolated = None
    if not files_only:
        before = plan.inventory(closure, args.deadline-90)
        isolated = plan.prepare_copy(args, closure, before, "successor-keyed-plan")
    copies = {}
    copies_root = work / "comparison-databases"
    copies_root.mkdir(exist_ok=False)
    labels = ("original",) if files_only else ("original", "candidate")
    state = {label: {"status": "NOT_REACHED"} for label in labels}
    runtime.save(out / "comparison-terminal.json", state, out)
    for label in labels:
        target = copies_root / label
        if target.resolve() == database.resolve():
            raise RuntimeError("comparison copy aliases extraction database")
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
    for label, db in copies.items():
        intact, before_detail = after_facts(database, db, baseline, out if files_only else None, "before")
        if not intact:
            state[label] = {"status": "FAILED_PREFLIGHT_FACTS", "facts_before": before_detail}
            runtime.save(out/"comparison-terminal.json",state,out,terminal=True)
            raise RuntimeError("database facts changed before arm")
        selected = {k: dict(v) for k,v in closure.items()}
        if label == "candidate":
            selected["codeql/rust-all"]["root"] = str(isolated)
        packs = os.pathsep.join(p["root"] for p in selected.values())
        cache, common = work / (label+"-cold-compile"), work / (label+"-cold-common")
        cache.mkdir(exist_ok=False); common.mkdir(exist_ok=False)
        options = ["--additional-packs="+packs, "--compilation-cache="+str(cache),
            "--no-default-compilation-cache", "--common-caches="+str(common), "--threads=2", "--ram=3072"]
        completed, tables = {}, {}
        state[label] = {"status": "RUNNING", "shared_budget_seconds": ARM_SECONDS, "facts_before": before_detail,
            "cache_start": "empty independent DB/results/compilation/common caches; intra-arm reuse",
            "queries": {n: "NOT_REACHED" for n in texts}}
        failure, pending_summaries, original_write_cap, query_attempted = None, [], None, False
        try:
            for name in texts:
                query = out / "queries" / (name+".ql")
                resolved = out / (label+"-"+name+"-library.json")
                runtime.run([cli,"resolve","library-path","--query="+str(query),"--format=json",
                    "--additional-packs="+packs],work,resolved,args.deadline-315,20,out,env)
                verify_library(json.loads(resolved.read_text()),query.parent,name,selected)
            required = (2*(ARM_SECONDS+45)+90+20*len(texts)
                if label == "original" and not files_only else ARM_SECONDS+135)
            if args.deadline-time.monotonic() < required:
                raise RuntimeError("NOT_REACHED insufficient full arm and final reserve")
            if files_only:
                original_write_cap = reserve_after_maps(out,baseline)
            end = time.monotonic()+ARM_SECONDS
            for name, text in texts.items():
                state[label]["queries"][name] = "RUNNING"
                query = out / "queries" / (name+".ql")
                log = out / (label+"-"+name+"-evaluator.jsonl")
                query_error, query_success = None, False
                try:
                    query_attempted = True
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
                        pending_summaries.append((log,query_error))
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
                with decoded.open() as stream:
                    table = list(csv.reader(stream))
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
            if not files_only:
                model.complete_union(completed)
            arms[label] = tables
            state[label]["status"] = "FILES_VALIDATED" if files_only else "COMPLETE_VALIDATED"
        except BaseException as exc:
            failure = exc
            clean = clean_query_timeout(exc)
            if query_attempted:
                state[label]["queries"][name] = "CLEAN_QUERY_TIMEOUT" if clean else "FAILED"
                status = "CLEAN_QUERY_TIMEOUT" if clean else "FAILED"
            else:
                status = "NOT_REACHED"
            state[label].update(status=status,error=repr(exc)[:2048])
            if not clean: raise
        finally:
            post_error = None
            if query_attempted and (failure is None or clean_query_timeout(failure)):
                summary_end = min(args.deadline-90,time.monotonic()+45)
                try:
                    for log, query_error in pending_summaries:
                        summary = runtime.official_summaries(cli,log,work,out,summary_end,env,query_error)
                        runtime.save(out/(log.stem+"-summary-status.json"),summary,out)
                        if set(summary) != {"overall", "predicates"} or any(
                                v != "GENERATED_NOT_EVALUATION_COMPLETION" for v in summary.values()):
                            raise RuntimeError("official summary incomplete or unsafe; stop comparison")
                except BaseException as exc:
                    post_error = exc
                    state[label].update(status="FAILED_POSTPROCESSING",postprocessing_error=repr(exc)[:2048])
            if original_write_cap is not None:
                runtime.WRITE_CAP = original_write_cap
            intact = True
            if query_attempted:
                intact, detail = after_facts(database, db, baseline, out if files_only else None, "after")
                state[label]["facts_unchanged"] = intact
                state[label]["facts_after"] = detail
                if not intact:
                    state[label]["pre_fact_status"] = state[label]["status"]
                    state[label]["status"] = "FAILED_FACTS_AFTER"
            else:
                state[label]["facts_after_status"] = "NOT_REACHED_NO_QUERY"
            runtime.save(out/"comparison-terminal.json",state,out,terminal=True)
            if not intact: raise RuntimeError("database facts changed after arm") from failure
            if post_error is not None: raise post_error
    if files_only:
        runtime.save(out/"files-inventory-result.json", {"status": state["original"]["status"],
            "full_rust_coverage": "HOLD", "candidate_queries": 0, "model_comparison": "NOT_PLANNED"}, out, terminal=True)
        return
    result = {"status":"INCOMPLETE_EQUALITY_UNKNOWN","full_rust_coverage":"HOLD",
        "runtime_performance":"NOT_ESTABLISHED", "relation_counts":"inspect raw evaluator and official summaries; absent is UNKNOWN"}
    if set(arms)=={"original","candidate"}:
        result.update(status="COMPLETE_RESULTS_EQUAL",differences=same_results(arms["original"],arms["candidate"]))
    runtime.save(out/"comparison-result.json",result,out)
