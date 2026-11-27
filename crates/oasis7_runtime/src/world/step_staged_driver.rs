//! Unified top-level `StagedRootTransaction` driver for the four public
//! `World::step*` and `resume` entrypoints (`#6150 PR 3/4` Adapter slice).
//!
//! This Adapter preserves the four outer `World::clone()` wrappers in `step.rs`
//! until `PR 4/4` Cutover while routing all four paths through a single
//! Stage 1 (prepare) -> Stage 2 (durable gate + base-head verify) -> Stage 3
//! (`apply_infallible` via `RootInstallContext`) contract and verifying
//! event-by-event equivalence plus five-domain fault/replay semantics.

#![cfg_attr(not(test), allow(dead_code))]

use super::step_staged_clock::{NextTickClock, TickClockDelta};
use super::transition::{
    DurableCommitConfirmation, PreparedStagedCommit, RootInstallContext, StagedRootTransaction,
    StagedTransition, SubsystemDelta, TransitionContext, TransitionReject,
};
use super::{World, WorldError};
use crate::RuntimeCommittedTickContext;
use oasis7_wasm_abi::ModuleSandbox;

/// Unified entrypoint selector for top-level staged tick execution.
pub(crate) enum StagedTickEntrypoint<'a> {
    Ordinary,
    WithModules {
        sandbox: &'a mut dyn ModuleSandbox,
    },
    CommittedContext {
        sandbox: &'a mut dyn ModuleSandbox,
        context: &'a RuntimeCommittedTickContext,
        product_validation_checkpoint: &'a mut dyn FnMut(&World) -> Result<(), WorldError>,
    },
    ProductValidationResume {
        sandbox: &'a mut dyn ModuleSandbox,
        context: &'a RuntimeCommittedTickContext,
    },
}

/// Root subsystem delta capturing the Stage 1 tick execution result and clock
/// advance for atomic Stage 3 installation via [`RootInstallContext`].
pub(crate) struct StagedTickExecutionDelta {
    clock_delta: Option<TickClockDelta>,
    staged_world: Box<World>,
}

impl SubsystemDelta for StagedTickExecutionDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        *ctx.world_mut() = *self.staged_world;
        if let Some(clock_delta) = self.clock_delta {
            clock_delta.apply_infallible(ctx);
        }
    }
}

/// Stage 1: prepare any of the four public `World::step*` / `resume` paths
/// inside a single `StagedRootTransaction` without mutating canonical `world`.
pub(crate) fn prepare_staged_tick(
    world: &World,
    entrypoint: StagedTickEntrypoint<'_>,
) -> Result<PreparedStagedCommit<Vec<StagedTickExecutionDelta>>, TransitionReject> {
    let target_tick = match &entrypoint {
        StagedTickEntrypoint::Ordinary | StagedTickEntrypoint::WithModules { .. } => {
            world.reject_ordinary_step_during_formal_execution()?;
            world.state.time.saturating_add(1)
        }
        StagedTickEntrypoint::CommittedContext { context, .. }
        | StagedTickEntrypoint::ProductValidationResume { context, .. } => context.height,
    };

    let mut tx =
        StagedRootTransaction::<StagedTickExecutionDelta>::begin(world, TransitionContext::new(target_tick, None))?;

    let clock_delta = match &entrypoint {
        StagedTickEntrypoint::Ordinary | StagedTickEntrypoint::WithModules { .. } => {
            Some(*NextTickClock.prepare(tx.view(), tx.context())?.delta())
        }
        StagedTickEntrypoint::CommittedContext { context, .. }
            if context.height == world.state.time.saturating_add(1) =>
        {
            Some(*NextTickClock.prepare(tx.view(), tx.context())?.delta())
        }
        StagedTickEntrypoint::CommittedContext { .. }
        | StagedTickEntrypoint::ProductValidationResume { .. } => None,
    };

    let mut staged = world.clone();
    match entrypoint {
        StagedTickEntrypoint::Ordinary => {
            staged.step_inner()?;
        }
        StagedTickEntrypoint::WithModules { sandbox } => {
            let mut no_checkpoint = |_world: &World| Ok(());
            staged.step_with_modules_inner(sandbox, &mut no_checkpoint)?;
        }
        StagedTickEntrypoint::CommittedContext {
            sandbox,
            context,
            product_validation_checkpoint,
        } => {
            staged.step_with_modules_for_committed_context_inner(
                sandbox,
                context,
                false,
                product_validation_checkpoint,
            )?;
        }
        StagedTickEntrypoint::ProductValidationResume { sandbox, context } => {
            let mut no_checkpoint = |_world: &World| Ok(());
            staged.step_with_modules_for_committed_context_inner(
                sandbox,
                context,
                true,
                &mut no_checkpoint,
            )?;
        }
    }

    tx.stage_delta(StagedTickExecutionDelta {
        clock_delta,
        staged_world: Box::new(staged),
    });
    Ok(tx.finish_prepare())
}

/// Stage 1 -> Stage 2 -> Stage 3 unified execution driver.
pub(crate) fn execute_staged_tick(
    world: &mut World,
    entrypoint: StagedTickEntrypoint<'_>,
    confirmation: DurableCommitConfirmation,
) -> Result<(), TransitionReject> {
    let prepared = prepare_staged_tick(world, entrypoint)?;
    prepared.install_confirmed(world, confirmation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ResourceKind};
    use oasis7_agent_api::world_action::GeoPos;
    use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput};

    struct NoopSandbox {
        calls: usize,
    }

    impl NoopSandbox {
        fn new() -> Self {
            Self { calls: 0 }
        }
    }

    impl ModuleSandbox for NoopSandbox {
        fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
            self.calls += 1;
            Ok(ModuleOutput {
                new_state: None,
                effects: Vec::new(),
                emits: Vec::new(),
                tick_lifecycle: None,
                output_bytes: 0,
            })
        }
    }

    fn pos(x: i64, y: i64) -> GeoPos {
        GeoPos {
            x_cm: x * 100_000,
            y_cm: y * 100_000,
            z_cm: 0,
        }
    }

    fn committed_ctx(world: &World, height: u64) -> RuntimeCommittedTickContext {
        RuntimeCommittedTickContext {
            height,
            slot: height.saturating_sub(1),
            epoch: 0,
            node_block_hash: "a".repeat(64),
            action_root: "staged-driver-fixture".to_string(),
            authority_node_id: world.tick_consensus_authority_source().to_string(),
            committed_at_unix_ms: 0,
        }
    }

    fn seed_cross_domain_world() -> World {
        let mut world = World::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "alice".to_string(),
            pos: pos(0, 0),
        });
        world.submit_action(Action::RegisterAgent {
            agent_id: "bob".to_string(),
            pos: pos(1, 0),
        });
        world.step().expect("register initial agents");

        // Industrial + monetary + institutions + capability_authz + module_lifecycle inputs:
        world
            .set_agent_resource_balance("alice", ResourceKind::Electricity, 500)
            .expect("seed alice electricity");
        world.set_resource_balance(ResourceKind::Electricity, 500);
        world.submit_action(Action::MoveAgent {
            agent_id: "alice".to_string(),
            to: pos(1, 0),
        });
        world.submit_action(Action::TransferResource {
            from_agent_id: "alice".to_string(),
            to_agent_id: "bob".to_string(),
            kind: ResourceKind::Electricity,
            amount: 120,
        });
        world.submit_action(Action::UpdateGameplayPolicy {
            operator_agent_id: "alice".to_string(),
            electricity_tax_bps: 0,
            power_trade_fee_bps: 0,
            data_tax_bps: 0,
        });
        world
    }

    fn assert_worlds_equivalent(legacy: &World, staged: &World) {
        assert_eq!(legacy.state, staged.state);
        assert_eq!(legacy.journal.events, staged.journal.events);
        assert_eq!(legacy.next_event_id, staged.next_event_id);
        assert_eq!(legacy.next_event_id_era, staged.next_event_id_era);
        assert_eq!(legacy.pending_actions.len(), staged.pending_actions.len());
        assert_eq!(legacy.scheduler_cursor, staged.scheduler_cursor);
        assert_eq!(
            legacy.tick_consensus_records,
            staged.tick_consensus_records
        );
        assert_eq!(
            legacy
                .current_state_root_hash()
                .expect("legacy state root"),
            staged
                .current_state_root_hash()
                .expect("staged state root"),
        );
    }

    #[test]
    fn staged_driver_matches_legacy_event_by_event_across_all_four_entrypoints() {
        // 1. Ordinary step()
        {
            let mut legacy = seed_cross_domain_world();
            let mut staged = legacy.clone();
            legacy.step().expect("legacy step");
            execute_staged_tick(
                &mut staged,
                StagedTickEntrypoint::Ordinary,
                DurableCommitConfirmation::Confirmed,
            )
            .expect("staged ordinary step");
            assert_worlds_equivalent(&legacy, &staged);
        }

        // 2. step_with_modules()
        {
            let mut legacy = seed_cross_domain_world();
            let mut staged = legacy.clone();
            let mut legacy_sandbox = NoopSandbox::new();
            let mut staged_sandbox = NoopSandbox::new();
            legacy
                .step_with_modules(&mut legacy_sandbox)
                .expect("legacy step_with_modules");
            execute_staged_tick(
                &mut staged,
                StagedTickEntrypoint::WithModules {
                    sandbox: &mut staged_sandbox,
                },
                DurableCommitConfirmation::Confirmed,
            )
            .expect("staged step_with_modules");
            assert_eq!(legacy_sandbox.calls, staged_sandbox.calls);
            assert_worlds_equivalent(&legacy, &staged);
        }

        // 3. CommittedContext with product-validation checkpoint
        {
            let mut legacy = seed_cross_domain_world();
            let mut staged = legacy.clone();
            let ctx = committed_ctx(&legacy, legacy.state.time + 1);
            let mut legacy_sandbox = NoopSandbox::new();
            let mut staged_sandbox = NoopSandbox::new();
            let mut legacy_cp_calls = 0usize;
            let mut staged_cp_calls = 0usize;
            let mut legacy_cp = |_w: &World| {
                legacy_cp_calls += 1;
                Ok(())
            };
            let mut staged_cp = |_w: &World| {
                staged_cp_calls += 1;
                Ok(())
            };
            legacy
                .step_with_modules_for_committed_context_with_product_validation_checkpoint(
                    &mut legacy_sandbox,
                    &ctx,
                    &mut legacy_cp,
                )
                .expect("legacy committed context");
            execute_staged_tick(
                &mut staged,
                StagedTickEntrypoint::CommittedContext {
                    sandbox: &mut staged_sandbox,
                    context: &ctx,
                    product_validation_checkpoint: &mut staged_cp,
                },
                DurableCommitConfirmation::Confirmed,
            )
            .expect("staged committed context");
            assert_eq!(legacy_cp_calls, staged_cp_calls);
            assert_worlds_equivalent(&legacy, &staged);
        }

        // 4. ProductValidationResume (height == world.state.time)
        {
            let mut legacy = seed_cross_domain_world();
            let ctx = committed_ctx(&legacy, legacy.state.time);
            let mut staged = legacy.clone();
            let mut legacy_sandbox = NoopSandbox::new();
            let mut staged_sandbox = NoopSandbox::new();
            legacy
                .step_with_modules_for_committed_context_after_product_validation_intent(
                    &mut legacy_sandbox,
                    &ctx,
                )
                .expect("legacy product validation resume");
            execute_staged_tick(
                &mut staged,
                StagedTickEntrypoint::ProductValidationResume {
                    sandbox: &mut staged_sandbox,
                    context: &ctx,
                },
                DurableCommitConfirmation::Confirmed,
            )
            .expect("staged product validation resume");
            assert_worlds_equivalent(&legacy, &staged);
        }
    }

    #[test]
    fn staged_driver_fault_matrix_stage1_stage2_fencing_stale_head_and_replay() {
        // Stage 1 rejection: invalid committed height & untrusted consensus authority
        {
            let mut world = seed_cross_domain_world();
            let before = world.snapshot();
            let before_journal = world.journal.events.clone();
            let bad_ctx = committed_ctx(&world, world.state.time);
            let mut sandbox = NoopSandbox::new();
            let mut cp = |_w: &World| Ok(());
            let err = execute_staged_tick(
                &mut world,
                StagedTickEntrypoint::CommittedContext {
                    sandbox: &mut sandbox,
                    context: &bad_ctx,
                    product_validation_checkpoint: &mut cp,
                },
                DurableCommitConfirmation::Confirmed,
            )
            .expect_err("stale committed height must fail Stage 1");
            assert!(matches!(
                err,
                TransitionReject::World(WorldError::DistributedValidationFailed { .. })
            ));
            assert_eq!(world.snapshot(), before);
            assert_eq!(world.journal.events, before_journal);

            world.tick_consensus_authority_source = "untrusted.node".to_string();
            let before_untrusted = world.snapshot();
            let err = execute_staged_tick(
                &mut world,
                StagedTickEntrypoint::Ordinary,
                DurableCommitConfirmation::Confirmed,
            )
            .expect_err("untrusted authority must fail Stage 1");
            assert!(matches!(
                err,
                TransitionReject::World(WorldError::DistributedValidationFailed { .. })
            ));
            assert_eq!(world.snapshot(), before_untrusted);
        }

        // Stage 2 StatusUnknown -> Fenced without mutating canonical World
        {
            let mut world = seed_cross_domain_world();
            let before = world.snapshot();
            let before_journal = world.journal.events.clone();
            let before_consensus_len = world.tick_consensus_records.len();
            let err = execute_staged_tick(
                &mut world,
                StagedTickEntrypoint::Ordinary,
                DurableCommitConfirmation::StatusUnknown {
                    reason: "durable commit status unknown".to_string(),
                },
            )
            .expect_err("StatusUnknown must fence");
            assert!(matches!(
                err,
                TransitionReject::StatusUnknownFenced { .. }
            ));
            assert_eq!(world.snapshot(), before);
            assert_eq!(world.journal.events, before_journal);
            assert_eq!(world.tick_consensus_records.len(), before_consensus_len);
        }

        // Stage 2 BaseHeadStale -> Reject without overwriting concurrent mutation
        {
            let mut world = seed_cross_domain_world();
            let prepared = prepare_staged_tick(&world, StagedTickEntrypoint::Ordinary)
                .expect("prepare staged ordinary tick");
            let concurrent_id = world.submit_action(Action::RegisterAgent {
                agent_id: "charlie".to_string(),
                pos: pos(2, 0),
            });
            let err = prepared
                .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
                .expect_err("concurrent mutation must trigger BaseHeadStale");
            assert!(matches!(err, TransitionReject::BaseHeadStale));
            assert_eq!(
                world.pending_actions.back().map(|env| env.id),
                Some(concurrent_id)
            );
        }

        // Deterministic replay across identical successor inputs
        {
            let mut replay_a = seed_cross_domain_world();
            let mut replay_b = replay_a.clone();
            execute_staged_tick(
                &mut replay_a,
                StagedTickEntrypoint::Ordinary,
                DurableCommitConfirmation::Confirmed,
            )
            .expect("replay A");
            execute_staged_tick(
                &mut replay_b,
                StagedTickEntrypoint::Ordinary,
                DurableCommitConfirmation::Confirmed,
            )
            .expect("replay B");
            assert_worlds_equivalent(&replay_a, &replay_b);
        }
    }
}
