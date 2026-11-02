//! Fixed physical inventory for the explicit pending-start profile only.
//! A closed inventory is accounting metadata, never a Current or send token.
use super::*;
use crate::execution_dispatch_gate_v2::{ExecutionReentryHopV2, reentry_hops};
use crate::native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartCurrentPhaseV2 {
    PreInspect,
    PreArchive,
    BeforePrivateRead,
    AfterPrivateRead,
    BeforeLifetime,
    Complete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartCurrentPortV2 {
    ManagerAssociationsActor,
    RecoveryFenceDispatcherActor,
    RecoveryFenceActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartObservationV2 {
    BeforePrivateRead,
    AfterPrivateRead,
    BeforeLifetime,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartObservationPortV2 {
    OwnerService,
    AccountActor,
    IdentityFenceDispatcherActor,
    IdentityFenceActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartCipherPortV2 {
    SidBefore,
    SidAfter,
    CsrfBefore,
    CsrfAfter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartPrefixActorV2 {
    CurrentFenceActor,
    PrivateManifestActor,
    RkvInitializeActor,
    RkvSignActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartIngressPortV2 {
    Host,
    AbuseProviderActor,
    AbuseRealmActor,
    OwnerService,
    AccountActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartRkvPortV2 {
    InitializeService,
    InitializeActor,
    SignService,
    SignActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartPrivatePortV2 {
    Service,
    ManifestActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartCompletionPortV2 {
    OwnerService,
    AccountActor,
    AttestationService,
    AttestationManifestActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartReentryHopV2 {
    OwnerService,
    AccountActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartBusinessLeafV2 {
    SourceVerifyService,
    SourceVerifyNative,
    LifetimeService,
    LifetimeManifestActor,
    Current {
        phase: StartCurrentPhaseV2,
        port: StartCurrentPortV2,
    },
    Observation {
        phase: StartObservationV2,
        port: StartObservationPortV2,
    },
    Cipher(StartCipherPortV2),
    NativeLifetime,
    Completion(StartCompletionPortV2),
    PrefixCurrent(StartCurrentPortV2),
    PrefixPrivate(StartPrivatePortV2),
    PrefixRkv(StartRkvPortV2),
    PrefixNative(ExecutionPreparePhysicalEdgeV2),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum StartGateTargetV2 {
    SourceVerifyNative,
    LifetimeManifestActor,
    CurrentFenceActor(StartCurrentPhaseV2),
    IdentityFenceDispatcherActor(StartObservationV2),
    IdentityFenceActor(StartObservationV2),
    Cipher(StartCipherPortV2),
    NativeLifetime,
    CompletionManifestActor,
    PrefixActor(StartPrefixActorV2),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum ExecutionPendingLeafV2 {
    Ingress(StartIngressPortV2),
    Business(StartBusinessLeafV2),
    NativeReentry {
        edge: ExecutionPreparePhysicalEdgeV2,
        hop: ExecutionReentryHopV2,
    },
    AccountReentry {
        target: StartGateTargetV2,
        hop: StartReentryHopV2,
    },
}

/// Exact list is built internally; a caller cannot supply a count or replace
/// a two-hop service/Actor subtree with a one-hop annotation.
pub(super) fn fixed_inventory() -> Result<Vec<ExecutionPendingLeafV2>> {
    use ExecutionPendingLeafV2::{AccountReentry, Business, Ingress, NativeReentry};
    use StartBusinessLeafV2 as B;
    use StartCurrentPortV2 as C;
    use StartGateTargetV2 as G;
    use StartIngressPortV2 as I;
    use StartObservationPortV2 as O;
    let mut leaves = vec![
        Ingress(I::Host),
        Ingress(I::AbuseProviderActor),
        Ingress(I::AbuseRealmActor),
        Ingress(I::OwnerService),
        Ingress(I::AccountActor),
    ];
    let currents = [
        StartCurrentPhaseV2::PreInspect,
        StartCurrentPhaseV2::PreArchive,
        StartCurrentPhaseV2::BeforePrivateRead,
        StartCurrentPhaseV2::AfterPrivateRead,
        StartCurrentPhaseV2::BeforeLifetime,
        StartCurrentPhaseV2::Complete,
    ];
    let observations = [
        StartObservationV2::BeforePrivateRead,
        StartObservationV2::AfterPrivateRead,
        StartObservationV2::BeforeLifetime,
    ];
    let ciphers = [
        StartCipherPortV2::SidBefore,
        StartCipherPortV2::SidAfter,
        StartCipherPortV2::CsrfBefore,
        StartCipherPortV2::CsrfAfter,
    ];
    for port in [
        C::ManagerAssociationsActor,
        C::RecoveryFenceDispatcherActor,
        C::RecoveryFenceActor,
    ] {
        leaves.push(Business(B::PrefixCurrent(port)));
    }
    for port in [
        StartPrivatePortV2::Service,
        StartPrivatePortV2::ManifestActor,
    ] {
        leaves.push(Business(B::PrefixPrivate(port)));
    }
    for port in [
        StartRkvPortV2::InitializeService,
        StartRkvPortV2::InitializeActor,
        StartRkvPortV2::SignService,
        StartRkvPortV2::SignActor,
    ] {
        leaves.push(Business(B::PrefixRkv(port)));
    }
    // The existing closed Native10 and its24 callbacks keep their own exact
    // leaf descriptions; this profile references them rather than inventing a batch.
    for edge in crate::native_execution_prepare_v2::execution_prepare_physical_edges_v2() {
        leaves.push(Business(B::PrefixNative(edge)));
        for hop in reentry_hops(edge) {
            leaves.push(NativeReentry { edge, hop: *hop });
        }
    }
    for phase in currents {
        for port in [
            C::ManagerAssociationsActor,
            C::RecoveryFenceDispatcherActor,
            C::RecoveryFenceActor,
        ] {
            leaves.push(Business(B::Current { phase, port }));
        }
    }
    leaves.extend([
        Business(B::SourceVerifyService),
        Business(B::SourceVerifyNative),
        Business(B::LifetimeService),
        Business(B::LifetimeManifestActor),
    ]);
    for phase in observations {
        for port in [
            O::OwnerService,
            O::AccountActor,
            O::IdentityFenceDispatcherActor,
            O::IdentityFenceActor,
        ] {
            leaves.push(Business(B::Observation { phase, port }));
        }
    }
    for port in ciphers {
        leaves.push(Business(B::Cipher(port)));
    }
    leaves.push(Business(B::NativeLifetime));
    for port in [
        StartCompletionPortV2::OwnerService,
        StartCompletionPortV2::AccountActor,
        StartCompletionPortV2::AttestationService,
        StartCompletionPortV2::AttestationManifestActor,
    ] {
        leaves.push(Business(B::Completion(port)));
    }
    let mut targets = vec![
        G::SourceVerifyNative,
        G::LifetimeManifestActor,
        G::NativeLifetime,
        G::CompletionManifestActor,
    ];
    targets.extend(currents.into_iter().map(G::CurrentFenceActor));
    for phase in observations {
        targets.extend([
            G::IdentityFenceDispatcherActor(phase),
            G::IdentityFenceActor(phase),
        ]);
    }
    targets.extend(ciphers.into_iter().map(G::Cipher));
    for port in [
        StartPrefixActorV2::CurrentFenceActor,
        StartPrefixActorV2::PrivateManifestActor,
        StartPrefixActorV2::RkvInitializeActor,
        StartPrefixActorV2::RkvSignActor,
    ] {
        targets.push(G::PrefixActor(port));
    }
    if targets.len() != 24 {
        return Err(invalid());
    }
    for target in targets {
        for hop in [
            StartReentryHopV2::OwnerService,
            StartReentryHopV2::AccountActor,
        ] {
            leaves.push(AccountReentry { target, hop });
        }
    }
    // Domain-separated canonical representations detect accidental duplicate
    // enum entries without accepting any caller-provided inventory or amount.
    let mut seen = std::collections::BTreeSet::new();
    for leaf in &leaves {
        if !seen.insert(digest(&("oasis7.custody.execution-pending-leaf.v2", leaf))?) {
            return Err(invalid());
        }
    }
    if leaves.len() != 139 {
        return Err(invalid());
    }
    Ok(leaves)
}
