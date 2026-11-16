//! Trusted startup adapter. Mode is selected from a genuine full GEN capture,
//! or an explicit verified Host configuration and the actual held parent source.
//! Missing local configuration is always an error in the local branch.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    NormalGenesisProcessSourceV1, NormalProcessStartupSourceV1,
    VerifiedNormalColdProcessConfigurationV1,
};

#[cfg(feature = "node-libp2p")]
pub(crate) fn devlocal_genesis_process_input_v1<'a>(
    capture: &'a ReservedValue<NormalGenesisProcessSourceV1>,
    control: &'a crate::execution_config::DevLocalParticipantConfig,
    actual_authority: &oasis7_formal_protocol::controlled_authority::formal::VerifiedAuthorityV1,
    actual_head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<NormalProcessStartupSourceV1<'a>, IwsHostRuntimeErrorV1> {
    let authority = control.authority.as_ref().ok_or_else(|| {
        FormalError::Invalid(
            "GEN requires explicit independently pinned local Host configuration".into(),
        )
    })?;
    let stamp = control.verified_iws_local_process.as_ref().ok_or_else(|| {
        FormalError::Invalid("GEN actual CLI local process configuration missing".into())
    })?;
    stamp
        .validate_actual_restore_v1(control, authority, actual_authority, actual_head, common)
        .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
    // Runtime's private binder must additionally match this actual pin/policy
    // to the source emitted during the same borrowed full World GEN producer.
    Ok(NormalProcessStartupSourceV1::GenesisDevLocal {
        genesis: capture,
        authority,
    })
}

impl IwsHostRuntimeV1 {
    /// Explicit governed cold branch. The actual retained source supplies the
    /// verified activation lineage; no optional local field is inspected.
    pub fn prepare_governed_cold_process_configuration_bounded(
        &self,
        policy: &oasis7_runtime::ReleaseSecurityPolicy,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, IwsHostRuntimeErrorV1>
    {
        let current = self.current.current().ok_or_else(|| {
            FormalError::Invalid("cold process source requires actual installed parent".into())
        })?;
        current.source.with_parent_read_view_bounded(
            memory,
            work,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| {
                oasis7_runtime::prepare_verified_normal_cold_governed_configuration_v1(
                    parent.authority(),
                    policy,
                    &mut work.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                )
                .map_err(IwsHostRuntimeErrorV1::NormalSource)
            },
        )
    }
    /// Actual private Node Host configuration carries the independently checked
    /// existing-origin identity. Signed startup policy is verified again under
    /// the continuing formal Core before Runtime mints its opaque cold owner.
    #[cfg(feature = "node-libp2p")]
    pub(crate) fn prepare_devlocal_cold_process_configuration_bounded(
        &self,
        control: &crate::execution_config::DevLocalParticipantConfig,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, IwsHostRuntimeErrorV1>
    {
        let current = self.current.current().ok_or_else(|| {
            FormalError::Invalid("cold local process actual parent missing".into())
        })?;
        current.source.with_parent_read_view_bounded(
            memory,
            work,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| {
                verified_local_config_for_restore(
                    control,
                    parent.authority(),
                    parent.head(),
                    &mut work.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                    canonical,
                )
            },
        )
    }
}

/// Explicit restore inputs come from the actual archive/authority verifier.
/// This constructor does not require a live CheckedCore and cannot mint one.
pub(crate) fn prepare_governed_cold_process_from_verified_restore_v1(
    authority: &oasis7_formal_protocol::controlled_authority::formal::VerifiedAuthorityV1,
    head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    policy: &oasis7_runtime::ReleaseSecurityPolicy,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, IwsHostRuntimeErrorV1> {
    common.charge(
        1,
        (head.record().world.world_id.len()
            + head.record().world.genesis_digest.len()
            + head.record().world.chain_id.len()
            + authority.definition().world.world_id.len()
            + authority.definition().world.genesis_digest.len()
            + authority.definition().world.chain_id.len()) as u64,
    )?;
    if authority.definition().world != head.record().world {
        return Err(
            FormalError::Invalid("cold process actual verified restore identity".into()).into(),
        );
    }
    oasis7_runtime::prepare_verified_normal_cold_governed_configuration_v1(
        authority, policy, common,
    )
    .map_err(IwsHostRuntimeErrorV1::NormalSource)
}
#[cfg(feature = "node-libp2p")]
pub(crate) fn prepare_devlocal_cold_process_from_verified_restore_v1(
    control: &crate::execution_config::DevLocalParticipantConfig,
    authority: &oasis7_formal_protocol::controlled_authority::formal::VerifiedAuthorityV1,
    head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, IwsHostRuntimeErrorV1> {
    verified_local_config_for_restore(control, authority, head, common, canonical)
}
#[cfg(feature = "node-libp2p")]
fn verified_local_config_for_restore(
    control: &crate::execution_config::DevLocalParticipantConfig,
    authority: &oasis7_formal_protocol::controlled_authority::formal::VerifiedAuthorityV1,
    head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, IwsHostRuntimeErrorV1> {
    let local = control.authority.as_ref().ok_or_else(|| {
        FormalError::Invalid("cold local independent Host authority missing".into())
    })?;
    let stamp = control.verified_iws_local_process.as_ref().ok_or_else(|| {
        FormalError::Invalid("actual CLI-verified local process configuration missing".into())
    })?;
    stamp
        .validate_actual_restore_v1(control, local, authority, head, common)
        .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
    if let Some(origin) = control.verified_origin.as_ref() {
        if control.startup.is_none() {
            return Err(FormalError::Invalid(
                "declared local origin requires its signed startup policy".into(),
            )
            .into());
        }
        common.charge(
            1,
            (origin.identity().world_id.len()
                + origin.identity().genesis_digest.len()
                + head.record().world.world_id.len()
                + head.record().world.genesis_digest.len()) as u64,
        )?;
        if origin.identity().world_id != head.record().world.world_id
            || origin.identity().genesis_digest != head.record().world.genesis_digest
        {
            return Err(FormalError::Invalid(
                "cold local declared origin differs from actual verified restore".into(),
            )
            .into());
        }
    }
    match control.startup.as_ref() {
        Some(signed) => oasis7_runtime::prepare_verified_normal_cold_devlocal_configuration_v1(
            local, signed, authority, common, canonical,
        )
        .map_err(IwsHostRuntimeErrorV1::NormalSource),
        None => oasis7_runtime::prepare_verified_normal_cold_devlocal_pinned_configuration_v1(
            local, authority, common,
        )
        .map_err(IwsHostRuntimeErrorV1::NormalSource),
    }
}
