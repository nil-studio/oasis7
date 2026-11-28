"""Pure-stdlib deterministic shard scheduler and partition planner."""

from __future__ import annotations
from collections.abc import Mapping, Sequence


def compute_inline_partitions(selected_packages: Sequence[str], dedicated_partition_packages: frozenset[str]) -> tuple[str, ...]:
    unique = tuple(sorted(set(selected_packages)))
    return unique if len(unique) >= 2 and all(p in dedicated_partition_packages for p in unique) else ("all",)


def compute_artifact_partitions(
    selected_packages: Sequence[str], *, splittable_aliases: Mapping[str, str],
    compact_excluded_packages: frozenset[str], supports_universal_v2: bool = True,
) -> tuple[str, ...]:
    unique = tuple(sorted(set(selected_packages)))
    if not unique:
        return ()
    if not any(pkg in compact_excluded_packages for pkg in unique):
        return ("compact",)
    out: list[str] = []
    for pkg in unique:
        alias = splittable_aliases.get(pkg)
        if alias and supports_universal_v2:
            out.extend((f"{alias}:even", f"{alias}:odd"))
        else:
            out.append(alias or pkg)
    return tuple(out)


def assign_lpt_shards(item_costs: Mapping[str, int | float], shard_count: int) -> list[list[str]]:
    if shard_count < 1:
        raise ValueError("shard_count must be >= 1")
    buckets: list[list[str]] = [[] for _ in range(shard_count)]
    loads = [0.0] * shard_count
    for name, cost in sorted(item_costs.items(), key=lambda pair: (-float(pair[1]), pair[0])):
        if float(cost) < 0:
            raise ValueError(f"negative cost for {name!r}")
        best = min(range(shard_count), key=lambda idx: (loads[idx], idx))
        buckets[best].append(name)
        loads[best] += float(cost)
    return buckets


def compute_support_partitions(
    selected_profiles: Sequence[str], profile_specs: Mapping[str, Mapping[str, object]],
) -> dict[str, object]:
    partitions: dict[str, list[str]] = {"custody": [], "other": []}
    host_profiles, image_profiles = [], []
    for pid in sorted(set(selected_profiles)):
        spec = profile_specs.get(pid)
        if spec is None or spec.get("partition") not in partitions:
            raise ValueError(f"invalid or unknown support profile: {pid!r}")
        partitions[str(spec["partition"])].append(pid)
        (host_profiles if spec.get("requires_host_runner") else image_profiles).append(pid)
    return {"partitions": partitions, "host_profiles": host_profiles, "image_profiles": image_profiles}
