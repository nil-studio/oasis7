//! Deliberate, same-owner original pending freeze and read-only verification.
//! Neither a private owner candidate nor this CLI issues protected P/R authority.
use super::super::{
    formal_live_recovery_original_adapter, 
    formal_live_runtime::{self, FormalRuntimeOwnerSnapshotV1},
};
use std::{path::Path, time::Duration};

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let freeze = match args.first().map(String::as_str) {
        Some("formal-original-freeze") => true,
        Some("formal-original-verify") => false,
        _ => return Err("unsupported original pending operation".into()),
    };
    let expected_len = if freeze { 5 } else { 9 };
    if args.len() != expected_len || args[1] != "--config" || args[3] != "--owner-snapshot" {
        return Err(
            "formal-original-freeze --config FILE --owner-snapshot FILE; \
             formal-original-verify --config FILE --owner-snapshot FILE --root HEX --count N"
                .into(),
        );
    }
    if !freeze && (args[5] != "--root" || args[7] != "--count") {
        return Err("original pending independent root/count must be explicit".into());
    }
    // Never use a self-returned candidate pin as a protected recovery credential.
    let external = if freeze {
        None
    } else {
        let root = &args[6];
        if root.len() != 64
            || !root
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("original pending independent root must be canonical hex".into());
        }
        let count: usize = args[8]
            .parse()
            .map_err(|_| "original pending independent count invalid")?;
        if count > 4096 {
            return Err("original pending independent count exceeds bound".into());
        }
        Some((root.as_str(), count))
    };
    let config = Path::new(&args[2]);
    let supplied: FormalRuntimeOwnerSnapshotV1 = serde_json::from_slice(
        &std::fs::read(&args[4]).map_err(|_| "original pending owner snapshot unreadable")?,
    )
    .map_err(|_| "original pending owner snapshot invalid")?;
    let current = if freeze {
        // Stop commands require the currently serving owner and its live socket.
        formal_live_runtime::current_operation_owner(config)?
    } else {
        // After SIGKILL the former process socket may be gone; obtain only the
        // exact persisted incarnation, never re-enable it or infer a new grant.
        formal_live_runtime::query_operation_owner(
            config,
            &supplied.instance.operation_id,
            &supplied.instance.request_digest,
            &supplied.instance.config_digest,
            Some(&supplied.instance.incarnation),
        )?.ok_or("original pending exact owner is missing")?
    };
    if current.instance != supplied.instance {
        return Err("original pending CLI owner instance differs from exact recorded owner".into());
    }

    if let Some((independent_root, independent_count)) = external {
        // Enter the *verified Host* cold-original consumer, not just the
        // local owner byte-reader. The Host additionally checks original C0
        // authority/World/config and rereads the exact shipping operation's
        // private immutable source. Neither result is recovery permission.
        let original = formal_live_recovery_original_adapter::cold_read_original_for_config(
            config,
            &supplied.instance,
            &supplied.instance.authority_id,
            independent_root,
            independent_count,
        )?;
        // Never print raw signed requests or imply that local byte equality
        // authorizes a new writer, recovered receipt or fresh request.
        println!(
            "{}",
            serde_json::json!({
                "status": "original_bytes_only_not_protected_authority",
                "root": independent_root,
                "count": original.pending().handoff().requests.len()
            })
        );
    } else {
        if current.state != "ready" {
            return Err("original pending freeze requires the ready exact owner".into());
        }
        let pin = formal_live_runtime::freeze_original_pending(
            config,
            &supplied.instance,
            Duration::from_secs(45),
        )?;
        println!(
            "{}",
            serde_json::json!({
                "status": "untrusted_candidate_requires_independent_pin",
                "root": pin.root,
                "count": pin.count
            })
        );
    }
    Ok(())
}
