use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactDeployed")]
struct ModuleArtifactDeployedPayload<'a> {
    publisher_agent_id: &'a String,
    wasm_hash: &'a String,
    bytes_len: &'a u64,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleInstalled")]
struct ModuleInstalledPayload<'a> {
    installer_agent_id: &'a String,
    instance_id: &'a String,
    module_id: &'a String,
    module_version: &'a String,
    wasm_hash: &'a String,
    install_target: &'a ModuleInstallTarget,
    active: &'a bool,
    proposal_id: &'a ProposalId,
    manifest_hash: &'a String,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleUpgraded")]
struct ModuleUpgradedPayload<'a> {
    upgrader_agent_id: &'a String,
    instance_id: &'a String,
    module_id: &'a String,
    from_module_version: &'a String,
    to_module_version: &'a String,
    wasm_hash: &'a String,
    install_target: &'a ModuleInstallTarget,
    active: &'a bool,
    proposal_id: &'a ProposalId,
    manifest_hash: &'a String,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseRequested")]
struct ModuleReleaseRequestedPayload<'a> {
    request_id: &'a u64,
    requester_agent_id: &'a String,
    manifest: &'a ModuleManifest,
    activate: &'a bool,
    install_target: &'a ModuleInstallTarget,
    required_roles: &'a Vec<String>,
    profile_changes: &'a ModuleProfileChanges,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseShadowed")]
struct ModuleReleaseShadowedPayload<'a> {
    request_id: &'a u64,
    operator_agent_id: &'a String,
    manifest_hash: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseAttested")]
struct ModuleReleaseAttestedPayload<'a> {
    request_id: &'a u64,
    operator_agent_id: &'a String,
    signer_node_id: &'a String,
    platform: &'a String,
    build_manifest_hash: &'a String,
    source_hash: &'a String,
    wasm_hash: &'a String,
    proof_cid: &'a String,
    builder_image_digest: &'a String,
    container_platform: &'a String,
    canonicalizer_version: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseRoleApproved")]
struct ModuleReleaseRoleApprovedPayload<'a> {
    request_id: &'a u64,
    approver_agent_id: &'a String,
    role: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseRolesBound")]
struct ModuleReleaseRolesBoundPayload<'a> {
    operator_agent_id: &'a String,
    target_agent_id: &'a String,
    roles: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseRejected")]
struct ModuleReleaseRejectedPayload<'a> {
    request_id: &'a u64,
    rejector_agent_id: &'a String,
    reason: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleReleaseApplied")]
struct ModuleReleaseAppliedPayload<'a> {
    request_id: &'a u64,
    operator_agent_id: &'a String,
    installer_agent_id: &'a String,
    instance_id: &'a String,
    module_id: &'a String,
    module_version: &'a String,
    proposal_id: &'a ProposalId,
    manifest_hash: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleRollbackApplied")]
struct ModuleRollbackAppliedPayload<'a> {
    operator_agent_id: &'a String,
    instance_id: &'a String,
    module_id: &'a String,
    from_module_version: &'a String,
    to_module_version: &'a String,
    wasm_hash: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_wasm_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_release_request_id: &'a Option<u64>,
    install_target: &'a ModuleInstallTarget,
    active: &'a bool,
    proposal_id: &'a ProposalId,
    manifest_hash: &'a String,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactListed")]
struct ModuleArtifactListedPayload<'a> {
    seller_agent_id: &'a String,
    wasm_hash: &'a String,
    price_kind: &'a ResourceKind,
    price_amount: &'a i64,
    order_id: &'a u64,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactDelisted")]
struct ModuleArtifactDelistedPayload<'a> {
    seller_agent_id: &'a String,
    wasm_hash: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: &'a Option<u64>,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactDestroyed")]
struct ModuleArtifactDestroyedPayload<'a> {
    owner_agent_id: &'a String,
    wasm_hash: &'a String,
    reason: &'a String,
    fee_kind: &'a ResourceKind,
    fee_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactBidPlaced")]
struct ModuleArtifactBidPlacedPayload<'a> {
    bidder_agent_id: &'a String,
    wasm_hash: &'a String,
    order_id: &'a u64,
    price_kind: &'a ResourceKind,
    price_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactBidCancelled")]
struct ModuleArtifactBidCancelledPayload<'a> {
    bidder_agent_id: &'a String,
    wasm_hash: &'a String,
    order_id: &'a u64,
    reason: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "ModuleArtifactSaleCompleted")]
struct ModuleArtifactSaleCompletedPayload<'a> {
    buyer_agent_id: &'a String,
    seller_agent_id: &'a String,
    wasm_hash: &'a String,
    price_kind: &'a ResourceKind,
    price_amount: &'a i64,
    sale_id: &'a u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    listing_order_id: &'a Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bid_order_id: &'a Option<u64>,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash,
            bytes_len,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactDeployed,
            &ModuleArtifactDeployedPayload {
                publisher_agent_id,
                wasm_hash,
                bytes_len,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleInstalled {
            installer_agent_id,
            instance_id,
            module_id,
            module_version,
            wasm_hash,
            install_target,
            active,
            proposal_id,
            manifest_hash,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleInstalled,
            &ModuleInstalledPayload {
                installer_agent_id,
                instance_id,
                module_id,
                module_version,
                wasm_hash,
                install_target,
                active,
                proposal_id,
                manifest_hash,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleUpgraded {
            upgrader_agent_id,
            instance_id,
            module_id,
            from_module_version,
            to_module_version,
            wasm_hash,
            install_target,
            active,
            proposal_id,
            manifest_hash,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleUpgraded,
            &ModuleUpgradedPayload {
                upgrader_agent_id,
                instance_id,
                module_id,
                from_module_version,
                to_module_version,
                wasm_hash,
                install_target,
                active,
                proposal_id,
                manifest_hash,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleReleaseRequested {
            request_id,
            requester_agent_id,
            manifest,
            activate,
            install_target,
            required_roles,
            profile_changes,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseRequested,
            &ModuleReleaseRequestedPayload {
                request_id,
                requester_agent_id,
                manifest,
                activate,
                install_target,
                required_roles,
                profile_changes,
            },
        ),
        DomainEvent::ModuleReleaseShadowed {
            request_id,
            operator_agent_id,
            manifest_hash,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseShadowed,
            &ModuleReleaseShadowedPayload {
                request_id,
                operator_agent_id,
                manifest_hash,
            },
        ),
        DomainEvent::ModuleReleaseAttested {
            request_id,
            operator_agent_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseAttested,
            &ModuleReleaseAttestedPayload {
                request_id,
                operator_agent_id,
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
        DomainEvent::ModuleReleaseRoleApproved {
            request_id,
            approver_agent_id,
            role,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseRoleApproved,
            &ModuleReleaseRoleApprovedPayload {
                request_id,
                approver_agent_id,
                role,
            },
        ),
        DomainEvent::ModuleReleaseRolesBound {
            operator_agent_id,
            target_agent_id,
            roles,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseRolesBound,
            &ModuleReleaseRolesBoundPayload {
                operator_agent_id,
                target_agent_id,
                roles,
            },
        ),
        DomainEvent::ModuleReleaseRejected {
            request_id,
            rejector_agent_id,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseRejected,
            &ModuleReleaseRejectedPayload {
                request_id,
                rejector_agent_id,
                reason,
            },
        ),
        DomainEvent::ModuleReleaseApplied {
            request_id,
            operator_agent_id,
            installer_agent_id,
            instance_id,
            module_id,
            module_version,
            proposal_id,
            manifest_hash,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleReleaseApplied,
            &ModuleReleaseAppliedPayload {
                request_id,
                operator_agent_id,
                installer_agent_id,
                instance_id,
                module_id,
                module_version,
                proposal_id,
                manifest_hash,
            },
        ),
        DomainEvent::ModuleRollbackApplied {
            operator_agent_id,
            instance_id,
            module_id,
            from_module_version,
            to_module_version,
            wasm_hash,
            from_wasm_hash,
            from_release_request_id,
            install_target,
            active,
            proposal_id,
            manifest_hash,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleRollbackApplied,
            &ModuleRollbackAppliedPayload {
                operator_agent_id,
                instance_id,
                module_id,
                from_module_version,
                to_module_version,
                wasm_hash,
                from_wasm_hash,
                from_release_request_id,
                install_target,
                active,
                proposal_id,
                manifest_hash,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleArtifactListed {
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
            order_id,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactListed,
            &ModuleArtifactListedPayload {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                order_id,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id,
            wasm_hash,
            order_id,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactDelisted,
            &ModuleArtifactDelistedPayload {
                seller_agent_id,
                wasm_hash,
                order_id,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id,
            wasm_hash,
            reason,
            fee_kind,
            fee_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactDestroyed,
            &ModuleArtifactDestroyedPayload {
                owner_agent_id,
                wasm_hash,
                reason,
                fee_kind,
                fee_amount,
            },
        ),
        DomainEvent::ModuleArtifactBidPlaced {
            bidder_agent_id,
            wasm_hash,
            order_id,
            price_kind,
            price_amount,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactBidPlaced,
            &ModuleArtifactBidPlacedPayload {
                bidder_agent_id,
                wasm_hash,
                order_id,
                price_kind,
                price_amount,
            },
        ),
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id,
            wasm_hash,
            order_id,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactBidCancelled,
            &ModuleArtifactBidCancelledPayload {
                bidder_agent_id,
                wasm_hash,
                order_id,
                reason,
            },
        ),
        DomainEvent::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
            sale_id,
            listing_order_id,
            bid_order_id,
        } => serialize_adjacent(
            serializer,
            Tag::ModuleArtifactSaleCompleted,
            &ModuleArtifactSaleCompletedPayload {
                buyer_agent_id,
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                sale_id,
                listing_order_id,
                bid_order_id,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
