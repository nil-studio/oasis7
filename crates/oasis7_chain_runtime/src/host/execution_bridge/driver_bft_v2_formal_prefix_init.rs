//! Crash-safe creation and recovery of a formal BFT prefix origin.
//!
//! The intent is the durable identity reservation. An origin without its
//! intent is accepted only after both format markers prove initialization
//! completed; an intent may be resumed only for its exact original H/target.
use super::{
    DOWNGRADE_FENCE, LEGACY_MARKER, Ledger, MARKER, MARKER_BYTES, OriginPin, common_storage, files,
};
use std::path::Path;

const ORIGIN_FILE: &str = "origin.cbor";
const INIT_INTENT_FILE: &str = "init-intent.cbor";
const ORIGIN_MAX_BYTES: usize = 4096;

impl Ledger {
    /// Load an immutable origin or its exact interrupted initialization.
    ///
    /// A clean, marker-free directory is new only when initialization is
    /// explicitly allowed. Existing markers without an origin/intent are
    /// evidence of lost identity and must never authorize a new ledger id.
    pub(super) fn load_origin(
        dir: &Path,
        root: &Path,
        initialize: bool,
    ) -> Result<(Option<OriginPin>, bool), String> {
        let origin_path = dir.join(ORIGIN_FILE);
        let intent_path = dir.join(INIT_INTENT_FILE);

        // The new protocol never writes origin through a private temp file:
        // origin is hard-linked from the already durable init intent. Refuse
        // legacy/direct-origin stages instead of silently replacing identity.
        if files::staged_value::<OriginPin>(&origin_path, ORIGIN_MAX_BYTES)?.is_some() {
            return Err(
                "formal prefix origin staging has no verified initialization intent".into(),
            );
        }
        let staged_intent = files::staged_value::<OriginPin>(&intent_path, ORIGIN_MAX_BYTES)?;
        let origin: Option<OriginPin> =
            common_storage::read_optional_canonical(&origin_path, ORIGIN_MAX_BYTES)?;
        let intent: Option<OriginPin> =
            common_storage::read_optional_canonical(&intent_path, ORIGIN_MAX_BYTES)?;

        if let Some(pin) = &origin {
            pin.validate()?;
        }
        if let Some(pin) = &intent {
            pin.validate()?;
        }
        if let Some(staged) = &staged_intent {
            staged.validate()?;
        }
        match (origin.is_some(), intent.is_some()) {
            (true, true) => files::same_pair(&origin_path, &intent_path)?,
            (true, false) => files::single_link(&origin_path)?,
            (false, true) => files::single_link(&intent_path)?,
            (false, false) => {}
        }

        let legacy_marker = read_marker(root, LEGACY_MARKER, DOWNGRADE_FENCE)?;
        let prefix_marker = read_marker(root, MARKER, MARKER_BYTES)?;
        let legacy_stages = files::stages(&root.join(LEGACY_MARKER))?;
        let prefix_stages = files::stages(&root.join(MARKER))?;

        match (origin.as_ref(), intent.as_ref(), staged_intent.as_ref()) {
            (Some(origin), Some(intent), staged) => {
                if origin != intent {
                    return Err("formal prefix origin and init intent differ".into());
                }
                if staged.is_some_and(|staged| staged != intent) {
                    return Err("formal prefix init stages differ from the durable intent".into());
                }
                Ok((Some(origin.clone()), true))
            }
            (None, Some(intent), staged) => {
                if staged.is_some_and(|staged| staged != intent) {
                    return Err("formal prefix init stages differ from the durable intent".into());
                }
                if legacy_marker
                    || prefix_marker
                    || !legacy_stages.is_empty()
                    || !prefix_stages.is_empty()
                {
                    return Err("formal prefix markers exist before the origin hard link".into());
                }
                Ok((Some(intent.clone()), true))
            }
            (None, None, Some(staged)) => {
                if legacy_marker
                    || prefix_marker
                    || !legacy_stages.is_empty()
                    || !prefix_stages.is_empty()
                {
                    return Err("formal prefix markers exist before the origin intent".into());
                }
                Ok((Some(staged.clone()), true))
            }
            (Some(origin), None, None) => {
                if !legacy_marker
                    || !prefix_marker
                    || !legacy_stages.is_empty()
                    || !prefix_stages.is_empty()
                {
                    return Err(
                        "completed formal prefix origin has missing or staged markers".into(),
                    );
                }
                Ok((Some(origin.clone()), false))
            }
            (None, None, None) => {
                if legacy_marker
                    || prefix_marker
                    || !legacy_stages.is_empty()
                    || !prefix_stages.is_empty()
                {
                    return Err(
                        "formal prefix markers or stages exist without the original identity"
                            .into(),
                    );
                }
                if initialize {
                    Ok((None, false))
                } else {
                    Err("formal prefix origin is absent; verification cannot initialize it".into())
                }
            }
            (Some(_), None, Some(_)) => {
                Err("formal prefix origin has an incomplete or ambiguous intent".into())
            }
        }
    }

    /// Resume the exact durable initialization, or begin one in a clean
    /// marker-free directory. The completed origin is never rewritten.
    pub(super) fn bind_origin(
        &mut self,
        material_pin_digest: &str,
        first_height: u64,
    ) -> Result<(), String> {
        self.verify_carriers()?;
        super::hash(material_pin_digest)?;

        if !self.init_pending
            && let Some(pin) = &self.pin
        {
            if pin.material_pin_digest != material_pin_digest || pin.first_height != first_height {
                return Err("formal prefix origin differs from qualified H/target/material".into());
            }
            return Ok(());
        }

        let pin = match self.pin.clone() {
            Some(pin) => pin,
            None if self.initialize => {
                let mut random = [0u8; 32];
                getrandom::fill(&mut random).map_err(|error| error.to_string())?;
                let pin = OriginPin {
                    schema_version: 1,
                    domain: "oasis7.formal.bft.prefix.origin.v1".into(),
                    ledger_id: hex::encode(random),
                    material_pin_digest: material_pin_digest.into(),
                    first_height,
                    format_lock: common_storage::lock_carrier_identity(&self._format_lock)?,
                    writer_lock: common_storage::lock_carrier_identity(&self._lock)?,
                };
                pin.validate()?;
                // Retain this identity in memory even if publication returns
                // an uncertain error and the same handle is retried.
                self.pin = Some(pin.clone());
                self.init_pending = true;
                pin
            }
            None => return Err("formal prefix origin is absent".into()),
        };
        if pin.material_pin_digest != material_pin_digest || pin.first_height != first_height {
            return Err(
                "formal prefix pending initialization differs from qualified H/target".into(),
            );
        }
        pin.validate()?;
        self.verify_carriers()?;
        let bytes = serde_cbor::to_vec(&pin).map_err(|error| error.to_string())?;

        let intent_path = self.dir.join(INIT_INTENT_FILE);
        let origin_path = self.dir.join(ORIGIN_FILE);
        if common_storage::read_optional_bytes(&origin_path, bytes.len())?.is_some() {
            // A prior attempt may already have published the origin hard link.
            // Republishing the intent through the single-link helper would
            // reject the legitimate two-link pending state, so verify and
            // resume that exact pair instead.
            files::same_pair(&origin_path, &intent_path)?;
            let staged = files::staged_value::<OriginPin>(&intent_path, ORIGIN_MAX_BYTES)?;
            if staged.as_ref().is_some_and(|staged| staged != &pin) {
                return Err("formal prefix init stages differ from the durable origin".into());
            }
            if staged.is_some() {
                files::clean_equivalent_stages(&intent_path, &bytes)?;
            }
        } else {
            files::publish(&intent_path, &bytes)?;
        }
        self.verify_carriers()?;
        files::link_record(&origin_path, &intent_path, &bytes)?;
        self.verify_carriers()?;
        files::publish(&self.root.join(LEGACY_MARKER), DOWNGRADE_FENCE)?;
        self.verify_carriers()?;
        files::publish(&self.root.join(MARKER), MARKER_BYTES)?;
        self.verify_carriers()?;
        files::finish(&origin_path, &intent_path, &bytes)?;
        self.init_pending = false;
        Ok(())
    }
}

fn read_marker(root: &Path, name: &str, expected: &[u8]) -> Result<bool, String> {
    let path = root.join(name);
    match common_storage::read_optional_bytes(&path, expected.len())? {
        Some(bytes) if bytes == expected => {
            files::single_link(&path)?;
            Ok(true)
        }
        Some(_) => Err(format!(
            "formal prefix marker {name} has an unsupported value"
        )),
        None => Ok(false),
    }
}
