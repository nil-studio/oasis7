#!/usr/bin/env python3
"""Export offline development tasks and compare externally recorded attempts.

No agent, network, repository command or benchmark execution is started here.
The checks cover fixture answer fields, not the correctness of an entire review.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import sys

CASES = Path(__file__).resolve().parents[1] / "assets" / "cases.json"
METRICS = ("wall_seconds", "model_tokens", "runner_seconds", "cost_usd", "human_interventions")
CONTEXT = ("model", "model_settings", "tools", "environment", "memory_snapshot", "cache_state", "budget")
STATUSES = {"completed", "failed", "timeout", "cancelled"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path):
    def invalid(value):
        raise ValueError(f"non-finite JSON number: {value}")
    def finite_float(value):
        parsed = float(value)
        require(math.isfinite(parsed), f"non-finite JSON number: {value}")
        return parsed
    return json.loads(Path(path).read_text(encoding="utf-8"),
                      object_pairs_hook=unique_object, parse_constant=invalid, parse_float=finite_float)


def digest(value):
    raw = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def keys(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), f"{label}: expected fields {sorted(expected)}")


def nonempty(value, label):
    require(isinstance(value, str) and bool(value.strip()), f"{label}: expected non-empty text")


def number(value, label, integer=False, positive=False):
    valid = type(value) is int if integer else type(value) in (int, float)
    require(valid and math.isfinite(value) and value >= 0, f"{label}: expected finite non-negative number")
    require(not positive or value > 0, f"{label}: expected positive number")


def task(case):
    return {key: case[key] for key in ("id", "family", "prompt", "input", "answer_format")}


def load_suite(path):
    suite = read_json(path)
    keys(suite, {"schema_version", "cases"}, "suite")
    require(type(suite["schema_version"]) is int and suite["schema_version"] == 1, "unsupported suite version")
    require(isinstance(suite["cases"], list) and suite["cases"], "suite has no cases")
    seen = set()
    for case in suite["cases"]:
        keys(case, {"id", "split", "family", "prompt", "input", "answer_format", "expected", "provenance"}, "case")
        name = case["id"]
        require(isinstance(name, str) and re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,63}", name), "unsafe case id")
        require(name not in seen, f"duplicate case: {name}")
        seen.add(name)
        require(case["split"] in ("pilot", "holdout"), f"{name}: invalid split")
        for field in ("family", "prompt"):
            nonempty(case[field], f"{name}.{field}")
        for field in ("input", "answer_format", "expected", "provenance"):
            require(isinstance(case[field], dict) and case[field], f"{name}.{field}: expected non-empty object")
        require(set(case["expected"]) <= set(case["answer_format"]), f"{name}: undocumented answer fields")
        for value in case["expected"].values():
            if isinstance(value, list):
                require(all(isinstance(item, str) for item in value) and len(value) == len(set(value)),
                        f"{name}: expected lists must contain unique strings")
            else:
                require(value is None or type(value) in (str, int, float, bool), f"{name}: unsupported expected value")
                if type(value) in (int, float):
                    require(math.isfinite(value), f"{name}: non-finite answer")
    return suite


def make_plan(suite, split, repetitions):
    require(split in ("pilot", "holdout", "all"), "invalid split")
    number(repetitions, "repetitions", integer=True, positive=True)
    require(repetitions <= 100, "repetitions exceeds the offline pilot limit of 100")
    cases = [case for case in suite["cases"] if split == "all" or case["split"] == split]
    require(bool(cases), "selected split is empty")
    plan = {"schema_version": 1, "suite_sha256": digest(suite), "split": split,
            "repetitions": repetitions, "case_ids": [case["id"] for case in cases],
            "task_sha256": {case["id"]: digest(task(case)) for case in cases}}
    return {**plan, "plan_sha256": digest(plan)}


def write_json(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.write("\n")


def prepare(suite, split, repetitions, output):
    plan = make_plan(suite, split, repetitions)
    root = Path(output)
    root.mkdir(parents=True, exist_ok=False)
    (root / "tasks").mkdir()
    for case in suite["cases"]:
        if case["id"] in plan["case_ids"]:
            write_json(root / "tasks" / (case["id"] + ".json"), task(case))
    write_json(root / "plan.json", plan)
    write_json(root / "results.template.json", {
        "schema_version": 1, "plan_sha256": plan["plan_sha256"], "method_ref": None,
        "context": {"model": None, "model_settings": {}, "tools": None, "environment": None,
                    "memory_snapshot": None, "cache_state": None,
                    "budget": {"max_seconds": None, "max_tokens": None}}, "runs": [],
    })
    return {"plan_sha256": plan["plan_sha256"], "tasks": len(plan["case_ids"]),
            "planned_attempts_per_method": len(plan["case_ids"]) * repetitions,
            "output": str(root), "executed": False}


def read_results(path, plan):
    result = read_json(path)
    keys(result, {"schema_version", "plan_sha256", "method_ref", "context", "runs"}, "results")
    require(type(result["schema_version"]) is int and result["schema_version"] == 1, "unsupported results version")
    require(result["plan_sha256"] == plan["plan_sha256"], "results do not match the fixed plan")
    nonempty(result["method_ref"], "method_ref")
    context = result["context"]
    keys(context, CONTEXT, "context")
    for field in ("model", "tools", "environment", "memory_snapshot"):
        nonempty(context[field], f"context.{field}")
    require(isinstance(context["model_settings"], dict), "model_settings must be an object")
    require(context["cache_state"] in ("cold", "warm", "not-applicable"), "invalid cache_state")
    keys(context["budget"], {"max_seconds", "max_tokens"}, "budget")
    number(context["budget"]["max_seconds"], "max_seconds", positive=True)
    number(context["budget"]["max_tokens"], "max_tokens", integer=True, positive=True)
    require(isinstance(result["runs"], list), "runs must be a list")
    indexed = {}
    for row in result["runs"]:
        keys(row, {"case_id", "attempt", "status", "answer", "evidence", *METRICS}, "run")
        require(isinstance(row["case_id"], str) and row["case_id"] in plan["case_ids"], "unplanned case")
        number(row["attempt"], "attempt", integer=True, positive=True)
        require(row["attempt"] <= plan["repetitions"], "unplanned attempt")
        key = (row["case_id"], row["attempt"])
        require(key not in indexed, f"duplicate attempt: {key}")
        require(isinstance(row["status"], str) and row["status"] in STATUSES, "invalid run status")
        require(isinstance(row["answer"], dict) or (row["answer"] is None and row["status"] != "completed"),
                "completed runs require an answer object")
        nonempty(row["evidence"], "evidence")
        for field in METRICS:
            if row[field] is not None:
                number(row[field], field, integer=field in ("model_tokens", "human_interventions"))
        indexed[key] = row
    return result, indexed


def equal_answer(actual, expected):
    if isinstance(expected, list):
        return (isinstance(actual, list) and all(isinstance(item, str) for item in actual)
                and len(actual) == len(set(actual)) and set(actual) == set(expected))
    if type(expected) in (int, float):
        return type(actual) in (int, float) and math.isfinite(actual) and actual == expected
    return type(actual) is type(expected) and actual == expected


def assess(row, case, budget):
    if row is None:
        return {"status": "missing", "checks_passed": None, "failed_fields": [],
                "budget_exceeded": [], "budget_status": "unknown"}
    failed = [field for field, expected in case["expected"].items()
              if not isinstance(row["answer"], dict) or field not in row["answer"]
              or not equal_answer(row["answer"][field], expected)]
    exceeded = [metric for metric, limit in (("wall_seconds", "max_seconds"), ("model_tokens", "max_tokens"))
                if row[metric] is not None and row[metric] > budget[limit]]
    budget_status = ("exceeded" if exceeded else "unknown" if any(row[field] is None
                     for field in ("wall_seconds", "model_tokens")) else "within")
    return {"status": row["status"], "checks_passed": row["status"] == "completed" and not failed,
            "failed_fields": failed, "budget_exceeded": exceeded, "budget_status": budget_status}


def totals(rows, expected_count):
    output = {}
    for metric in METRICS:
        values = [row[metric] for row in rows.values() if row[metric] is not None]
        known = sum(values)
        require(math.isfinite(known), f"{metric}: aggregate overflow")
        output[metric] = {"known_sum": known, "observed": len(values), "planned": expected_count,
                          "total": known if len(values) == expected_count else None}
    return output


def compare(suite, plan_path, baseline_path, candidate_path):
    plan = read_json(plan_path)
    require(isinstance(plan, dict) and "split" in plan and "repetitions" in plan, "invalid plan")
    require(digest(plan) == digest(make_plan(suite, plan["split"], plan["repetitions"])), "plan or suite fingerprint changed")
    baseline, left = read_results(baseline_path, plan)
    candidate, right = read_results(candidate_path, plan)
    require(digest(baseline["context"]) == digest(candidate["context"]), "incomparable contexts; use matched conditions")
    budget = baseline["context"]["budget"]
    cases = {case["id"]: case for case in suite["cases"]}
    pairs, both_pass_deltas = [], []
    for case_id in plan["case_ids"]:
        for attempt in range(1, plan["repetitions"] + 1):
            key = (case_id, attempt)
            a, b = assess(left.get(key), cases[case_id], budget), assess(right.get(key), cases[case_id], budget)
            if a["budget_status"] != "within" or b["budget_status"] != "within":
                outcome = "inconclusive"
            elif a["checks_passed"] == b["checks_passed"]:
                outcome = "tie" if a["checks_passed"] else "neither_passed"
            else:
                outcome = "win" if b["checks_passed"] else "loss"
            delta = None
            if a["checks_passed"] and b["checks_passed"] and a["budget_status"] == b["budget_status"] == "within":
                times = (left[key]["wall_seconds"], right[key]["wall_seconds"])
                if all(value is not None for value in times):
                    delta = times[1] - times[0]
                    both_pass_deltas.append(delta)
            pairs.append({"case_id": case_id, "attempt": attempt, "baseline": a, "candidate": b,
                          "outcome": outcome, "both_pass_wall_delta_seconds": delta})
    count = len(pairs)
    metrics = {"baseline": totals(left, count), "candidate": totals(right, count)}
    metrics["candidate_minus_baseline"] = {
        name: (metrics["candidate"][name]["total"] - metrics["baseline"][name]["total"]
               if all(metrics[side][name]["total"] is not None for side in ("baseline", "candidate")) else None)
        for name in METRICS}
    coverage = {side: {status: sum(pair[side]["status"] == status for pair in pairs)
                       for status in sorted(STATUSES | {"missing"})} for side in ("baseline", "candidate")}
    return {"schema_version": 1, "evaluation_scope": "contract_fixture",
            "evidence_verified": False, "plan_sha256": plan["plan_sha256"],
            "methods": {"baseline": baseline["method_ref"], "candidate": candidate["method_ref"]},
            "planned_pairs": count, "coverage": coverage,
            "outcomes": {name: sum(pair["outcome"] == name for pair in pairs)
                         for name in ("win", "loss", "tie", "neither_passed", "inconclusive")},
            "fixture_passes": {side: sum(pair[side]["checks_passed"] is True for pair in pairs)
                               for side in ("baseline", "candidate")},
            "budget": {side: {status: sum(pair[side]["budget_status"] == status for pair in pairs)
                              for status in ("within", "exceeded", "unknown")}
                       for side in ("baseline", "candidate")},
            "metrics": metrics,
            "both_pass_timing": {"pairs": len(both_pass_deltas),
                                 "median_candidate_minus_baseline_seconds": statistics.median(both_pass_deltas)
                                 if both_pass_deltas else None},
            "pairs": pairs, "adoption": "not_decided",
            "limitations": ["Only specified fixture fields are checked; review the complete answers and evidence.",
                            "Inputs and costs are externally recorded, not independently authenticated.",
                            "This report does not approve a method, change permissions, or satisfy a merge gate."]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=CASES)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list", help="list task identifiers, families and splits without answers")
    export = commands.add_parser("prepare", help="freeze a plan and export tasks; never execute them")
    export.add_argument("--split", choices=("pilot", "holdout", "all"), default="pilot")
    export.add_argument("--repetitions", type=int, default=1)
    export.add_argument("--out", type=Path, required=True)
    report = commands.add_parser("compare", help="compare all attempts in the already fixed plan")
    for name in ("plan", "baseline", "candidate"):
        report.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        suite = load_suite(args.cases)
        if args.command == "list":
            output = [{key: case[key] for key in ("id", "family", "split")} for case in suite["cases"]]
        elif args.command == "prepare":
            output = prepare(suite, args.split, args.repetitions, args.out)
        else:
            output = compare(suite, args.plan, args.baseline, args.candidate)
        print(json.dumps(output, ensure_ascii=False, indent=2, allow_nan=False))
    except (OSError, ValueError, TypeError, OverflowError) as error:
        parser.exit(2, f"trial: {error}\n")


if __name__ == "__main__":
    main()
