use super::{
    io,
    source::{self, Launch, Plan},
    tree, verify,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub api_version: String,
    pub plan_digest: String,
    pub operation_id: String,
    pub destination: PathBuf,
    pub source_preserved: bool,
    pub switched_source_launch: bool,
    pub closure: verify::Closure,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    api_version: String,
    plan_digest: String,
    verified_entries: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    api_version: String,
    phase: String,
    plan: Plan,
}
fn intent_path(plan: &Plan) -> Result<PathBuf, String> {
    Ok(plan
        .destination
        .parent()
        .ok_or("destination parent")?
        .join(format!(
            ".legacy-migration-{}.intent.json",
            plan.operation_id
        )))
}
fn read_intent(plan: &Plan) -> Result<Option<Intent>, String> {
    let path = intent_path(plan)?;
    if !path.exists() {
        io::reject_symlinks(&path)?;
        return Ok(None);
    }
    crate::controlled_live_config::require_private_owned(&path, false)?;
    let saved: Intent = io::read(&path)?;
    if saved.api_version != "oasis7.legacy-migration-intent/v1"
        || !matches!(saved.phase.as_str(), "admitted" | "copying" | "published")
        || io::digest(&saved.plan)? != io::digest(plan)?
    {
        return Err("immutable migration admission/source changed".into());
    }
    Ok(Some(saved))
}
fn save_phase(plan: &Plan, phase: &str) -> Result<(), String> {
    io::write(
        &intent_path(plan)?,
        &Intent {
            api_version: "oasis7.legacy-migration-intent/v1".into(),
            phase: phase.into(),
            plan: plan.clone(),
        },
    )
}
fn unpopulated_stage(plan: &Plan) -> Result<(), String> {
    crate::controlled_live_config::require_private_owned(&plan.stage, true)?;
    for entry in fs::read_dir(&plan.stage).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".node-write-")
        {
            return Err(
                "ORIGINAL_OUTCOME_UNKNOWN: stage lost plan after material admission".into(),
            );
        }
        io::regular(&entry.path())?;
        crate::controlled_live_config::require_private_owned(&entry.path(), false)?;
        if entry.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
            return Err("partial plan metadata bound".into());
        }
    }
    Ok(())
}
fn paths_at(plan: &Plan, root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    plan.paths
        .iter()
        .map(|(flag, path)| {
            let component = plan
                .components
                .iter()
                .find(|c| path.starts_with(&c.source))
                .ok_or("migration path has no owning component")?;
            Ok((flag.clone(), source::relocated_path(root, component, path)?))
        })
        .collect()
}
fn validate_published(plan: &Plan) -> Result<Receipt, String> {
    crate::controlled_live_config::require_private_owned(&plan.destination, true)?;
    let saved: Plan = io::read(&plan.destination.join("migration-plan.json"))?;
    let digest = io::digest(plan)?;
    if io::digest(&saved)? != digest {
        return Err("published destination belongs to another immutable migration".into());
    }
    check_stage_root(&plan.destination, false)?;
    tree::verify_exact(&plan.files, &plan.components, &plan.destination)?;
    tree::verify_entries(&plan.external_dependencies, None)?;
    let closure = verify::closure(
        &paths_at(plan, &plan.destination)?,
        &plan.world_id,
        &plan.node_id,
    )?;
    if closure != plan.closure {
        return Err("published migrated head/business/replication closure changed".into());
    }
    let receipt: Receipt = io::read(&plan.destination.join("migration-receipt.json"))?;
    if receipt.api_version != "oasis7.legacy-migration-receipt/v1"
        || receipt.plan_digest != digest
        || receipt.operation_id != plan.operation_id
        || receipt.destination != plan.destination
        || receipt.closure != plan.closure
        || !receipt.source_preserved
        || receipt.switched_source_launch
    {
        return Err("migration receipt mismatch".into());
    }
    source::load_launch(&plan.destination.join("legacy-launch.json"))?;
    Ok(receipt)
}
pub(super) fn status(
    source: &Path,
    destination: &Path,
    id: &str,
) -> Result<Option<Receipt>, String> {
    let plan = source::plan(source, destination, id)?;
    let intent = read_intent(&plan)?;
    if destination.exists() {
        if intent.is_none() {
            return Err("published destination lacks original admission".into());
        }
        return validate_published(&plan).map(Some);
    }
    if let Some(intent) = intent {
        if !plan.stage.exists() {
            if intent.phase != "admitted" {
                return Err("ORIGINAL_OUTCOME_UNKNOWN: original copying/publication stage and destination absent".into());
            }
        } else if plan.stage.join("migration-plan.json").exists() {
            let saved: Plan = io::read(&plan.stage.join("migration-plan.json"))?;
            if io::digest(&saved)? != io::digest(&plan)? {
                return Err("partial stage plan changed".into());
            }
        } else if intent.phase == "admitted" {
            unpopulated_stage(&plan)?;
        } else {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: copying stage plan missing".into());
        }
    } else if plan.stage.exists() {
        return Err("stage exists without original admission".into());
    }
    Ok(None)
}
pub(super) fn apply(source: &Path, destination: &Path, id: &str) -> Result<Receipt, String> {
    let preview = source::plan(source, destination, id)?;
    // The SAME fence is held by legacy run_chain_runtime. A live source cannot
    // be imported, and we never signal a PID or infer stop from a process list.
    let _world_fence = crate::world_writer_lock::acquire_live_world_writer_lock(
        &preview.paths["--execution-world-dir"],
    )?;
    let plan = source::plan(source, destination, id)?;
    if io::digest(&preview)? != io::digest(&plan)? {
        return Err("source changed before migration writer fence".into());
    }
    let lock = io::private_file(
        &destination
            .parent()
            .ok_or("destination parent missing")?
            .join(format!(".legacy-migration-{id}.lock")),
    )?;
    lock.try_lock()
        .map_err(|_| "migration operation writer busy")?;
    let intent = if let Some(intent) = read_intent(&plan)? {
        intent
    } else {
        if destination.exists() || plan.stage.exists() {
            return Err("migration destination/stage exists without original admission".into());
        }
        save_phase(&plan, "admitted")?;
        Intent {
            api_version: "oasis7.legacy-migration-intent/v1".into(),
            phase: "admitted".into(),
            plan: plan.clone(),
        }
    };
    if destination.exists() {
        let receipt = validate_published(&plan).map_err(|error| {
            format!(
                "validate already-published migration {}: {error}",
                destination.display()
            )
        })?;
        save_phase(&plan, "published")?;
        return Ok(receipt);
    }
    if intent.phase == "published" {
        return Err("ORIGINAL_OUTCOME_UNKNOWN: published destination disappeared".into());
    }
    let digest = io::digest(&plan)?;
    if !plan.stage.exists() {
        if intent.phase != "admitted" {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: original copying stage disappeared".into());
        }
        tree::require_capacity(&plan.files, &plan.stage).map_err(|error| {
            format!(
                "check migration stage capacity {}: {error}",
                plan.stage.display()
            )
        })?;
        io::private_dir(&plan.stage)
            .map_err(|error| format!("create migration stage {}: {error}", plan.stage.display()))?;
    }
    #[cfg(test)]
    if FAIL_BEFORE_STAGE_PLAN.with(|n| n.get()) {
        return Err("injected admission-stage interruption".into());
    }
    if plan.stage.join("migration-plan.json").exists() {
        crate::controlled_live_config::require_private_owned(&plan.stage, true)?;
        let saved: Plan = io::read(&plan.stage.join("migration-plan.json"))?;
        if io::digest(&saved)? != digest {
            return Err("partial migration belongs to another source/request".into());
        }
    } else {
        if intent.phase != "admitted" {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: copying stage plan disappeared".into());
        }
        unpopulated_stage(&plan)?;
        check_stage_root(&plan.stage, true)?;
        io::write(&plan.stage.join("migration-plan.json"), &plan).map_err(|error| {
            format!(
                "persist immutable migration plan {}: {error}",
                plan.stage.join("migration-plan.json").display()
            )
        })?;
    }
    tree::require_capacity(&plan.files, &plan.stage).map_err(|error| {
        format!(
            "check migration stage capacity {}: {error}",
            plan.stage.display()
        )
    })?;
    check_stage_root(&plan.stage, true)?;
    io::private_dir(&plan.stage.join("components")).map_err(|error| {
        format!(
            "create migration component root {}: {error}",
            plan.stage.join("components").display()
        )
    })?;
    tree::verify_entries(&plan.files, None)
        .map_err(|error| format!("reverify legacy source files before copying: {error}"))?;
    tree::verify_entries(&plan.external_dependencies, None)
        .map_err(|error| format!("reverify external migration dependencies: {error}"))?;
    save_phase(&plan, "copying")
        .map_err(|error| format!("persist migration copying phase: {error}"))?;
    for (index, entry) in plan.files.iter().enumerate() {
        tree::copy(entry, &plan.stage).map_err(|error| {
            format!(
                "copy migration entry {} to {}: {error}",
                entry.source.display(),
                plan.stage.join(&entry.target).display()
            )
        })?;
        io::write(
            &plan.stage.join("migration-progress.json"),
            &Progress {
                api_version: "oasis7.legacy-migration-progress/v1".into(),
                plan_digest: digest.clone(),
                verified_entries: index + 1,
            },
        )
        .map_err(|error| {
            format!(
                "persist migration progress after {} entries: {error}",
                index + 1
            )
        })?;
        #[cfg(test)]
        if FAIL_AFTER_ENTRY.with(|n| n.get() == Some(index + 1)) {
            return Err("injected migration interruption".into());
        }
    }
    tree::verify_exact(&plan.files, &plan.components, &plan.stage)
        .map_err(|error| format!("verify complete migration stage: {error}"))?;
    let closure = verify::closure(
        &paths_at(&plan, &plan.stage)?,
        &plan.world_id,
        &plan.node_id,
    )
    .map_err(|error| format!("verify staged migrated-world closure: {error}"))?;
    if closure != plan.closure {
        return Err("staged original committed head/business/CAS closure differs".into());
    }
    // Source digest/tree are checked again while the original writer fence is
    // still held. Other data owners may not bypass this migration precondition.
    let current = source::plan(source, destination, id)
        .map_err(|error| format!("revalidate original source before publication: {error}"))?;
    if io::digest(&current)? != digest {
        return Err("source changed during migration".into());
    }
    let launch = Launch {
        runtime_binary_sha256: plan.runtime_binary_sha256.clone(),
        api_version: "oasis7.legacy-runtime-launch/v1".into(),
        profile: "legacy_observer_compatibility_v1".into(),
        plan_digest: digest.clone(),
        effective_options_digest: io::digest(&format!(
            "{:?}",
            source::options(&plan.launch_argv)?
        ))?,
        destination: plan.destination.clone(),
        argv: plan.launch_argv.clone(),
        backup_components: plan.components.clone(),
        external_dependencies: plan.external_dependencies.clone(),
        node_id: plan.node_id.clone(),
        public_key: plan.public_key.clone(),
        peer_id: plan.peer_id.clone(),
        reward_signer_id: plan.reward_signer_id.clone(),
        reward_signer_public_key: plan.reward_signer_public_key.clone(),
        world_id: plan.world_id.clone(),
    };
    io::write(&plan.stage.join("legacy-launch.json"), &launch).map_err(|error| {
        format!(
            "persist aggregate launch {}: {error}",
            plan.stage.join("legacy-launch.json").display()
        )
    })?;
    let receipt = Receipt {
        api_version: "oasis7.legacy-migration-receipt/v1".into(),
        plan_digest: digest,
        operation_id: id.into(),
        destination: destination.into(),
        source_preserved: true,
        switched_source_launch: false,
        closure,
    };
    io::write(&plan.stage.join("migration-receipt.json"), &receipt).map_err(|error| {
        format!(
            "persist migration receipt {}: {error}",
            plan.stage.join("migration-receipt.json").display()
        )
    })?;
    tree::sync_directories(&plan.files, &plan.stage)
        .map_err(|error| format!("sync completed migration stage: {error}"))?;
    tree::publish(&plan.stage, destination).map_err(|error| {
        format!(
            "publish completed migration {}: {error}",
            destination.display()
        )
    })?;
    let receipt = validate_published(&plan).map_err(|error| {
        format!(
            "validate published migration {}: {error}",
            destination.display()
        )
    })?;
    save_phase(&plan, "published")
        .map_err(|error| format!("persist published migration phase: {error}"))?;
    Ok(receipt)
}
#[cfg(test)]
thread_local! {pub(super) static FAIL_AFTER_ENTRY:std::cell::Cell<Option<usize>>=const{std::cell::Cell::new(None)};pub(super) static FAIL_BEFORE_STAGE_PLAN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}

fn check_stage_root(root: &Path, clean_temporary: bool) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non UTF8 stage metadata")?;
        if matches!(
            name,
            "components"
                | "migration-plan.json"
                | "migration-progress.json"
                | "legacy-launch.json"
                | "migration-receipt.json"
        ) {
            continue;
        }
        if clean_temporary && name.starts_with(".node-write-") {
            crate::controlled_live_config::require_private_owned(&entry.path(), false)?;
            io::regular(&entry.path())?;
            if entry.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
                return Err("oversized partial migration metadata".into());
            }
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        } else {
            return Err("foreign/unexpected migration stage metadata refused".into());
        }
    }
    if clean_temporary {
        io::sync_dir(root)?;
    }
    Ok(())
}
