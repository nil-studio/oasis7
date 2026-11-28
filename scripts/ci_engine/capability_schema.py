"""Pure-stdlib composable package_capabilities schema validator and view projector."""

from __future__ import annotations
from collections.abc import Mapping, Set

ALLOWED_CAPABILITY_KEYS = frozenset({
    "ordinary_package", "support_profile", "inline_dedicated_partition", "compact_non_runtime_excluded",
    "required_wasm_build", "full_tier_requires_host_docker", "host_docker_wasm_fixtures",
    "splittable_lib_harness", "group_library_proof", "platform_cfg_assumptions",
})
ALLOWED_SUPPORT_KEYS = frozenset({"profile_id", "partition", "requires_host_runner", "extra_features", "wasm_compile_feature_sets"})
ALLOWED_SUPPORT_PARTITIONS = frozenset({"custody", "other"})
ALLOWED_HOST_DOCKER_FIXTURES = frozenset({"builtin_test_modules", "provider_preview_wasm"})
ALLOWED_TARGET_ARCHS = frozenset({"x86_64", "aarch64", "wasm32"})
ALLOWED_PLATFORM_CFG_KEYS = frozenset({"assumed_target_archs", "known_target_features", "external_ffi_bridge_group"})


def _req_bool(obj: Mapping[str, object], key: str, default: bool, pkg: str) -> bool:
    val = obj.get(key, default)
    if type(val) is not bool:
        raise ValueError(f"{pkg}.{key} must be a boolean")
    return val


def _req_strs(val: object, label: str) -> list[str]:
    if not isinstance(val, list) or any(not isinstance(item, str) or not item for item in val):
        raise ValueError(f"{label} must be a list of non-empty strings")
    return val


def validate_package_capabilities_static(
    capabilities: Mapping[str, object], *, package_groups: Mapping[str, object] | None = None,
    known_groups: Set[str] | None = None,
) -> None:
    """Layer-1 static schema, lexicographic order, physical constraints, and coverage check."""
    if not isinstance(capabilities, Mapping) or not capabilities:
        raise ValueError("package_capabilities must be a non-empty object")
    keys = list(capabilities.keys())
    if any(not isinstance(k, str) or not k for k in keys) or keys != sorted(keys):
        raise ValueError("package_capabilities keys must be non-empty strings in strict ASCII lexicographic order")
    seen_profiles: set[str] = set()
    seen_proof_groups: set[str] = set()
    for pkg, raw in capabilities.items():
        if not isinstance(raw, Mapping) or (set(raw.keys()) - ALLOWED_CAPABILITY_KEYS):
            raise ValueError(f"{pkg}: invalid capability object or unknown keys")
        if "ordinary_package" not in raw or type(raw["ordinary_package"]) is not bool:
            raise ValueError(f"{pkg}: ordinary_package must be a boolean")
        ordinary = raw["ordinary_package"]
        inline_ded = _req_bool(raw, "inline_dedicated_partition", False, pkg)
        compact_excl = _req_bool(raw, "compact_non_runtime_excluded", False, pkg)
        _req_bool(raw, "required_wasm_build", False, pkg)
        host_docker = _req_bool(raw, "full_tier_requires_host_docker", False, pkg)
        if (inline_ded or compact_excl) and not ordinary:
            raise ValueError(f"{pkg}: inline_dedicated_partition/compact_non_runtime_excluded require ordinary_package=true")
        fixtures = _req_strs(raw.get("host_docker_wasm_fixtures", []), f"{pkg}.host_docker_wasm_fixtures")
        if set(fixtures) - ALLOWED_HOST_DOCKER_FIXTURES or (fixtures and not host_docker):
            raise ValueError(f"{pkg}: invalid host_docker_wasm_fixtures or missing full_tier_requires_host_docker")
        support = raw.get("support_profile")
        if support is not None:
            if (not isinstance(support, Mapping) or set(support.keys()) != ALLOWED_SUPPORT_KEYS
                    or support["profile_id"] != pkg
                    or support["profile_id"] in seen_profiles or support["partition"] not in ALLOWED_SUPPORT_PARTITIONS
                    or type(support["requires_host_runner"]) is not bool):
                raise ValueError(f"{pkg}.support_profile invalid")
            seen_profiles.add(support["profile_id"])
            _req_strs(support["extra_features"], f"{pkg}.support_profile.extra_features")
            fsets = support["wasm_compile_feature_sets"]
            if not isinstance(fsets, list) or any(
                not isinstance(fs, list) or any(not isinstance(f, str) or not f for f in fs) for fs in fsets
            ):
                raise ValueError(f"{pkg}.support_profile.wasm_compile_feature_sets invalid")
        splittable = raw.get("splittable_lib_harness")
        if splittable is not None and (
            not isinstance(splittable, Mapping) or set(splittable.keys()) != {"logical_alias", "universal_v2_anchor"}
            or not isinstance(splittable["logical_alias"], str) or not splittable["logical_alias"]
            or type(splittable["universal_v2_anchor"]) is not bool
        ):
            raise ValueError(f"{pkg}.splittable_lib_harness invalid")
        proof = raw.get("group_library_proof")
        if proof is not None:
            if (not isinstance(proof, Mapping) or set(proof.keys()) != {"group"}
                    or not isinstance(proof["group"], str) or not proof["group"]
                    or proof["group"] in seen_proof_groups
                    or (known_groups is not None and proof["group"] not in known_groups)
                    or (package_groups is not None and proof["group"] not in (package_groups.get(pkg) or ()))):
                raise ValueError(f"{pkg}.group_library_proof invalid")
            seen_proof_groups.add(proof["group"])
        pcfg = raw.get("platform_cfg_assumptions")
        if pcfg is not None:
            if not isinstance(pcfg, Mapping) or not set(pcfg.keys()) <= ALLOWED_PLATFORM_CFG_KEYS:
                raise ValueError(f"{pkg}.platform_cfg_assumptions invalid")
            if "assumed_target_archs" in pcfg and not set(_req_strs(pcfg["assumed_target_archs"], pkg)) <= ALLOWED_TARGET_ARCHS:
                raise ValueError(f"{pkg}: invalid assumed_target_archs")
            if "known_target_features" in pcfg:
                _req_strs(pcfg["known_target_features"], f"{pkg}.known_target_features")
            if "external_ffi_bridge_group" in pcfg:
                grp = pcfg["external_ffi_bridge_group"]
                if not isinstance(grp, str) or not grp or (known_groups is not None and grp not in known_groups):
                    raise ValueError(f"{pkg}: invalid external_ffi_bridge_group")
        if not (ordinary or support is not None or bool(package_groups and package_groups.get(pkg))):
            raise ValueError(f"{pkg}: orphan workspace package has no ordinary, support, or group ownership")


def validate_package_capabilities_against_workspace(
    capabilities: Mapping[str, object], workspace_members: Set[str], *,
    package_groups: Mapping[str, object] | None = None, known_groups: Set[str] | None = None,
    package_features: Mapping[str, Set[str]] | None = None,
) -> None:
    """Layer-2 same-tree alignment check against Cargo.toml workspace members and features."""
    validate_package_capabilities_static(capabilities, package_groups=package_groups, known_groups=known_groups)
    missing, extra = sorted(set(workspace_members) - set(capabilities)), sorted(set(capabilities) - set(workspace_members))
    if missing or extra:
        raise ValueError(f"package_capabilities workspace mismatch: missing={missing}, extra={extra}")
    if package_features is not None:
        for pkg, raw in capabilities.items():
            pcfg = raw.get("platform_cfg_assumptions") if isinstance(raw, Mapping) else None
            if isinstance(pcfg, Mapping) and "known_target_features" in pcfg:
                unknown = sorted(set(pcfg["known_target_features"]) - set(package_features.get(pkg, ())))
                if unknown:
                    raise ValueError(f"{pkg}: unknown known_target_features: {unknown}")


def derive_ordinary_packages(capabilities: Mapping[str, Mapping[str, object]]) -> tuple[str, ...]:
    return tuple(pkg for pkg, entry in capabilities.items() if entry.get("ordinary_package"))


def derive_required_wasm_packages(capabilities: Mapping[str, Mapping[str, object]]) -> tuple[str, ...]:
    return tuple(pkg for pkg, entry in capabilities.items() if entry.get("required_wasm_build"))


def derive_support_profiles(capabilities: Mapping[str, Mapping[str, object]]) -> dict[str, dict[str, object]]:
    return {str(sp["profile_id"]): {"package": pkg, **dict(sp)}
            for pkg, entry in capabilities.items() if isinstance(sp := entry.get("support_profile"), Mapping)}


def derive_support_host_profiles(capabilities: Mapping[str, Mapping[str, object]]) -> frozenset[str]:
    return frozenset(pid for pid, spec in derive_support_profiles(capabilities).items() if spec.get("requires_host_runner"))


def derive_inline_dedicated_packages(capabilities: Mapping[str, Mapping[str, object]]) -> frozenset[str]:
    return frozenset(pkg for pkg, entry in capabilities.items() if entry.get("inline_dedicated_partition"))


def derive_compact_excluded_packages(capabilities: Mapping[str, Mapping[str, object]]) -> frozenset[str]:
    return frozenset(pkg for pkg, entry in capabilities.items() if entry.get("compact_non_runtime_excluded"))


def derive_logical_harness_aliases(capabilities: Mapping[str, Mapping[str, object]]) -> dict[str, str]:
    return {pkg: str(sp["logical_alias"]) for pkg, entry in sorted(capabilities.items())
            if isinstance(sp := entry.get("splittable_lib_harness"), Mapping)}


def derive_universal_v2_anchors(capabilities: Mapping[str, Mapping[str, object]]) -> frozenset[str]:
    return frozenset(pkg for pkg, entry in capabilities.items()
                     if isinstance(sp := entry.get("splittable_lib_harness"), Mapping) and sp.get("universal_v2_anchor"))


def derive_fixture_owner_packages(capabilities: Mapping[str, Mapping[str, object]], fixture_name: str) -> frozenset[str]:
    return frozenset(pkg for pkg, entry in capabilities.items()
                     if fixture_name in (entry.get("host_docker_wasm_fixtures") or ()))


def derive_group_library_map(capabilities: Mapping[str, Mapping[str, object]]) -> dict[str, str]:
    return {str(gp["group"]): pkg for pkg, entry in capabilities.items()
            if isinstance(gp := entry.get("group_library_proof"), Mapping)}


def reconcile_cross_version_members(
    policy_capabilities: Mapping[str, Mapping[str, object]], candidate_members: Set[str],
) -> dict[str, tuple[str, ...]]:
    policy_set, cand_set = set(policy_capabilities), set(candidate_members)
    return {"added": tuple(sorted(cand_set - policy_set)), "removed": tuple(sorted(policy_set - cand_set)),
            "kept": tuple(sorted(policy_set & cand_set))}
