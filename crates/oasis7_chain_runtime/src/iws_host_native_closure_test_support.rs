//! One real native ELF exercises GEN, nonempty Node commits, strict Cold and Warm.
//! Only fixture observation DATA crosses the complete old-owner release point.
use super::native_cold_test_support::{
    ColdFixtureInputV1, ColdFixtureProcessV1, ColdFixtureWorkV1, reopen_host_bounded,
    selected_fact_hints_bounded,
};
use super::native_node_test_support::{
    NativeColdRecoverySettingsV1, NativeNodeCognitionV1, NativeNodeOwnersV1, NativeNodeSettingsV1,
    admit_initial_cold_recovery_work_v1, run_native_node_tick_v1,
};
use super::native_observe_test_support::{
    HostObservationV1, HostObservationWorkV1, observe_current_bounded,
};
use super::*;
use crate::execution_bridge::formal_iws_genesis_integration_support::{
    GenuineGenesisHostsV1, cognition_startup_work, with_genuine_genesis_hosts, worker_limits,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::controlled_authority::{
    formal::bounded_cbor_data::CborDataLimitsV1, replicated_protocol::EndpointRole,
};
use oasis7_formal_protocol::incremental_material::FactLocationV1;
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_runtime::{NormalCommitHeaderLimitsV1, NormalHostWorkV1, RuntimeSourceCompiler};
use oasis7_world_state::{DomainEvent, WorldEventBody};
use std::collections::BTreeMap;
use std::{path::Path, sync::Arc};

#[path = "../tests/support/genesis_endpoint_work.rs"]
mod endpoint_work;
#[path = "iws_host_native_live_test_support.rs"]
pub(crate) mod live_node;
#[path = "iws_host_native_suffix_refusal_test_support.rs"]
mod suffix_refusals;

fn diagnostic(error: impl std::fmt::Display) -> String {
    error.to_string()
}

// These finite ceilings are instantiated once at GEN or a new Cold process.
// Warm ticks move the returned whole work owner; retries never call this.
fn normal_work(
    runtime: NormalHostWorkV1,
    io: [HostColdIoWorkV1; 2],
    cold: super::normal_commit::NormalColdRecoveryWorkV1,
) -> NormalCommitWorkV1 {
    let [primary, replica] = io;
    NormalCommitWorkV1::new(
        primary.process_canonical,
        endpoint_work::source_work().frozen_material,
        endpoint_work::source_work(),
        endpoint_work::source_work(),
        super::normal::HostNormalIoWorkV1 {
            state_pages: primary.state,
            value_pages: primary.values,
            original_pages: primary.originals,
            original_values: primary.original_values,
            replica_value_pages: replica.values,
            replica_original_pages: replica.originals,
            replica_original_values: replica.original_values,
        },
        runtime,
        cold,
    )
}
fn settings(
    world_id: &str,
    writer: &ed25519_dalek::SigningKey,
    primary: &Arc<RemoteMonotonicEndpointProvider>,
    replica: &Arc<RemoteMonotonicEndpointProvider>,
    pinned: &TrustedFencingPolicyV1,
    transaction: oasis7_formal_protocol::incremental_execution::TransactionLimits,
    cold: NativeColdRecoverySettingsV1,
) -> NativeNodeSettingsV1 {
    NativeNodeSettingsV1 {
        world_id: world_id.into(),
        writer: writer.clone(),
        primary_provider: primary.clone(),
        replica_provider: replica.clone(),
        primary_policy: pinned.clone(),
        replica_policy: pinned.clone(),
        compiler: RuntimeSourceCompiler::default(),
        input_limits: CborDataLimitsV1 {
            max_wire_bytes: 1 << 20,
            max_graph_bytes: 256 << 20,
            max_depth: 128,
        },
        action_limits: CborDataLimitsV1 {
            max_wire_bytes: 1 << 20,
            max_graph_bytes: 256 << 20,
            max_depth: 128,
        },
        policy: NormalCommitHeaderLimitsV1 {
            transaction,
            checkpoint: true,
        },
        max_action_bytes: 1 << 20,
        max_grant_payload_bytes: 1 << 20,
        cold,
    }
}
fn balances(observation: &HostObservationV1) -> Result<[i64; 4], String> {
    Ok([
        observation
            .sender
            .electricity
            .ok_or("actual sender Electricity missing")?,
        observation
            .sender
            .data
            .ok_or("actual sender Data missing")?,
        observation
            .receiver
            .electricity
            .ok_or("actual receiver Electricity missing")?,
        observation
            .receiver
            .data
            .ok_or("actual receiver Data missing")?,
    ])
}
fn summary(observation: &HostObservationV1) -> Result<serde_json::Value, String> {
    let events: Vec<_> = observation.events.iter().map(|entry| serde_json::json!({
        "ordinal":entry.ordinal, "original":hex::encode(entry.original), "event":entry.decoded.event(),
    })).collect();
    Ok(
        serde_json::json!({"record":hex::encode(observation.record),"body":hex::encode(observation.body),
        "height":observation.height,"balances":balances(observation)?,"events":events}),
    )
}
fn verify_tick(
    observation: &HostObservationV1,
    before: [i64; 4],
    amounts: &[i64],
    height: u64,
    world_clock: u64,
) -> Result<serde_json::Value, String> {
    if observation.height != height {
        return Err("actual accepted body height mismatch".into());
    }
    let mut transfers = Vec::new();
    let mut electricity = 0i64;
    let mut data = 0i64;
    let mut charges = 0usize;
    let mut previous_ordinal = None;
    for entry in observation.events.iter() {
        if previous_ordinal.is_some_and(|previous| previous >= entry.ordinal) {
            return Err("actual CommitBody event order changed".into());
        }
        previous_ordinal = Some(entry.ordinal);
        let event = entry.decoded.event();
        if event.time != world_clock {
            return Err(format!(
                "actual normal event clock {} != {world_clock}",
                event.time
            ));
        }
        match &event.body {
            WorldEventBody::Domain(DomainEvent::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            }) if from_agent_id == "sender"
                && to_agent_id == "receiver"
                && *kind == ResourceKind::Electricity =>
            {
                transfers.push(*amount)
            }
            WorldEventBody::ModuleRuntimeCharged(charge) => {
                if charge.module_id != "m1.rule.transfer"
                    || charge.payer_agent_id != "receiver"
                    || charge.compute_fee_kind != ResourceKind::Data
                    || charge.electricity_fee_kind != ResourceKind::Electricity
                    || charge.compute_fee_amount <= 0
                    || charge.electricity_fee_amount <= 0
                {
                    return Err(format!("actual Rule fee mismatch: {charge:?}"));
                }
                electricity = electricity
                    .checked_add(charge.electricity_fee_amount)
                    .ok_or("fee overflow")?;
                data = data
                    .checked_add(charge.compute_fee_amount)
                    .ok_or("fee overflow")?;
                charges += 1;
            }
            _ => {}
        }
    }
    if transfers != amounts || charges != amounts.len() {
        return Err(format!(
            "actual FIFO/VM effects mismatch: {transfers:?}, charges={charges}"
        ));
    }
    let transfer = amounts
        .iter()
        .try_fold(0i64, |sum, amount| sum.checked_add(*amount))
        .ok_or("transfer overflow")?;
    let expected = [
        before[0] - transfer,
        before[1],
        before[2] + transfer - electricity,
        before[3] - data,
    ];
    if balances(observation)? != expected {
        return Err(format!(
            "actual balances {:?} != {expected:?}",
            balances(observation)?
        ));
    }
    Ok(
        serde_json::json!({"accepted":summary(observation)?,"vm_charges":charges,
        "electricity_fee":electricity,"data_fee":data,"fifo_transfers":transfers,"world_clock":world_clock}),
    )
}

pub(crate) fn run(root: &Path) -> Result<Vec<u8>, String> {
    run_with_recovery(root, None, false)
}
pub(crate) fn run_with_fault(root: &Path, before_publication: bool) -> Result<Vec<u8>, String> {
    use super::normal_commit::NormalCommitFaultV1;
    run_with_recovery(
        root,
        Some(if before_publication {
            NormalCommitFaultV1::BeforeResident
        } else {
            NormalCommitFaultV1::AfterPublication
        }),
        false,
    )
}
pub(crate) fn run_checkpoint_suffix(root: &Path) -> Result<Vec<u8>, String> {
    run_with_recovery(root, None, true)
}
pub(crate) fn run_source_work_return_fault(
    root: &Path,
    partial_source: bool,
) -> Result<Vec<u8>, String> {
    run_with_recovery(
        root,
        Some(if partial_source {
            super::normal_commit::NormalCommitFaultV1::BeforeReplicaSource
        } else {
            super::normal_commit::NormalCommitFaultV1::BeforeSourceWorkReturn
        }),
        false,
    )
}
fn run_with_recovery(
    root: &Path,
    fault: Option<super::normal_commit::NormalCommitFaultV1>,
    checkpoint_suffix: bool,
) -> Result<Vec<u8>, String> {
    with_genuine_genesis_hosts(root, |fixture| {
        let GenuineGenesisHostsV1 {
            primary,
            replica,
            memory,
            archive,
            runtime_work,
            mut genesis_io,
            transfer,
            producer,
            primary_provider,
            replica_provider,
            authority,
            certificate,
            trust,
            writer,
            pinned,
            release_policy,
            configuration,
            current_limits,
            projection_limits,
            paths,
            seed_receipt,
            genesis_record,
            genesis_body,
            transaction,
            stop,
        } = fixture;
        let checkpoint_page = if checkpoint_suffix {
            Some(suffix_refusals::business_page_bounded(
                &primary, &memory, &archive, &stop,
            )?)
        } else {
            None
        };
        let checkpoints = if checkpoint_suffix {
            Some([
                selected_fact_hints_bounded(&primary, &memory, &archive, &stop).map_err(diagnostic)?,
                selected_fact_hints_bounded(&replica, &memory, &archive, &stop).map_err(diagnostic)?,
            ].map(|checkpoint_hints| oasis7_distfs::controlled_authority::formal::endpoint::IwsColdCheckpointSuffixV1 {
                checkpoint_position: 1, checkpoint_hints,
            }))
        } else {
            None
        };
        let checkpoints = checkpoints.map_or([None, None], |values| values.map(Some));
        let mut observe_work = HostObservationWorkV1::new_bounded(
            &mut archive.bounded_context(&memory, &mut || false),
        )
        .map_err(diagnostic)?;
        let genesis = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let before = balances(&genesis)?;
        if genesis.record != genesis_record
            || genesis.body != genesis_body
            || genesis.height != 0
            || before[0] != 8
        {
            return Err("actual GEN observation mismatch".into());
        }
        let genesis_observation = summary(&genesis)?;
        let mut mailbox_hints = BTreeMap::new();
        for event in genesis.events.iter() {
            mailbox_hints.insert(
                event.original,
                FactLocationV1 {
                    height: genesis.height,
                    ordinal: event.ordinal as u64,
                    original: event.original,
                },
            );
        }
        drop(genesis);
        let [mut runtime, replica_runtime] = runtime_work;
        let mut cognition_work = cognition_startup_work();
        let cognition = primary
            .prepare_cognition_startup_bounded(
                &mut genesis_io[0],
                &mut cognition_work,
                &mut runtime,
                transaction,
                &memory,
                &archive,
                &stop,
            )
            .map_err(diagnostic)?;
        drop(cognition_work);
        let cold_work = admit_initial_cold_recovery_work_v1(&current_limits, &memory, &archive)?;
        let mut work = normal_work(runtime, genesis_io, cold_work);
        work.fault = fault;
        let first_settings = settings(
            "iws-genuine-host-transfer",
            &writer,
            &primary_provider,
            &replica_provider,
            &pinned,
            transaction,
            NativeColdRecoverySettingsV1 {
                endpoint_paths: [
                    paths.primary_endpoint.clone(),
                    paths.replica_endpoint.clone(),
                ],
                pages_paths: [paths.primary_pages.clone(), paths.replica_pages.clone()],
                authorization_hint_path: paths.authorization_hint.clone(),
                authority: authority.clone(),
                process: release_policy.clone(),
                configuration: configuration.clone(),
                worker: worker_limits(),
                current_limits,
                budgets: endpoint_work::budgets(),
                projection_limits,
                mailbox_hints: mailbox_hints.clone(),
                checkpoints,
            },
        );
        let (owners, first_settings, first_node) = run_native_node_tick_v1(
            NativeNodeOwnersV1 {
                primary,
                replica,
                memory,
                archive,
                work,
            },
            first_settings,
            NativeNodeCognitionV1::Startup(cognition),
            &[3, 2],
        )?;
        let NativeNodeOwnersV1 {
            primary,
            replica,
            memory,
            archive,
            work,
        } = owners;
        let first = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let other = observe_current_bounded(
            &replica,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let first_receipt = verify_tick(&first, before, &[3, 2], 1, 4)?;
        let first_summary = summary(&first)?;
        if first_summary != summary(&other)? {
            return Err("actual first dual acceptance differs".into());
        }
        let after_first = balances(&first)?;
        for event in first.events.iter() {
            mailbox_hints.insert(
                event.original,
                FactLocationV1 {
                    height: first.height,
                    ordinal: event.ordinal as u64,
                    original: event.original,
                },
            );
        }
        if after_first[0] != 3 {
            return Err("first actual sender balance must be 3".into());
        }
        let primary_hints =
            selected_fact_hints_bounded(&primary, &memory, &archive, &stop).map_err(diagnostic)?;
        let replica_hints =
            selected_fact_hints_bounded(&replica, &memory, &archive, &stop).map_err(diagnostic)?;
        let target_page = if let Some(checkpoint_page) = checkpoint_page {
            let target =
                suffix_refusals::business_page_bounded(&primary, &memory, &archive, &stop)?;
            if target == checkpoint_page {
                return Err(
                    "actual nonempty transfer did not change the selected Business root".into(),
                );
            }
            Some(target)
        } else {
            None
        };
        let first_remaining = (archive.remaining_steps(), archive.remaining_read_bytes());
        let old_pool = memory.clone();
        // These are every original Common/pool-bound owner of the first phase.
        // DATA receipts, paths, approval trust and remote providers are retained.
        drop(first);
        drop(other);
        drop(observe_work);
        drop(first_settings);
        drop(primary);
        drop(replica);
        drop(work);
        drop(replica_runtime);
        drop(transfer);
        drop(producer);
        drop(archive);
        drop(memory);
        if old_pool.used() != 0 {
            return Err(format!(
                "old GEN/Node/Host owners leaked {} charged bytes",
                old_pool.used()
            ));
        }
        drop(old_pool);
        eprintln!(
            "REAL_FIRST_NONEMPTY_NODE_ACCEPTED_AND_RELEASED sender=3 height=1 old_pool_used=0"
        );

        // An independently supplied new cold request; no old runtime work,
        // accepted proof, page owner, World or cache can cross this boundary.
        let memory = MemoryBudget::new(2usize << 30, 32 << 20).map_err(diagnostic)?;
        let archive =
            FormalArchiveReadWorkV1::new(1_000_000_000, 16u64 << 30).map_err(diagnostic)?;
        let restored_authority =
            oasis7_formal_protocol::controlled_authority::formal::verify_initial_activation(
                &certificate,
                &trust,
            )
            .map_err(diagnostic)?;
        drop(authority);
        let suffix_refusal_receipt = if let Some(target_page) = target_page {
            let budgets = endpoint_work::budgets();
            let worker = worker_limits();
            Some(suffix_refusals::run_refusals(
                super::cold_recovery::HostColdReopenInputV1 {
                    endpoint: super::cold_recovery::ColdEndpointReopenRefsV1 {
                        endpoint_path: &paths.primary_endpoint,
                        pages_path: &paths.primary_pages,
                        authority: &restored_authority,
                        role: EndpointRole::Primary,
                        provider: &primary_provider,
                        policy: &pinned,
                        budgets: &budgets,
                        current_limits,
                        checkpoint: checkpoints[0],
                    },
                    authorization_hint_path: &paths.authorization_hint,
                    projection_limits,
                    process: super::cold::ProcessInput::Governed(&release_policy),
                    configuration: &configuration,
                    worker: &worker,
                    transaction,
                    facts: primary_hints,
                },
                &mailbox_hints,
                target_page,
            )?)
        } else {
            None
        };
        let cold_work = admit_initial_cold_recovery_work_v1(&current_limits, &memory, &archive)?;
        let mut primary_work = ColdFixtureWorkV1::new_bounded(
            &current_limits,
            &mut archive.bounded_context(&memory, &mut || false),
        )
        .map_err(diagnostic)?;
        let mut replica_work = ColdFixtureWorkV1::new_bounded(
            &current_limits,
            &mut archive.bounded_context(&memory, &mut || false),
        )
        .map_err(diagnostic)?;
        let cold_compiler = RuntimeSourceCompiler::default();
        let primary = reopen_host_bounded(
            ColdFixtureInputV1 {
                endpoint_path: &paths.primary_endpoint,
                pages_path: &paths.primary_pages,
                authorization_hint_path: &paths.authorization_hint,
                authority: &restored_authority,
                role: EndpointRole::Primary,
                provider: &primary_provider,
                pinned: &pinned,
                process: ColdFixtureProcessV1::Governed(&release_policy),
                configuration: configuration.clone(),
                worker: worker_limits(),
                transaction,
                current_limits: current_limits.clone(),
                projection_limits,
                facts: primary_hints,
                checkpoint: checkpoints[0],
            },
            &mut primary_work,
            |id| {
                mailbox_hints
                    .get(id)
                    .copied()
                    .ok_or_else(|| "actual mailbox original location not supplied".into())
            },
            &cold_compiler,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let replica = reopen_host_bounded(
            ColdFixtureInputV1 {
                endpoint_path: &paths.replica_endpoint,
                pages_path: &paths.replica_pages,
                authorization_hint_path: &paths.authorization_hint,
                authority: &restored_authority,
                role: EndpointRole::Replica,
                provider: &replica_provider,
                pinned: &pinned,
                process: ColdFixtureProcessV1::Governed(&release_policy),
                configuration: configuration.clone(),
                worker: worker_limits(),
                transaction,
                current_limits,
                projection_limits,
                facts: replica_hints,
                checkpoint: checkpoints[1],
            },
            &mut replica_work,
            |id| {
                mailbox_hints
                    .get(id)
                    .copied()
                    .ok_or_else(|| "actual mailbox original location not supplied".into())
            },
            &cold_compiler,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let mut observe_work = HostObservationWorkV1::new_bounded(
            &mut archive.bounded_context(&memory, &mut || false),
        )
        .map_err(diagnostic)?;
        let cold = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let cold_other = observe_current_bounded(
            &replica,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let cold_summary = summary(&cold)?;
        if cold_summary != first_summary || summary(&cold_other)? != first_summary {
            return Err("cold did not recover exact previously accepted originals".into());
        }
        drop(cold);
        drop(cold_other);
        drop(cold_compiler);
        eprintln!("REAL_AUTHENTICATED_COLD_REOPENED_BOTH height=1 sender=3");
        let mut cognition_work = cognition_startup_work();
        let cognition = primary
            .prepare_cognition_startup_bounded(
                &mut primary_work.physical,
                &mut cognition_work,
                &mut primary_work.runtime,
                transaction,
                &memory,
                &archive,
                &stop,
            )
            .map_err(diagnostic)?;
        drop(cognition_work);
        let work = normal_work(
            primary_work.runtime,
            [primary_work.physical, replica_work.physical],
            cold_work,
        );
        let second_settings = settings(
            "iws-genuine-host-transfer",
            &writer,
            &primary_provider,
            &replica_provider,
            &pinned,
            transaction,
            NativeColdRecoverySettingsV1 {
                endpoint_paths: [
                    paths.primary_endpoint.clone(),
                    paths.replica_endpoint.clone(),
                ],
                pages_paths: [paths.primary_pages.clone(), paths.replica_pages.clone()],
                authorization_hint_path: paths.authorization_hint.clone(),
                authority: restored_authority.clone(),
                process: release_policy.clone(),
                configuration: configuration.clone(),
                worker: worker_limits(),
                current_limits,
                budgets: endpoint_work::budgets(),
                projection_limits,
                mailbox_hints: mailbox_hints.clone(),
                checkpoints,
            },
        );
        let (owners, mut second_settings, second_node) = run_native_node_tick_v1(
            NativeNodeOwnersV1 {
                primary,
                replica,
                memory,
                archive,
                work,
            },
            second_settings,
            NativeNodeCognitionV1::Startup(cognition),
            &[1],
        )?;
        let NativeNodeOwnersV1 {
            primary,
            replica,
            memory,
            archive,
            work,
        } = owners;
        let second = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let second_other = observe_current_bounded(
            &replica,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let second_receipt = verify_tick(&second, after_first, &[1], 2, 5)?;
        if summary(&second)? != summary(&second_other)? || balances(&second)?[0] != 2 {
            return Err("second actual dual acceptance/balance differs".into());
        }
        let after_second = balances(&second)?;
        for event in second.events.iter() {
            second_settings.cold.mailbox_hints.insert(
                event.original,
                FactLocationV1 {
                    height: second.height,
                    ordinal: event.ordinal as u64,
                    original: event.original,
                },
            );
        }
        drop(second);
        drop(second_other);
        eprintln!("REAL_SECOND_NONEMPTY_NODE_ACCEPTED height=2 sender=2");
        let before_warm_common = [archive.remaining_steps(), archive.remaining_read_bytes()];
        let before_warm_cold_work = work.cold.remaining_for_fixture_v1();
        // Continue from the returned accepted Hosts and their remaining work.
        // The resident holds the actual terminal Cognition delta from the
        // preceding acceptance; no GEN/Cold scan or work factory runs here.
        let (owners, third_settings, third_node) = run_native_node_tick_v1(
            NativeNodeOwnersV1 {
                primary,
                replica,
                memory,
                archive,
                work,
            },
            second_settings,
            NativeNodeCognitionV1::Resident,
            &[1],
        )?;
        let after_warm_common = [
            owners.archive.remaining_steps(),
            owners.archive.remaining_read_bytes(),
        ];
        let after_warm_cold_work = owners.work.cold.remaining_for_fixture_v1();
        if before_warm_cold_work != after_warm_cold_work {
            return Err(
                "Warm tick replaced or consumed its original inactive Cold accounts".into(),
            );
        }
        if after_warm_common[0] >= before_warm_common[0]
            || after_warm_common[1] >= before_warm_common[1]
        {
            return Err("warm tick did not debit the original remaining Common".into());
        }
        let NativeNodeOwnersV1 {
            primary,
            replica,
            memory,
            archive,
            work,
        } = owners;
        let third = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let third_other = observe_current_bounded(
            &replica,
            "sender",
            "receiver",
            transaction,
            &mut observe_work,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        let third_receipt = verify_tick(&third, after_second, &[1], 3, 6)?;
        if summary(&third)? != summary(&third_other)? || balances(&third)?[0] != 1 {
            return Err("warm actual dual acceptance/balance differs".into());
        }
        let result = serde_json::json!({"schema":"oasis7.iws.nonempty_normal_cold.fixture.v1",
            "seed":seed_receipt,"genesis":genesis_observation,"first_node":first_node,"first":first_receipt,
            "old_pool_used_after_drop":0,"first_remaining_common":first_remaining,"cold":cold_summary,
            "cold_checkpoint_position":if checkpoint_suffix { Some(1u64) } else { None },"cold_target_position":2,
            "cold_suffix_refusals":suffix_refusal_receipt,
            "second_node":second_node,"second":second_receipt,"second_sender_electricity":2,
            "third_node":third_node,"third":third_receipt,"final_sender_electricity":1,
            "cold_remaining_common":before_warm_common,"warm_remaining_common":after_warm_common});
        drop(third);
        drop(third_other);
        drop(observe_work);
        drop(third_settings);
        drop(primary);
        drop(replica);
        drop(work);
        drop(archive);
        // Remaining partially moved cold work has no authority and is released
        // at scope end; the first complete-release check is the cold boundary.
        eprintln!("REAL_WARM_NONEMPTY_NODE_ACCEPTED height=3 sender=1");
        serde_json::to_vec_pretty(&result).map_err(diagnostic)
    })
}
