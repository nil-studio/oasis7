use super::*;

impl World {
    fn normalize_module_release_roles(roles: &[String]) -> Vec<String> {
        let mut normalized: Vec<String> = roles
            .iter()
            .filter_map(|role| {
                let role = role.trim();
                (!role.is_empty()).then(|| role.to_ascii_lowercase())
            })
            .collect();
        normalized.sort();
        normalized.dedup();
        normalized
    }

    pub(super) fn normalize_module_release_required_roles(
        required_roles: &[String],
    ) -> Vec<String> {
        let mut normalized = Self::normalize_module_release_roles(required_roles);
        if normalized.is_empty() {
            normalized = MODULE_RELEASE_DEFAULT_REQUIRED_ROLES
                .iter()
                .map(|role| role.to_string())
                .collect();
        }
        normalized
    }

    pub(super) fn normalize_module_release_role_set(roles: &[String]) -> Vec<String> {
        Self::normalize_module_release_roles(roles)
    }

    pub(super) fn normalize_module_release_role(role: &str) -> Option<String> {
        let normalized = role.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    pub(super) fn module_release_roles_satisfied(
        required_roles: &[String],
        role_approvals: &std::collections::BTreeMap<String, String>,
    ) -> bool {
        required_roles
            .iter()
            .all(|required| role_approvals.contains_key(required))
    }

    pub(super) fn module_release_profile_commitment(
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        profile_changes: &ModuleProfileChanges,
    ) -> Result<Option<String>, String> {
        if profile_changes.is_empty() {
            return Ok(None);
        }

        let mut normalized_profiles = profile_changes.clone();
        normalized_profiles
            .product_profiles
            .sort_by(|left, right| left.product_id.cmp(&right.product_id));
        normalized_profiles
            .recipe_profiles
            .sort_by(|left, right| left.recipe_id.cmp(&right.recipe_id));
        normalized_profiles
            .factory_profiles
            .sort_by(|left, right| left.factory_id.cmp(&right.factory_id));
        let commitment_payload = serde_json::json!({
            "module_manifest": manifest,
            "activate": activate,
            "profile_changes": normalized_profiles,
        });
        super::super::super::util::hash_json(&commitment_payload)
            .map(Some)
            .map_err(|err| format!("module release profile commitment failed: {err:?}"))
    }

    pub(super) fn add_module_release_profile_commitment(
        manifest: &mut super::super::super::manifest::Manifest,
        reviewed_module: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        profile_changes: &ModuleProfileChanges,
    ) -> Result<(), String> {
        let Some(commitment) =
            Self::module_release_profile_commitment(reviewed_module, activate, profile_changes)?
        else {
            return Ok(());
        };
        let serde_json::Value::Object(content) = &mut manifest.content else {
            return Err("current manifest content must be object".to_string());
        };
        content.insert(
            "module_profile_commitment".to_string(),
            serde_json::Value::String(commitment),
        );
        Ok(())
    }

    pub(super) fn module_release_attestation_key(signer_node_id: &str, platform: &str) -> String {
        format!(
            "{}|{}",
            signer_node_id.trim(),
            platform.trim().to_ascii_lowercase()
        )
    }

    pub(super) fn normalize_module_release_attestation_platform(platform: &str) -> Option<String> {
        let normalized = platform.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    pub(super) fn normalize_module_release_attestation_hash(
        raw: &str,
        field: &str,
    ) -> Result<String, String> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "module release attestation rejected: {field} must be 64-char hex"
            ));
        }
        Ok(normalized)
    }

    pub(super) fn normalize_module_release_attestation_builder_image_digest(
        raw: &str,
    ) -> Result<String, String> {
        let normalized = raw.trim().to_ascii_lowercase();
        let Some(digest_hex) = normalized.strip_prefix("sha256:") else {
            return Err(
                "module release attestation rejected: builder_image_digest must be sha256:<64-hex>"
                    .to_string(),
            );
        };
        if digest_hex.len() != 64 || !digest_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(
                "module release attestation rejected: builder_image_digest must be sha256:<64-hex>"
                    .to_string(),
            );
        }
        Ok(normalized)
    }

    pub(super) fn normalize_module_release_attestation_label(
        raw: &str,
        field: &str,
    ) -> Result<String, String> {
        let normalized = raw.trim().to_string();
        if normalized.is_empty() {
            return Err(format!(
                "module release attestation rejected: {field} is empty"
            ));
        }
        if normalized.len() > 128 {
            return Err(format!(
                "module release attestation rejected: {field} exceeds 128 chars"
            ));
        }
        Ok(normalized)
    }

    pub(super) fn normalize_module_release_attestation_proof_cid(
        proof_cid: &str,
    ) -> Option<String> {
        let normalized = proof_cid.trim().to_string();
        if normalized.is_empty() {
            return None;
        }
        if normalized.len() > 256 {
            return None;
        }
        Some(normalized)
    }

    pub(super) fn evaluate_module_release_shadow_hash(
        &self,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        profile_changes: &ModuleProfileChanges,
    ) -> Result<String, String> {
        let mut changes = ModuleChangeSet::default();
        let record_key = oasis7_wasm_abi::ModuleRegistry::record_key(
            manifest.module_id.as_str(),
            manifest.version.as_str(),
        );
        if let Some(record) = self.module_registry.records.get(record_key.as_str()) {
            if record.manifest != *manifest {
                return Err(format!(
                    "module release shadow rejected: existing manifest mismatch for {}",
                    record_key
                ));
            }
        } else {
            changes.register.push(manifest.clone());
        }

        if activate {
            let already_active_same = self
                .module_registry
                .active
                .get(&manifest.module_id)
                .map(|version| version == &manifest.version)
                .unwrap_or(false);
            if !already_active_same {
                changes.activate.push(ModuleActivation {
                    module_id: manifest.module_id.clone(),
                    version: manifest.version.clone(),
                });
            }
        }

        let profile_commitment =
            Self::module_release_profile_commitment(manifest, activate, profile_changes)?;
        if changes.is_empty() && profile_commitment.is_none() {
            return self
                .current_manifest_hash()
                .map_err(|err| format!("module release shadow hash failed: {err:?}"));
        }

        self.validate_module_changes(&changes)
            .map_err(|err| format!("module release shadow validate failed: {err:?}"))?;
        self.shadow_validate_module_changes(&changes)
            .map_err(|err| format!("module release shadow dry-run failed: {err:?}"))?;

        let module_changes_value = serde_json::to_value(&changes)
            .map_err(|err| format!("module release shadow serialize failed: {err}"))?;
        let mut manifest_update = self.manifest.clone();
        manifest_update.version = manifest_update.version.saturating_add(1);
        let serde_json::Value::Object(content) = &mut manifest_update.content else {
            return Err(
                "module release shadow rejected: current manifest content must be object"
                    .to_string(),
            );
        };
        content.insert("module_changes".to_string(), module_changes_value);
        if let Some(profile_commitment) = profile_commitment {
            content.insert(
                "module_profile_commitment".to_string(),
                serde_json::Value::String(profile_commitment),
            );
        }
        super::super::super::util::hash_json(&manifest_update)
            .map_err(|err| format!("module release shadow hash failed: {err:?}"))
    }
}
