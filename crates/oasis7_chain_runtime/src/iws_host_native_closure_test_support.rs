//! One real native ELF exercises GEN, nonempty Node commits and strict Cold.
//! Only fixture observation DATA crosses the complete old-owner release point.
use super::native_cold_test_support::{
    ColdFixtureInputV1, ColdFixtureProcessV1, ColdFixtureWorkV1, reopen_host_bounded,
    selected_fact_hints_bounded,
};
use super::native_node_test_support::{
    NativeNodeOwnersV1, NativeNodeSettingsV1, run_native_node_tick_v1,
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

fn diagnostic(error: impl std::fmt::Display) -> String {
    error.to_string()
}

// These finite ceilings are instantiated once at each new request boundary.
// Runtime and physical balances move from GEN/Cold; retries never call this.
fn normal_work(runtime: NormalHostWorkV1, io: [HostColdIoWorkV1; 2]) -> NormalCommitWorkV1 {
    let [primary, replica] = io;
    NormalCommitWorkV1 {
        canonical: primary.process_canonical,
        input_material: endpoint_work::source_work().frozen_material,
        primary_source: Some(endpoint_work::source_work()),
        replica_source: Some(endpoint_work::source_work()),
        physical: super::normal::HostNormalIoWorkV1 {
            state_pages: primary.state,
            value_pages: primary.values,
            original_pages: primary.originals,
            original_values: primary.original_values,
            replica_value_pages: replica.values,
            replica_original_pages: replica.originals,
            replica_original_values: replica.original_values,
        },
        runtime,
    }
}
fn settings(
    world_id: &str,
    writer: &ed25519_dalek::SigningKey,
    primary: &Arc<RemoteMonotonicEndpointProvider>,
    replica: &Arc<RemoteMonotonicEndpointProvider>,
    pinned: &TrustedFencingPolicyV1,
    transaction: oasis7_formal_protocol::incremental_execution::TransactionLimits,
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
        let work = normal_work(runtime, genesis_io);
        let first_settings = settings(
            "iws-genuine-host-transfer",
            &writer,
            &primary_provider,
            &replica_provider,
            &pinned,
            transaction,
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
            cognition,
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
                configuration,
                worker: worker_limits(),
                transaction,
                current_limits,
                projection_limits,
                facts: replica_hints,
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
        );
        let second_settings = settings(
            "iws-genuine-host-transfer",
            &writer,
            &primary_provider,
            &replica_provider,
            &pinned,
            transaction,
        );
        let (owners, second_settings, second_node) = run_native_node_tick_v1(
            NativeNodeOwnersV1 {
                primary,
                replica,
                memory,
                archive,
                work,
            },
            second_settings,
            cognition,
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
        let result = serde_json::json!({"schema":"oasis7.iws.nonempty_normal_cold.fixture.v1",
            "seed":seed_receipt,"genesis":genesis_observation,"first_node":first_node,"first":first_receipt,
            "old_pool_used_after_drop":0,"first_remaining_common":first_remaining,"cold":cold_summary,
            "second_node":second_node,"second":second_receipt,"final_sender_electricity":2,
            "cold_remaining_common":[archive.remaining_steps(),archive.remaining_read_bytes()]});
        drop(second);
        drop(second_other);
        drop(observe_work);
        drop(second_settings);
        drop(primary);
        drop(replica);
        drop(work);
        drop(archive);
        // Remaining partially moved cold work has no authority and is released
        // at scope end; the first complete-release check is the cold boundary.
        eprintln!("REAL_SECOND_NONEMPTY_NODE_ACCEPTED height=2 sender=2");
        serde_json::to_vec_pretty(&result).map_err(diagnostic)
    })
}
