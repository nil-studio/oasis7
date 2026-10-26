//! Test-only observation of successful completion publication; never drains results.
use super::{ActorCommand, AsyncAgentRunner, AsyncTurnId};
use serde_json::Value;
use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
struct State {
    blocked: bool,
    reached: BTreeSet<AsyncTurnId>,
    published: BTreeSet<AsyncTurnId>,
}

#[derive(Default)]
pub(super) struct Publication {
    state: Mutex<State>,
    changed: Condvar,
}

impl Publication {
    pub(super) fn before_send(&self, turn: AsyncTurnId) {
        let mut state = self.state.lock().unwrap();
        state.reached.insert(turn);
        self.changed.notify_all();
        while state.blocked {
            state = self.changed.wait(state).unwrap();
        }
    }

    pub(super) fn after_send(&self, turn: AsyncTurnId) {
        self.state.lock().unwrap().published.insert(turn);
        self.changed.notify_all();
    }

    fn release(&self) {
        self.state.lock().unwrap().blocked = false;
        self.changed.notify_all();
    }
}

type Runner = AsyncAgentRunner<Value, Value, String, String, String>;

/// Releases every test gate and joins the workers even when an assertion unwinds.
/// Production Drop remains nonblocking and is deliberately not used as proof of cleanup.
pub(super) struct PublicationRunner(pub(super) Runner);

impl Deref for PublicationRunner {
    type Target = Runner;
    fn deref(&self) -> &Runner {
        &self.0
    }
}
impl DerefMut for PublicationRunner {
    fn deref_mut(&mut self) -> &mut Runner {
        &mut self.0
    }
}

impl PublicationRunner {
    pub(super) fn block_publication(&self) {
        for actor in self.actors.values() {
            actor.publication.state.lock().unwrap().blocked = true;
        }
    }

    pub(super) fn wait_before_send(&self, agent: &str, turn: AsyncTurnId) {
        let publication = &self.actors[agent].publication;
        let (state, _) = publication
            .changed
            .wait_timeout_while(
                publication.state.lock().unwrap(),
                Duration::from_secs(2),
                |state| !state.reached.contains(&turn),
            )
            .unwrap();
        let reached = state.reached.contains(&turn);
        let published = state.published.contains(&turn);
        drop(state);
        assert!(
            reached,
            "worker did not reach publication for {agent}/{turn:?}"
        );
        assert!(!published, "closed test gate must prevent publication");
    }

    pub(super) fn release_and_wait(&self, turns: &[(&str, AsyncTurnId)]) {
        for (agent, _) in turns {
            self.actors[*agent].publication.release();
        }
        for (agent, turn) in turns {
            let publication = &self.actors[*agent].publication;
            let (state, _) = publication
                .changed
                .wait_timeout_while(
                    publication.state.lock().unwrap(),
                    Duration::from_secs(2),
                    |state| !state.published.contains(turn),
                )
                .unwrap();
            let published = state.published.contains(turn);
            drop(state);
            assert!(published, "completion was not sent for {agent}/{turn:?}");
        }
    }
}

impl Drop for PublicationRunner {
    fn drop(&mut self) {
        for actor in self.actors.values() {
            actor.publication.release();
        }
        for actor in self.actors.values_mut() {
            let _ = actor.sender.try_send(ActorCommand::Shutdown);
            if let Some(join) = actor.join.take() {
                let _ = join.join();
            }
        }
    }
}
