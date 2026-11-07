//! Opt-in failure-only diagnostics. Never authority, persistence, or an error formatter.
#[cfg(feature = "owner")]
use oasis7_custody_core::provisioning::{AccountProvisionApprovalState, ProvisionApprovalError};
use wasm_bindgen::prelude::*;
use worker::Env;

#[wasm_bindgen(module = "/platform/rejection_diagnostic.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = startRejectionTrace)]
    fn start(
        enabled: bool,
        kind: &str,
        started: JsValue,
        observed: JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = setRejectionFact)]
    fn fact(trace: &JsValue, name: &str, value: JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = failRejectionTrace)]
    fn fail(
        trace: &JsValue,
        stage: &str,
        category: &str,
        checked_at: JsValue,
    ) -> Result<(), JsValue>;
}

pub(crate) struct Trace(Option<JsValue>);

fn number(value: Option<u64>) -> JsValue {
    value
        .filter(|v| *v <= 9_007_199_254_740_991)
        .map_or(JsValue::NULL, |v| JsValue::from_f64(v as f64))
}

impl Trace {
    pub(crate) fn new(
        env: &Env,
        kind: &'static str,
        started: Option<u64>,
        observed: Option<u64>,
    ) -> Self {
        // Disabled means no JS call, clock read, state read, or diagnostic output.
        if env
            .var("CUSTODY_REJECTION_DIAGNOSTIC")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Self(None);
        }
        Self(start(true, kind, number(started), number(observed)).ok())
    }

    pub(crate) fn fact(&self, name: &'static str, value: Option<u64>) {
        if let Some(trace) = &self.0 {
            let _ = fact(trace, name, number(value));
        }
    }

    pub(crate) fn fail(&self, stage: &'static str) {
        if let Some(trace) = &self.0 {
            let _ = fail(trace, stage, "unknown", JsValue::NULL);
        }
    }

    #[cfg(feature = "identity")]
    pub(crate) fn at(&self, stage: &'static str, time: u64) {
        if let Some(trace) = &self.0 {
            let _ = fail(trace, stage, "unknown", number(Some(time)));
        }
    }

    #[cfg(feature = "owner")]
    pub(crate) fn session(
        &self,
        state: &AccountProvisionApprovalState,
        id: &str,
        source: Option<&str>,
    ) {
        if self.0.is_none() {
            return;
        }
        if let Some(session) = state.test_sessions().get(id) {
            use oasis7_custody_core::provisioning::SessionPurposeV1;
            self.fact(
                "execution_root_deadline",
                session
                    .roots
                    .get(&SessionPurposeV1::Execution)
                    .map(|root| root.deadline.get()),
            );
            self.fact("lease_until", session.lease_until.map(|until| until.get()));
            if let Some(source) = source.and_then(|id| session.sources.get(id)) {
                self.fact("source_expires_at", Some(source.expires_at.0));
                self.fact(
                    "source_observed_at",
                    source
                        .verification
                        .as_ref()
                        .map(|signed| signed.verification.observed_at.0),
                );
            }
        }
    }

    pub(crate) fn lifetime(
        &self,
        lifetime: &oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1,
    ) {
        self.fact(
            "lifetime_until",
            Some(
                lifetime
                    .lifetime
                    .session_expires_at
                    .0
                    .min(lifetime.lifetime.session_idle_expires_at.0),
            ),
        );
        self.fact(
            "lifetime_status_until",
            Some(lifetime.lifetime.status.expires_at.0),
        );
    }

    #[cfg(feature = "owner")]
    pub(crate) fn core_at(&self, stage: &'static str, error: &ProvisionApprovalError, time: u64) {
        use oasis7_custody_core::{
            approval_admission::ApprovalAdmissionError, provisioning::ProvisionError,
        };
        let category = match error {
            ProvisionApprovalError::Admission(error) => match error {
                ApprovalAdmissionError::Capacity => "admission_capacity",
                ApprovalAdmissionError::Invalid => "admission_invalid",
                ApprovalAdmissionError::Frozen => "admission_frozen",
                ApprovalAdmissionError::StaleContext => "admission_stale_context",
                ApprovalAdmissionError::Expired => "admission_expired",
                ApprovalAdmissionError::BindingMismatch => "admission_binding_mismatch",
                ApprovalAdmissionError::UnknownKey => "admission_unknown_key",
                ApprovalAdmissionError::SignatureRejected => "admission_signature_rejected",
                ApprovalAdmissionError::Conflict => "admission_conflict",
                ApprovalAdmissionError::NonceReused => "admission_nonce_reused",
                ApprovalAdmissionError::Missing => "admission_missing",
                ApprovalAdmissionError::InvalidPersistence => "admission_invalid_persistence",
            },
            ProvisionApprovalError::Provision(error) => match error {
                ProvisionError::Invalid => "provision_invalid",
                ProvisionError::Forbidden => "provision_forbidden",
                ProvisionError::Frozen => "provision_frozen",
                ProvisionError::Conflict => "provision_conflict",
                ProvisionError::Expired => "provision_expired",
                ProvisionError::Exhausted => "provision_exhausted",
                ProvisionError::Missing => "provision_missing",
                ProvisionError::InvalidTransition => "provision_invalid_transition",
                ProvisionError::Encoding => "provision_encoding",
                ProvisionError::Overflow => "provision_overflow",
            },
            ProvisionApprovalError::BindingMismatch => "binding_mismatch",
            ProvisionApprovalError::ContextTooLarge => "context_too_large",
            ProvisionApprovalError::Conflict => "conflict",
            ProvisionApprovalError::InvalidPersistence => "invalid_persistence",
        };
        if let Some(trace) = &self.0 {
            let _ = fail(trace, stage, category, number(Some(time)));
        }
    }
}
