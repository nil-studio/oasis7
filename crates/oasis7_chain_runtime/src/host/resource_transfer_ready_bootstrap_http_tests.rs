//! Bootstrap-only genuine Owner command; not a management RPC or CF payer.
use super::resource_preflight_http::Participant;
use super::*;
use oasis7_agent_api::game::GameParticipantReceiptV1;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_proto::agency_control::{
    AGENCY_CONTROL_REQUEST_TYPE, AgencyControlOperation, AgencyControlRequest,
};
use oasis7_runtime::{DelegationControlDisposition, OwnerControlHostAuthority};
const SITE: &str = "bootstrap-transfer-site";
const LOCATION: &str = "bootstrap-transfer-location";
const GRANT: &str = "bootstrap-transfer-grant";
impl CloseReplyCase {
    pub(super) fn uses_registered_participant(self) -> bool {
        matches!(
            self,
            Self::ResourcePreflight | Self::ResourceGoal | Self::ResourceGoalTransfer(_)
        )
    }
    pub(super) fn is_resource_goal(self) -> bool {
        matches!(self, Self::ResourceGoal | Self::ResourceGoalTransfer(_))
    }
}
pub(super) fn bootstrap_case(
    world: &mut World,
    p: &Participant,
    delegate: &str,
    case: CloseReplyCase,
) {
    if case.is_resource_goal() {
        super::resource_goal_paid_http::delegate(world, p, delegate);
    }
    if let CloseReplyCase::ResourceGoalTransfer(revoked) = case {
        bootstrap(world, p, revoked);
    }
}
struct RegisteredOwner {
    receipt: GameParticipantReceiptV1,
    player: String,
    world: String,
    reorg: u64,
    admitted_nonces: std::collections::BTreeSet<u64>,
}
impl RegisteredOwner {
    fn from_actual(world: &World, p: &Participant) -> Self {
        let current = oasis7_runtime::service_admission::agent_chat::owner_view(world, &p.agent)
            .unwrap()
            .unwrap();
        let receipt = world
            .capability_revocation_state()
            .participants
            .bindings
            .values()
            .find(|r| r.agent_id == p.agent)
            .expect("actual canonical registration")
            .clone();
        assert_eq!(receipt.owner_public_key, p.owner.subject_public_key);
        assert_eq!(receipt.owner_public_key, current.public_key);
        assert_eq!(receipt.owner_binding, current.player_id);
        assert_eq!(receipt.account_id, p.owner.request.account_id);
        assert_eq!(receipt.world_id, current.world_id);
        Self {
            receipt,
            player: current.player_id,
            world: current.world_id,
            reorg: current.reorg_epoch,
            admitted_nonces: Default::default(),
        }
    }
    fn apply(&mut self, world: &mut World, command: AgencyControlOperation, nonce: u64) {
        let mut request = AgencyControlRequest {
            frame_type: AGENCY_CONTROL_REQUEST_TYPE.into(),
            request_id: format!("bootstrap-command-{nonce}"),
            player_id: self.player.clone(),
            public_key: self.receipt.owner_public_key.clone(),
            agent_id: self.receipt.agent_id.clone(),
            world_id: self.world.clone(),
            reorg_epoch: self.reorg,
            command,
            auth: None,
        };
        let command = serde_json::to_value(&request.command).unwrap();
        request.auth = Some(
            oasis7_proto::player_auth::sign_agency_control_auth_proof(
                &request.request_id,
                &request.player_id,
                &request.public_key,
                &request.agent_id,
                &request.world_id,
                request.reorg_epoch,
                &command,
                nonce,
                &request.public_key,
                &"44".repeat(32),
            )
            .unwrap(),
        );
        let proof = request.auth.as_ref().unwrap();
        let verified = oasis7_proto::player_auth::verify_agency_control_auth_proof(
            &request.request_id,
            &request.player_id,
            &request.public_key,
            &request.agent_id,
            &request.world_id,
            request.reorg_epoch,
            &command,
            proof,
        )
        .unwrap();
        self.validate_registered_owner(
            &verified.player_id,
            &verified.public_key,
            &request.agent_id,
            &request.world_id,
            request.reorg_epoch,
        )
        .unwrap();
        // Host nonce admission is explicit bootstrap state, not a paid online port.
        assert!(!self.admitted_nonces.contains(&verified.nonce));
        let outcome = world
            .apply_signed_delegation_control(&request, self)
            .unwrap();
        assert_eq!(outcome.disposition, DelegationControlDisposition::Applied);
        assert!(self.admitted_nonces.insert(verified.nonce));
        let before = serde_json::to_vec(&world.snapshot()).unwrap();
        assert!(matches!(
            world
                .signed_delegation_control_replay(&request, self)
                .unwrap(),
            oasis7_runtime::DelegationReplayMatch::ExactReplay(_)
        ));
        assert!(
            !self.admitted_nonces.insert(verified.nonce),
            "same command cannot get a fresh host nonce"
        );
        assert_eq!(serde_json::to_vec(&world.snapshot()).unwrap(), before);
    }
}
impl OwnerControlHostAuthority for RegisteredOwner {
    fn validate_registered_owner(
        &self,
        player: &str,
        key: &str,
        agent: &str,
        world: &str,
        reorg: u64,
    ) -> Result<(), String> {
        if player == self.player
            && key == self.receipt.owner_public_key
            && agent == self.receipt.agent_id
            && world == self.world
            && world == self.receipt.world_id
            && reorg == self.reorg
        {
            Ok(())
        } else {
            Err("bootstrap registration tuple mismatch".into())
        }
    }
}
pub(super) fn bootstrap(world: &mut World, p: &Participant, revoked: bool) {
    let mut host = RegisteredOwner::from_actual(world, p);
    assert!(
        host.validate_registered_owner(
            &host.player,
            &"00".repeat(32),
            &p.agent,
            &host.world,
            host.reorg
        )
        .is_err()
    );
    world
        .set_location_anchor(oasis7_runtime::LocationAnchorV1 {
            location_id: LOCATION.into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_agent_location_authority(oasis7_runtime::AgentLocationAuthorityV1 {
            agent_id: p.agent.clone(),
            location_id: LOCATION.into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_factory_site_authority(oasis7_runtime::FactorySiteAuthorityV1 {
            site_id: SITE.into(),
            location_id: LOCATION.into(),
            owner_agent_id: p.agent.clone(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .unwrap();
    world
        .set_agent_resource_balance(
            &p.agent,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            1000,
        )
        .unwrap();
    world
        .set_ledger_material_balance(
            oasis7_runtime::MaterialLedgerId::agent(&p.agent),
            "iron_ore",
            20,
        )
        .unwrap();
    world
        .set_ledger_material_balance(
            oasis7_runtime::MaterialLedgerId::Site(SITE.into()),
            "iron_ore",
            0,
        )
        .unwrap();
    let tick = world.state().time;
    host.apply(
        world,
        AgencyControlOperation::InstallDelegation {
            grant_id: GRANT.into(),
            object_id: format!("agent:{}->site:{SITE}:iron_ore", p.agent),
            action_kinds: vec!["TransferMaterial".into()],
            period_id: "bootstrap-transfer-period".into(),
            valid_from_tick: tick,
            valid_until_tick: tick + 1000,
            limit_units: 1000,
        },
        1,
    );
    if revoked {
        host.apply(
            world,
            AgencyControlOperation::RevokeDelegation {
                grant_id: GRANT.into(),
                expected_revision: 1,
            },
            2,
        );
    }
    // Bootstrap setters changed materialized state after the fixture's last
    // native BFT tick. Commit one ordinary Runtime tick before export/import;
    // the real consensus recorder computes its own root and certificate.
    world.step().unwrap();
    assert!(
        world
            .state()
            .material_ledgers
            .get(&oasis7_runtime::MaterialLedgerId::Site(SITE.into()))
            .is_some_and(|ledger| ledger.is_empty()),
        "actual existing destination ledger has no material key"
    );
    let before = serde_json::to_vec(&world.snapshot()).unwrap();
    let journal = world.journal().clone();
    let native = oasis7_runtime::ResourceBasisSelectionV1::Transfer {
        from: oasis7_runtime::MaterialLedgerId::agent(&p.agent),
        to: oasis7_runtime::MaterialLedgerId::Site(SITE.into()),
        material: "iron_ore".into(),
        amount: 5,
        distance_km: 0,
        priority: oasis7_runtime::MaterialTransitPriority::Standard,
        routing: oasis7_runtime::ResourceBasisRoutingV1::Direct,
    };
    let result = world.native_resource_basis(&p.agent, &native);
    assert_eq!(
        result.is_ok(),
        !revoked,
        "actual grant controls read-only quote"
    );
    assert_eq!(serde_json::to_vec(&world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
    World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
}
pub(super) fn selection(case: CloseReplyCase, agent: &str) -> resource::ResourceBasisSelectionV1 {
    if matches!(case, CloseReplyCase::ResourceGoalTransfer(_)) {
        resource::ResourceBasisSelectionV1::Transfer {
            from: resource::ResourceLedgerV1::Agent {
                agent_id: agent.into(),
            },
            to: resource::ResourceLedgerV1::Site {
                site_id: SITE.into(),
            },
            material: "iron_ore".into(),
            amount: resource::ResourceDecimalI64V1(5),
            distance_km: resource::ResourceDecimalI64V1(0),
            priority: oasis7_runtime::MaterialTransitPriority::Standard,
            routing: resource::ResourceRoutingV1::Direct,
        }
    } else {
        resource::ResourceBasisSelectionV1::Build {
            site: "unavailable-site".into(),
            factory: "component-smelter".into(),
            factory_kind: "factory.smelter.mk1".into(),
        }
    }
}
pub(super) fn assert_resolution(
    case: CloseReplyCase,
    result: &resource::ResourceBasisResolutionV1,
    selection: &resource::ResourceBasisSelectionV1,
    agent: &str,
) {
    if matches!(case, CloseReplyCase::ResourceGoalTransfer(false)) {
        let resource::ResourceBasisResolutionV1::Ready { fact } = result else {
            panic!("actual signed Transfer grant must produce Ready")
        };
        assert_eq!(fact.agent_id, agent);
        assert_eq!(fact.selection, *selection);
        assert!(!fact.actor_owner.source_id.is_empty());
        assert!(fact.actor_owner.public_key.as_ref().is_some_and(|k| {
            *k == sign_read_request("key", (), &"44".repeat(32))
                .unwrap()
                .subject_public_key
        }));
        assert!(!fact.derived_resource_refs.is_empty());
        assert_eq!(fact.ledgers.len(), 2);
        assert!(
            fact.ledgers
                .iter()
                .all(|l| l.exists && !l.materials.is_empty())
        );
        let resource::ResourceBasisQuoteV1::Transfer(quote) = &fact.quote else {
            panic!("exact Transfer quote")
        };
        assert_eq!(quote.authorization.grant.grant_id, GRANT);
        assert_eq!(
            quote.authorization.grant.source_id,
            fact.actor_owner.source_id
        );
        assert_eq!(
            quote.authorization.grant.owner_id,
            fact.actor_owner.player_id
        );
        assert_eq!(quote.authorization.grant.agent_id, agent);
        assert!(!quote.authorization.grant.revoked);
        assert_eq!(
            quote.authorization.grant.action_kinds,
            vec!["TransferMaterial"]
        );
        let from = fact
            .ledgers
            .iter()
            .find(|l| {
                l.ledger
                    == resource::ResourceLedgerV1::Agent {
                        agent_id: agent.into(),
                    }
            })
            .unwrap();
        let to = fact
            .ledgers
            .iter()
            .find(|l| {
                l.ledger
                    == resource::ResourceLedgerV1::Site {
                        site_id: SITE.into(),
                    }
            })
            .unwrap();
        assert!(
            matches!(&from.owner, resource::ResourceLedgerOwnerV1::Agent { agent_id, owner } if agent_id == agent && owner == &fact.actor_owner)
        );
        assert!(
            matches!(&to.owner, resource::ResourceLedgerOwnerV1::Site { site_id, owner_agent_id, .. } if site_id == SITE && owner_agent_id == agent)
        );
        assert!(
            from.materials.iter().any(|m| m.material == "iron_ore"
                && m.amount == Some(resource::ResourceDecimalI64V1(20)))
        );
        assert_eq!(to.materials.len(), 1);
        assert!(
            to.materials
                .iter()
                .any(|m| m.material == "iron_ore" && m.amount.is_none())
        );
    } else {
        assert!(
            matches!(result, resource::ResourceBasisResolutionV1::Blocked { .. }),
            "missing or revoked resource authority must remain Blocked"
        );
    }
}
#[cfg(unix)]
#[test]
fn resource_transfer_ready_and_revoked_bootstrap_use_genuine_owner_commands_and_paid_tcp() {
    // Two isolated bootstrap worlds: no online management port is invented.
    mutation_case(CloseReplyCase::ResourceGoalTransfer(false), false);
    mutation_case(CloseReplyCase::ResourceGoalTransfer(true), false);
}
