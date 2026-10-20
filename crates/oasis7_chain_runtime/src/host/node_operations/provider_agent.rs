use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
};

pub(super) trait SystemdRunner {
    fn validate_unit(&mut self, target: &AgentTargetV1) -> Result<(), String>;
    fn active(&mut self, unit: &str) -> Result<bool, String>;
    fn effect(&mut self, request: &AgentRequestV1) -> Result<(), String>;
}

struct RealSystemd;
impl SystemdRunner for RealSystemd {
    fn validate_unit(&mut self, target: &AgentTargetV1) -> Result<(), String> {
        verify_systemd_unit(target)
    }
    fn active(&mut self, unit: &str) -> Result<bool, String> {
        systemd_active(unit)
    }
    fn effect(&mut self, request: &AgentRequestV1) -> Result<(), String> {
        if request.step == RemoteStepV1::Stop {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("remote stop target missing")?;
            actions::stop(
                &request.target.data_dir,
                &request.action_id,
                expected,
                30_000,
            )?;
        } else {
            actions::start(
                &request.target.config,
                &request.action_id,
                request.expected_generation,
                &request.expected_config_digest,
            )?;
        }
        systemctl_effect(request)
    }
}

pub(super) struct Agent;
impl Agent {
    fn apply_or_query(request: AgentRequestV1) -> Result<Option<RemoteActionReceiptV1>, String> {
        Self::with_runner(request, &mut RealSystemd)
    }

    pub(super) fn with_runner(
        request: AgentRequestV1,
        runner: &mut impl SystemdRunner,
    ) -> Result<Option<RemoteActionReceiptV1>, String> {
        validate_agent_request(&request)?;
        let root = &request.target.data_dir;
        let action_dir = root.join("managed-actions");
        verify_target_binding(&request, runner)?;
        // Query and apply share this lock so a query cannot overwrite a
        // partially reconciled receipt while an apply is persisting its
        // original intent or effect readback.
        io::private_dir(&action_dir)?;
        let lock = io::private_file(&action_dir.join("agent.lock"))?;
        lock.try_lock().map_err(|_| "remote operation agent busy")?;
        verify_target_binding(&request, runner)?;
        if request.mode == AgentModeV1::Query {
            let path = action_path(&action_dir, &request.action_id)?;
            io::reject_symlinks(&path)?;
            if !path.exists() {
                let bound = verify_management_binding(&request)?;
                if !bound {
                    if actions::query(&request.target.data_dir, &request.action_id)?.is_some() {
                        return Err("native action exists without an immutable original management request; retroactive adoption refused".into());
                    }
                    return Ok(None);
                }
                let original = reconstruct_agent_receipt(&request, runner)?;
                let updated = reconcile(&request, original, runner)?;
                io::write(&path, &updated)?;
                return Ok(Some(updated));
            }
            let old: RemoteActionReceiptV1 = io::read(&path)?;
            validate_receipt(&old, &request)?;
            if !verify_management_binding(&request)? {
                return Err(
                    "remote action receipt lacks its original immutable management request".into(),
                );
            }
            let updated = reconcile(&request, old, runner)?;
            io::write(&path, &updated)?;
            return Ok(Some(updated));
        }
        let path = action_path(&action_dir, &request.action_id)?;
        if path.exists() {
            let old: RemoteActionReceiptV1 = io::read(&path)?;
            validate_receipt(&old, &request)?;
            if !verify_management_binding(&request)? {
                return Err(
                    "remote action receipt lacks its original immutable management request".into(),
                );
            }
            let updated = reconcile(&request, old, runner)?;
            io::write(&path, &updated)?;
            return Ok(Some(updated));
        }
        if verify_management_binding(&request)? {
            let original = reconstruct_agent_receipt(&request, runner)?;
            let updated = reconcile(&request, original, runner)?;
            io::write(&path, &updated)?;
            return Ok(Some(updated));
        }
        validate_action_target(&request)?;
        let pending = receipt(&request, RemoteOutcomeV1::Pending, None, None);
        io::write(&path, &pending)?; // durable original intent precedes systemd
        if let Err(error) = bind_management_request(&request) {
            let unknown = receipt(
                &request,
                RemoteOutcomeV1::Unknown,
                None,
                Some(format!(
                    "ORIGINAL_OUTCOME_UNKNOWN: immutable management request was not bound: {error}"
                )),
            );
            io::write(&path, &unknown)?;
            return Ok(Some(unknown));
        }
        if let Err(error) = runner.effect(&request) {
            let unknown = receipt(
                &request,
                RemoteOutcomeV1::Unknown,
                None,
                Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {error}")),
            );
            io::write(&path, &unknown)?;
            return Ok(Some(unknown));
        }
        let updated = reconcile(&request, pending, runner)?;
        io::write(&path, &updated)?;
        Ok(Some(updated))
    }
}

pub(crate) fn run_agent_stdin() -> Result<(), String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((MAX_AGENT_MESSAGE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_AGENT_MESSAGE {
        return Err("remote agent request size limit".into());
    }
    let request: AgentRequestV1 =
        serde_json::from_slice(&bytes).map_err(|error| format!("remote agent request: {error}"))?;
    let receipt = Agent::apply_or_query(request)?;
    let response = AgentReplyV1 {
        api_version: "oasis7.remote-operation-agent-reply/v1".into(),
        receipt,
    };
    println!(
        "{}",
        serde_json::to_string(&response).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn management_binding(
    request: &AgentRequestV1,
) -> super::super::super::node_lifecycle::management::ManagementRequestBinding {
    super::super::super::node_lifecycle::management::ManagementRequestBinding {
        api_version: "oasis7.local-management-request/v1".into(),
        operation_id: request.operation_id.clone(),
        step_id: match request.step {
            RemoteStepV1::Start => "start".into(),
            RemoteStepV1::Stop => "stop".into(),
        },
        action_id: request.action_id.clone(),
        request_digest: request.operation_request_digest.clone(),
        request_json: request.operation_request_json.clone(),
    }
}

fn verify_management_binding(request: &AgentRequestV1) -> Result<bool, String> {
    let Some(bound) = super::super::super::node_lifecycle::management::read(
        &request.target.config,
        &request.action_id,
    )?
    else {
        return Ok(false);
    };
    if bound != management_binding(request) {
        return Err("immutable node-side management request differs from remote operation".into());
    }
    Ok(true)
}

fn bind_management_request(request: &AgentRequestV1) -> Result<(), String> {
    super::super::super::node_lifecycle::management::bind(
        &request.target.config,
        &management_binding(request),
    )
}

fn reconstruct_agent_receipt(
    request: &AgentRequestV1,
    runner: &mut impl SystemdRunner,
) -> Result<RemoteActionReceiptV1, String> {
    let Some(native) = actions::query(&request.target.data_dir, &request.action_id)? else {
        return Ok(receipt(
            request,
            RemoteOutcomeV1::Unknown,
            None,
            Some("ORIGINAL_OUTCOME_UNKNOWN: immutable management request exists without a native action receipt; query only".into()),
        ));
    };
    match request.step {
        RemoteStepV1::Start => {
            let expected_generation = request
                .expected_generation
                .checked_add(1)
                .ok_or("remote generation overflow")?;
            if native.kind != "start"
                || native.action_id != request.action_id
                || native.expected_generation != request.expected_generation
                || native.target.action_id != request.action_id
                || native.target.generation != expected_generation
                || native.target.config_digest != request.expected_config_digest
            {
                return Err("native start action differs from immutable remote request".into());
            }
            let current = actions::binding(&request.target.config)?.current;
            let active = runner.active(&request.target.unit)?;
            let exact_running = active
                && current.as_ref().is_some_and(|instance| {
                    instance.action_id == request.action_id
                        && instance.generation == expected_generation
                        && instance.config_digest == request.expected_config_digest
                        && instance.state == "running"
                })
                && native.target.state == "running";
            Ok(receipt(
                request,
                if exact_running {
                    RemoteOutcomeV1::Running
                } else {
                    RemoteOutcomeV1::Unknown
                },
                current.filter(|instance| {
                    instance.action_id == request.action_id
                        && instance.generation == expected_generation
                        && instance.config_digest == request.expected_config_digest
                }),
                (!exact_running).then(|| "ORIGINAL_OUTCOME_UNKNOWN: exact native start exists but current process/systemd readback is incomplete; query only".into()),
            ))
        }
        RemoteStepV1::Stop => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("remote stop target missing")?;
            if native.kind != "stop"
                || native.action_id != request.action_id
                || native.target.action_id != expected.action_id
                || native.target.incarnation != expected.incarnation
                || native.target.generation != expected.generation
                || native.target.config_digest != expected.config_digest
            {
                return Err("native stop action differs from immutable remote request".into());
            }
            let current = actions::binding(&request.target.config)?.current;
            let active = runner.active(&request.target.unit)?;
            let exact_stopped = !active
                && current.as_ref().is_some_and(|instance| {
                    instance.action_id == expected.action_id
                        && instance.incarnation == expected.incarnation
                        && instance.generation == expected.generation
                        && instance.config_digest == expected.config_digest
                        && matches!(instance.state.as_str(), "stopped" | "failed")
                })
                && matches!(native.target.state.as_str(), "stopped" | "failed");
            Ok(receipt(
                request,
                if exact_stopped {
                    RemoteOutcomeV1::Stopped
                } else {
                    RemoteOutcomeV1::Unknown
                },
                current.filter(|instance| {
                    instance.action_id == expected.action_id
                        && instance.incarnation == expected.incarnation
                        && instance.generation == expected.generation
                        && instance.config_digest == expected.config_digest
                }),
                (!exact_stopped).then(|| "ORIGINAL_OUTCOME_UNKNOWN: exact native stop exists but current process/systemd readback is incomplete; query only".into()),
            ))
        }
    }
}

fn validate_agent_request(request: &AgentRequestV1) -> Result<(), String> {
    if request.api_version != "oasis7.remote-operation-agent/v1"
        || request.operation_id.trim().is_empty()
        || request.operation_id.len() > 128
        || !request
            .operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
        || request.action_id
            != format!(
                "op:{}:{}",
                request.operation_id,
                match request.step {
                    RemoteStepV1::Start => "start",
                    RemoteStepV1::Stop => "stop",
                }
            )
        || !is_digest(&request.request_digest)
        || !is_digest(&request.operation_request_digest)
        || request.operation_request_json.len() > MAX_AGENT_MESSAGE / 2
        || !is_digest(&request.expected_config_digest)
        || request.world_id.trim().is_empty()
        || request.chain_id.trim().is_empty()
        || !request.target.config.is_absolute()
        || !request.target.data_dir.is_absolute()
        || request.target.config != request.target.data_dir.join("node.json")
        || !safe_unit(&request.target.unit)
        || !safe_node_id(&request.target.node_id)
        || !safe_user(&request.target.service_user)
        || !safe_remote_path(&request.target.config)
        || !safe_remote_path(&request.target.data_dir)
        || (request.step == RemoteStepV1::Stop && request.expected_instance.is_none())
        || (request.step == RemoteStepV1::Start && request.expected_instance.is_some())
        || request.expected_instance.as_ref().is_some_and(|instance| {
            instance.generation != request.expected_generation
                || instance.config_digest != request.expected_config_digest
        })
        || action_digest(ActionDigestV1 {
            operation_request_digest: &request.operation_request_digest,
            operation_id: &request.operation_id,
            action_id: &request.action_id,
            world_id: &request.world_id,
            chain_id: &request.chain_id,
            target: &request.target,
            step: request.step,
            expected_generation: request.expected_generation,
            expected_config_digest: &request.expected_config_digest,
            expected_instance: request.expected_instance.as_ref(),
        })? != request.request_digest
    {
        return Err("invalid remote operation agent request".into());
    }
    validate_original_request_binding(request)?;
    Ok(())
}

fn validate_original_request_binding(request: &AgentRequestV1) -> Result<(), String> {
    use super::super::model::{Kind, ManagedTargetV1, Request};
    let original: Request = serde_json::from_str(&request.operation_request_json)
        .map_err(|error| format!("invalid original operation request: {error}"))?;
    super::super::planner::validate_identity(&original)?;
    if serde_json::to_string(&original).map_err(|error| error.to_string())?
        != request.operation_request_json
        || io::digest(&original)? != request.operation_request_digest
        || original.operation_id != request.operation_id
        || original.world_id != request.world_id
        || original.chain_id != request.chain_id
        || original.config != request.target.config
        || original.expected_generation != request.expected_generation
        || original.expected_config_digest != request.expected_config_digest
    {
        return Err("remote action differs from exact serialized original operation".into());
    }
    let Some(ManagedTargetV1::LinuxSshSystemd(target)) = &original.managed_target else {
        return Err("original operation omitted its managed SSH target".into());
    };
    validate_target_shape(target)?;
    if target.node_id != request.target.node_id
        || target.username != request.target.service_user
        || target.unit != request.target.unit
        || target.remote_config != request.target.config
        || target.remote_data_dir != request.target.data_dir
    {
        return Err("remote action target differs from original managed target".into());
    }
    let step_matches_kind = matches!(
        (request.step, original.kind),
        (RemoteStepV1::Start, Kind::Start | Kind::Restart)
            | (
                RemoteStepV1::Stop,
                Kind::Stop | Kind::Restart | Kind::MaintenanceStop
            )
    );
    if !step_matches_kind {
        return Err("remote action step is not part of original operation kind".into());
    }
    match request.step {
        RemoteStepV1::Start if request.expected_instance.is_none() => {}
        RemoteStepV1::Stop if original.expected_instance == request.expected_instance => {}
        _ => return Err("remote action exact instance differs from original operation".into()),
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn action_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(dir.join(format!("{}.json", io::digest(&id)?)))
}
pub(crate) fn validate_receipt(
    receipt: &RemoteActionReceiptV1,
    request: &AgentRequestV1,
) -> Result<(), String> {
    if receipt.api_version != "oasis7.remote-action-receipt/v1"
        || receipt.operation_id != request.operation_id
        || receipt.action_id != request.action_id
        || receipt.request_digest != request.request_digest
        || receipt.operation_request_digest != request.operation_request_digest
        || receipt.node_id != request.target.node_id
        || receipt.world_id != request.world_id
        || receipt.chain_id != request.chain_id
        || receipt.step != request.step
        || receipt.expected_generation != request.expected_generation
        || receipt.expected_instance != request.expected_instance
    {
        return Err("remote action identity reused with another payload".into());
    }
    match (receipt.outcome, request.step, receipt.instance.as_ref()) {
        (RemoteOutcomeV1::Running, RemoteStepV1::Start, Some(instance)) => {
            let next_generation = request
                .expected_generation
                .checked_add(1)
                .ok_or("remote generation overflow")?;
            if instance.action_id != request.action_id
                || instance.generation != next_generation
                || instance.config_digest != request.expected_config_digest
                || instance.state != "running"
            {
                return Err("remote start receipt instance differs from exact action".into());
            }
        }
        (RemoteOutcomeV1::Stopped, RemoteStepV1::Stop, Some(instance)) => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("stop target missing")?;
            if instance.action_id != expected.action_id
                || instance.incarnation != expected.incarnation
                || instance.generation != expected.generation
                || instance.config_digest != expected.config_digest
                || !matches!(instance.state.as_str(), "stopped" | "failed")
            {
                return Err("remote stop receipt instance differs from exact target".into());
            }
        }
        (RemoteOutcomeV1::Running | RemoteOutcomeV1::Stopped, _, _) => {
            return Err("remote terminal action receipt omits exact instance".into());
        }
        (RemoteOutcomeV1::Pending, _, Some(_)) => {
            return Err("pending remote action receipt cannot claim an observed instance".into());
        }
        (RemoteOutcomeV1::Rejected, _, Some(_)) => {
            return Err("rejected remote action receipt cannot claim a resulting instance".into());
        }
        (RemoteOutcomeV1::Rejected, _, None)
            if receipt.blocker.as_deref().is_none_or(str::is_empty) =>
        {
            return Err("rejected remote action receipt omits its blocker".into());
        }
        (RemoteOutcomeV1::Unknown, RemoteStepV1::Start, Some(instance)) => {
            if instance.action_id != request.action_id
                || request.expected_generation.checked_add(1) != Some(instance.generation)
                || instance.config_digest != request.expected_config_digest
            {
                return Err("unknown remote start receipt includes a different instance".into());
            }
        }
        (RemoteOutcomeV1::Unknown, RemoteStepV1::Stop, Some(instance)) => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("stop target missing")?;
            if instance.action_id != expected.action_id
                || instance.incarnation != expected.incarnation
                || instance.generation != expected.generation
                || instance.config_digest != expected.config_digest
            {
                return Err("unknown remote stop receipt includes a different instance".into());
            }
        }
        (RemoteOutcomeV1::Unknown, _, None) => {}
        (RemoteOutcomeV1::Pending, _, None) => {}
        (RemoteOutcomeV1::Rejected, _, None) => {}
    }
    Ok(())
}
pub(super) fn receipt(
    request: &AgentRequestV1,
    outcome: RemoteOutcomeV1,
    instance: Option<LocalInstance>,
    blocker: Option<String>,
) -> RemoteActionReceiptV1 {
    RemoteActionReceiptV1 {
        api_version: "oasis7.remote-action-receipt/v1".into(),
        operation_id: request.operation_id.clone(),
        action_id: request.action_id.clone(),
        request_digest: request.request_digest.clone(),
        operation_request_digest: request.operation_request_digest.clone(),
        node_id: request.target.node_id.clone(),
        world_id: request.world_id.clone(),
        chain_id: request.chain_id.clone(),
        step: request.step,
        outcome,
        expected_generation: request.expected_generation,
        expected_instance: request.expected_instance.clone(),
        instance,
        blocker,
    }
}
fn verify_target_binding(
    request: &AgentRequestV1,
    runner: &mut impl SystemdRunner,
) -> Result<(), String> {
    let binding = actions::binding(&request.target.config)?;
    if binding.data_dir != request.target.data_dir
        || binding.config_digest != request.expected_config_digest
        || binding.world_id != request.world_id
        || binding.chain_id != request.chain_id
    {
        return Err("remote node config/data identity changed".into());
    }
    let node: serde_json::Value = io::read(&request.target.config)?;
    if node["identity"]["node_id"].as_str() != Some(request.target.node_id.as_str()) {
        return Err("remote node id differs from exact managed target".into());
    }
    runner.validate_unit(&request.target)?;
    Ok(())
}

fn validate_action_target(request: &AgentRequestV1) -> Result<(), String> {
    let binding = actions::binding(&request.target.config)?;
    if binding.data_dir != request.target.data_dir
        || binding.config_digest != request.expected_config_digest
    {
        return Err("remote action target config/data binding changed".into());
    }
    match request.step {
        RemoteStepV1::Start => {
            if binding
                .current
                .as_ref()
                .map_or(0, |instance| instance.generation)
                != request.expected_generation
                || binding.current.as_ref().is_some_and(|instance| {
                    !matches!(instance.state.as_str(), "stopped" | "failed")
                })
            {
                return Err("STALE_GENERATION: remote start target is no longer the expected terminal generation".into());
            }
        }
        RemoteStepV1::Stop => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("remote stop target missing")?;
            let actual = binding
                .current
                .as_ref()
                .ok_or("remote stop instance absent")?;
            if actual.action_id != expected.action_id
                || actual.incarnation != expected.incarnation
                || actual.generation != expected.generation
                || actual.config_digest != expected.config_digest
            {
                return Err(
                    "STALE_INSTANCE: remote stop request differs from exact current instance"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

fn verify_systemd_unit(target: &AgentTargetV1) -> Result<(), String> {
    let property = "LoadState";
    let expected = "loaded";
    let value = systemctl_output(&["show", "--property", property, "--value", &target.unit])?;
    if value.trim() != expected {
        return Err("systemd target unit is not loaded".into());
    }
    let fragment = systemctl_output(&[
        "show",
        "--property",
        "FragmentPath",
        "--value",
        &target.unit,
    ])?;
    if fragment.trim().is_empty() || !Path::new(fragment.trim()).is_absolute() {
        return Err("systemd unit fragment path unavailable".into());
    }
    let dropins =
        systemctl_output(&["show", "--property", "DropInPaths", "--value", &target.unit])?;
    if !dropins.trim().is_empty() {
        return Err("systemd drop-in override is unsupported for managed operation".into());
    }
    io::reject_symlinks(Path::new(fragment.trim()))?;
    let content = read_bounded_text(Path::new(fragment.trim()), 64 * 1024)?;
    validate_unit_contents(target, &content)
}

fn read_bounded_text(path: &Path, limit: usize) -> Result<String, String> {
    io::reject_symlinks(path)?;
    let file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > limit as u64 {
        return Err("systemd unit exceeds supported size".into());
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("systemd unit exceeds supported size".into());
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

pub(super) fn validate_unit_contents(target: &AgentTargetV1, content: &str) -> Result<(), String> {
    if content.len() > 64 * 1024 || !safe_remote_path(&target.config) {
        return Err("systemd unit or config path exceeds supported exact profile".into());
    }
    let mut section = "";
    let mut seen_sections = std::collections::BTreeSet::new();
    let mut unit_description = false;
    let mut service_type = false;
    let mut remain_after_exit = false;
    let mut service_user = false;
    let mut exec_start = false;
    let mut wanted_by = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let next = &line[1..line.len() - 1];
            if !matches!(next, "Unit" | "Service" | "Install")
                || !seen_sections.insert(next.to_owned())
            {
                return Err("systemd unit contains unsupported or repeated section".into());
            }
            section = next;
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or("systemd unit contains malformed directive")?;
        match (section, key, value) {
            ("Unit", "Description", value) if !unit_description && !value.trim().is_empty() => {
                unit_description = true;
            }
            ("Service", "Type", "oneshot") if !service_type => service_type = true,
            ("Service", "RemainAfterExit", "yes") if !remain_after_exit => {
                remain_after_exit = true;
            }
            ("Service", "User", value) if !service_user && value == target_user(target) => {
                service_user = true;
            }
            ("Service", "ExecStart", value)
                if !exec_start
                    && value
                        == format!(
                            "{NODE_EXECUTABLE} node start --config {}",
                            target.config.display()
                        ) =>
            {
                exec_start = true;
            }
            ("Install", "WantedBy", "multi-user.target") if !wanted_by => wanted_by = true,
            _ => {
                return Err(
                    "systemd unit contains a directive outside the exact managed profile".into(),
                );
            }
        }
    }
    if !seen_sections.contains("Unit")
        || !seen_sections.contains("Service")
        || !unit_description
        || !service_type
        || !remain_after_exit
        || !service_user
        || !exec_start
        || (seen_sections.contains("Install") && !wanted_by)
    {
        return Err("systemd unit does not have the exact supported node start command".into());
    }
    Ok(())
}

fn target_user(target: &AgentTargetV1) -> &str {
    // The service identity must match the SSH principal that owns the private
    // node directory; systemd must not launch the instance under another user.
    &target.service_user
}

fn systemctl_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new(SYSTEMCTL)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("systemctl query unavailable: {error}"))?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("systemctl query failed or exceeded bound".into());
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}
fn systemctl_effect(request: &AgentRequestV1) -> Result<(), String> {
    verify_systemd_unit(&request.target)?;
    let command = match request.step {
        RemoteStepV1::Start => "start",
        RemoteStepV1::Stop => "stop",
    };
    let output = Command::new(SYSTEMCTL)
        .arg(command)
        .arg(&request.target.unit)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("systemctl {command} returned {}", output.status));
    }
    Ok(())
}
fn systemd_active(unit: &str) -> Result<bool, String> {
    let output = Command::new(SYSTEMCTL)
        .arg("is-active")
        .arg("--quiet")
        .arg(unit)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    Ok(output.success())
}
fn reconcile(
    request: &AgentRequestV1,
    mut old: RemoteActionReceiptV1,
    runner: &mut impl SystemdRunner,
) -> Result<RemoteActionReceiptV1, String> {
    let already_terminal = match (old.outcome, request.step, old.instance.as_ref()) {
        (RemoteOutcomeV1::Running, RemoteStepV1::Start, Some(instance)) => {
            instance.action_id == request.action_id
                && request.expected_generation.checked_add(1) == Some(instance.generation)
                && instance.config_digest == request.expected_config_digest
                && instance.state == "running"
        }
        (RemoteOutcomeV1::Stopped, RemoteStepV1::Stop, Some(instance)) => {
            request.expected_instance.as_ref().is_some_and(|expected| {
                instance.action_id == expected.action_id
                    && instance.incarnation == expected.incarnation
                    && instance.generation == expected.generation
                    && instance.config_digest == expected.config_digest
                    && matches!(instance.state.as_str(), "stopped" | "failed")
            })
        }
        _ => false,
    };
    if already_terminal {
        return Ok(old);
    }
    let binding = actions::binding(&request.target.config)?;
    let mut active = runner.active(&request.target.unit)?;
    match request.step {
        RemoteStepV1::Start => {
            let expected_next_generation = request
                .expected_generation
                .checked_add(1)
                .ok_or("remote generation overflow")?;
            let current = binding.current;
            let native = actions::query(&request.target.data_dir, &request.action_id)?;
            let native_started = native.as_ref().is_some_and(|action| {
                action.kind == "start"
                    && action.action_id == request.action_id
                    && action.expected_generation == request.expected_generation
                    && action.target.action_id == request.action_id
                    && action.target.generation == expected_next_generation
                    && action.target.config_digest == request.expected_config_digest
                    && action.target.state == "running"
            });
            if active
                && native_started
                && current.as_ref().is_some_and(|instance| {
                    instance.action_id == request.action_id
                        && instance.generation == expected_next_generation
                        && instance.config_digest == request.expected_config_digest
                        && instance.state == "running"
                })
            {
                old.outcome = RemoteOutcomeV1::Running;
                old.instance = current;
                old.blocker = None;
            } else {
                old.outcome = RemoteOutcomeV1::Unknown;
                old.instance = current.filter(|instance| {
                    instance.action_id == request.action_id
                        && instance.generation == expected_next_generation
                        && instance.config_digest == request.expected_config_digest
                });
                old.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: exact requested generation is not running; query only".into());
            }
        }
        RemoteStepV1::Stop => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("remote stop target missing")?;
            let current = binding.current;
            let native = actions::query(&request.target.data_dir, &request.action_id)?;
            let native_terminal = native.as_ref().is_some_and(|action| {
                action.kind == "stop"
                    && action.action_id == request.action_id
                    && action.target.action_id == expected.action_id
                    && action.target.incarnation == expected.incarnation
                    && action.target.generation == expected.generation
                    && action.target.config_digest == expected.config_digest
                    && matches!(action.target.state.as_str(), "stopped" | "failed")
            });
            let current_terminal = current.as_ref().is_some_and(|instance| {
                instance.action_id == expected.action_id
                    && instance.incarnation == expected.incarnation
                    && instance.generation == expected.generation
                    && instance.config_digest == expected.config_digest
                    && matches!(instance.state.as_str(), "stopped" | "failed")
            });
            if active && native_terminal && current_terminal {
                // The instance receipt proves the exact process stop already
                // completed. The supported oneshot unit has no ExecStop side
                // effect, so this only reconciles systemd's RemainAfterExit bit.
                runner.effect(request)?;
                active = runner.active(&request.target.unit)?;
            }
            if !active && current_terminal {
                old.outcome = RemoteOutcomeV1::Stopped;
                old.instance = current.filter(|instance| {
                    instance.action_id == expected.action_id
                        && instance.incarnation == expected.incarnation
                        && instance.generation == expected.generation
                        && instance.config_digest == expected.config_digest
                });
                old.blocker = None;
            } else {
                old.outcome = RemoteOutcomeV1::Unknown;
                old.instance = current.filter(|instance| {
                    instance.action_id == expected.action_id
                        && instance.incarnation == expected.incarnation
                        && instance.generation == expected.generation
                        && instance.config_digest == expected.config_digest
                });
                old.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: exact requested instance has not reached terminal state; query only".into());
            }
        }
    }
    Ok(old)
}
