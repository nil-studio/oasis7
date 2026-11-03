//! Exact Active input adapter; the store owns the shared protected transaction.
use super::*;
impl ActiveNodeDriver {
    #[expect(
        clippy::too_many_arguments,
        reason = "Original operation inputs remain explicit."
    )]
    pub(super) fn retain_active_protected(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
        local: bool,
        received: Option<&[(String, Vec<u8>)]>,
        original_read: Option<
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1,
        >,
        deadline: Instant,
    ) -> Result<String, String> {
        self.retain_active_protected_inner(
            binding,
            lease,
            context,
            candidate,
            local,
            received,
            original_read,
            deadline,
            false,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Only authenticated startup invokes the internal original parent phase."
    )]
    fn retain_active_protected_inner(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
        local: bool,
        received: Option<&[(String, Vec<u8>)]>,
        original_read: Option<
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1,
        >,
        deadline: Instant,
        startup: bool,
    ) -> Result<String, String> {
        self.check_binding(binding)?;
        let carry = self
            .window
            .base
            .pending
            .iter()
            .any(|(_, package)| package.candidate == *candidate);
        if carry {
            // An inherited original remains the exact existing alias. Its
            // provider-side signing check still proves its original purpose.
            self.preview_at(context, candidate, deadline)?;
            let package = self.window.private.export(&candidate.public_value)?;
            if received.is_some_and(|nodes| nodes != package.dependency_nodes.as_slice()) {
                return Err("active inherited original dependency inventory changed".into());
            }
            let nodes = package.dependency_nodes.into_iter().collect();
            return if local {
                self.window
                    .private
                    .retain(candidate, &nodes, &mut self.window.ledger)
            } else {
                self.window
                    .private
                    .retain_received(candidate, &nodes, &mut self.window.ledger)
            };
        }
        self.window.base.require_fresh_active_capability()?;
        if startup {
            if self.configured
                || context
                    != &NodeBftExecutionContextV3::from_schedule(
                        &lease.current_verified_schedule().map_err(err)?,
                    )
            {
                return Err(
                    "Active original startup parent differs from current verified lease".into(),
                );
            }
            let parent = context
                .height
                .checked_sub(1)
                .ok_or("Active startup parent underflow")?;
            self.window.reset_through(Some(parent), deadline)?;
            if context != &NodeBftExecutionContextV3::from_schedule(self.window.schedule()) {
                return Err("Active original startup actual parent differs".into());
            }
            self.window.execution = self.window.execution.fork_live_closed()?;
            let actual = self
                .window
                .execution
                .build_candidate(&candidate.actions_cbor)?;
            if actual.header != candidate.public_value.header
                || actual.commitment != candidate.public_value.execution_commitment
                || (!self.membership
                    && (actual.commitment.transition.is_some()
                        || actual.commitment.transition_ref.is_some()))
            {
                return Err("Active original startup candidate did not reproduce".into());
            }
        } else {
            self.preview_at(context, candidate, deadline)?;
        }
        let observation = lease.observe_private_retention(deadline).map_err(err)?;
        let role = self.window.base.original_retention_attestor()?;
        if observation.checkpoint().provider_public_key != role.provider_public_key
            || observation.checkpoint().state.binding_digest != self.binding_digest
            || observation.ledger().map.source != role.source
            || observation.ledger().receipt.body.head.capability_digest
                != role.digest().map_err(err)?
        {
            return Err("Active observation differs from fixed original role/provider".into());
        }
        let executed = Box::new(self.window.execution.execute(&candidate.actions_cbor)?);
        let reader = self.window.execution.retained_nodes.clone();
        let io = self
            .admission_input_io
            .as_ref()
            .ok_or("Active original input IO not configured")?
            .clone();
        reader.with_accessed_nodes(|nodes| {
            if let Some(supplied) = received {
                if supplied.len() != nodes.len()
                    || supplied
                        .iter()
                        .any(|(key, bytes)| nodes.get(key) != Some(bytes))
                {
                    return Err(
                        "active received dependencies did not independently reproduce".into(),
                    );
                }
            }
            let prefix = self
                .window
                .ledger
                .records()
                .filter(|record| record.height() < context.height)
                .map(|record| record.input().certificate)
                .collect::<Vec<_>>();
            let plan =
                super::super::super::super::publication_plan::PublicationPlanV1::from_actual(
                    self.window.schedule(),
                    &executed,
                    &candidate.actions_cbor,
                    nodes,
                    &prefix,
                    super::super::super::window_storage::RECORD_DOMAIN,
                )?;
            let window = &mut self.window;
            let base = &window.base;
            let carriers = &window.carriers;
            let installed = &window.installed;
            let pins = &window.directory_pins;
            let protected_origin = &window.protected_origin;
            let segment = &window.segment;
            let mut verify_owner = || {
                live(deadline)?;
                pins.verify()?;
                let original = base
                    .original
                    .as_ref()
                    .ok_or("Active original request absent")?;
                let immutable = installed
                    .successor_window
                    .as_ref()
                    .and_then(|state| {
                        state.request(&original.request.intent.body.checkpoint.request_id)
                    })
                    .ok_or("Active installed original absent")?;
                carriers.read_installed(&base.origin, immutable, installed, deadline)?;
                if carriers.read_window_origin(base, deadline)?.1 != *protected_origin {
                    return Err("Active original Origin3 changed during admission".into());
                }
                let bytes =
                    common_storage::read_optional_bytes(&segment.join("base.cbor"), MAX_METADATA)?
                        .ok_or("Active private base absent")?;
                if bytes != base_identity(base)? {
                    return Err("Active private base changed during admission".into());
                }
                pins.verify()
            };
            window.private.retain_protected(
                base,
                binding,
                lease,
                candidate,
                &plan,
                nodes,
                local,
                &mut window.ledger,
                &observation,
                io.as_ref(),
                original_read,
                deadline,
                &mut verify_owner,
            )
        })
    }
}

impl ActiveNodeDriver {
    pub(super) fn prepare_active_originals(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<(), String> {
        use oasis7_distfs::protected_bft_v3::{
            BftPrivateRetentionAdmissionInputRefV1 as Ref, BftPrivateRetentionLifecycleV1 as Life,
        };
        if lease
            .checkpoint()
            .map_err(err)?
            .state
            .private_retention
            .is_none()
        {
            return Ok(());
        }
        let observation = lease.observe_private_retention(deadline).map_err(err)?;
        let role = self.window.base.original_retention_attestor()?;
        if observation.checkpoint().provider_public_key != role.provider_public_key
            || observation.ledger().map.source != role.source
            || observation.ledger().receipt.body.head.capability_digest
                != role.digest().map_err(err)?
        {
            return Err(
                "Active startup current observation differs from fixed original role".into(),
            );
        }
        let namespace = self.window.ledger.origin_digest()?;
        let (_, inherited) = self.window.base.verified_c0_retention_baseline()?;
        let schedule = lease.current_verified_schedule().map_err(err)?;
        for entry in &observation.ledger().map.entries {
            let reference =
                Ref::from_entry(&observation.ledger().map.source, entry).map_err(err)?;
            if entry.key.ledger_origin_digest != namespace {
                if !inherited.contains(&reference) {
                    return Err("Active startup contains unknown inherited original".into());
                }
                continue;
            }
            let selected = observation
                .ledger()
                .selection_index
                .as_ref()
                .is_some_and(|index| {
                    index
                        .selections
                        .iter()
                        .any(|choice| choice.key == entry.key && choice.reference == reference)
                });
            let (candidate, read, local) = if entry.lifecycle == Life::Ready {
                let candidate = self
                    .window
                    .private
                    .package_for_header(entry.key.height, &entry.key.header_id)?
                    .candidate;
                (candidate, None, selected)
            } else {
                if entry.key.height != schedule.height() {
                    return Err("Active unresolved original differs from current parent".into());
                }
                let read = self
                    .admission_input_io
                    .as_ref()
                    .ok_or("Active startup original input IO absent")?
                    .read_original(&reference, deadline)?;
                reference.validate_input(read.input()).map_err(err)?;
                let candidate = node_execution::active_admission_candidate(read.input())?;
                let local = node_execution::WindowPrivateStore::original_is_local(read.input())?;
                (candidate, Some(read), local)
            };
            self.cursor = entry
                .key
                .height
                .checked_sub(1)
                .ok_or("Active original parent underflow")?;
            let context = NodeBftExecutionContextV3::from_schedule(&schedule);
            // Historical Ready slots are verified physically when loading the
            // inventory; only the exact live parent can resume execution.
            if entry.key.height != schedule.height() {
                self.window
                    .private
                    .verify_ready_original(&candidate, &reference, &entry.fact)?;
                continue;
            }
            self.retain_active_protected_inner(
                binding, lease, &context, &candidate, local, None, read, deadline, true,
            )?;
        }
        Ok(())
    }
}
