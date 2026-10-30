//! Two-phase original identity bootstrap. No writable driver is opened before Acquire.
use super::*;
use oasis7_distfs::protected_bft_v3::{BftSignerLeaseV3, LocalBftSuccessorArchiveStoreV1};
use std::time::Instant;
struct Deferred {
    identity: Option<OriginalHeaderIdentityV1>,
    records: PathBuf,
    store: LocalCasStore,
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
    material: ClosedRecord,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    peer_network: bool,
    archive: Option<Arc<LocalBftSuccessorArchiveStoreV1>>,
    provider_pin: String,
    auto_prepare_owner: super::auto_prepare::AutoPrepareOwnerV1,
    delegate: Option<Driver>,
    membership_enabled: bool,
}
enum Driver {
    Legacy(Box<FormalBftHeaderNodeDriverV3>),
    Active(Box<super::super::window_execution::node_adapter::ActiveNodeDriver>),
}
impl Driver {
    fn hook(&mut self) -> &mut (dyn NodeExecutionHook + 'static) {
        match self {
            Self::Legacy(driver) => driver.as_mut(),
            Self::Active(driver) => driver.as_mut(),
        }
    }
}
impl FormalBftHeaderNodeDriverV3 {
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub(crate) fn deferred_existing(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        archive: Option<Arc<LocalBftSuccessorArchiveStoreV1>>,
        provider_pin: String,
        peer_network: bool,
    ) -> Result<
        (
            Box<dyn NodeExecutionHook>,
            VerifiedBftHandoffTargetV2,
            BftHeaderHeightScheduleV3,
            String,
        ),
        String,
    > {
        if let Some(archive) = &archive {
            let archive_path = canonical_storage_path(archive.root())?;
            let records_path = canonical_storage_path(records)?;
            let store_path = canonical_storage_path(store.root())?;
            if [&records_path, &store_path]
                .iter()
                .any(|path| archive_path.starts_with(path) || path.starts_with(&archive_path))
            {
                return Err(
                    "private archive, original ledger and live CAS must be disjoint".into(),
                );
            }
        }
        let identity = OriginalHeaderIdentityV1::open(
            records,
            store.clone(),
            anchor.clone(),
            target.clone(),
            material.clone(),
            source.clone(),
        )?;
        let auto_prepare_owner =
            super::auto_prepare::AutoPrepareOwnerV1::from_identity(&identity, &provider_pin)?;
        let handoff = identity.handoff().clone();
        let initial = identity.initial_schedule().clone();
        let material_identity = identity.material_identity().to_owned();
        Ok((
            Box::new(Deferred {
                identity: Some(identity),
                records: records.into(),
                store,
                anchor,
                target,
                material,
                source,
                peer_network,
                archive,
                provider_pin,
                auto_prepare_owner,
                delegate: None,
                membership_enabled: false,
            }),
            handoff,
            initial,
            material_identity,
        ))
    }
}
impl Deferred {
    fn driver(&mut self) -> Result<&mut (dyn NodeExecutionHook + 'static), String> {
        self.delegate
            .as_mut()
            .map(Driver::hook)
            .ok_or("wire-3 driver has not completed authenticated startup".into())
    }
}
impl NodeExecutionHook for Deferred {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("legacy execution is unavailable for wire-3".into())
    }
    fn preflight_bft_v3_capacity(
        &mut self,
        context: &NodeBftBuildContextV3,
    ) -> Result<oasis7_node::NodeBftCapacityAdmissionV3, String> {
        self.driver()?.preflight_bft_v3_capacity(context)
    }
    fn prepare_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<(), String> {
        self.prepare_authenticated(binding, lease, deadline)
    }

    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        self.membership_enabled = true;
        if let Some(driver) = &mut self.delegate {
            driver.hook().enable_bft_v3_membership_execution()?;
        }
        Ok(())
    }
    fn validate_bft_v3_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let envelope =
            oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(payload)
                .map_err(|_| "Node action payload decoding rejected")?;
        match envelope.body {
            oasis7::consensus_action_payload::ConsensusActionPayloadBody::RuntimeAction {
                ..
            } => Ok(()),
            _ => Err("formal header Node requires a closed RuntimeAction payload".into()),
        }
    }

    fn configure_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        self.driver()?.configure_bft_v3_execution(binding)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.driver()?.current_bft_v3_execution_context(schedule)
    }
    fn build_bft_v3_candidate(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.driver()?.build_bft_v3_candidate(context)
    }
    fn preview_bft_v3_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.driver()?.preview_bft_v3_candidate(context, candidate)
    }
    fn retain_bft_v3_reserved_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.driver()?
            .retain_bft_v3_reserved_candidate(context, candidate)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.driver()?
            .retain_bft_v3_private_candidate(context, candidate)
    }
    fn export_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        self.driver()?.export_bft_v3_private_candidate(value)
    }
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        package: &NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        self.driver()?
            .retain_received_bft_v3_private_candidate(context, package)
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.driver()?.pending_bft_v3_private_candidate(context)
    }
    fn lookup_bft_v3_public_value(
        &mut self,
        height: u64,
        id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        self.driver()?.lookup_bft_v3_public_value(height, id)
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.driver()?.load_bft_v3_private_candidate(value)
    }
    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        self.driver()?.commit_bft_v3_certificate(token)
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        binding: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.driver()?
            .load_bft_v3_commit_history_page(binding, after, max)
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
        actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        self.driver()?
            .validate_bft_v3_action_batch(schedule, actions)
    }
}

#[path = "driver_bft_v3_node_deferred_prepare.rs"]
mod preparation;
