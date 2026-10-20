use super::*;
use std::path::PathBuf;

struct FakeAgentRunner {
    active: bool,
    effect_count: usize,
}

impl SystemdRunner for FakeAgentRunner {
    fn validate_unit(&mut self, _target: &AgentTargetV1) -> Result<(), String> {
        Ok(())
    }

    fn active(&mut self, _unit: &str) -> Result<bool, String> {
        Ok(self.active)
    }

    fn effect(&mut self, request: &AgentRequestV1) -> Result<(), String> {
        self.effect_count += 1;
        match request.step {
            RemoteStepV1::Start => {
                actions::start(
                    &request.target.config,
                    &request.action_id,
                    request.expected_generation,
                    &request.expected_config_digest,
                )?;
                self.active = true;
            }
            RemoteStepV1::Stop => {
                let expected = request
                    .expected_instance
                    .as_ref()
                    .ok_or("stop target missing")?;
                actions::stop(
                    &request.target.data_dir,
                    &request.action_id,
                    expected,
                    2_000,
                )?;
                self.active = false;
            }
        }
        Ok(())
    }
}

struct LocalAgentTransport {
    runner: FakeAgentRunner,
    lose_apply_reply: bool,
}

impl AgentTransport for LocalAgentTransport {
    fn exchange(
        &mut self,
        _target: &LinuxSshSystemdTargetV1,
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        let request: AgentRequestV1 =
            serde_json::from_slice(message).map_err(|error| error.to_string())?;
        let response = AgentReplyV1 {
            api_version: "oasis7.remote-operation-agent-reply/v1".into(),
            receipt: Agent::with_runner(request.clone(), &mut self.runner)?,
        };
        let bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        if request.mode == AgentModeV1::Apply && self.lose_apply_reply {
            self.lose_apply_reply = false;
            return Err("simulated dropped agent response".into());
        }
        Ok(bytes)
    }
}

struct FakeSshSystemd {
    current_generation: u64,
    receipt: Option<RemoteActionReceiptV1>,
    apply_count: usize,
    lose_first_apply_reply: bool,
}

impl AgentTransport for FakeSshSystemd {
    fn exchange(
        &mut self,
        _target: &LinuxSshSystemdTargetV1,
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        let request: AgentRequestV1 =
            serde_json::from_slice(message).map_err(|error| error.to_string())?;
        let receipt = match request.mode {
            AgentModeV1::Query => self.receipt.clone(),
            AgentModeV1::Apply => {
                self.apply_count += 1;
                if request.expected_generation != self.current_generation {
                    Some(receipt(
                        &request,
                        RemoteOutcomeV1::Rejected,
                        None,
                        Some("STALE_GENERATION".into()),
                    ))
                } else {
                    let instance = match request.step {
                        RemoteStepV1::Start => {
                            self.current_generation += 1;
                            Some(LocalInstance {
                                api_version: "oasis7.local-node-instance/v1".into(),
                                action_id: request.action_id.clone(),
                                generation: self.current_generation,
                                incarnation: "0123456789abcdef0123456789abcdef".into(),
                                config_digest: request.expected_config_digest.clone(),
                                supervisor_pid: 44,
                                runtime_pid: Some(45),
                                started_unix_ms: 7,
                                state: "running".into(),
                                exit_code: None,
                            })
                        }
                        RemoteStepV1::Stop => {
                            request.expected_instance.clone().map(|mut instance| {
                                instance.state = "stopped".into();
                                instance.runtime_pid = None;
                                instance
                            })
                        }
                    };
                    let outcome = match request.step {
                        RemoteStepV1::Start => RemoteOutcomeV1::Running,
                        RemoteStepV1::Stop => RemoteOutcomeV1::Stopped,
                    };
                    self.receipt = Some(receipt(&request, outcome, instance, None));
                    if self.lose_first_apply_reply {
                        self.lose_first_apply_reply = false;
                        return Err("simulated lost SSH reply".into());
                    }
                    self.receipt.clone()
                }
            }
        };
        serde_json::to_vec(&AgentReplyV1 {
            api_version: "oasis7.remote-operation-agent-reply/v1".into(),
            receipt,
        })
        .map_err(|error| error.to_string())
    }
}

fn target() -> LinuxSshSystemdTargetV1 {
    LinuxSshSystemdTargetV1 {
        host: "node-a.example.net".into(),
        username: "oasis7".into(),
        port: 22,
        identity_file: PathBuf::from("/unused/test-key"),
        known_hosts_file: PathBuf::from("/unused/test-known-hosts"),
        unit: "oasis7-node-a.service".into(),
        node_id: "node-a".into(),
        remote_config: PathBuf::from("/var/lib/oasis7/node-a/node.json"),
        remote_data_dir: PathBuf::from("/var/lib/oasis7/node-a"),
        timeout_ms: 10_000,
    }
}

fn request(step: RemoteStepV1) -> AgentRequestV1 {
    use super::super::model::{Kind, ManagedTargetV1, Request};
    let target = target();
    let expected_instance = (step == RemoteStepV1::Stop).then(|| LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: "native-start-3".into(),
        generation: 3,
        incarnation: "0123456789abcdef0123456789abcdef".into(),
        config_digest: "b".repeat(64),
        supervisor_pid: 44,
        runtime_pid: Some(45),
        started_unix_ms: 7,
        state: "running".into(),
        exit_code: None,
    });
    let original = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "op-loss".into(),
        world_id: "world-a".into(),
        chain_id: "chain-a".into(),
        kind: if step == RemoteStepV1::Start {
            Kind::Start
        } else {
            Kind::Stop
        },
        config: target.remote_config.clone(),
        expected_generation: 3,
        expected_config_digest: "b".repeat(64),
        expected_instance: expected_instance.clone(),
        managed_target: Some(ManagedTargetV1::LinuxSshSystemd(target.clone())),
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let operation_request_json = serde_json::to_string(&original).unwrap();
    let operation_request_digest = io::digest(&original).unwrap();
    let mut request = AgentRequestV1 {
        api_version: "oasis7.remote-operation-agent/v1".into(),
        mode: AgentModeV1::Query,
        operation_id: "op-loss".into(),
        action_id: format!(
            "op:op-loss:{}",
            if step == RemoteStepV1::Start {
                "start"
            } else {
                "stop"
            }
        ),
        request_digest: String::new(),
        operation_request_digest,
        operation_request_json,
        world_id: "world-a".into(),
        chain_id: "chain-a".into(),
        target: AgentTargetV1 {
            node_id: target.node_id.clone(),
            service_user: target.username.clone(),
            unit: target.unit.clone(),
            config: target.remote_config.clone(),
            data_dir: target.remote_data_dir.clone(),
        },
        step,
        expected_generation: 3,
        expected_config_digest: "b".repeat(64),
        expected_instance,
    };
    request.request_digest = action_digest(ActionDigestV1 {
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
    })
    .unwrap();
    request
}

fn local_stop_request(
    config: &std::path::Path,
    instance: &LocalInstance,
) -> (LinuxSshSystemdTargetV1, AgentRequestV1) {
    let binding = actions::binding(config).unwrap();
    let node: serde_json::Value = io::read(config).unwrap();
    use super::super::model::{Kind, ManagedTargetV1, Request};
    let target = LinuxSshSystemdTargetV1 {
        host: "node-a.example.net".into(),
        username: "oasis7".into(),
        port: 22,
        identity_file: PathBuf::from("/unused/test-key"),
        known_hosts_file: PathBuf::from("/unused/test-known-hosts"),
        unit: "oasis7-node-test.service".into(),
        node_id: node["identity"]["node_id"].as_str().unwrap().into(),
        remote_config: config.to_path_buf(),
        remote_data_dir: binding.data_dir.clone(),
        timeout_ms: 10_000,
    };
    let original = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "op-agent-lost-reply".into(),
        world_id: binding.world_id.clone(),
        chain_id: binding.chain_id.clone(),
        kind: Kind::Stop,
        config: config.to_path_buf(),
        expected_generation: instance.generation,
        expected_config_digest: binding.config_digest.clone(),
        expected_instance: Some(instance.clone()),
        managed_target: Some(ManagedTargetV1::LinuxSshSystemd(target.clone())),
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let operation_request_json = serde_json::to_string(&original).unwrap();
    let operation_request_digest = io::digest(&original).unwrap();
    let mut request = AgentRequestV1 {
        api_version: "oasis7.remote-operation-agent/v1".into(),
        mode: AgentModeV1::Query,
        operation_id: "op-agent-lost-reply".into(),
        action_id: "op:op-agent-lost-reply:stop".into(),
        request_digest: String::new(),
        operation_request_digest,
        operation_request_json,
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        target: AgentTargetV1 {
            node_id: node["identity"]["node_id"].as_str().unwrap().into(),
            service_user: target.username.clone(),
            unit: target.unit.clone(),
            config: config.to_path_buf(),
            data_dir: binding.data_dir.clone(),
        },
        step: RemoteStepV1::Stop,
        expected_generation: instance.generation,
        expected_config_digest: binding.config_digest.clone(),
        expected_instance: Some(instance.clone()),
    };
    request.request_digest = action_digest(ActionDigestV1 {
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
    })
    .unwrap();
    (target, request)
}

fn local_start_request(
    config: &std::path::Path,
    expected_generation: u64,
) -> (LinuxSshSystemdTargetV1, AgentRequestV1) {
    use super::super::model::{Kind, ManagedTargetV1, Request};
    let binding = actions::binding(config).unwrap();
    let node: serde_json::Value = io::read(config).unwrap();
    let target = LinuxSshSystemdTargetV1 {
        host: "node-a.example.net".into(),
        username: "oasis7".into(),
        port: 22,
        identity_file: PathBuf::from("/unused/test-key"),
        known_hosts_file: PathBuf::from("/unused/test-known-hosts"),
        unit: "oasis7-node-test.service".into(),
        node_id: node["identity"]["node_id"].as_str().unwrap().into(),
        remote_config: config.to_path_buf(),
        remote_data_dir: binding.data_dir.clone(),
        timeout_ms: 10_000,
    };
    let original = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "op-agent-stale-start".into(),
        world_id: binding.world_id.clone(),
        chain_id: binding.chain_id.clone(),
        kind: Kind::Start,
        config: config.to_path_buf(),
        expected_generation,
        expected_config_digest: binding.config_digest.clone(),
        expected_instance: None,
        managed_target: Some(ManagedTargetV1::LinuxSshSystemd(target.clone())),
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let operation_request_json = serde_json::to_string(&original).unwrap();
    let operation_request_digest = io::digest(&original).unwrap();
    let mut request = AgentRequestV1 {
        api_version: "oasis7.remote-operation-agent/v1".into(),
        mode: AgentModeV1::Query,
        operation_id: original.operation_id.clone(),
        action_id: format!("op:{}:start", original.operation_id),
        request_digest: String::new(),
        operation_request_digest,
        operation_request_json,
        world_id: original.world_id.clone(),
        chain_id: original.chain_id.clone(),
        target: AgentTargetV1 {
            node_id: target.node_id.clone(),
            service_user: target.username.clone(),
            unit: target.unit.clone(),
            config: target.remote_config.clone(),
            data_dir: target.remote_data_dir.clone(),
        },
        step: RemoteStepV1::Start,
        expected_generation,
        expected_config_digest: binding.config_digest,
        expected_instance: None,
    };
    request.request_digest = action_digest(ActionDigestV1 {
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
    })
    .unwrap();
    (target, request)
}

#[test]
fn lost_ssh_reply_is_recovered_by_same_action_query_without_redelivery() {
    let mut request = request(RemoteStepV1::Start);
    let mut fake = FakeSshSystemd {
        current_generation: 3,
        receipt: None,
        apply_count: 0,
        lose_first_apply_reply: true,
    };
    assert!(
        exchange_with(&target(), &request, &mut fake)
            .unwrap()
            .is_none()
    );
    request.mode = AgentModeV1::Apply;
    assert!(exchange_with(&target(), &request, &mut fake).is_err());
    request.mode = AgentModeV1::Query;
    let recovered = exchange_with(&target(), &request, &mut fake)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.outcome, RemoteOutcomeV1::Running);
    assert_eq!(recovered.action_id, request.action_id);
    assert_eq!(recovered.instance.unwrap().generation, 4);
    assert_eq!(fake.apply_count, 1);
}

#[test]
fn stale_generation_receipt_never_changes_remote_instance() {
    let mut request = request(RemoteStepV1::Start);
    request.mode = AgentModeV1::Apply;
    let mut fake = FakeSshSystemd {
        current_generation: 4,
        receipt: None,
        apply_count: 0,
        lose_first_apply_reply: false,
    };
    let rejected = exchange_with(&target(), &request, &mut fake)
        .unwrap()
        .unwrap();
    assert_eq!(rejected.outcome, RemoteOutcomeV1::Rejected);
    assert_eq!(fake.current_generation, 4);
    assert_eq!(fake.apply_count, 1);
}

#[test]
fn durable_agent_recovers_lost_reply_by_query_without_repeating_native_stop() {
    let (_fixture, config, handle, current) =
        super::super::super::node_lifecycle::tests::operation_fixture();
    let (target, mut request) = local_stop_request(&config, &current);
    let mut transport = LocalAgentTransport {
        runner: FakeAgentRunner {
            active: true,
            effect_count: 0,
        },
        lose_apply_reply: true,
    };

    assert!(
        exchange_with(&target, &request, &mut transport)
            .unwrap()
            .is_none()
    );
    request.mode = AgentModeV1::Apply;
    assert!(exchange_with(&target, &request, &mut transport).is_err());
    request.mode = AgentModeV1::Query;
    let recovered = exchange_with(&target, &request, &mut transport)
        .unwrap()
        .unwrap();

    assert_eq!(recovered.outcome, RemoteOutcomeV1::Stopped);
    assert_eq!(recovered.action_id, request.action_id);
    assert_eq!(transport.runner.effect_count, 1);
    handle.join().unwrap().unwrap();
}

#[test]
fn durable_agent_rejects_stale_generation_before_effect_or_intent() {
    let (_fixture, config, handle, current) =
        super::super::super::node_lifecycle::tests::operation_fixture();
    let (_target, mut request) = local_start_request(&config, current.generation.saturating_sub(1));
    request.mode = AgentModeV1::Apply;
    let mut runner = FakeAgentRunner {
        active: true,
        effect_count: 0,
    };

    assert!(Agent::with_runner(request.clone(), &mut runner).is_err());
    assert_eq!(runner.effect_count, 0);
    assert!(
        actions::query(&request.target.data_dir, &request.action_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        actions::inspect(&request.target.data_dir).unwrap()["instance"]["incarnation"],
        current.incarnation
    );
    actions::stop(
        &request.target.data_dir,
        "cleanup-stale-agent-start",
        &current,
        2000,
    )
    .unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn late_stop_action_cannot_stop_a_replacement_incarnation() {
    let (_fixture, config, handle, original) =
        super::super::super::node_lifecycle::tests::operation_fixture();
    let binding = actions::binding(&config).unwrap();
    actions::stop(&binding.data_dir, "setup-replacement", &original, 2000).unwrap();
    handle.join().unwrap().unwrap();
    let (replacement, current) =
        super::super::super::node_lifecycle::tests::operation_replacement_receiver(&config);
    let (_target, mut request) = local_stop_request(&config, &original);
    request.mode = AgentModeV1::Apply;
    let mut runner = FakeAgentRunner {
        active: true,
        effect_count: 0,
    };

    assert!(Agent::with_runner(request.clone(), &mut runner).is_err());
    assert_eq!(runner.effect_count, 0);
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["incarnation"],
        current.incarnation
    );
    actions::stop(
        &binding.data_dir,
        "cleanup-replacement-agent-stop",
        &current,
        2000,
    )
    .unwrap();
    replacement.join().unwrap().unwrap();
}

#[test]
fn remote_target_rejects_command_injection_fields_before_transport() {
    let mut target = target();
    target.host = "node-a; touch /tmp/pwned".into();
    assert!(!safe_host(&target.host));
    target.host = "node-a.example.net".into();
    target.unit = "node.service;systemctl stop other.service".into();
    assert!(!safe_unit(&target.unit));
}

#[test]
fn managed_unit_profile_rejects_extra_commands_dependencies_and_wrong_service_user() {
    let target = AgentTargetV1 {
        node_id: "node-a".into(),
        service_user: "oasis7".into(),
        unit: "oasis7-node-a.service".into(),
        config: PathBuf::from("/var/lib/oasis7/node-a/node.json"),
        data_dir: PathBuf::from("/var/lib/oasis7/node-a"),
    };
    let exact = format!(
        "[Unit]\nDescription=Oasis7 observer\n[Service]\nType=oneshot\nRemainAfterExit=yes\nUser=oasis7\nExecStart={NODE_EXECUTABLE} node start --config {}\n[Install]\nWantedBy=multi-user.target\n",
        target.config.display()
    );
    assert!(validate_unit_contents(&target, &exact).is_ok());
    for unsafe_line in [
        "ExecStartPre=/bin/touch /tmp/side-effect\n",
        "After=other.service\n",
        "EnvironmentFile=/etc/oasis7-secrets\n",
        "User=root\n",
    ] {
        let unsafe_contents = exact.replace("ExecStart=", &format!("{unsafe_line}ExecStart="));
        assert!(validate_unit_contents(&target, &unsafe_contents).is_err());
    }
    assert!(
        validate_unit_contents(
            &target,
            &exact.replace("[Service]\n", "[Service]\nExecStart=/bin/true\n")
        )
        .is_err()
    );
}
