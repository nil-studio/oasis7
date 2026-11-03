//! Node admission ordering with a host double; no material/custody attestation.
use super::*;
use crate::NodeBftPrivateCandidateV3;
use oasis7_consensus::tendermint_v3::{BftHeaderValueV3, FormalBftExecutionCommitmentV1};
use oasis7_proto::distributed_bft::{v2::BftScopeV2, v3::BftBlockHeaderV3};
use std::{collections::VecDeque, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedOriginal {
    context: NodeBftExecutionContextV3,
    package: NodeBftPrivateCandidatePackageV3,
    deadline: Instant,
}
impl From<Original<'_>> for OwnedOriginal {
    fn from(original: Original<'_>) -> Self {
        Self {
            context: original.context.clone(),
            package: original.package.clone(),
            deadline: original.deadline,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Retain(OwnedOriginal),
    Roll(OwnedOriginal),
    Refresh(OwnedOriginal),
}
struct HostDouble {
    outcomes: VecDeque<Result<NodeBftProtectedCandidateAdmissionV3, &'static str>>,
    trace: Vec<Step>,
    retained: Vec<OwnedOriginal>,
    unknown_reserved: Option<OwnedOriginal>,
    roll_error: bool,
}
impl HostDouble {
    fn new(outcomes: Vec<Result<NodeBftProtectedCandidateAdmissionV3, &'static str>>) -> Self {
        Self {
            outcomes: outcomes.into(),
            trace: Vec::new(),
            retained: Vec::new(),
            unknown_reserved: None,
            roll_error: false,
        }
    }
}
impl Host for HostDouble {
    fn retain_original(
        &mut self,
        original: Original<'_>,
    ) -> Result<NodeBftProtectedCandidateAdmissionV3, NodeError> {
        let owned = OwnedOriginal::from(original);
        self.trace.push(Step::Retain(owned.clone()));
        match self
            .outcomes
            .pop_front()
            .expect("unexpected extra admission")
        {
            Ok(outcome) => {
                if matches!(outcome, NodeBftProtectedCandidateAdmissionV3::Retained(_)) {
                    self.retained.push(owned);
                }
                Ok(outcome)
            }
            Err(reason) => {
                self.unknown_reserved = Some(owned);
                Err(error(reason))
            }
        }
    }
    fn roll_original(&mut self, original: Original<'_>) -> Result<(), NodeError> {
        self.trace.push(Step::Roll(original.into()));
        if self.roll_error {
            return Err(error("unknown original successor transaction"));
        }
        Ok(())
    }
    fn refresh_original(&mut self, original: Original<'_>) -> Result<(), NodeError> {
        self.trace.push(Step::Refresh(original.into()));
        Ok(())
    }
}

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn input(tag: u8) -> (NodeBftExecutionContextV3, NodeBftPrivateCandidatePackageV3) {
    let scope = BftScopeV2 {
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_hash: hash(1),
    };
    let context = NodeBftExecutionContextV3 {
        scope: scope.clone(),
        activation_digest: hash(2),
        height: 65,
        parent_commit_ref: hash(3),
        authority_epoch: 1,
        parent_state_root: hash(4),
        parent_tick: 64,
        active_validator_set_hash: hash(5),
        schedule_policy_hash: hash(6),
        manifest_hash: hash(7),
    };
    // Deliberately opaque at the Node orchestration seam. Real package validity
    // belongs to the Runtime verifier, which this host double does not replace.
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: "oasis7.formal.bft.execution.v1".into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: hash(2),
        scope,
        height: 65,
        parent_tick: 64,
        parent_commit_ref: hash(3),
        parent_semantic_state_root: hash(4),
        authority_epoch: 1,
        active_validator_set_hash: hash(5),
        schedule_policy_hash: hash(6),
        manifest_hash: hash(7),
        post_manifest_hash: hash(8),
        action_root: hash(tag),
        event_root: hash(9),
        execution_state_root: hash(10),
        state_root: hash(11),
        projection_root: hash(12),
        result_root: hash(13),
        transition: None,
        transition_ref: None,
    };
    let package = NodeBftPrivateCandidatePackageV3 {
        candidate: NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: BftBlockHeaderV3 {
                    protocol_version: 3,
                    execution_commitment_hash: hash(tag),
                    actions_cbor_digest: hash(14),
                },
                execution_commitment: commitment,
            },
            actions_cbor: vec![0x81, tag],
        },
        dependency_nodes: vec![(hash(15), vec![tag, 3, 7])],
    };
    (context, package)
}
fn retained() -> Result<NodeBftProtectedCandidateAdmissionV3, &'static str> {
    Ok(NodeBftProtectedCandidateAdmissionV3::Retained(hash(31)))
}

#[test]
fn received_rejection_has_no_roll_or_effect_and_next_valid_input_continues() {
    let (context, bad) = input(16);
    let (_, valid) = input(17);
    let deadline = Instant::now() + Duration::from_secs(60);
    let bad = Original {
        context: &context,
        package: &bad,
        deadline,
    };
    let valid = Original {
        context: &context,
        package: &valid,
        deadline,
    };
    let mut host = HostDouble::new(vec![
        Ok(NodeBftProtectedCandidateAdmissionV3::Rejected(
            "pure invalid dependency".into(),
        )),
        retained(),
    ]);
    assert!(!retain(&mut host, bad).unwrap());
    assert_eq!(host.trace, vec![Step::Retain(bad.into())]);
    assert!(host.retained.is_empty());
    assert!(host.unknown_reserved.is_none());
    assert!(retain(&mut host, valid).unwrap());
    assert_eq!(
        host.trace,
        vec![
            Step::Retain(bad.into()),
            Step::Retain(valid.into()),
            Step::Refresh(valid.into())
        ]
    );
    assert_eq!(host.retained, vec![OwnedOriginal::from(valid)]);
}

#[test]
fn received_roll_retries_exact_original_context_package_and_spent_deadline_once() {
    let (context, package) = input(18);
    let original = Original {
        context: &context,
        package: &package,
        deadline: Instant::now() - Duration::from_secs(1),
    };
    let mut host = HostDouble::new(vec![
        Ok(NodeBftProtectedCandidateAdmissionV3::NeedsRoll),
        retained(),
    ]);
    assert!(retain(&mut host, original).unwrap());
    assert_eq!(
        host.trace,
        vec![
            Step::Retain(original.into()),
            Step::Roll(original.into()),
            Step::Retain(original.into()),
            Step::Refresh(original.into())
        ]
    );
    assert_eq!(host.retained, vec![OwnedOriginal::from(original)]);
}

#[test]
fn received_unknown_preserves_original_reservation_without_roll_retry_or_refresh() {
    let (context, package) = input(19);
    let original = Original {
        context: &context,
        package: &package,
        deadline: Instant::now(),
    };
    let mut host = HostDouble::new(vec![Err("unknown original Reserved"), retained()]);
    assert!(retain(&mut host, original).is_err());
    assert_eq!(host.trace, vec![Step::Retain(original.into())]);
    assert_eq!(host.unknown_reserved, Some(original.into()));
    assert!(host.retained.is_empty());
    assert_eq!(
        host.outcomes.len(),
        1,
        "unknown must not be retried as a new operation"
    );
}

#[test]
fn received_still_needs_roll_is_host_blocked_without_second_roll() {
    let (context, package) = input(20);
    let original = Original {
        context: &context,
        package: &package,
        deadline: Instant::now(),
    };
    let mut host = HostDouble::new(vec![
        Ok(NodeBftProtectedCandidateAdmissionV3::NeedsRoll),
        Ok(NodeBftProtectedCandidateAdmissionV3::NeedsRoll),
        retained(),
    ]);
    assert!(retain(&mut host, original).is_err());
    assert_eq!(
        host.trace,
        vec![
            Step::Retain(original.into()),
            Step::Roll(original.into()),
            Step::Retain(original.into())
        ]
    );
    assert_eq!(host.outcomes.len(), 1);
    assert!(host.retained.is_empty());
}

#[test]
fn received_unknown_roll_never_retries_or_refreshes() {
    let (context, package) = input(21);
    let original = Original {
        context: &context,
        package: &package,
        deadline: Instant::now(),
    };
    let mut host = HostDouble::new(vec![
        Ok(NodeBftProtectedCandidateAdmissionV3::NeedsRoll),
        retained(),
    ]);
    host.roll_error = true;
    assert!(retain(&mut host, original).is_err());
    assert_eq!(
        host.trace,
        vec![Step::Retain(original.into()), Step::Roll(original.into())]
    );
    assert_eq!(host.outcomes.len(), 1);
}
