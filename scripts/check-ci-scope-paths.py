#!/usr/bin/env python3
"""Verify exact repository paths and fast-path anti-fallback invariants for CI scope."""

from __future__ import annotations

import argparse
import ast
import fnmatch
import hashlib
import importlib.util
import json
import re
import sys
import tomllib
from pathlib import Path

REPO_TOP_DIRS = frozenset(
    {"crates", "tools", "scripts", "doc", "site", "skills", "docker", ".github", ".agents"}
)
SKIP_OBJECT_KEYS = frozenset({"reserved_empty_dir_prefixes"})

# Frozen legacy set of fast-component production files on main that predate the
# production full-tier attribute ratchet. Do not add new production src/** files
# here; keep helpers #[cfg(test)] and anchor liveness inside test modules.
LEGACY_PRODUCTION_FULL_TIER_FILES = frozenset(
    {
        "crates/oasis7_runtime/src/m1_builtin_wasm_artifact.rs",
        "crates/oasis7_runtime/src/m4_builtin_wasm_artifact.rs",
        "crates/oasis7_runtime/src/m5_builtin_wasm_artifact.rs",
        "crates/oasis7_runtime/src/native.rs",
        "crates/oasis7_runtime/src/world/agent_intent_terminal.rs",
        "crates/oasis7_runtime/src/world/bootstrap_power.rs",
        "crates/oasis7_runtime/src/world/cognition_economy.rs",
        "crates/oasis7_runtime/src/world/mod.rs",
        "crates/oasis7_runtime/src/world_rules.rs",
    }
)

_RUST_COMMENT_OR_WS = r"(?:\s|/\*[\s\S]*?\*/|//[^\n]*\n)"
_MODULE_OR_INCLUDE_RE = re.compile(
    rf"#{_RUST_COMMENT_OR_WS}*\[\s*(?:r#)?(?:path|cfg_attr)\b"
    rf"|\binclude(?:_bytes|_str)?{_RUST_COMMENT_OR_WS}*!"
)


def is_exact_repo_path(value: str) -> bool:
    if "/" not in value:
        return False
    if any(ch in value for ch in "*?[]"):
        return False
    if value.startswith(("http://", "https://", "/")):
        return False
    top = value.split("/", 1)[0]
    return top in REPO_TOP_DIRS


def find_missing_scope_paths(repo_root: Path, scope_file: Path | None = None) -> list[tuple[str, str]]:
    scope_path = scope_file or (repo_root / "scripts/ci-required-scope.json")
    data = json.loads(scope_path.read_text(encoding="utf-8"))
    missing: list[tuple[str, str]] = []

    def walk(obj: object, trail: str) -> None:
        if isinstance(obj, dict):
            for key, val in obj.items():
                if key in SKIP_OBJECT_KEYS:
                    continue
                if isinstance(key, str) and is_exact_repo_path(key):
                    if not (repo_root / key).exists():
                        missing.append((f"{trail}.{key} (key)", key))
                walk(val, f"{trail}.{key}" if trail else str(key))
        elif isinstance(obj, list):
            for idx, item in enumerate(obj):
                walk(item, f"{trail}[{idx}]")
        elif isinstance(obj, str):
            if is_exact_repo_path(obj) and not (repo_root / obj).exists():
                missing.append((trail, obj))

    walk(data, "")
    return missing


def _load_planner_module(planner_path: Path):
    spec = importlib.util.spec_from_file_location("_ci_scope_planner_canary", planner_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load planner module from {planner_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _is_test_only_rust_file(rel_path: str) -> bool:
    return "/tests/" in rel_path or rel_path.endswith(("_tests.rs", "/tests.rs", "_regressions.rs"))


def _has_narrow_unproven_full_tier_gate(selector, text: str) -> bool:
    tokens, closing = selector.rust_source_inputs(text, return_tokens=True)
    for index, (_, token, _) in enumerate(tokens):
        inner = index + 1 < len(tokens) and tokens[index + 1][1] == "!"
        opening = index + 2 if inner else index + 1
        if token != "#" or opening not in closing or tokens[opening][1] != "[":
            continue
        end = closing[opening]
        values = tuple(
            val.removeprefix("r#") if kind == "identifier" else val
            for kind, val, _ in tokens[opening + 1 : end]
        )
        if not values or values[0] not in ("cfg", "cfg_attr"):
            continue
        if values[0] == "cfg_attr":
            nesting = 0
            comma = None
            for offset, val in enumerate(values[2:], 2):
                if val == "," and nesting == 0:
                    comma = offset
                    break
                nesting += (val == "(") - (val == ")")
            if comma is not None and not set(values[comma + 1 :]) & {"cfg", "cfg_attr", "test", "ignore"}:
                continue
        if any("test_tier_full" in part for part in values) and not any(
            "test_tier_required" in part for part in values
        ):
            return True
    return False


_SIMPLE_LITERAL_PATH_ATTR_RE = re.compile(r'#\s*\[\s*path\s*=\s*"[^"\\\r\n]+"\s*\]')
_SIMPLE_LITERAL_DATA_INCLUDE_RE = re.compile(
    r'(?<![A-Za-z0-9_:])(?:(?:std|core)::)?include_(?:bytes|str)!\(\s*"[^"\\\r\n]+"\s*,?\s*\)'
)
_SIMPLE_NON_PATH_CFG_ATTR_RE = re.compile(
    r'#\s*\[\s*cfg_attr\((?:(?!\bpath\b)[^"\[\]])*(?:"(?:\\.|[^"\\])*"(?:(?!\bpath\b)[^"\[\]])*)*\)\s*\]'
)
_INLINE_PATH_BLOCK_RE = re.compile(
    rf"#{_RUST_COMMENT_OR_WS}*\[\s*(?:r#)?path\b[\s\S]*?\bmod{_RUST_COMMENT_OR_WS}+[A-Za-z0-9_#]+{_RUST_COMMENT_OR_WS}*\{{",
)


def _needs_full_source_input_scan(text: str) -> bool:
    if not _MODULE_OR_INCLUDE_RE.search(text):
        return False
    scrubbed = _SIMPLE_LITERAL_PATH_ATTR_RE.sub("", text)
    scrubbed = _SIMPLE_LITERAL_DATA_INCLUDE_RE.sub("", scrubbed)
    if "path" not in scrubbed:
        scrubbed = re.sub(rf"#{_RUST_COMMENT_OR_WS}*\[\s*(?:r#)?cfg_attr\b", "", scrubbed)
    else:
        scrubbed = _SIMPLE_NON_PATH_CFG_ATTR_RE.sub("", scrubbed)
    if _MODULE_OR_INCLUDE_RE.search(scrubbed):
        return True
    # Inline `#[path = "..."] mod name {` blocks (regardless of doc-comment length) make module paths uncertain.
    return bool(_INLINE_PATH_BLOCK_RE.search(text))


def check_ci_fast_path_health(
    repo_root: Path,
    scope_file: Path | None = None,
    planner_file: Path | None = None,
) -> list[str]:
    """Check CI fast-path contracts against silent slow-fallback regressions."""
    scope_path = scope_file or (repo_root / "scripts/ci-required-scope.json")
    planner_path = planner_file or (repo_root / "scripts/plan-rust-required-scope.py")
    data = json.loads(scope_path.read_text(encoding="utf-8"))
    selector = _load_planner_module(planner_path)
    errors: list[str] = []

    def verify_sha256(trail: str, rel_path: str, expected: str) -> None:
        target = repo_root / rel_path
        if not target.is_file():
            errors.append(f"{trail}: missing pinned file {rel_path}")
            return
        actual = hashlib.sha256(target.read_bytes()).hexdigest()
        if actual != expected:
            errors.append(
                f"{trail}: stale SHA-256 for {rel_path} (expected {expected}, actual {actual}); "
                "review fast-contract boundary and update scripts/ci-required-scope.json"
            )

    # 1. Pinned SHA-256 fast-contract anchors (prevent silent fast-contract invalidation).
    for idx, item in enumerate(data.get("generated_inputs", ())):
        verify_sha256(
            f"generated_inputs[{idx}].producer_sha256",
            item["producer"],
            item["producer_sha256"],
        )
    scale_contract = data.get("ordinary_scale_contract") or {}
    for rel_path, digest in sorted((scale_contract.get("anchors") or {}).items()):
        verify_sha256(f"ordinary_scale_contract.anchors[{rel_path}]", rel_path, digest)
    for idx, contract in enumerate(data.get("ordinary_fast_support_contracts", ())):
        for rel_path, digest in sorted((contract.get("callers") or {}).items()):
            verify_sha256(
                f"ordinary_fast_support_contracts[{idx}].callers[{rel_path}]",
                rel_path,
                digest,
            )

    # 2. Static module/include/OUT_DIR/ambiguity & production full-tier ratchet scan.
    registered_generated: dict[str, set[str]] = {}
    for item in data.get("generated_inputs", ()):
        registered_generated.setdefault(item["package"], set()).update(item.get("outputs", ()))

    component_rules = data.get("ordinary_fast_component_rules", ())
    targets_by_key = data.get("ordinary_fast_targets", {})
    fast_packages = {
        targets_by_key[key]["package"]
        for rule in component_rules
        for key in rule.get("targets", ())
        if key in targets_by_key
    }

    crates_dir = repo_root / "crates"
    if crates_dir.is_dir():
        root_str = repo_root.as_posix().rstrip("/") + "/"
        all_rs_files = sorted(crates_dir.rglob("*.rs"))
        rel_list = [path.as_posix().removeprefix(root_str) for path in all_rs_files]
        rel_set = set(rel_list)
        for path, rel in zip(all_rs_files, rel_list):
            parts = rel.split("/")
            if len(parts) < 3:
                continue
            pkg = parts[1]
            if not rel.endswith("/mod.rs") and rel.removesuffix(".rs") + "/mod.rs" in rel_set:
                errors.append(
                    f"ambiguous Rust module files: both {rel} and {rel.removesuffix('.rs')}/mod.rs exist"
                )
            text = path.read_text(encoding="utf-8")
            if _needs_full_source_input_scan(text):
                _, calls, uncertain_module_paths = selector.rust_source_inputs(
                    text, with_module_paths=True
                )
                if uncertain_module_paths:
                    errors.append(
                        f"{rel}: uncertain module path attribute (conditional #[cfg_attr(..., path = ...)], "
                        "non-literal #[path], or inline #[path] block) poisons CI fast-path reachability"
                    )
                for call in calls:
                    macro, line_no = call["macro"], call["line"]
                    if call["kind"] == "unknown":
                        errors.append(
                            f"{rel}:{line_no}: unresolved {macro}! call poisons CI fast-path analysis"
                        )
                    elif call["kind"] == "generated":
                        out_name = (call["path"] or "").lstrip("/")
                        if macro == "include" or out_name not in registered_generated.get(pkg, set()):
                            errors.append(
                                f"{rel}:{line_no}: unregistered OUT_DIR {macro}!({out_name!r}) for {pkg}; "
                                "register producer/outputs in scripts/ci-required-scope.json generated_inputs"
                            )
            if (
                pkg in fast_packages
                and "test_tier_full" in text
                and rel not in LEGACY_PRODUCTION_FULL_TIER_FILES
                and not _is_test_only_rust_file(rel)
                and any(
                    any(fnmatch.fnmatchcase(rel, m) for m in rule.get("match", ()))
                    and not any(fnmatch.fnmatchcase(rel, e) for e in rule.get("exclude", ()))
                    for rule in component_rules
                )
                and _has_narrow_unproven_full_tier_gate(selector, text)
            ):
                errors.append(
                    f"{rel}: production fast-component file introduces narrow-unproven #[cfg(...test_tier_full...)] "
                    "gate (causes test_policy_changed=true fallback on future edits); keep helper #[cfg(test)] "
                    "and anchor full-tier liveness with `const _: () = { let _ = (...); };` inside the test module"
                )

    # 3. Live planner reachability & Adapter/Cutover slice archetype canary per fast-component crate.
    by_pkg: dict[str, tuple[dict, dict]] = {}
    for rule in component_rules:
        lib_targets = [
            targets_by_key[key]
            for key in rule.get("targets", ())
            if key in targets_by_key and targets_by_key[key].get("target_kind") == "lib"
        ]
        if not lib_targets:
            continue
        pkg = lib_targets[0]["package"]
        if pkg not in by_pkg and (repo_root / "crates" / pkg / "Cargo.toml").is_file():
            by_pkg[pkg] = (rule, lib_targets[0])

    preferred_probes = {
        "oasis7_runtime": [
            "crates/oasis7_runtime/src/domains/capability_authz/catalog.rs",
            "crates/oasis7_runtime/src/world/governance.rs",
        ],
        "oasis7_world_state": [
            "crates/oasis7_world_state/src/formal_retained_history_journal.rs",
            "crates/oasis7_world_state/src/events/domain_event_rejections.rs",
        ],
    }
    representative_pkgs = (
        set(by_pkg)
        if len(by_pkg) <= 4
        else {
            pkg
            for pkg in (
                "oasis7_runtime",
                "oasis7_world_state",
                "oasis7_viewer_runtime",
                "oasis7_simulator",
            )
            if pkg in by_pkg
        }
    )

    root_prefix = repo_root.as_posix().rstrip("/") + "/"
    for pkg in sorted(representative_pkgs):
        rule, target = by_pkg[pkg]
        crate_rel = f"crates/{pkg}"
        manifest = tomllib.loads((repo_root / crate_rel / "Cargo.toml").read_text(encoding="utf-8"))
        src_dir = repo_root / crate_rel / "src"
        if not src_dir.is_dir():
            continue
        sources = {
            p.as_posix().removeprefix(root_prefix): p.read_bytes()
            for p in sorted(src_dir.rglob("*.rs"))
        }
        eligible = [
            rel
            for rel in sorted(sources)
            if not _is_test_only_rust_file(rel)
            and not rel.endswith(("/native.rs", "/world/mod.rs"))
            and rel not in LEGACY_PRODUCTION_FULL_TIER_FILES
            and any(fnmatch.fnmatchcase(rel, m) for m in rule.get("match", ()))
            and not any(fnmatch.fnmatchcase(rel, e) for e in rule.get("exclude", ()))
            and not any(
                tok in sources[rel].decode("utf-8")
                for tok in ("test_tier_full", "wasm32", "windows", "macos")
            )
        ]
        if not eligible:
            continue
        chosen = [p for p in preferred_probes.get(pkg, ()) if p in eligible]
        for candidate in eligible:
            if len(chosen) >= 2:
                break
            if candidate not in chosen:
                chosen.append(candidate)

        adapter_path = chosen[0]
        cutover_path = chosen[1] if len(chosen) > 1 else None
        base_map: dict[str, str] = {}
        head_map: dict[str, str] = {}
        mod_sources = dict(sources)
        probed_paths = [adapter_path]

        adapter_orig = sources[adapter_path].decode("utf-8")
        adapter_head = (
            adapter_orig
            + '\n#[cfg_attr(not(test), expect(dead_code, reason = "canary"))]\n'
            + "pub fn __ci_canary_adapter() -> usize { 1 }\n"
            + "#[test]\nfn __ci_canary_adapter_test() { assert_eq!(__ci_canary_adapter(), 1); }\n"
        )
        base_map[adapter_path] = adapter_orig
        head_map[adapter_path] = adapter_head
        mod_sources[adapter_path] = adapter_head.encode("utf-8")

        if cutover_path is not None:
            cutover_orig = sources[cutover_path].decode("utf-8")
            cutover_base = (
                cutover_orig
                + '\n#[cfg_attr(not(test), expect(dead_code, reason = "canary"))]\n'
                + "pub fn __ci_canary_cutover() -> usize { 1 }\n"
            )
            cutover_head = cutover_orig + "\npub fn __ci_canary_cutover() -> usize { 1 }\n"
            base_map[cutover_path] = cutover_base
            head_map[cutover_path] = cutover_head
            mod_sources[cutover_path] = cutover_head.encode("utf-8")
            probed_paths.append(cutover_path)

        feats = set(manifest.get("features", ())) | {"test_tier_required", "test_tier_full"}
        defaults = set(manifest.get("features", {}).get("default", ()))
        lib_root = f"{crate_rel}/src/lib.rs"
        narrow_features = set(target.get("features", ())) | (
            defaults if target.get("default_features", True) else set()
        )
        includes = {p for p in mod_sources if p.endswith("/native.rs")}
        s_inputs = {
            "sources": mod_sources,
            "directories": {pkg: crate_rel},
            "manifests": {pkg: manifest},
            "default_features": {pkg: sorted(defaults)},
            "includes": includes,
            "ref_sources": {"BASE": base_map, "HEAD": head_map},
        }
        narrow_unavailable: set[str] = set()
        try:
            policy_paths, ignored_risk, prod_cfg = selector.changed_test_policy(
                "BASE",
                "HEAD",
                probed_paths,
                feature_owners={p: {pkg} for p in probed_paths},
                feature_packages={pkg},
                available_features={pkg: feats},
                ordinary_roots={lib_root},
                source_inputs=s_inputs,
                narrow_features_by_path={p: narrow_features for p in probed_paths},
                narrow_unavailable=narrow_unavailable,
                selected_targets=[target],
                required_source_paths=set(probed_paths),
            )
        except selector.TestPolicyError as exc:
            errors.append(f"fast-component canary raised TestPolicyError for {pkg}: {exc}")
            continue
        if policy_paths or ignored_risk or prod_cfg or narrow_unavailable:
            errors.append(
                f"fast-component canary failed for {pkg} ({probed_paths}): "
                f"policy_paths={policy_paths}, narrow_unavailable={sorted(narrow_unavailable)}, "
                f"production_cfg={prod_cfg}, ignored_risk={ignored_risk}"
            )

    # 4. Same-tree package_capabilities alignment and 5 end-to-end routing probes.
    if "package_capabilities" in data:
        engine = selector._load_sibling_ci_engine()
        root_manifest = repo_root / "Cargo.toml"
        if engine is not None and root_manifest.is_file():
            try:
                root_toml = tomllib.loads(root_manifest.read_text(encoding="utf-8"))
                ws_members = {}
                for member in (root_toml.get("workspace") or {}).get("members", ()):
                    mp = repo_root / member / "Cargo.toml"
                    if mp.is_file():
                        parsed = tomllib.loads(mp.read_text(encoding="utf-8"))
                        pkg_name = (parsed.get("package") or {}).get("name")
                        if pkg_name:
                            ws_members[pkg_name] = set((parsed.get("features") or {}).keys())
                engine.validate_package_capabilities_against_workspace(
                    data["package_capabilities"], set(ws_members),
                    package_groups=data.get("package_groups"), known_groups=set(data.get("groups", ())),
                    package_features=ws_members,
                )
            except (OSError, ValueError, KeyError, UnicodeError) as exc:
                errors.append(f"package_capabilities workspace alignment failed: {exc}")
        probes = [
            ("single-pkg-inline", {
                "scope": "targeted", "execution_profile": "pr-fast", "fast_plan_version": 1,
                "test_policy_changed": False, "ordinary_execution": "inline",
                "ordinary_partitions": ["all"], "ordinary_fast_targets": [{"package": "oasis7_runtime"}],
                "oasis7_required_packages": ["oasis7_runtime"], "oasis7_required_wasm_packages": [],
                "workspace_support_profiles": [],
            }, lambda c: c["mode"] == "inline" and c["partitions"] == ["all"] and c["candidate_container_eligible"] is True),
            ("dual-pkg-inline", {
                "scope": "targeted", "execution_profile": "pr-fast", "fast_plan_version": 1,
                "test_policy_changed": False, "ordinary_execution": "inline",
                "ordinary_partitions": ["oasis7_runtime", "oasis7_world_state"],
                "ordinary_fast_targets": [{"package": "oasis7_runtime"}, {"package": "oasis7_world_state"}],
                "oasis7_required_packages": ["oasis7_runtime", "oasis7_world_state"], "oasis7_required_wasm_packages": [],
                "workspace_support_profiles": [],
            }, lambda c: c["mode"] == "inline" and c["partitions"] == ["oasis7_runtime", "oasis7_world_state"]),
            ("heavy-lib-split", {
                "scope": "targeted", "execution_profile": "pr-fast", "fast_plan_version": 1,
                "test_policy_changed": False, "ordinary_artifact_partition_version": 2,
                "ordinary_universal_sharding_version": 1,
                "oasis7_required_packages": ["oasis7_runtime", "oasis7_world_state"],
                "oasis7_required_wasm_packages": [], "workspace_support_profiles": [],
            }, lambda c: c["mode"] == "artifact-v2" and c["supports_universal_v2"] is True
                         and "runtime-tests:even" in c["artifact_partitions"] and "runtime-tests:odd" in c["artifact_partitions"]),
            ("host-docker-risk-fallback", {
                "scope": "full", "execution_profile": "pr-fast", "fast_plan_version": 1,
                "test_policy_changed": False,
                "ordinary_risk_targets": [{"package": "oasis7_chain_runtime", "target_kind": "test"}],
                "oasis7_required_packages": ["oasis7_chain_runtime", "oasis7_runtime"],
                "oasis7_required_wasm_packages": [], "workspace_support_profiles": [],
            }, lambda c: c["candidate_container_eligible"] is False
                         and "builtin_test_modules" in c["required_host_docker_wasm_fixtures"]
                         and "provider_preview_wasm" in c["required_host_docker_wasm_fixtures"]),
            ("strict-clippy-delegation", {
                "scope": "targeted", "execution_profile": "pr-fast", "fast_plan_version": 1,
                "test_policy_changed": False, "ordinary_execution": "inline",
                "ordinary_partitions": ["oasis7_runtime", "oasis7_world_state"],
                "ordinary_fast_targets": [{"package": "oasis7_runtime"}, {"package": "oasis7_world_state"}],
                "distfs_clippy_group": "rust_baseline",
                "oasis7_required_packages": ["oasis7_runtime", "oasis7_world_state"],
                "oasis7_required_wasm_packages": [], "workspace_support_profiles": [],
            }, lambda c: len(c["delegated_clippy"]) == len(selector.DISTFS_CLIPPY_TARGETS) and bool(c["delegated_clippy"])),
        ]
        for label, sample_plan, predicate in probes:
            try:
                out = selector.materialize_execution_contracts(data, dict(sample_plan))
                if not predicate(out["ordinary_execution_contract"]):
                    errors.append(f"routing probe {label!r} failed contract assertion")
            except Exception as exc:
                errors.append(f"routing probe {label!r} raised error: {exc}")

    return errors


_AUDITED_GENERIC_WORDS = frozenset({
    "test", "tests", "main", "lib", "core", "cli", "app", "bin", "api",
    "sdk", "web", "ui", "net", "node", "store", "common", "types", "proto",
    "utils", "tools", "crates", "build", "check", "bench", "example", "examples",
})


def collect_workspace_forbidden_identifiers(repo_root: Path) -> set[str]:
    """Dynamically collect all workspace/standalone Cargo package, target, and crate/tool dir names."""
    forbidden: set[str] = set()
    for base_name in ("crates", "tools"):
        base_dir = repo_root / base_name
        if base_dir.is_dir():
            for child in base_dir.iterdir():
                if child.is_dir() and not child.name.startswith("."):
                    forbidden.add(child.name)
    manifests: list[Path] = []
    root_manifest = repo_root / "Cargo.toml"
    if root_manifest.is_file():
        manifests.append(root_manifest)
    for base_name in ("crates", "tools"):
        base_dir = repo_root / base_name
        if base_dir.is_dir():
            for path in base_dir.rglob("Cargo.toml"):
                if "target" not in path.parts and "third_party" not in path.parts:
                    manifests.append(path)
    for manifest in manifests:
        try:
            parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, ValueError, UnicodeError):
            continue
        pkg = parsed.get("package")
        if isinstance(pkg, dict) and isinstance(pkg.get("name"), str):
            forbidden.add(pkg["name"])
        lib = parsed.get("lib")
        if isinstance(lib, dict) and isinstance(lib.get("name"), str):
            forbidden.add(lib["name"])
        for section in ("bin", "test"):
            entries = parsed.get(section)
            if isinstance(entries, list):
                for entry in entries:
                    if isinstance(entry, dict) and isinstance(entry.get("name"), str):
                        forbidden.add(entry["name"])
    return {name for name in forbidden if len(name) >= 3 and name not in _AUDITED_GENERIC_WORDS}


def check_ci_engine_zero_business_identifiers(repo_root: Path) -> list[str]:
    """AST-scan scripts/ci_engine/**/*.py to enforce zero workspace package/target/dir literals."""
    engine_dir = repo_root / "scripts/ci_engine"
    if not engine_dir.is_dir():
        return []
    forbidden = collect_workspace_forbidden_identifiers(repo_root)
    if not forbidden:
        return []
    pattern = re.compile(
        r"(?<![A-Za-z0-9_])(?:" + "|".join(re.escape(w) for w in sorted(forbidden, key=len, reverse=True)) + r")(?![A-Za-z0-9_])"
    )
    errors: list[str] = []
    for py_file in sorted(engine_dir.rglob("*.py")):
        rel = py_file.relative_to(repo_root).as_posix()
        try:
            tree = ast.parse(py_file.read_text(encoding="utf-8"), filename=rel)
        except SyntaxError as exc:
            errors.append(f"{rel}: syntax error during AST gate: {exc}")
            continue
        for node in ast.walk(tree):
            candidates: list[tuple[int, str]] = []
            lineno = getattr(node, "lineno", 0)
            if isinstance(node, ast.Name):
                candidates.append((lineno, node.id))
            elif isinstance(node, ast.Attribute):
                candidates.append((lineno, node.attr))
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                candidates.append((lineno, node.name))
            elif isinstance(node, ast.arg):
                candidates.append((lineno, node.arg))
            elif isinstance(node, ast.Constant) and isinstance(node.value, str):
                candidates.append((lineno, node.value))
            for line, text in candidates:
                for match in pattern.finditer(text):
                    errors.append(
                        f"{rel}:{line}: forbidden workspace identifier {match.group(0)!r} in L1 pure engine"
                    )
    return errors


def check_candidate_executors_zero_engine_imports(repo_root: Path) -> list[str]:
    """Enforce zero direct or transitive ci_engine/planner imports in ci-test-artifacts.py and ci-tests.sh."""
    errors: list[str] = []

    def scan_source(label: str, source: str) -> None:
        try:
            tree = ast.parse(source, filename=label)
        except SyntaxError as exc:
            errors.append(f"{label}: syntax error during executor import gate: {exc}")
            return
        for node in ast.walk(tree):
            line = getattr(node, "lineno", 0)
            if isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name == "ci_engine" or alias.name.startswith("ci_engine."):
                        errors.append(f"{label}:{line}: forbidden import {alias.name!r} in candidate executor")
            elif isinstance(node, ast.ImportFrom):
                mod = node.module or ""
                if mod == "ci_engine" or mod.startswith("ci_engine."):
                    errors.append(f"{label}:{line}: forbidden from-import {mod!r} in candidate executor")
            elif isinstance(node, ast.Constant) and isinstance(node.value, str):
                if "ci_engine" in node.value or "plan-rust-required-scope" in node.value:
                    errors.append(f"{label}:{line}: forbidden planner/engine reference {node.value!r} in candidate executor")

    artifacts_py = repo_root / "scripts/ci-test-artifacts.py"
    if artifacts_py.is_file():
        scan_source("scripts/ci-test-artifacts.py", artifacts_py.read_text(encoding="utf-8"))
    ci_tests_sh = repo_root / "scripts/ci-tests.sh"
    if ci_tests_sh.is_file():
        sh_text = ci_tests_sh.read_text(encoding="utf-8")
        for idx, match in enumerate(re.finditer(r"<<'?PY'?\n(.*?)\nPY\b", sh_text, re.S), 1):
            scan_source(f"scripts/ci-tests.sh#PY-{idx}", match.group(1))
    return errors


def check_executor_hardcoded_package_ratchet(repo_root: Path) -> list[str]:
    """S5 ratchet: enforce 0 transitional S2 shims and 0 hardcoded support tuples in executors."""
    errors: list[str] = []
    checks = (
        ("scripts/ci-tests.sh", ("_S2_FALLBACK_SUPPORT", "_S2_FALLBACK_HOST", "_s2_legacy_support_contract",
                                 "SUPPORT_PROFILES", "WORKSPACE_SUPPORT_HOST_PROFILES")),
        ("scripts/ci-test-artifacts.py", ("_s2_legacy_ordinary_contract",)),
        ("scripts/plan-rust-required-scope.py", ("legacy_eligible",)),
    )
    for rel, forbidden_tokens in checks:
        path = repo_root / rel
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        for token in forbidden_tokens:
            if token in text:
                errors.append(f"{rel}: S5 ratchet violation: forbidden transitional/hardcoded token {token!r}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check exact paths and fast-path anti-fallback invariants in scripts/ci-required-scope.json."
    )
    parser.add_argument("--repo-root", default=".", help="Repository root directory")
    parser.add_argument("--scope-file", default=None, help="Optional override path to ci-required-scope.json")
    parser.add_argument(
        "--check-fast-paths",
        action="store_true",
        help="Force running CI fast-path anti-fallback canary even when --scope-file is overridden",
    )
    parser.add_argument(
        "--skip-fast-paths",
        action="store_true",
        help="Skip CI fast-path anti-fallback canary and check exact path existence only",
    )
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    scope_file = Path(args.scope_file).resolve() if args.scope_file else None
    missing = find_missing_scope_paths(repo_root, scope_file)
    if missing:
        print(
            "check-ci-scope-paths: FAIL: exact path(s) declared in scripts/ci-required-scope.json do not exist:",
            file=sys.stderr,
        )
        for loc, rel_path in missing:
            print(f"  - {rel_path} (at {loc})", file=sys.stderr)
        return 1

    engine_errors = check_ci_engine_zero_business_identifiers(repo_root)
    if engine_errors:
        print(
            "check-ci-scope-paths: FAIL: scripts/ci_engine contains forbidden workspace identifier(s):",
            file=sys.stderr,
        )
        for err in engine_errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    executor_errors = check_candidate_executors_zero_engine_imports(repo_root) + check_executor_hardcoded_package_ratchet(repo_root)
    if executor_errors:
        print(
            "check-ci-scope-paths: FAIL: candidate executor import or S5 ratchet violation(s):",
            file=sys.stderr,
        )
        for err in executor_errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    run_fast_canary = not args.skip_fast_paths and (
        args.check_fast_paths
        or (scope_file is None and (repo_root / "scripts/plan-rust-required-scope.py").is_file())
    )
    if run_fast_canary:
        fast_errors = check_ci_fast_path_health(repo_root, scope_file)
        if fast_errors:
            print(
                "check-ci-scope-paths: FAIL: CI fast-path anti-fallback canary detected regression(s):",
                file=sys.stderr,
            )
            for err in fast_errors:
                print(f"  - {err}", file=sys.stderr)
            return 1
        print("check-ci-scope-paths: OK (0 missing exact paths, ci_engine AST gate & fast-path canary passed)")
        return 0

    print("check-ci-scope-paths: OK (0 missing exact paths)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
