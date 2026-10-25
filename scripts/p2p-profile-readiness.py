"""Allowlisted diagnostic projection of actual local-node status v1.

This consumes producer observations; it does not verify proofs or grant admission.
"""
import re

OBSERVER = "formal_public_observer_v1"
FULL_SYNC = "formal_full_state_sync_read_only_v1"
MAX_AGE_MS = 60_000


RUST_WHITESPACE = " \t\n\r\v\f\u0085\u00a0\u1680\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u2028\u2029\u202f\u205f\u3000"


def text(value):
    # Contract identifiers are opaque UTF-8 strings, not ASCII protocol tokens.
    if not isinstance(value, str) or not value.strip(RUST_WHITESPACE):
        return None
    try:
        value.encode("utf-8")
        return value
    except UnicodeEncodeError:
        return None


def node_label(value):
    # Match the formal CLI's UTF-8 byte budget and Rust str::trim contract.
    value = text(value)
    if value is None or value.startswith("triad-testnet-"):
        return None
    return value if len(value.encode("utf-8")) <= 256 else None


def uint(value):
    return value if type(value) is int and 0 <= value <= 2**64-1 else None


def project(status, source, as_of_unix_ms):
    result = dict(api_version="oasis7.profile-readiness/v1", source=source,
        status="unknown", reason="invalid_status", runtime_profile=None, can_propose=False,
        as_of_unix_ms=as_of_unix_ms, observed_at_unix_ms=None, age_ms=None,
        max_sample_age_ms=MAX_AGE_MS, source_freshness="unknown",
        trust_boundary="producer_observation_only", membership_readiness="unsupported",
        durable_signing_readiness="unsupported", protected_services_readiness="unsupported")

    def finish(state, reason):
        result.update(status=state, reason=reason)
        return result

    if not isinstance(status, dict):
        return result
    if "api_version" not in status:
        return finish("unsupported", "legacy_chain_status")
    if source not in ("file", "local_cli"):
        return finish("unsupported", "formal_status_requires_supported_source")
    if status.get("api_version") != "oasis7.local-node-status/v1":
        return finish("unknown", "unsupported_status_schema")
    profile = status.get("runtime_profile")
    if profile not in (OBSERVER, FULL_SYNC):
        return finish("unknown", "unsupported_runtime_profile")
    result["runtime_profile"] = profile
    node_id = node_label(status.get("node_id"))
    if not node_id:
        return finish("unknown", "invalid_node_identity")
    result["node_id"] = node_id
    observer = profile == OBSERVER
    child = status.get("formal_public_observer" if observer else "formal_full_state_sync")
    chain = status.get("chain")
    instance = status.get("instance")
    if not isinstance(chain, dict) or not isinstance(instance, dict) or not isinstance(child, dict):
        # A stopped instance can legitimately have no retained role observation.
        if isinstance(instance, dict) and instance.get("state") in ("stopped", "failed"):
            return finish("blocked", "instance_not_running")
        return finish("unknown", "missing_identity_or_observation")
    expected_api = "oasis7.formal-public-observer-status/v1" if observer else "oasis7.formal-full-sync-status/v1"
    if (child.get("api_version") != expected_api or chain.get("runtime_profile") != profile
        or child.get("runtime_profile") != profile
        or instance.get("api_version") != "oasis7.local-node-instance/v1"):
        return finish("unknown", "schema_or_profile_mismatch")
    world, chain_id = text(chain.get("world_id")), text(chain.get("chain_id"))
    role_config = chain.get("formal_observer" if observer else "formal_full_sync")
    expected_config_api = "oasis7.formal-public-observer/v1" if observer else "oasis7.formal-full-state-sync/v1"
    if not isinstance(role_config, dict) or role_config.get("api_version") != expected_config_api:
        return finish("unknown", "unsupported_role_config_schema")
    genesis = text(role_config.get("genesis_digest"))
    if not genesis or role_config.get("world_id") != world or role_config.get("chain_id") != chain_id:
        return finish("unknown", "role_config_identity_mismatch")
    incarnation, digest = text(instance.get("incarnation")), text(instance.get("config_digest"))
    generation = uint(instance.get("generation"))
    if not world or not chain_id or not incarnation or not digest or not generation:
        return finish("unknown", "invalid_identity")
    if (child.get("incarnation") != incarnation or type(child.get("generation")) is not int
        or child.get("generation") != generation or child.get("config_digest") != digest
        or status.get("config_digest") != digest or type(status.get("local_generation")) is not int
        or status.get("local_generation") != generation):
        return finish("unknown", "instance_identity_mismatch")
    result.update(world_id=world, chain_id=chain_id, incarnation=incarnation,
                  generation=generation, config_digest=digest)
    allowed_states = ({"starting", "qualified_public_read", "local_status_too_large",
                       "local_status_unavailable", "history_unavailable", "read_unavailable", "stopped"}
                      if observer else {"starting", "blocked_missing_issuer_grant", "syncing",
                                        "qualified_full_state_read_only", "sync_unavailable"})
    producer_state = child.get("read_state" if observer else "state")
    result["producer_state"] = producer_state if isinstance(producer_state, str) and producer_state in allowed_states else "unknown"
    ready = child.get("read_ready" if observer else "ready")
    result["producer_ready"] = ready if type(ready) is bool else None
    formal_ready = status.get("formal_readiness")
    result["formal_readiness"] = formal_ready if type(formal_ready) is bool else None
    instance_state = instance.get("state")
    result["instance_state"] = instance_state if isinstance(instance_state, str) and instance_state in {"queued", "running", "draining", "stopped", "failed"} else "unknown"
    # Historical witnesses stay visible even when the current attempt is blocked.
    # They remain producer observations bound to the admitted instance above.
    observed = uint(child.get("last_read_observed_unix_ms" if observer else "last_checked_unix_ms"))
    if observed is not None:
        result.update(observed_at_unix_ms=observed, age_ms=as_of_unix_ms-observed)
    if observer:
        freshness = child.get("source_freshness")
        result["source_freshness"] = freshness if freshness in ("unknown", "fresh", "stale") else "unknown"
        commit = child.get("last_verified_commit")
        if isinstance(commit, dict) and isinstance(commit.get("world"), dict) and commit["world"].get("world_id") == world and commit["world"].get("genesis_digest") == genesis:
            position = uint(commit.get("position"))
            block, root = text(commit.get("execution_block_hash")), text(commit.get("state_root_ref"))
            if position is not None and block and root and text(commit["world"].get("genesis_digest")):
                result["last_verified_commit"] = dict(position=position, execution_block_hash=block, state_root_ref=root)
        result["logical_tick"] = uint(child.get("logical_tick"))
    else:
        result["grant_epoch"] = uint(child.get("grant_epoch"))
        for name in ("loaded_cut", "last_verified_cut"):
            cut = child.get(name)
            if not isinstance(cut, dict) or not isinstance(cut.get("world"), dict) or cut["world"].get("world_id") != world or cut["world"].get("genesis_digest") != genesis or cut.get("chain_id") != chain_id:
                continue
            fields = {key: text(cut.get(key)) for key in ("record_id", "after_state_root", "active_authority_id", "finalizing_authority_id")}
            position, authority_epoch = uint(cut.get("position")), uint(cut.get("authority_epoch"))
            if all(fields.values()) and position and authority_epoch and text(cut["world"].get("genesis_digest")):
                result[name] = dict(position=position, authority_epoch=authority_epoch, **fields)
        for name in ("state_root_verified", "world_loaded", "catch_up_pending"):
            result[name] = child.get(name) if type(child.get(name)) is bool else None
        result["lag_positions"] = uint(child.get("lag_positions"))
    role = "observer" if observer else "full_state_sync"
    if status.get("configured_role") != role:
        return finish("unknown", "configured_role_mismatch")
    result["configured_role"] = role
    bool_fields = [(status, "formal_readiness"), (status, "configuration_changed"),
                   (status, "can_propose"), (child, "can_propose"),
                   (child, "read_ready" if observer else "ready")]
    if not observer:
        bool_fields += [(child, "world_loaded"), (child, "state_root_verified")]
    if any(type(obj.get(key)) is not bool for obj, key in bool_fields):
        return finish("unknown", "invalid_boolean")
    if "blocker" not in status or "error" not in child:
        return finish("unknown", "missing_error_state")
    if status["can_propose"] or child["can_propose"]:
        return finish("unknown", "read_only_boundary_mismatch")
    if instance.get("state") != "running":
        return finish("blocked", "instance_not_running")
    if (status["blocker"] is not None and not isinstance(status["blocker"], str)) or (child["error"] is not None and not isinstance(child["error"], str)):
        return finish("unknown", "invalid_error_state")
    if status["blocker"] is not None or child["error"] is not None:
        return finish("blocked", "producer_blocked")
    if status["configuration_changed"]:
        return finish("blocked", "configuration_changed")
    observed = uint(child.get("last_read_observed_unix_ms" if observer else "last_checked_unix_ms"))
    if observed is None:
        return finish("unknown", "missing_observation_time")
    result["observed_at_unix_ms"] = observed
    result["age_ms"] = as_of_unix_ms - observed
    if observed > as_of_unix_ms:
        return finish("unknown", "future_observation")
    if result["age_ms"] > MAX_AGE_MS:
        return finish("stale", "observation_expired")
    scope_key = "state_sync_scope" if observer else "sync_scope"
    expected_scope = "public_projection_only" if observer else "issuer_granted_full_state_read_only"
    if child.get(scope_key) != expected_scope:
        return finish("unknown", "read_only_scope_mismatch")
    result["scope"] = expected_scope
    if observer:
        freshness = child.get("source_freshness")
        result["source_freshness"] = freshness if freshness in ("unknown", "fresh", "stale") else "unknown"
        if child.get("full_state_sync_readiness") != "unknown":
            return finish("unknown", "projection_scope_mismatch")
        commit = child.get("last_verified_commit")
        if not isinstance(commit, dict) or not isinstance(commit.get("world"), dict):
            return finish("unknown", "missing_verified_commit")
        if commit["world"].get("world_id") != world or commit["world"].get("genesis_digest") != genesis:
            return finish("unknown", "commit_world_mismatch")
        position = uint(commit.get("position"))
        block, root = text(commit.get("execution_block_hash")), text(commit.get("state_root_ref"))
        if position is None or not block or not root:
            return finish("unknown", "invalid_verified_commit")
        result["last_verified_commit"] = dict(position=position, execution_block_hash=block, state_root_ref=root)
        result["logical_tick"] = uint(child.get("logical_tick"))
        if result["logical_tick"] is None:
            return finish("unknown", "invalid_logical_tick")
        if child.get("read_state") != "qualified_public_read" or not child["read_ready"]:
            return finish("blocked", "public_read_not_ready")
    else:
        if child.get("world_id") != world or child.get("chain_id") != chain_id:
            return finish("unknown", "sync_identity_mismatch")
        epoch = uint(child.get("grant_epoch"))
        if epoch is None or epoch == 0:
            return finish("unknown", "invalid_grant_epoch")
        result["grant_epoch"] = epoch
        for name in ("loaded_cut", "last_verified_cut"):
            cut = child.get(name)
            if not isinstance(cut, dict) or not isinstance(cut.get("world"), dict):
                return finish("unknown", "missing_sync_cut")
            if cut["world"].get("world_id") != world or cut["world"].get("genesis_digest") != genesis or cut.get("chain_id") != chain_id:
                return finish("unknown", "sync_cut_identity_mismatch")
            fields = {key: text(cut.get(key)) for key in ("record_id", "after_state_root", "active_authority_id", "finalizing_authority_id")}
            position, authority_epoch = uint(cut.get("position")), uint(cut.get("authority_epoch"))
            if not all(fields.values()) or not position or not authority_epoch or not text(cut["world"].get("genesis_digest")):
                return finish("unknown", "invalid_sync_cut")
            result[name] = dict(position=position, authority_epoch=authority_epoch, **fields)
        if child["loaded_cut"]["world"] != child["last_verified_cut"]["world"]:
            return finish("unknown", "sync_cut_world_mismatch")
        loaded, verified = result["loaded_cut"], result["last_verified_cut"]
        if loaded["position"] > verified["position"] or (loaded["position"] == verified["position"] and loaded != verified):
            return finish("unknown", "sync_cut_mismatch")
        lag = uint(child.get("lag_positions"))
        catching_up = child.get("catch_up_pending")
        if lag is None or type(catching_up) is not bool or lag != verified["position"] - loaded["position"] or catching_up != (lag > 0):
            return finish("unknown", "sync_lag_mismatch")
        result.update(state_root_verified=child["state_root_verified"], world_loaded=child["world_loaded"], lag_positions=lag, catch_up_pending=catching_up)
        if not child["ready"] or not child["state_root_verified"] or not child["world_loaded"] or child.get("state") != "qualified_full_state_read_only":
            return finish("blocked", "full_sync_not_ready")
    if not status["formal_readiness"]:
        return finish("blocked", "formal_readiness_false")
    return finish("ready", "qualified_producer_observation")


def markdown(result):
    import json
    lines = ["", "## Profile Readiness"]
    for key, value in result.items():
        rendered = json.dumps(value, ensure_ascii=True, sort_keys=True)
        longest = max((len(run) for run in re.findall(r"`+", rendered)), default=0)
        delimiter = "`" * (longest + 1)
        # JSON strings begin/end with a quote; other allowlisted JSON values
        # cannot begin/end in backticks. Escaped newlines stay inside the span.
        lines.append(f"- {key}: {delimiter}{rendered}{delimiter}")
    return lines
