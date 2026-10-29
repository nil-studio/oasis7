// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::DeployModuleArtifact {
            publisher_agent_id,
            wasm_hash,
            wasm_bytes,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DeployModuleArtifact,
            payloads::DeployModuleArtifactPayload {
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
            },
        ),
        Action::CompileModuleArtifactFromSource {
            publisher_agent_id,
            module_id,
            source_package,
        } => serialize_action_envelope(
            serializer,
            ActionTag::CompileModuleArtifactFromSource,
            payloads::CompileModuleArtifactFromSourcePayload {
                publisher_agent_id,
                module_id,
                source_package,
            },
        ),
        Action::InstallModuleFromArtifact {
            installer_agent_id,
            manifest,
            activate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::InstallModuleFromArtifact,
            payloads::InstallModuleFromArtifactPayload {
                installer_agent_id,
                manifest,
                activate,
            },
        ),
        Action::InstallModuleFromArtifactWithFinality {
            installer_agent_id,
            manifest,
            activate,
            finality_certificate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::InstallModuleFromArtifactWithFinality,
            payloads::InstallModuleFromArtifactWithFinalityPayload {
                installer_agent_id,
                manifest,
                activate,
                finality_certificate,
            },
        ),
        Action::InstallModuleToTargetFromArtifact {
            installer_agent_id,
            manifest,
            activate,
            install_target,
        } => serialize_action_envelope(
            serializer,
            ActionTag::InstallModuleToTargetFromArtifact,
            payloads::InstallModuleToTargetFromArtifactPayload {
                installer_agent_id,
                manifest,
                activate,
                install_target,
            },
        ),
        Action::InstallModuleToTargetFromArtifactWithFinality {
            installer_agent_id,
            manifest,
            activate,
            install_target,
            finality_certificate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::InstallModuleToTargetFromArtifactWithFinality,
            payloads::InstallModuleToTargetFromArtifactWithFinalityPayload {
                installer_agent_id,
                manifest,
                activate,
                install_target,
                finality_certificate,
            },
        ),
        Action::UpgradeModuleFromArtifact {
            upgrader_agent_id,
            instance_id,
            from_module_version,
            manifest,
            activate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::UpgradeModuleFromArtifact,
            payloads::UpgradeModuleFromArtifactPayload {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
            },
        ),
        Action::UpgradeModuleFromArtifactWithFinality {
            upgrader_agent_id,
            instance_id,
            from_module_version,
            manifest,
            activate,
            finality_certificate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::UpgradeModuleFromArtifactWithFinality,
            payloads::UpgradeModuleFromArtifactWithFinalityPayload {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
                finality_certificate,
            },
        ),
        Action::ModuleReleaseSubmit {
            requester_agent_id,
            manifest,
            activate,
            install_target,
            required_roles,
            profile_changes,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseSubmit,
            payloads::ModuleReleaseSubmitPayload {
                requester_agent_id,
                manifest,
                activate,
                install_target,
                required_roles,
                profile_changes,
            },
        ),
        Action::ModuleReleaseShadow {
            operator_agent_id,
            request_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseShadow,
            payloads::ModuleReleaseShadowPayload {
                operator_agent_id,
                request_id,
            },
        ),
        Action::ModuleReleaseSubmitAttestation {
            operator_agent_id,
            request_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseSubmitAttestation,
            payloads::ModuleReleaseSubmitAttestationPayload {
                operator_agent_id,
                request_id,
                signer_node_id,
                platform,
                build_manifest_hash,
                source_hash,
                wasm_hash,
                proof_cid,
                builder_image_digest,
                container_platform,
                canonicalizer_version,
            },
        ),
        Action::ModuleReleaseApproveRole {
            approver_agent_id,
            request_id,
            role,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseApproveRole,
            payloads::ModuleReleaseApproveRolePayload {
                approver_agent_id,
                request_id,
                role,
            },
        ),
        Action::ModuleReleaseBindRoles {
            operator_agent_id,
            target_agent_id,
            roles,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseBindRoles,
            payloads::ModuleReleaseBindRolesPayload {
                operator_agent_id,
                target_agent_id,
                roles,
            },
        ),
        Action::ModuleReleaseReject {
            rejector_agent_id,
            request_id,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseReject,
            payloads::ModuleReleaseRejectPayload {
                rejector_agent_id,
                request_id,
                reason,
            },
        ),
        Action::ModuleReleaseApply {
            operator_agent_id,
            request_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseApply,
            payloads::ModuleReleaseApplyPayload {
                operator_agent_id,
                request_id,
            },
        ),
        Action::ModuleReleaseApplyWithFinality {
            operator_agent_id,
            request_id,
            finality_certificate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ModuleReleaseApplyWithFinality,
            payloads::ModuleReleaseApplyWithFinalityPayload {
                operator_agent_id,
                request_id,
                finality_certificate,
            },
        ),
        Action::RollbackModuleInstance {
            operator_agent_id,
            instance_id,
            target_module_version,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RollbackModuleInstance,
            payloads::RollbackModuleInstancePayload {
                operator_agent_id,
                instance_id,
                target_module_version,
            },
        ),
        Action::RollbackModuleInstanceWithFinality {
            operator_agent_id,
            instance_id,
            target_module_version,
            finality_certificate,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RollbackModuleInstanceWithFinality,
            payloads::RollbackModuleInstanceWithFinalityPayload {
                operator_agent_id,
                instance_id,
                target_module_version,
                finality_certificate,
            },
        ),
        Action::ListModuleArtifactForSale {
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::ListModuleArtifactForSale,
            payloads::ListModuleArtifactForSalePayload {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            },
        ),
        Action::BuyModuleArtifact {
            buyer_agent_id,
            wasm_hash,
        } => serialize_action_envelope(
            serializer,
            ActionTag::BuyModuleArtifact,
            payloads::BuyModuleArtifactPayload {
                buyer_agent_id,
                wasm_hash,
            },
        ),
        Action::DelistModuleArtifact {
            seller_agent_id,
            wasm_hash,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DelistModuleArtifact,
            payloads::DelistModuleArtifactPayload {
                seller_agent_id,
                wasm_hash,
            },
        ),
        Action::DestroyModuleArtifact {
            owner_agent_id,
            wasm_hash,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::DestroyModuleArtifact,
            payloads::DestroyModuleArtifactPayload {
                owner_agent_id,
                wasm_hash,
                reason,
            },
        ),
        Action::PlaceModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::PlaceModuleArtifactBid,
            payloads::PlaceModuleArtifactBidPayload {
                bidder_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            },
        ),
        Action::CancelModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            bid_order_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::CancelModuleArtifactBid,
            payloads::CancelModuleArtifactBidPayload {
                bidder_agent_id,
                wasm_hash,
                bid_order_id,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong module_artifacts serializer",
        )),
    }
}
