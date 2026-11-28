"""Pure-stdlib CI planning and schema engine with zero repository-specific identifiers."""

from .capability_schema import (
    derive_compact_excluded_packages, derive_logical_harness_aliases, derive_ordinary_packages,
    derive_required_wasm_packages, derive_support_profiles, derive_universal_v2_anchors,
    reconcile_cross_version_members, validate_package_capabilities_against_workspace, validate_package_capabilities_static,
)
from .rust_source_graph import classify_item_bodies, eval_cfg_expr, extract_source_inputs, rust_literal_path, scan_rust_source_inputs, tokenize_rust_source
from .shard_scheduler import assign_lpt_shards, compute_artifact_partitions, compute_inline_partitions, compute_support_partitions, supports_universal_v2

__all__ = [
    "assign_lpt_shards", "classify_item_bodies", "compute_artifact_partitions", "compute_inline_partitions",
    "compute_support_partitions", "derive_compact_excluded_packages", "derive_logical_harness_aliases",
    "derive_ordinary_packages", "derive_required_wasm_packages", "derive_support_profiles",
    "derive_universal_v2_anchors", "eval_cfg_expr", "extract_source_inputs", "reconcile_cross_version_members",
    "rust_literal_path", "scan_rust_source_inputs", "supports_universal_v2", "tokenize_rust_source",
    "validate_package_capabilities_against_workspace", "validate_package_capabilities_static",
]
