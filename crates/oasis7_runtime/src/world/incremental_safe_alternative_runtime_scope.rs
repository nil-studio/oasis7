//! Exact consumed projection of the original Runtime binding predicate.
//! This borrowed result is DATA; only the actual ledger/Current join can mint a
//! matching owner. No RuntimeBinding or replacement base-world hash is created.
use serde_json::Value;
pub(super) struct SafeAlternativeRuntimeScopeV1<'a> {
    pub world: &'a str,
    pub branch: &'a str,
    pub reorg: u64,
}
pub(super) fn runtime_scope_v1<'a>(
    raw: &'a Value,
    manifest_world: &str,
    manifest_hash: &str,
) -> Result<SafeAlternativeRuntimeScopeV1<'a>, &'static str> {
    let binding = raw.as_object().ok_or("runtime_binding_missing")?;
    let text = |key: &str| {
        binding
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .ok_or("runtime_binding_invalid")
    };
    let world = text("world_id")?;
    let branch = text("branch_id")?;
    if manifest_world != "unbound" && manifest_world != world {
        return Err("runtime_world_id_mismatch");
    }
    let status = text("finality_status")?;
    binding
        .get("finality_epoch")
        .and_then(Value::as_u64)
        .ok_or("runtime_binding_invalid")?;
    let reorg = binding
        .get("reorg_epoch")
        .and_then(Value::as_u64)
        .ok_or("runtime_binding_invalid")?;
    let hash = match binding.get("finality_block_hash") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.as_str()),
        _ => return Err("runtime_binding_invalid"),
    };
    if !crate::cognition::finality_binding_is_legal(status, hash) {
        return Err("runtime_binding_invalid");
    }
    if binding.get("runtime_manifest_hash").and_then(Value::as_str) != Some(manifest_hash) {
        return Err("runtime_manifest_mismatch");
    }
    // Original current_cognition_runtime_binding then derives two digests from
    // genuine typed Current inputs. Their constructors always return canonical
    // BLAKE3 text; RuntimeBinding.validate only tests their shape. SafeAlternative
    // consumes neither digest. Keep every remaining fallible validation here,
    // rather than serialize the entire World to compute unused output hashes.
    if branch.trim().is_empty() || branch.len() > 128 {
        return Err("recovery_pending");
    }
    Ok(SafeAlternativeRuntimeScopeV1 {
        world,
        branch,
        reorg,
    })
}
