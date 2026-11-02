//! Prepaid-port vocabulary only. None of these values is Current or an RPC proof.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReadCurrentPortV3 {
    #[serde(rename = "claim_source.callback.owner")]
    ClaimSourceCallbackOwner,
    #[serde(rename = "claim_source.callback.account")]
    ClaimSourceCallbackAccount,
    #[serde(rename = "claim_source.directory")]
    ClaimSourceDirectory,
    #[serde(rename = "claim_source.rf.dispatcher")]
    ClaimSourceRfDispatcher,
    #[serde(rename = "claim_source.rf.causal_owner")]
    ClaimSourceRfCausalOwner,
    #[serde(rename = "claim_source.rf.causal_account")]
    ClaimSourceRfCausalAccount,
    #[serde(rename = "claim_source.rf.fences")]
    ClaimSourceRfFences,
    #[serde(rename = "source_record_and_predecrypt.callback.owner")]
    SourceRecordAndPredecryptCallbackOwner,
    #[serde(rename = "source_record_and_predecrypt.callback.account")]
    SourceRecordAndPredecryptCallbackAccount,
    #[serde(rename = "source_record_and_predecrypt.directory")]
    SourceRecordAndPredecryptDirectory,
    #[serde(rename = "source_record_and_predecrypt.rf.dispatcher")]
    SourceRecordAndPredecryptRfDispatcher,
    #[serde(rename = "source_record_and_predecrypt.rf.causal_owner")]
    SourceRecordAndPredecryptRfCausalOwner,
    #[serde(rename = "source_record_and_predecrypt.rf.causal_account")]
    SourceRecordAndPredecryptRfCausalAccount,
    #[serde(rename = "source_record_and_predecrypt.rf.fences")]
    SourceRecordAndPredecryptRfFences,
    #[serde(rename = "postdecrypt_and_claim_lifetime.callback.owner")]
    PostdecryptAndClaimLifetimeCallbackOwner,
    #[serde(rename = "postdecrypt_and_claim_lifetime.callback.account")]
    PostdecryptAndClaimLifetimeCallbackAccount,
    #[serde(rename = "postdecrypt_and_claim_lifetime.directory")]
    PostdecryptAndClaimLifetimeDirectory,
    #[serde(rename = "postdecrypt_and_claim_lifetime.rf.dispatcher")]
    PostdecryptAndClaimLifetimeRfDispatcher,
    #[serde(rename = "postdecrypt_and_claim_lifetime.rf.causal_owner")]
    PostdecryptAndClaimLifetimeRfCausalOwner,
    #[serde(rename = "postdecrypt_and_claim_lifetime.rf.causal_account")]
    PostdecryptAndClaimLifetimeRfCausalAccount,
    #[serde(rename = "postdecrypt_and_claim_lifetime.rf.fences")]
    PostdecryptAndClaimLifetimeRfFences,
    #[serde(rename = "final_admit.callback.owner")]
    FinalAdmitCallbackOwner,
    #[serde(rename = "final_admit.callback.account")]
    FinalAdmitCallbackAccount,
    #[serde(rename = "final_admit.directory")]
    FinalAdmitDirectory,
    #[serde(rename = "final_admit.rf.dispatcher")]
    FinalAdmitRfDispatcher,
    #[serde(rename = "final_admit.rf.causal_owner")]
    FinalAdmitRfCausalOwner,
    #[serde(rename = "final_admit.rf.causal_account")]
    FinalAdmitRfCausalAccount,
    #[serde(rename = "final_admit.rf.fences")]
    FinalAdmitRfFences,
    #[serde(rename = "before_decrypt.identity_rf.auth_owner")]
    BeforeDecryptIdentityRfAuthOwner,
    #[serde(rename = "before_decrypt.identity_rf.auth_account")]
    BeforeDecryptIdentityRfAuthAccount,
    #[serde(rename = "before_decrypt.identity_rf.dispatcher")]
    BeforeDecryptIdentityRfDispatcher,
    #[serde(rename = "before_decrypt.identity_rf.causal_owner")]
    BeforeDecryptIdentityRfCausalOwner,
    #[serde(rename = "before_decrypt.identity_rf.causal_account")]
    BeforeDecryptIdentityRfCausalAccount,
    #[serde(rename = "before_decrypt.identity_rf.fences")]
    BeforeDecryptIdentityRfFences,
    #[serde(rename = "after_decrypt.identity_rf.auth_owner")]
    AfterDecryptIdentityRfAuthOwner,
    #[serde(rename = "after_decrypt.identity_rf.auth_account")]
    AfterDecryptIdentityRfAuthAccount,
    #[serde(rename = "after_decrypt.identity_rf.dispatcher")]
    AfterDecryptIdentityRfDispatcher,
    #[serde(rename = "after_decrypt.identity_rf.causal_owner")]
    AfterDecryptIdentityRfCausalOwner,
    #[serde(rename = "after_decrypt.identity_rf.causal_account")]
    AfterDecryptIdentityRfCausalAccount,
    #[serde(rename = "after_decrypt.identity_rf.fences")]
    AfterDecryptIdentityRfFences,
    #[serde(rename = "before_native_lifetime.identity_rf.auth_owner")]
    BeforeNativeLifetimeIdentityRfAuthOwner,
    #[serde(rename = "before_native_lifetime.identity_rf.auth_account")]
    BeforeNativeLifetimeIdentityRfAuthAccount,
    #[serde(rename = "before_native_lifetime.identity_rf.dispatcher")]
    BeforeNativeLifetimeIdentityRfDispatcher,
    #[serde(rename = "before_native_lifetime.identity_rf.causal_owner")]
    BeforeNativeLifetimeIdentityRfCausalOwner,
    #[serde(rename = "before_native_lifetime.identity_rf.causal_account")]
    BeforeNativeLifetimeIdentityRfCausalAccount,
    #[serde(rename = "before_native_lifetime.identity_rf.fences")]
    BeforeNativeLifetimeIdentityRfFences,
    #[serde(rename = "private.manifest_actor")]
    PrivateManifestActor,
    #[serde(rename = "final.private_attestation.service")]
    FinalPrivateAttestationService,
    #[serde(rename = "final.private_attestation.manifest")]
    FinalPrivateAttestationManifest,
    #[serde(rename = "private.cipher.sid_before")]
    PrivateCipherSidBefore,
    #[serde(rename = "private.cipher.sid_after")]
    PrivateCipherSidAfter,
    #[serde(rename = "private.cipher.csrf_before")]
    PrivateCipherCsrfBefore,
    #[serde(rename = "private.cipher.csrf_after")]
    PrivateCipherCsrfAfter,
    #[serde(rename = "native.source")]
    NativeSource,
    #[serde(rename = "native.lifetime")]
    NativeLifetime,
}
impl ReadCurrentPortV3 {
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaimSourceCallbackOwner => "claim_source.callback.owner",
            Self::ClaimSourceCallbackAccount => "claim_source.callback.account",
            Self::ClaimSourceDirectory => "claim_source.directory",
            Self::ClaimSourceRfDispatcher => "claim_source.rf.dispatcher",
            Self::ClaimSourceRfCausalOwner => "claim_source.rf.causal_owner",
            Self::ClaimSourceRfCausalAccount => "claim_source.rf.causal_account",
            Self::ClaimSourceRfFences => "claim_source.rf.fences",
            Self::SourceRecordAndPredecryptCallbackOwner => {
                "source_record_and_predecrypt.callback.owner"
            }
            Self::SourceRecordAndPredecryptCallbackAccount => {
                "source_record_and_predecrypt.callback.account"
            }
            Self::SourceRecordAndPredecryptDirectory => "source_record_and_predecrypt.directory",
            Self::SourceRecordAndPredecryptRfDispatcher => {
                "source_record_and_predecrypt.rf.dispatcher"
            }
            Self::SourceRecordAndPredecryptRfCausalOwner => {
                "source_record_and_predecrypt.rf.causal_owner"
            }
            Self::SourceRecordAndPredecryptRfCausalAccount => {
                "source_record_and_predecrypt.rf.causal_account"
            }
            Self::SourceRecordAndPredecryptRfFences => "source_record_and_predecrypt.rf.fences",
            Self::PostdecryptAndClaimLifetimeCallbackOwner => {
                "postdecrypt_and_claim_lifetime.callback.owner"
            }
            Self::PostdecryptAndClaimLifetimeCallbackAccount => {
                "postdecrypt_and_claim_lifetime.callback.account"
            }
            Self::PostdecryptAndClaimLifetimeDirectory => {
                "postdecrypt_and_claim_lifetime.directory"
            }
            Self::PostdecryptAndClaimLifetimeRfDispatcher => {
                "postdecrypt_and_claim_lifetime.rf.dispatcher"
            }
            Self::PostdecryptAndClaimLifetimeRfCausalOwner => {
                "postdecrypt_and_claim_lifetime.rf.causal_owner"
            }
            Self::PostdecryptAndClaimLifetimeRfCausalAccount => {
                "postdecrypt_and_claim_lifetime.rf.causal_account"
            }
            Self::PostdecryptAndClaimLifetimeRfFences => "postdecrypt_and_claim_lifetime.rf.fences",
            Self::FinalAdmitCallbackOwner => "final_admit.callback.owner",
            Self::FinalAdmitCallbackAccount => "final_admit.callback.account",
            Self::FinalAdmitDirectory => "final_admit.directory",
            Self::FinalAdmitRfDispatcher => "final_admit.rf.dispatcher",
            Self::FinalAdmitRfCausalOwner => "final_admit.rf.causal_owner",
            Self::FinalAdmitRfCausalAccount => "final_admit.rf.causal_account",
            Self::FinalAdmitRfFences => "final_admit.rf.fences",
            Self::BeforeDecryptIdentityRfAuthOwner => "before_decrypt.identity_rf.auth_owner",
            Self::BeforeDecryptIdentityRfAuthAccount => "before_decrypt.identity_rf.auth_account",
            Self::BeforeDecryptIdentityRfDispatcher => "before_decrypt.identity_rf.dispatcher",
            Self::BeforeDecryptIdentityRfCausalOwner => "before_decrypt.identity_rf.causal_owner",
            Self::BeforeDecryptIdentityRfCausalAccount => {
                "before_decrypt.identity_rf.causal_account"
            }
            Self::BeforeDecryptIdentityRfFences => "before_decrypt.identity_rf.fences",
            Self::AfterDecryptIdentityRfAuthOwner => "after_decrypt.identity_rf.auth_owner",
            Self::AfterDecryptIdentityRfAuthAccount => "after_decrypt.identity_rf.auth_account",
            Self::AfterDecryptIdentityRfDispatcher => "after_decrypt.identity_rf.dispatcher",
            Self::AfterDecryptIdentityRfCausalOwner => "after_decrypt.identity_rf.causal_owner",
            Self::AfterDecryptIdentityRfCausalAccount => "after_decrypt.identity_rf.causal_account",
            Self::AfterDecryptIdentityRfFences => "after_decrypt.identity_rf.fences",
            Self::BeforeNativeLifetimeIdentityRfAuthOwner => {
                "before_native_lifetime.identity_rf.auth_owner"
            }
            Self::BeforeNativeLifetimeIdentityRfAuthAccount => {
                "before_native_lifetime.identity_rf.auth_account"
            }
            Self::BeforeNativeLifetimeIdentityRfDispatcher => {
                "before_native_lifetime.identity_rf.dispatcher"
            }
            Self::BeforeNativeLifetimeIdentityRfCausalOwner => {
                "before_native_lifetime.identity_rf.causal_owner"
            }
            Self::BeforeNativeLifetimeIdentityRfCausalAccount => {
                "before_native_lifetime.identity_rf.causal_account"
            }
            Self::BeforeNativeLifetimeIdentityRfFences => {
                "before_native_lifetime.identity_rf.fences"
            }
            Self::PrivateManifestActor => "private.manifest_actor",
            Self::FinalPrivateAttestationService => "final.private_attestation.service",
            Self::FinalPrivateAttestationManifest => "final.private_attestation.manifest",
            Self::PrivateCipherSidBefore => "private.cipher.sid_before",
            Self::PrivateCipherSidAfter => "private.cipher.sid_after",
            Self::PrivateCipherCsrfBefore => "private.cipher.csrf_before",
            Self::PrivateCipherCsrfAfter => "private.cipher.csrf_after",
            Self::NativeSource => "native.source",
            Self::NativeLifetime => "native.lifetime",
        }
    }
    pub fn authority(self) -> &'static str {
        match self {
            Self::ClaimSourceCallbackOwner => "Owner service",
            Self::ClaimSourceCallbackAccount => "Account Actor",
            Self::ClaimSourceDirectory => "ManagerDirectory Actor",
            Self::ClaimSourceRfDispatcher => "Read RF DispatcherActor",
            Self::ClaimSourceRfCausalOwner => "Owner causal gate service",
            Self::ClaimSourceRfCausalAccount => "Account Actor",
            Self::ClaimSourceRfFences => "FENCES Actor",
            Self::SourceRecordAndPredecryptCallbackOwner => "Owner service",
            Self::SourceRecordAndPredecryptCallbackAccount => "Account Actor",
            Self::SourceRecordAndPredecryptDirectory => "ManagerDirectory Actor",
            Self::SourceRecordAndPredecryptRfDispatcher => "Read RF DispatcherActor",
            Self::SourceRecordAndPredecryptRfCausalOwner => "Owner causal gate service",
            Self::SourceRecordAndPredecryptRfCausalAccount => "Account Actor",
            Self::SourceRecordAndPredecryptRfFences => "FENCES Actor",
            Self::PostdecryptAndClaimLifetimeCallbackOwner => "Owner service",
            Self::PostdecryptAndClaimLifetimeCallbackAccount => "Account Actor",
            Self::PostdecryptAndClaimLifetimeDirectory => "ManagerDirectory Actor",
            Self::PostdecryptAndClaimLifetimeRfDispatcher => "Read RF DispatcherActor",
            Self::PostdecryptAndClaimLifetimeRfCausalOwner => "Owner causal gate service",
            Self::PostdecryptAndClaimLifetimeRfCausalAccount => "Account Actor",
            Self::PostdecryptAndClaimLifetimeRfFences => "FENCES Actor",
            Self::FinalAdmitCallbackOwner => "Owner service",
            Self::FinalAdmitCallbackAccount => "Account Actor",
            Self::FinalAdmitDirectory => "ManagerDirectory Actor",
            Self::FinalAdmitRfDispatcher => "Read RF DispatcherActor",
            Self::FinalAdmitRfCausalOwner => "Owner causal gate service",
            Self::FinalAdmitRfCausalAccount => "Account Actor",
            Self::FinalAdmitRfFences => "FENCES Actor",
            Self::BeforeDecryptIdentityRfAuthOwner => "Owner first-auth service",
            Self::BeforeDecryptIdentityRfAuthAccount => "Account Actor",
            Self::BeforeDecryptIdentityRfDispatcher => "Read RF DispatcherActor",
            Self::BeforeDecryptIdentityRfCausalOwner => "Owner causal gate service",
            Self::BeforeDecryptIdentityRfCausalAccount => "Account Actor",
            Self::BeforeDecryptIdentityRfFences => "FENCES Actor",
            Self::AfterDecryptIdentityRfAuthOwner => "Owner first-auth service",
            Self::AfterDecryptIdentityRfAuthAccount => "Account Actor",
            Self::AfterDecryptIdentityRfDispatcher => "Read RF DispatcherActor",
            Self::AfterDecryptIdentityRfCausalOwner => "Owner causal gate service",
            Self::AfterDecryptIdentityRfCausalAccount => "Account Actor",
            Self::AfterDecryptIdentityRfFences => "FENCES Actor",
            Self::BeforeNativeLifetimeIdentityRfAuthOwner => "Owner first-auth service",
            Self::BeforeNativeLifetimeIdentityRfAuthAccount => "Account Actor",
            Self::BeforeNativeLifetimeIdentityRfDispatcher => "Read RF DispatcherActor",
            Self::BeforeNativeLifetimeIdentityRfCausalOwner => "Owner causal gate service",
            Self::BeforeNativeLifetimeIdentityRfCausalAccount => "Account Actor",
            Self::BeforeNativeLifetimeIdentityRfFences => "FENCES Actor",
            Self::PrivateManifestActor => "Manifest Actor",
            Self::FinalPrivateAttestationService => "Identity private-history service",
            Self::FinalPrivateAttestationManifest => "Manifest Actor",
            Self::PrivateCipherSidBefore => "Cipher Actor",
            Self::PrivateCipherSidAfter => "Cipher Actor",
            Self::PrivateCipherCsrfBefore => "Cipher Actor",
            Self::PrivateCipherCsrfAfter => "Cipher Actor",
            Self::NativeSource => "Native approvals/verify InspectOrigin",
            Self::NativeLifetime => "Native SessionLifetime",
        }
    }
}
/// A first in-memory budget claim. No signature/serde or historical constructor.
/// Receivers must require their own strong typed Read-purpose proof as well.
pub struct ReadCurrentPortPrepaidV3 {
    pub(super) scope_digest: String,
    pub(super) port: ReadCurrentPortV3,
    pub(super) until: UnixMillis,
}
impl ReadCurrentPortPrepaidV3 {
    pub fn scope_digest(&self) -> &str {
        &self.scope_digest
    }
    pub fn port(&self) -> ReadCurrentPortV3 {
        self.port
    }
    pub fn expires_at(&self) -> UnixMillis {
        self.until
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct ReadCurrentPortClaimV3 {
    pub(in crate::provisioning) at: UnixMillis,
    pub(in crate::provisioning) ordinal: u64,
}
