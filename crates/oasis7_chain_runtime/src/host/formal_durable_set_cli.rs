//! Private exact-owner admission for a signed healthy boundary. This command
//! does not produce a Qualified record, seal or successor activation receipt.
pub(crate) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    #[cfg(unix)]
    {
        run_native(args)
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        Err("UNSUPPORTED_ROLE_OR_PROFILE: durable-set owner control requires Unix".into())
    }
}
#[cfg(unix)]
fn run_native<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    use super::formal_live_runtime::{
        DurableSetControlRequestV1, FormalRuntimeInstanceV1, control_durable_set_transition,
    };
    use oasis7_distfs::controlled_authority::formal::SignedNormalDurableSetTransitionV1;
    use std::{collections::BTreeMap, path::Path, time::Duration};
    let action = args.next().ok_or("durable-set requires arm or query")?;
    if !matches!(action, "arm" | "query") {
        return Err("durable-set requires arm or query".into());
    }
    let mut options = BTreeMap::new();
    while let Some(name) = args.next() {
        if !matches!(
            name,
            "--config"
                | "--owner-instance"
                | "--operation-id"
                | "--transition"
                | "--boundary-position"
                | "--timeout-ms"
        ) {
            return Err(format!("unsupported durable-set argument {name}"));
        }
        let value = args.next().ok_or_else(|| format!("missing {name} value"))?;
        if options.insert(name, value).is_some() {
            return Err(format!("duplicate durable-set argument {name}"));
        }
    }
    let required = |name| {
        options
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing {name}"))
    };
    let config = Path::new(required("--config")?);
    let owner_path = Path::new(required("--owner-instance")?);
    super::controlled_live_config::require_private_owned(owner_path, false)?;
    let instance: FormalRuntimeInstanceV1 = super::node_lifecycle::local_io::read(owner_path)?;
    let operation_id = required("--operation-id")?.to_owned();
    let request = if action == "arm" {
        if options.contains_key("--boundary-position") {
            return Err("arm does not accept --boundary-position".into());
        }
        let path = Path::new(required("--transition")?);
        super::controlled_live_config::require_private_owned(path, false)?;
        let transition: SignedNormalDurableSetTransitionV1 =
            super::node_lifecycle::local_io::read(path)?;
        if transition.body.operation_id != operation_id {
            return Err("durable-set operation differs from signed transition".into());
        }
        DurableSetControlRequestV1::Arm {
            operation_id: operation_id.clone(),
            transition: Box::new(transition),
        }
    } else {
        if options.contains_key("--transition") {
            return Err("query does not accept --transition".into());
        }
        let boundary_position = required("--boundary-position")?
            .parse::<u64>()
            .map_err(|_| "invalid boundary position")?;
        if boundary_position == 0 {
            return Err("durable-set boundary position must be positive".into());
        }
        DurableSetControlRequestV1::Query {
            operation_id: operation_id.clone(),
            boundary_position,
        }
    };
    let timeout_ms = options
        .get("--timeout-ms")
        .map_or(Ok(30_000_u64), |value| {
            value.parse::<u64>().map_err(|_| "invalid timeout")
        })?;
    if !(1..=60_000).contains(&timeout_ms) {
        return Err("timeout in [1,60000] milliseconds required".into());
    }
    let status = control_durable_set_transition(
        config,
        &instance,
        request,
        Duration::from_millis(timeout_ms),
    )?;
    if action == "arm" && status.is_none() {
        return Err(
            "ORIGINAL_OUTCOME_UNKNOWN: arm receipt missing; query original identity".into(),
        );
    }
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "operation_id": operation_id, "owner_instance": instance, "status": status,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}
