#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify builtin wasm summaries collected from multi-runner CI jobs."
    )
    parser.add_argument(
        "--module-set",
        default="m1",
        help="Builtin wasm module set expected in summary files (default: m1).",
    )
    parser.add_argument(
        "--summary-dir",
        required=True,
        help="Directory containing per-runner summary JSON files.",
    )
    parser.add_argument(
        "--required-runners",
        default="linux-x86_64",
        help="Comma-separated runner labels required for the stable gate.",
    )
    parser.add_argument(
        "--expected-runners",
        default="linux-x86_64,darwin-arm64",
        help="Comma-separated runner labels expected for full cross-host evidence.",
    )
    parser.add_argument(
        "--expected-canonical-platform",
        default="linux-x86_64",
        help="Canonical container platform expected in every summary (default: linux-x86_64).",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_summary(path: pathlib.Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except Exception as exc:  # noqa: BLE001
        fail(f"failed to parse summary {path}: {exc}")

    required_keys = {
        "schema_version",
        "module_set",
        "runner",
        "current_platform",
        "host_platform",
        "canonical_platform",
        "module_count",
        "module_hashes",
        "manifest_platform_hashes",
        "identity_hashes",
        "identity_build_recipe",
        "receipt_evidence",
    }
    missing = sorted(required_keys - set(payload.keys()))
    if missing:
        fail(f"summary {path} missing required keys: {missing}")
    return payload


def verify_summary_shape(path: pathlib.Path, payload: dict, module_set: str) -> None:
    if payload["schema_version"] != 1:
        fail(f"summary {path} schema_version must be 1")

    if payload["module_set"] != module_set:
        fail(
            "summary {} module_set mismatch: expected={} actual={}".format(
                path, module_set, payload["module_set"]
            )
        )

    for key in (
        "module_hashes",
        "manifest_platform_hashes",
        "identity_hashes",
        "identity_build_recipe",
        "receipt_evidence",
    ):
        if not isinstance(payload[key], dict):
            fail(f"summary {path} field {key} must be an object")

    module_hashes = payload["module_hashes"]
    manifest_hashes = payload["manifest_platform_hashes"]
    identity_hashes = payload["identity_hashes"]
    receipt_evidence = payload["receipt_evidence"]

    if len(module_hashes) != payload["module_count"]:
        fail(
            f"summary {path} module_count mismatch: declared={payload['module_count']} actual={len(module_hashes)}"
        )

    if set(module_hashes.keys()) != set(manifest_hashes.keys()):
        fail(f"summary {path} module set mismatch between module_hashes and manifest_platform_hashes")

    if set(module_hashes.keys()) != set(identity_hashes.keys()):
        fail(f"summary {path} module set mismatch between module_hashes and identity_hashes")

    if set(module_hashes.keys()) != set(receipt_evidence.keys()):
        fail(f"summary {path} module set mismatch between module_hashes and receipt_evidence")

    build_recipe = payload["identity_build_recipe"]
    for key in ("builder_image_digest", "container_platform", "canonicalizer_version"):
        value = build_recipe.get(key)
        if not isinstance(value, str) or not value:
            fail(f"summary {path} identity_build_recipe missing {key}")
    if payload["canonical_platform"] != build_recipe["container_platform"]:
        fail(
            f"summary {path} canonical_platform mismatch: summary={payload['canonical_platform']} recipe={build_recipe['container_platform']}"
        )

    for module_id, module_hash in module_hashes.items():
        expected_hash = manifest_hashes[module_id]
        if module_hash != expected_hash:
            fail(
                "summary {} module {} hash mismatch built={} manifest={}".format(
                    path, module_id, module_hash, expected_hash
                )
            )
        evidence = receipt_evidence[module_id]
        if not isinstance(evidence, dict):
            fail(f"summary {path} receipt evidence for module {module_id} must be an object")
        required_evidence_keys = {
            "source_input_rule_version",
            "source_inputs",
            "source_hash",
            "build_manifest_hash",
            "wasm_hash",
            "builder_image_digest",
            "container_platform",
            "canonicalizer_version",
        }
        missing_evidence = sorted(required_evidence_keys - set(evidence.keys()))
        if missing_evidence:
            fail(
                f"summary {path} receipt evidence for module {module_id} missing keys: {missing_evidence}"
            )
        if type(evidence["source_input_rule_version"]) is not int or evidence["source_input_rule_version"] != 2:
            fail(f"summary {path} legacy/unknown source input rule for module {module_id}; canonical rebuild required")
        records = evidence["source_inputs"]
        if not isinstance(records, list) or not records:
            fail(f"summary {path} missing source input details for module {module_id}")
        paths = []
        for record in records:
            if not isinstance(record, dict) or set(record) != {"kind", "path", "sha256"} or record["kind"] not in {"source-file", "input-declaration"}:
                fail(f"summary {path} invalid source input record for module {module_id}")
            label, digest = record["path"], record["sha256"]
            if not isinstance(label, str) or not label or "\\" in label or any(part in {"", ".", ".."} for part in label.split("/")):
                fail(f"summary {path} invalid source input path for module {module_id}")
            try:
                label.encode("utf-8")
            except UnicodeEncodeError:
                fail(f"summary {path} non-UTF-8 source input path for module {module_id}")
            if not isinstance(digest, str) or len(digest) != 64 or any(ch not in "0123456789abcdef" for ch in digest):
                fail(f"summary {path} invalid source input digest for module {module_id}")
            paths.append((label, record["kind"]))
        if paths != sorted(set(paths)):
            fail(f"summary {path} noncanonical source input ordering for module {module_id}")
        if evidence["wasm_hash"] != module_hash:
            fail(
                f"summary {path} receipt wasm_hash mismatch for module {module_id}: receipt={evidence['wasm_hash']} built={module_hash}"
            )
        if evidence["builder_image_digest"] != build_recipe["builder_image_digest"]:
            fail(
                f"summary {path} receipt builder_image_digest mismatch for module {module_id}"
            )
        if evidence["container_platform"] != build_recipe["container_platform"]:
            fail(
                f"summary {path} receipt container_platform mismatch for module {module_id}"
            )
        if evidence["canonicalizer_version"] != build_recipe["canonicalizer_version"]:
            fail(
                f"summary {path} receipt canonicalizer_version mismatch for module {module_id}"
            )


def main() -> None:
    args = parse_args()
    module_set = args.module_set.strip()
    if not module_set:
        fail("--module-set must not be empty")

    summary_dir = pathlib.Path(args.summary_dir)
    if not summary_dir.exists():
        fail(f"summary dir does not exist: {summary_dir}")

    required_runners = {
        value.strip() for value in args.required_runners.split(",") if value.strip()
    }
    if not required_runners:
        fail("--required-runners has no valid entries")

    expected_runners = {
        value.strip() for value in args.expected_runners.split(",") if value.strip()
    }
    if not expected_runners:
        fail("--expected-runners has no valid entries")
    if not required_runners.issubset(expected_runners):
        fail(
            "--required-runners must be a subset of --expected-runners"
        )
    expected_canonical_platform = args.expected_canonical_platform.strip()
    if not expected_canonical_platform:
        fail("--expected-canonical-platform must not be empty")

    summary_paths = sorted(summary_dir.glob("*.json"))
    if not summary_paths:
        fail(f"no summary json files found in {summary_dir}")

    summaries_by_runner = {}
    for path in summary_paths:
        payload = load_summary(path)
        verify_summary_shape(path, payload, module_set)
        if payload["canonical_platform"] != expected_canonical_platform:
            fail(
                "summary {} canonical_platform mismatch: expected={} actual={}".format(
                    path, expected_canonical_platform, payload["canonical_platform"]
                )
            )
        runner = payload["runner"]
        if runner in summaries_by_runner:
            fail(f"duplicate runner summary detected for {runner}")
        summaries_by_runner[runner] = payload

    found_runners = set(summaries_by_runner.keys())
    missing_required_runners = sorted(required_runners - found_runners)
    missing_runners = sorted(expected_runners - found_runners)
    extra_runners = sorted(found_runners - expected_runners)
    if missing_required_runners:
        fail(f"missing required runner summaries: {missing_required_runners}")
    if extra_runners:
        fail(f"found unexpected runner summaries: {extra_runners}")

    baseline_runner = sorted(found_runners)[0]
    baseline = summaries_by_runner[baseline_runner]
    baseline_module_keys = set(baseline["module_hashes"].keys())
    baseline_identity_hashes = baseline["identity_hashes"]
    baseline_receipt_evidence = baseline["receipt_evidence"]
    baseline_canonical_platform = baseline["canonical_platform"]
    baseline_build_recipe = baseline["identity_build_recipe"]

    for runner in sorted(found_runners):
        payload = summaries_by_runner[runner]
        module_keys = set(payload["module_hashes"].keys())
        if module_keys != baseline_module_keys:
            fail(
                f"module key mismatch between runners baseline={baseline_runner} runner={runner}"
            )

        if payload["identity_hashes"] != baseline_identity_hashes:
            fail(
                f"identity hash mismatch between runners baseline={baseline_runner} runner={runner}"
            )
        if payload["receipt_evidence"] != baseline_receipt_evidence:
            fail(
                f"receipt evidence mismatch between runners baseline={baseline_runner} runner={runner}"
            )
        if payload["canonical_platform"] != baseline_canonical_platform:
            fail(
                f"canonical platform mismatch between runners baseline={baseline_runner} runner={runner}"
            )
        if payload["identity_build_recipe"] != baseline_build_recipe:
            fail(
                f"identity build recipe mismatch between runners baseline={baseline_runner} runner={runner}"
            )

    print(
        "{} multi-runner summary verify ok: runners={} module_count={} missing_optional_runners={}".format(
            module_set,
            ",".join(sorted(found_runners)),
            len(baseline_module_keys),
            ",".join(missing_runners) if missing_runners else "none",
        )
    )


if __name__ == "__main__":
    main()
