"""Pure-stdlib deterministic shard scheduler and partition planner."""

from __future__ import annotations
from collections.abc import Mapping, Sequence, Set

FULL_TARGETED_ARTIFACT_PARTITIONS = ("controls", "native-0", "native-1", "native-2", "native-3")
COMPACT_TARGETED_ARTIFACT_PARTITIONS = ("controls", "native-0", "native-1")


def compute_inline_partitions(selected_packages: Sequence[str], dedicated_partition_packages: Set[str]) -> tuple[str, ...]:
    unique = tuple(sorted(set(selected_packages)))
    return unique if len(unique) >= 2 and set(unique) == set(dedicated_partition_packages) else ("all",)


def supports_universal_v2(selected: Set[str], *, anchor_packages: Set[str], harnesses: Sequence[Mapping[str, object]] = (), min_harnesses: int = 4) -> bool:
    return bool(selected) and (bool(selected & set(anchor_packages)) or sum(1 for r in harnesses if r.get("package") in selected) >= min_harnesses)


def compute_artifact_partitions(
    selected_packages: Sequence[str], *, compact_excluded_packages: Set[str], anchor_packages: Set[str] = frozenset(),
    harnesses: Sequence[Mapping[str, object]] = (), dynamic_enabled: bool = False, strip_controls: bool = False, compact_budget_ms: int = 60000,
) -> tuple[str, ...]:
    selected = set(selected_packages)
    if not selected:
        return ()
    parts = FULL_TARGETED_ARTIFACT_PARTITIONS
    if dynamic_enabled and not (selected & set(compact_excluded_packages)):
        non_anchor_ms = sum(
            int(r["elapsed_ms"]) for r in harnesses
            if r.get("package") in selected and not (r.get("package") in anchor_packages and r.get("target_kind") == "lib"
                                                     and r.get("target_name") == r.get("package") and r.get("variant") == "required" and r.get("mode") == "batch")
        )
        if non_anchor_ms <= compact_budget_ms:
            parts = COMPACT_TARGETED_ARTIFACT_PARTITIONS
    return tuple(p for p in parts if p != "controls") if strip_controls else parts


def assign_lpt_shards(item_costs: Mapping[str, int | float], shard_count: int) -> list[list[str]]:
    if shard_count < 1:
        raise ValueError("shard_count must be >= 1")
    buckets, loads = [[] for _ in range(shard_count)], [0.0] * shard_count
    for name, cost in sorted(item_costs.items(), key=lambda pair: (-float(pair[1]), pair[0])):
        if float(cost) < 0:
            raise ValueError(f"negative cost for {name!r}")
        best = min(range(shard_count), key=lambda idx: (loads[idx], idx))
        buckets[best].append(name)
        loads[best] += float(cost)
    return buckets


def compute_support_partitions(selected_profiles: Sequence[str], profile_specs: Mapping[str, Mapping[str, object]]) -> dict[str, object]:
    partitions: dict[str, list[str]] = {"custody": [], "other": []}
    host_profiles, image_profiles = [], []
    for pid in sorted(set(selected_profiles)):
        spec = profile_specs.get(pid)
        if spec is None or spec.get("partition") not in partitions:
            raise ValueError(f"invalid or unknown support profile: {pid!r}")
        partitions[str(spec["partition"])].append(pid)
        (host_profiles if spec.get("requires_host_runner") else image_profiles).append(pid)
    return {"partitions": partitions, "host_profiles": host_profiles, "image_profiles": image_profiles}
