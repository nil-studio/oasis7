//! Prepared tick clock for the one-root staged ordinary step.
//!
//! This deliberately does NOT replace the four legacy outer clone guards yet:
//! the remaining domain, publication, and recovery deltas must join the same
//! root before the production entrypoints can safely install this delta.

use super::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition,
    SubsystemDelta, TransitionContext, TransitionReject,
};
use super::WorldError;

/// Stage just the next logical tick; never mutate the World at prepare time.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "root clock is staged but not wired until five-domain tick cutover")
)]
pub(crate) struct NextTickClock;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "typed root install waits for complete staged tick acceptance")
)]
pub(crate) struct TickClockDelta {
    next_tick: u64,
}

impl SubsystemDelta for TickClockDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        ctx.state_mut().time = self.next_tick;
    }
}

impl StagedTransition for NextTickClock {
    type Delta = TickClockDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let next_tick = view.world().state().time.saturating_add(1);
        if ctx.tick != next_tick {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "staged root tick mismatch: expected {next_tick}, got {}",
                    ctx.tick
                ),
            }
            .into());
        }
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            None,
            TickClockDelta { next_tick },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::transition::DurableCommitConfirmation;
    use crate::world::World;

    #[test]
    fn next_tick_stages_without_mutation_and_installs_only_after_confirmation() {
        let mut world = World::new();
        let original_tick = world.state().time;
        let next_tick = original_tick.saturating_add(1);
        let view = BorrowedWorldView::new(&world).expect("capture original root");
        let prepared = NextTickClock
            .prepare(&view, &TransitionContext::new(next_tick, None))
            .expect("prepare typed clock delta");
        assert_eq!(world.state().time, original_tick);
        drop(view);

        let unknown = prepared.clone().install_confirmed(
            &mut world,
            DurableCommitConfirmation::StatusUnknown {
                reason: "durable tick confirmation unavailable".to_string(),
            },
        );
        assert!(matches!(
            unknown,
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!(world.state().time, original_tick);

        prepared
            .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
            .expect("confirmed root installs staged logical tick");
        assert_eq!(world.state().time, next_tick);
    }

    #[test]
    fn next_tick_rejects_mismatched_context_and_stale_parent_before_install() {
        let mut world = World::new();
        let original_tick = world.state().time;
        let next_tick = original_tick.saturating_add(1);
        let view = BorrowedWorldView::new(&world).expect("capture original root");
        let wrong = NextTickClock.prepare(
            &view,
            &TransitionContext::new(next_tick.saturating_add(1), None),
        );
        assert!(matches!(
            wrong,
            Err(TransitionReject::World(
                WorldError::DistributedValidationFailed { .. }
            ))
        ));
        assert_eq!(world.state().time, original_tick);

        let prepared = NextTickClock
            .prepare(&view, &TransitionContext::new(next_tick, None))
            .expect("prepare genuine successor clock");
        drop(view);
        world.state.time = next_tick;
        let stale =
            prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed);
        assert!(matches!(stale, Err(TransitionReject::BaseHeadStale)));
        assert_eq!(world.state().time, next_tick);
    }
}
