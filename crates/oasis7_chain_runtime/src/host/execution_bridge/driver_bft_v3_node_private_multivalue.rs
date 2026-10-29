//! Header-scoped received originals and immutable local selection references.
use super::*;

pub(super) fn parse_target(name: &str) -> Result<Option<(u64, Option<String>)>, String> {
    let (rest, selection) = if let Some(rest) = name.strip_prefix("selection-") {
        (rest, true)
    } else if let Some(rest) = name.strip_prefix("candidate-") {
        if rest.get(20..21) != Some("-") {
            return Ok(None);
        }
        (rest, false)
    } else {
        return Ok(None);
    };
    let digits = rest
        .get(..20)
        .ok_or("invalid header-scoped Node filename")?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid Node height filename".into());
    }
    let height = digits
        .parse()
        .map_err(|_| "Node height filename overflow")?;
    let (id, suffix) = if selection {
        (None, rest.get(20..).ok_or("invalid selection filename")?)
    } else {
        let id = rest
            .get(21..85)
            .ok_or("missing Node filename header identity")?;
        if !canonical_hash(id) {
            return Err("invalid Node filename header identity".into());
        }
        (
            Some(id.to_owned()),
            rest.get(85..).ok_or("invalid Node filename suffix")?,
        )
    };
    if ![".cbor", ".intent.cbor", ".reservation.cbor"].contains(&suffix) {
        return Err("unknown header-scoped Node filename suffix".into());
    }
    Ok(Some((height, id)))
}

impl NodePrivateStore {
    pub(in super::super) fn public_value(
        &self,
        height: u64,
        header_id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        let slot = self
            .candidates
            .get(&height)
            .filter(|slot| {
                slot.data
                    .value
                    .as_ref()
                    .is_some_and(|value| value.block_id().is_ok_and(|id| id == header_id))
            })
            .or_else(|| self.values.get(&(height, header_id.to_owned())));
        let Some(slot) = slot else { return Ok(None) };
        let value = slot
            .data
            .value
            .as_ref()
            .ok_or("original Node public value missing")?;
        // Inventory was checked when opened/retained. This public metadata
        // lookup grants no execution or read authority; export still verifies
        // the exact original slot and pins after the permission gate.
        value.validate_binding()?;
        if value.execution_commitment.height != height || value.block_id()? != header_id {
            return Err("original Node public lookup identity changed".into());
        }
        Ok(Some(value.clone()))
    }
    fn value_path(&self, height: u64, id: &str) -> PathBuf {
        self.dir.join(format!("candidate-{height:020}-{id}.cbor"))
    }
    fn selection_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("selection-{height:020}.cbor"))
    }
    pub(in super::super) fn check_counts(
        &self,
        additional: Option<(u64, String)>,
    ) -> Result<(), String> {
        let mut ids: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
        for data in self
            .candidates
            .values()
            .chain(self.values.values())
            .map(|s| &s.data)
        {
            let id = data
                .value
                .as_ref()
                .ok_or("candidate value missing")?
                .block_id()?;
            if !ids.entry(data.height).or_default().insert(id) {
                return Err("duplicate legacy and header-scoped Node candidate".into());
            }
        }
        if let Some((height, id)) = additional {
            ids.entry(height).or_default().insert(id);
        }
        validate_counts(&ids)
    }
    pub(in super::super) fn load_multivalue_inventory(
        &mut self,
        origin: &str,
        material: &str,
        entries: &BTreeSet<(u64, String)>,
        selections: &BTreeSet<u64>,
    ) -> Result<(), String> {
        for (height, id) in entries {
            let path = self.value_path(*height, id);
            let slot = read_slot(&path)?.ok_or("header-scoped Node original missing")?;
            if slot.data.schema_version != 4
                || slot.data.origin
                || slot.data.height != *height
                || slot.data.ledger_origin_digest != origin
                || slot.data.material_identity != material
                || slot.data.instance_id != self.instance_id
                || slot
                    .data
                    .value
                    .as_ref()
                    .ok_or("Node value missing")?
                    .block_id()?
                    != *id
            {
                return Err("header-scoped Node original identity differs".into());
            }
            if retention_required(&path)? {
                verify_material(&self.store, &slot.data)?;
            }
            self.values.insert((*height, id.clone()), slot);
        }
        for height in selections {
            let path = self.selection_path(*height);
            let slot = read_slot(&path)?.ok_or("original Node selection missing")?;
            if slot.data.schema_version != 5
                || slot.data.height != *height
                || slot.data.ledger_origin_digest != origin
                || slot.data.material_identity != material
                || slot.data.instance_id != self.instance_id
                || self.candidates.contains_key(height)
            {
                return Err("local Node selection conflicts with original identity".into());
            }
            let id = slot
                .data
                .value
                .as_ref()
                .ok_or("selected value missing")?
                .block_id()?;
            let candidate = self
                .values
                .get(&(*height, id))
                .ok_or("local selection lost its original candidate")?;
            if !candidate.complete
                || slot.data.value != candidate.data.value
                || slot.data.references != vec![candidate.data.digest()?]
            {
                return Err("local Node selection changed its original candidate".into());
            }
            if retention_required(&path)? {
                verify_material(&self.store, &slot.data)?;
            }
            self.selections.insert(*height, slot);
        }
        Ok(())
    }
    fn exact_slot(&self, value: &BftHeaderValueV3) -> Result<Option<(&Slot, PathBuf)>, String> {
        let height = value.execution_commitment.height;
        let id = value.block_id()?;
        if let Some(slot) = self.candidates.get(&height)
            && slot
                .data
                .value
                .as_ref()
                .is_some_and(|v| v.header == value.header)
        {
            return Ok(Some((slot, self.candidate_path(height))));
        }
        Ok(self
            .values
            .get(&(height, id.clone()))
            .map(|slot| (slot, self.value_path(height, &id))))
    }
    pub(in super::super) fn candidate_for_value(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        let Some((slot, path)) = self.exact_slot(value)? else {
            return Ok(None);
        };
        verify_original_slot(&self.store, &path, slot)?;
        if slot.data.value.as_ref() != Some(value) {
            return Err("original Node public value changed".into());
        }
        Ok(Some(NodeBftPrivateCandidateV3 {
            public_value: value.clone(),
            actions_cbor: slot.data.actions_cbor.clone(),
        }))
    }
    pub(in super::super) fn selected_candidate(
        &self,
        height: u64,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        let Some(selection) = self.selections.get(&height) else {
            return Ok(None);
        };
        verify_original_slot(&self.store, &self.selection_path(height), selection)?;
        let value = selection
            .data
            .value
            .as_ref()
            .ok_or("Node selection lost public value")?;
        let (slot, _) = self
            .exact_slot(value)?
            .ok_or("Node selection lost original record")?;
        if selection.data.references != vec![slot.data.digest()?] {
            return Err("Node selection record digest differs".into());
        }
        self.candidate_for_value(value)
    }
    pub(in super::super) fn export(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        let (slot, path) = self
            .exact_slot(value)?
            .ok_or("original Node candidate unavailable")?;
        if !slot.complete {
            return Err("Node original retention is incomplete".into());
        }
        verify_original_slot(&self.store, &path, slot)?;
        let candidate = self
            .candidate_for_value(value)?
            .ok_or("original Node value unavailable")?;
        let nodes: BTreeMap<_, _> = slot
            .data
            .references
            .iter()
            .map(|reference| {
                common_storage::read_cas_bounded(
                    &self.store,
                    reference,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )
                .map(|bytes| (reference.clone(), bytes))
            })
            .collect::<Result<_, _>>()?;
        formal_dependencies::validate_nodes(&nodes)?;
        Ok(NodeBftPrivateCandidatePackageV3 {
            candidate,
            dependency_nodes: nodes.into_iter().collect(),
        })
    }
    pub(in super::super) fn retain_received(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize) -> Result<(), String>,
    ) -> Result<String, String> {
        formal_dependencies::validate_nodes(nodes)?;
        let height = candidate.public_value.execution_commitment.height;
        let id = candidate.public_value.block_id()?;
        let origin = &self.origin.as_ref().ok_or("Node origin unavailable")?.data;
        if height <= origin.height || height - origin.height > 64 {
            return Err("private Node height capacity".into());
        }
        self.check_counts(Some((height, id.clone())))?;
        let mut data = Data {
            schema_version: 4,
            domain: MULTI_DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: candidate.actions_cbor.clone(),
            references: nodes.keys().cloned().collect(),
        };
        if let Some((old, path)) = self.exact_slot(&candidate.public_value)? {
            // Legacy originals keep their original schema, bytes and receipt digest.
            data.schema_version = old.data.schema_version;
            data.domain = old.data.domain.clone();
            if old.data != data {
                return Err(
                    "Node received candidate changes original bytes or dependencies".into(),
                );
            }
            verify_original_slot(&self.store, &path, old)?;
            let complete = old.complete;
            reserve(self.usage_with(Some(&data), nodes)?)?;
            publish(&self.store, &path, &data, nodes, complete, false)?;
            if data.schema_version == 3 {
                self.candidates.insert(
                    height,
                    Slot {
                        data: data.clone(),
                        complete: true,
                    },
                );
            } else {
                self.values.insert(
                    (height, id),
                    Slot {
                        data: data.clone(),
                        complete: true,
                    },
                );
            }
            return data.digest();
        }
        data.validate()?;
        reserve(self.usage_with(Some(&data), nodes)?)?;
        let path = self.value_path(height, &id);
        self.values.insert(
            (height, id.clone()),
            Slot {
                data: data.clone(),
                complete: false,
            },
        );
        publish(&self.store, &path, &data, nodes, false, false)?;
        self.values.insert(
            (height, id),
            Slot {
                data: data.clone(),
                complete: true,
            },
        );
        data.digest()
    }
    pub(in super::super) fn select_received(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize) -> Result<(), String>,
    ) -> Result<String, String> {
        let height = candidate.public_value.execution_commitment.height;
        if self.candidates.contains_key(&height) {
            return Err("original local Node selection is already fixed".into());
        }
        if let Some(selected) = self.selections.get(&height) {
            verify_original_slot(&self.store, &self.selection_path(height), selected)?;
            if selected.data.value.as_ref() != Some(&candidate.public_value) {
                return Err("original local Node selection differs".into());
            }
        }
        let digest = self.retain_received(candidate, nodes, reserve)?;
        let origin = &self.origin.as_ref().ok_or("Node origin unavailable")?.data;
        let data = Data {
            schema_version: 5,
            domain: SELECTION_DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: Vec::new(),
            references: vec![digest.clone()],
        };
        if self
            .selections
            .get(&height)
            .is_some_and(|old| old.data != data)
        {
            return Err("original Node selection bytes changed".into());
        }
        let path = self.selection_path(height);
        let complete = self.selections.get(&height).is_some_and(|s| s.complete);
        reserve(self.usage_with(Some(&data), nodes)?)?;
        self.selections.insert(
            height,
            Slot {
                data: data.clone(),
                complete: false,
            },
        );
        publish(&self.store, &path, &data, &BTreeMap::new(), complete, false)?;
        self.selections.insert(
            height,
            Slot {
                data,
                complete: true,
            },
        );
        Ok(digest)
    }
}

fn validate_counts(ids: &BTreeMap<u64, BTreeSet<String>>) -> Result<(), String> {
    if ids.values().any(|v| v.len() > MAX_CANDIDATES_PER_HEIGHT)
        || ids.values().map(BTreeSet::len).sum::<usize>() > MAX_CANDIDATES
    {
        return Err("Node candidate count capacity".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_multivalue_total_capacity_is_shared_across_heights() {
        let mut ids = BTreeMap::new();
        for height in 1..=16 {
            ids.insert(height, (0..16).map(|n| format!("{n:064x}")).collect());
        }
        assert!(validate_counts(&ids).is_ok());
        ids.insert(17, BTreeSet::from(["a".repeat(64)]));
        assert!(validate_counts(&ids).is_err());
        ids.remove(&17);
        ids.get_mut(&1).unwrap().insert("f".repeat(64));
        assert!(validate_counts(&ids).is_err());
    }
    #[test]
    fn node_multivalue_inventory_names_are_exact() {
        let id = "a".repeat(64);
        let valid = format!("candidate-{:020}-{id}.intent.cbor", 3);
        assert_eq!(parse_target(&valid).unwrap(), Some((3, Some(id))));
        assert!(parse_target(&valid.replace("intent", "unknown")).is_err());
        assert!(parse_target(&format!("candidate-{:020}-{}.cbor", 3, "A".repeat(64))).is_err());
        assert_eq!(
            parse_target("candidate-00000000000000000003.cbor").unwrap(),
            None
        );
        assert_eq!(
            parse_target("selection-00000000000000000003.reservation.cbor").unwrap(),
            Some((3, None))
        );
    }
}
