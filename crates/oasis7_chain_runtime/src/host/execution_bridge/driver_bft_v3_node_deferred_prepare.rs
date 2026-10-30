//! Authenticated active selection and exact original pending-install continuation.
use super::super::super::{
    successor_archive::{err, live},
    successor_verifier::ActualBftSuccessorWindowDriverVerifierV1,
    window_execution::{FormalBftHeaderActiveExecutionV1, node_adapter::ActiveNodeDriver, resume},
};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerCheckpointV3, BftSuccessorWindowSourceV1, BftSuccessorWindowStatusV1,
    BftSuccessorWindowStoredRequestV1,
};
impl Deferred {
    /// Mutually exclusive owned replay branches must not share one native frame.
    #[inline(never)]
    pub(super) fn prepare_authenticated(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<(), String> {
        live(deadline)?;
        if self.delegate.is_some() {
            return Err("wire-3 authenticated preparation already consumed".into());
        }
        let identity = self.identity.as_ref().ok_or("original identity absent")?;
        identity.verify_unchanged()?;
        if identity.material_identity() != binding.material_identity
            || identity.initial_schedule() != binding.initial_schedule()
            || identity.handoff().target_digest() != binding.handoff().target_digest()
        {
            return Err("deferred original identity differs from fixed binding".into());
        }
        let mut checkpoint = Box::new(lease.checkpoint().map_err(err)?);
        if let Some(reserved) = checkpoint
            .state
            .successor_window
            .as_ref()
            .and_then(|window| window.reservation.as_ref())
        {
            // A local file alone cannot select the request. Only this exact
            // authenticated reservation permits replay of its original Prepare.
            let intent = super::super::auto_prepare::AutoPrepareIntentV1::load_reserved(
                &self.records,
                &self.auto_prepare_owner,
                binding,
                &reserved.body,
                &reserved.manifest,
                checkpoint.lease_generation,
            )?;
            identity.verify_unchanged()?;
            live(deadline)?;
            intent.retry(lease)?;
            identity.verify_unchanged()?;
            checkpoint = Box::new(lease.checkpoint().map_err(err)?);
            if checkpoint
                .state
                .successor_window
                .as_ref()
                .is_some_and(|window| window.reservation.is_some())
            {
                return Err("original auto Prepare reservation remains unresolved".into());
            }
        }
        let pending = checkpoint
            .state
            .successor_window
            .as_ref()
            .and_then(|window| window.requests.last())
            .filter(|record| {
                matches!(
                    record.request.status,
                    BftSuccessorWindowStatusV1::Prepared
                        | BftSuccessorWindowStatusV1::InstalledPending
                )
            })
            .map(|record| Box::new(record.clone()));
        if let Some(record) = pending {
            self.resume_original_pending(binding, lease, record, checkpoint, deadline)?;
            checkpoint = Box::new(lease.checkpoint().map_err(err)?);
            if checkpoint.state.active_window.is_none() {
                return Err("pending install did not yield an authenticated active base".into());
            }
        }
        let driver = if checkpoint.state.active_window.is_some() {
            self.open_active_driver(binding, lease, deadline)?
        } else {
            self.open_legacy_driver(binding, deadline)?
        };
        self.delegate = Some(driver);
        Ok(())
    }

    #[inline(never)]
    fn resume_original_pending(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        mut record: Box<BftSuccessorWindowStoredRequestV1>,
        mut checkpoint: Box<BftSignerCheckpointV3>,
        deadline: Instant,
    ) -> Result<(), String> {
        let archive = self
            .archive
            .clone()
            .ok_or("pending startup requires fixed private archive directory")?;
        let source = BftSuccessorWindowSourceV1::from_binding(binding).map_err(err)?;
        if record.request.intent.body.checkpoint.source != source {
            return Err("pending original source differs from fixed identity".into());
        }
        let body = &record.request.intent.body.checkpoint;
        let id = body.request_id.clone();
        let digest = body.digest().map_err(err)?;
        if record.request.status == BftSuccessorWindowStatusV1::Prepared {
            let committed = lease.commit_successor_window(&id, &digest).map_err(err)?;
            *record = committed.0;
            *checkpoint = committed.1;
        }
        let public = Box::new(
            lease
                .read_successor_original_state(&id, &digest, deadline)
                .map_err(err)?,
        );
        let installed = Box::new(
            public
                .reconstruct_installed_pending(binding, &self.provider_pin, &record)
                .map_err(err)?,
        );
        if installed.installed_state() != &checkpoint.state {
            return Err("pending exact InstalledPending State differs".into());
        }
        let verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
            self.anchor.clone(),
            self.target.clone(),
            archive,
        )?
        .with_installation_root(self.records.clone())?
        .with_active_private_store(self.store.clone())?;
        let base = Box::new(verifier.recover_private_base(&record, &public, deadline)?);
        self.identity
            .as_ref()
            .ok_or("original identity lost")?
            .verify_unchanged()?;
        drop(self.identity.take());
        let evidence = resume::prepare_pending(
            &self.records,
            self.store.clone(),
            &base,
            &record,
            &checkpoint.state,
            deadline,
        )?;
        live(deadline)?;
        lease
            .confirm_successor_window_adopted(&id, &digest, evidence)
            .map_err(err)?;
        Ok(())
    }

    #[inline(never)]
    fn open_active_driver(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<Driver, String> {
        let archive = self
            .archive
            .clone()
            .ok_or("active startup requires fixed private archive directory")?;
        let active = lease
            .verified_active_window()
            .map_err(err)?
            .ok_or("active checkpoint lacks verified capability")?;
        if active.provider_public_key() != self.provider_pin
            || active.binding_digest() != binding.digest().map_err(err)?
        {
            return Err(
                "authenticated active provider/binding differs from fixed configuration".into(),
            );
        }
        let record = active.original_request();
        let id = &record.request.intent.body.checkpoint.request_id;
        let digest = record
            .request
            .intent
            .body
            .checkpoint
            .digest()
            .map_err(err)?;
        let public = Box::new(
            lease
                .read_successor_original_state(id, &digest, deadline)
                .map_err(err)?,
        );
        let installed = Box::new(
            public
                .reconstruct_installed_pending(binding, &self.provider_pin, record)
                .map_err(err)?,
        );
        let verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
            self.anchor.clone(),
            self.target.clone(),
            archive,
        )?
        .with_installation_root(self.records.clone())?
        .with_active_private_store(self.store.clone())?;
        let base = verifier.recover_private_base(record, &public, deadline)?;
        if let Some(identity) = &self.identity {
            identity.verify_unchanged()?;
        }
        drop(self.identity.take());
        let window = Box::new(FormalBftHeaderActiveExecutionV1::open(
            &self.records,
            self.store.clone(),
            base,
            &active,
            binding,
            &self.provider_pin,
            &installed,
            deadline,
        )?);
        let mut driver = ActiveNodeDriver::new(window, binding)?;
        if self.membership_enabled {
            driver.enable_bft_v3_membership_execution()?;
        }
        Ok(Driver::Active(Box::new(driver)))
    }

    #[inline(never)]
    fn open_legacy_driver(
        &mut self,
        binding: &BftSignerBindingV3,
        deadline: Instant,
    ) -> Result<Driver, String> {
        // Only the authenticated genuinely legacy checkpoint reaches this frame.
        live(deadline)?;
        drop(self.identity.take());
        let mut driver = FormalBftHeaderNodeDriverV3::open_with_network_mode(
            &self.records,
            self.store.clone(),
            self.anchor.clone(),
            self.target.clone(),
            self.material.clone(),
            self.source.clone(),
            self.peer_network,
        )?;
        driver.check_binding(binding)?;
        if self.membership_enabled {
            driver.enable_bft_v3_membership_execution()?;
        }
        Ok(Driver::Legacy(Box::new(driver)))
    }
}
