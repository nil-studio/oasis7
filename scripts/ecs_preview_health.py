"""Validate the trusted local deployment verifier's evidence, never channel code.

The verifier must perform signed reads using the existing stable identity. A
fresh nonce binds each response to the coordinator's request; the coordinator
checks it before this independent evidence validation.
"""
import math
import re


def validate_evidence(evidence, expected_identity, expected_components, min_elapsed=60):
    if not isinstance(evidence, dict) or evidence.get("identity") != expected_identity:
        raise ValueError("health: original stable world/read identity mismatch")
    if evidence.get("components") != expected_components:
        raise ValueError("health: actual component bytes/version/path mismatch")
    for component in expected_components.values():
        if not re.fullmatch(r"[0-9a-f]{64}", component.get("sha256", "")):
            raise ValueError("health: missing actual component digest")
    checks = evidence.get("checks", {})
    if any(checks.get(name) is not True for name in ("api", "web", "skill", "signed_world")):
        raise ValueError("health: public API, Web, Skill and signed world checks required")
    samples = evidence.get("samples")
    if not isinstance(samples, list) or len(samples) < 4:
        raise ValueError("health: baseline plus at least three world advances required")
    timestamps, ticks = [], []
    for sample in samples:
        if not isinstance(sample, dict):
            raise ValueError("health: invalid signed world sample")
        if sample.get("identity") != expected_identity or sample.get("signed_world") is not True:
            raise ValueError("health: every sample must verify the original signed identity")
        stamp, tick = sample.get("timestamp"), sample.get("world_tick")
        if type(stamp) not in (int, float) or not math.isfinite(stamp) or type(tick) is not int or tick < 0:
            raise ValueError("health: invalid timestamp or world tick")
        timestamps.append(stamp)
        ticks.append(tick)
    if any(b <= a for a, b in zip(timestamps, timestamps[1:])):
        raise ValueError("health: sample timestamps must strictly advance")
    if timestamps[-1] - timestamps[0] < max(60, min_elapsed):
        raise ValueError("health: full continuous 60-second observation required")
    if any(b < a for a, b in zip(ticks, ticks[1:])) or sum(b > a for a, b in zip(ticks, ticks[1:])) < 3:
        raise ValueError("health: at least three distinct canonical world advances required")
    if evidence.get("chain_write_status") not in ("unchanged", "compatible", "unknown", "incompatible"):
        raise ValueError("health: chain write state must be explicit")
    return evidence
