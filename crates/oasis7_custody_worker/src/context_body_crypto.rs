//! Identity-only private artifact crypto. This grants no paid or current-use authority.
use crate::envelope_crypto::{self, SecretBytes};
use oasis7_custody_core::{
    DecimalU64,
    context_body::{ContextBodyRecordV1, ContextBodyScopeV1, ValidatedContextBody},
};
use wasm_bindgen::JsValue;
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("private context crypto rejected")
}

/// Separate from role keys and Native private storage. Never generated on first use.
/// No Debug, Clone, serialization, or raw-key accessor is provided.
pub struct ContextBodyCrypto {
    kek: Zeroizing<[u8; 32]>,
    wrapping_version: DecimalU64,
    environment: String,
}
impl ContextBodyCrypto {
    pub fn load(env: &worker::Env) -> Result<Self> {
        if env
            .var("CUSTODY_CONTEXT_BODY_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        let raw = Zeroizing::new(
            env.secret("CUSTODY_CONTEXT_BODY_KEK")
                .map_err(|_| rejected())?
                .to_string(),
        );
        if raw.len() != 64
            || !raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        let mut kek = Zeroizing::new([0; 32]);
        hex::decode_to_slice(raw.as_bytes(), kek.as_mut_slice()).map_err(|_| rejected())?;
        let wrapping_version: DecimalU64 = serde_json::from_value(serde_json::Value::String(
            env.var("CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION")
                .map_err(|_| rejected())?
                .to_string(),
        ))
        .map_err(|_| rejected())?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| rejected())?
            .to_string();
        if wrapping_version.0 == 0 || environment.is_empty() {
            return Err(rejected());
        }
        Ok(Self {
            kek,
            wrapping_version,
            environment,
        })
    }
    fn check(&self, scope: &ContextBodyScopeV1) -> Result<()> {
        scope.validate().map_err(|_| rejected())?;
        let now = js_sys::Date::now();
        if scope.identity_scope.context.identity.environment != self.environment
            || !now.is_finite()
            || now < 0.0
            || now.fract() != 0.0
            || now > 9_007_199_254_740_991.0
            || now as u64 >= scope.write_deadline.0
        {
            return Err(rejected());
        }
        Ok(())
    }
    /// Call once before persisting the Account's exact envelope claim. An unknown
    /// PUT is recovered by reading that original envelope, never by sealing again.
    pub async fn seal(&self, body: &ValidatedContextBody) -> Result<ContextBodyRecordV1> {
        self.check(body.scope())?;
        let context = body.scope().envelope_context().map_err(|_| rejected())?;
        let envelope = envelope_crypto::seal(
            &context,
            self.wrapping_version,
            &self.kek,
            body.canonical_bytes(),
        )
        .await
        .map_err(|_| rejected())?;
        self.check(body.scope())?;
        let record = ContextBodyRecordV1 {
            scope: body.scope().clone(),
            envelope,
        };
        record.validate().map_err(|_| rejected())?;
        Ok(record)
    }
    /// Plaintext is only temporary input to full SDK revalidation. It is never a
    /// usable context, and metadata recovery must not call this method.
    pub async fn open(
        &self,
        expected: &ContextBodyScopeV1,
        record: &ContextBodyRecordV1,
    ) -> Result<SecretBytes> {
        self.check(expected)?;
        record.validate_for(expected).map_err(|_| rejected())?;
        if record.envelope.wrapping_key_version != self.wrapping_version {
            return Err(rejected());
        }
        let context = expected.envelope_context().map_err(|_| rejected())?;
        let bytes = envelope_crypto::open(&record.envelope, &context, &self.kek)
            .await
            .map_err(|_| rejected())?;
        self.check(expected)?;
        Ok(bytes)
    }
}
