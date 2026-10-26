//! Versioned tooling inputs shared by the build producer and identity freshness reader.
use crate::{SOURCE_INPUT_RULE_VERSION, SourceHashError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildManifest {
    pub source_input_rule_version: u32,
    pub profile: String,
    pub target: String,
    pub wasm_toolchain: String,
    pub wasm_build_std: String,
    pub wasm_build_std_components: String,
    pub wasm_build_std_features: String,
    pub wasm_deterministic_guard: String,
    pub canonicalizer_version: String,
    pub container_platform: String,
    pub builder_image_ref: String,
    pub builder_image_digest: String,
    pub build_suite_version: String,
}

pub fn compute_build_manifest_hash(manifest: &BuildManifest) -> Result<String, SourceHashError> {
    if manifest.source_input_rule_version != SOURCE_INPUT_RULE_VERSION {
        return Err(SourceHashError::UnsupportedInput(
            "unsupported build manifest source input rule version".into(),
        ));
    }
    let bytes = serde_json::to_vec(manifest).map_err(|error| {
        SourceHashError::UnsupportedInput(format!("serialize build manifest: {error}"))
    })?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rule_version_is_bound_to_the_same_producer_consumer_tooling_vector() {
        let mut input = BuildManifest {
            source_input_rule_version: 2,
            profile: "release".into(),
            target: "wasm32-unknown-unknown".into(),
            wasm_toolchain: "fixture".into(),
            wasm_build_std: "0".into(),
            wasm_build_std_components: "".into(),
            wasm_build_std_features: "".into(),
            wasm_deterministic_guard: "1".into(),
            canonicalizer_version: "fixture".into(),
            container_platform: "linux-x86_64".into(),
            builder_image_ref: "fixture".into(),
            builder_image_digest: "fixture".into(),
            build_suite_version: "fixture".into(),
        };
        // Independent Python hashlib vectors preserve the frozen v1 field order.
        assert_eq!(
            compute_build_manifest_hash(&input).unwrap(),
            "6fb763400a602b207393ebd46580d9975a9dd82b3004079d6f114de4128b9c5b"
        );
        assert_ne!(
            compute_build_manifest_hash(&input).unwrap(),
            "f864505bdb00c181860bd9951314ac965ad16a5a70e0df4895dbb3233b194ff6"
        );
        input.source_input_rule_version = 99;
        assert!(compute_build_manifest_hash(&input).is_err());
    }
}
