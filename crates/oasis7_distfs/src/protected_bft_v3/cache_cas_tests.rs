//! A backend can commit before losing its reply; no uncertain result is reusable.
use super::*;
use crate::protected_bft_v3::{
    BftSignerAcquireOutcomeV3, BftSignerBackendSnapshotV3, BftSignerBackendV3,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Fault {
    State,
    Generation,
    Token,
    Clock,
    LostReply,
}
struct FaultyCas {
    inner: fixture::Backend,
    fault: Fault,
}
impl BftSignerBackendV3 for FaultyCas {
    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.load_provisioned(binding)
    }
    fn acquire_lease(
        &mut self,
        binding: &str,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.inner.acquire_lease(binding, generation, token)
    }
    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.release_lease(generation, token)
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.read_for_lease(generation, token)
    }
    fn protected_time_ms(&mut self, generation: u64, token: &str) -> Result<i64, BftSignerErrorV3> {
        self.inner.protected_time_ms(generation, token)
    }
    fn compare_and_swap(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        time: Option<i64>,
        next: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut snapshot = self
            .inner
            .compare_and_swap(generation, token, revision, digest, time, next)?;
        match self.fault {
            Fault::State => snapshot.state.schema_version = 2,
            Fault::Generation => snapshot.lease_generation += 1,
            Fault::Token => snapshot.lease_token_digest = "00".repeat(32),
            Fault::Clock => snapshot.protected_time_floor_ms = 0,
            Fault::LostReply => return Err(BftSignerErrorV3("committed reply lost".into())),
        }
        Ok(snapshot)
    }
}

fn raw_rpc(session: &mut Session, command: BftSignerCommandV3) -> SignedBftSignerReplyV3 {
    session.nonce += 1;
    let request = SignedBftSignerRequestV3::sign(
        &session.service.binding,
        &f::key(32),
        session.credential.clone(),
        command,
        format!("{:064x}", session.nonce),
    )
    .unwrap();
    let bytes =
        crate::protected_bft_v3::crypto::encode_bounded(&request, BFT_SIGNER_MAX_FRAME_BYTES_V3)
            .unwrap();
    let frame = session
        .service
        .handle_frame(&mut session.session, &bytes)
        .unwrap();
    crate::protected_bft_v3::wire::read_frame(&mut frame.as_slice()).unwrap()
}

#[test]
fn untrusted_or_lost_cas_reply_poisons_provider_without_releasing_success() {
    let binding = fixture::binding();
    let state = fixture::signed_history(&binding, 1);
    let previous = validate_state(&binding, Some(&f::public(33)), &state).unwrap();
    let candidate = proposal_candidate(&binding, &previous);
    for fault in [
        Fault::State,
        Fault::Generation,
        Fault::Token,
        Fault::Clock,
        Fault::LostReply,
    ] {
        let mut session = Session::new(&binding, state.clone(), 1);
        let credential = session.credential.clone().unwrap();
        let digest = crate::protected_bft_v3::service::lease_digest(&credential.token).unwrap();
        let observed = {
            let mut runtime = session.service.runtime.lock().unwrap();
            let snapshot = runtime
                .backend
                .read_for_lease(credential.generation, &digest)
                .unwrap();
            let shared = Arc::new(Mutex::new(snapshot));
            runtime.backend = Box::new(FaultyCas {
                inner: fixture::Backend(shared.clone()),
                fault,
            });
            shared
        };
        let reply = raw_rpc(
            &mut session,
            BftSignerCommandV3::Advance {
                expected_revision: state.revision,
                expected_state_digest: state.state_digest.clone(),
                candidate: Box::new(candidate.clone()),
            },
        );
        assert!(matches!(
            reply.body.result,
            BftSignerReplyKindV3::Rejected { .. }
        ));
        assert!(reply.body.checkpoint.is_none());
        // The backend really committed. Neither an error nor a fabricated return
        // may be promoted to a successful signer checkpoint or a new cache entry.
        assert_eq!(observed.lock().unwrap().state.revision, state.revision + 1);
        let runtime = session.service.runtime.lock().unwrap();
        assert!(runtime.poisoned);
        assert_eq!(runtime.verified.as_ref().unwrap().state, state);
        drop(runtime);
        let again = raw_rpc(&mut session, BftSignerCommandV3::Checkpoint);
        assert!(matches!(
            again.body.result,
            BftSignerReplyKindV3::Rejected { .. }
        ));
        assert!(again.body.checkpoint.is_none());
    }
}
