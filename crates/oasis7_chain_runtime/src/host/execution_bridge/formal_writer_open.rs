//! Opening fixes the optional storage capture policy before any publication bytes are consumed.
use super::*;
impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    #[cfg(test)]
    pub(crate) fn open(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        world: World,
        anchor: VerifiedFormalReadAnchorV1,
        primary: P,
        replica: R,
    ) -> Result<Self, String> {
        Self::open_inner(
            root,
            writer,
            release,
            world,
            anchor,
            primary,
            replica,
            None,
            PublicationMaterialCaptureV1::Disabled,
        )
    }
    #[cfg(test)]
    #[expect(
        clippy::too_many_arguments,
        reason = "The initialization fixture supplies the complete signed activation and endpoint pair explicitly."
    )]
    pub(crate) fn open_initial(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        anchor: VerifiedFormalReadAnchorV1,
        candidate: FormalCandidateMaterialV1,
        proof: FormalQualifiedCommitProofV1,
        primary: P,
        replica: R,
    ) -> Result<Self, String> {
        Self::open_initial_with_material(
            root,
            writer,
            release,
            anchor,
            candidate,
            proof,
            primary,
            replica,
            PublicationMaterialCaptureV1::Disabled,
        )
    }
    #[cfg(test)]
    #[expect(
        clippy::too_many_arguments,
        reason = "Initial opening keeps the actual signed publication and explicit storage capture policy distinct."
    )]
    pub(crate) fn open_initial_with_material(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        anchor: VerifiedFormalReadAnchorV1,
        candidate: FormalCandidateMaterialV1,
        proof: FormalQualifiedCommitProofV1,
        primary: P,
        replica: R,
        capture_material: PublicationMaterialCaptureV1,
    ) -> Result<Self, String> {
        if anchor.head().record().kind != FormalRecordKindV1::InitialActivation
            || anchor.head().record().position != 1
            || &candidate.candidate.record != anchor.head().record()
            || proof.decision.record != candidate.candidate.record
        {
            return Err("formal initialization differs from externally verified activation".into());
        }
        let verified = verify_formal_qualified(&proof, anchor.authority(), None, None)
            .map_err(|e| e.to_string())?;
        if verified.head() != anchor.head().head() {
            return Err("formal initialization proof differs from fixed activation".into());
        }
        let world = restore(&candidate, &release)?;
        Self::open_inner(
            root,
            writer,
            release,
            world,
            anchor,
            primary,
            replica,
            Some(NativePublication {
                publication: Published {
                    schema_version: 1,
                    candidate,
                    proof,
                },
                pending: None,
                recovery: None,
            }),
            capture_material,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native opening keeps independent trust, two endpoint restores, authenticated reader and storage capture policy distinct."
    )]
    pub(crate) fn open_native_with_material(
        root: PathBuf,
        writer: SigningKey,
        release: ReleaseSecurityPolicy,
        trusted: VerifiedFormalReadAnchorV1,
        primary_restore: &VerifiedNativeCheckpointRestoreV1,
        replica_restore: &VerifiedNativeCheckpointRestoreV1,
        primary: P,
        replica: R,
        reader: std::sync::Arc<dyn business_index::BusinessIndexNodeReader + Send + Sync>,
        capture_material: PublicationMaterialCaptureV1,
    ) -> Result<Self, String> {
        Self::open_native_context(
            root,
            writer,
            release,
            trusted,
            None,
            primary_restore,
            replica_restore,
            primary,
            replica,
            reader,
            capture_material,
        )
    }
}
