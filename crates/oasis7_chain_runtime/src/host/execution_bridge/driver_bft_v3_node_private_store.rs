//! Original private Node candidates, retained before any remote signing request.
//! The canonical ledger's OS claim protects this subordinate directory.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

const MAX_RECORD: usize = 4 * 1024 * 1024;
const SCOPE: &str = "formal-bft-header-node-private-v3";
const MARKER: &[u8] = b"oasis7.formal.bft.node_private.required/v3\n";
const DOMAIN: &str = "oasis7.formal.bft.node_private.v3";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    schema_version: u32,
    domain: String,
    origin: bool,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    value: Option<BftHeaderValueV3>,
    #[serde(deserialize_with = "super::super::super::private::deserialize_actions")]
    actions_cbor: Vec<u8>,
    #[serde(deserialize_with = "bounded_references")]
    references: Vec<String>,
}
impl Data {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 3
            || self.domain != DOMAIN
            || self.height == 0
            || !canonical_hash(&self.ledger_origin_digest)
            || !canonical_hash(&self.material_identity)
            || !canonical_hash(&self.instance_id)
            || self.references.len()
                > super::super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self.references.windows(2).all(|pair| pair[0] < pair[1])
            || self
                .references
                .iter()
                .any(|reference| !canonical_hash(reference))
        {
            return Err("invalid original Node private record identity or capacity".into());
        }
        if self.origin {
            if self.value.is_some() || !self.actions_cbor.is_empty() {
                return Err("Node H retention record contains candidate data".into());
            }
        } else {
            let value = self
                .value
                .as_ref()
                .ok_or("Node candidate has no public value")?;
            if value.execution_commitment.height != self.height {
                return Err("Node private candidate height differs from commitment".into());
            }
            ValidatedBftPrivatePayloadV3::decode(value.validate_binding()?, &self.actions_cbor)
                .map_err(|_| "original Node private candidate binding rejected")?;
        }
        Ok(())
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode original private Node record")?;
        if bytes.len() > MAX_RECORD {
            return Err("private Node record byte capacity".into());
        }
        Ok(bytes)
    }
    fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&self.bytes()?))
    }
    fn retained(&self) -> Result<BTreeSet<String>, String> {
        Ok(self
            .references
            .iter()
            .cloned()
            .chain([self.digest()?])
            .collect())
    }
    fn shard(&self) -> String {
        if self.origin {
            format!("ledger-{}-origin", self.ledger_origin_digest)
        } else {
            format!(
                "ledger-{}-height-{:020}",
                self.ledger_origin_digest, self.height
            )
        }
    }
}

#[derive(Clone)]
struct Slot {
    data: Data,
    complete: bool,
}

pub(super) struct NodePrivateStore {
    instance_id: String,
    dir: PathBuf,
    store: LocalCasStore,
    origin: Option<Slot>,
    candidates: BTreeMap<u64, Slot>,
}

struct SeedReader {
    nodes: BTreeMap<String, Vec<u8>>,
    pending_references: Option<BTreeSet<String>>,
    fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}
impl BusinessIndexNodeReader for SeedReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.nodes.get(digest) {
            return Ok(Some(bytes.clone()));
        }
        if self
            .pending_references
            .as_ref()
            .is_none_or(|refs| refs.contains(digest))
        {
            return self.fallback.read_node(digest);
        }
        Ok(None)
    }
}

impl NodePrivateStore {
    /// A completed H record supplies only its original verified CAS nodes.
    /// Fallback is allowed solely before original Node initialization completes.
    pub(super) fn open(
        records: &Path,
        store: LocalCasStore,
        fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<(Self, Arc<dyn BusinessIndexNodeReader + Send + Sync>), String> {
        let dir = records
            .join(storage::DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY);
        if fs::symlink_metadata(&dir).is_ok() {
            crate::controlled_live_config::require_private_owned(&dir, true)?;
        }
        let origin = read_slot(&dir.join("origin.cbor"))?;
        let instance_id = match &origin {
            Some(slot) => slot.data.instance_id.clone(),
            None => {
                let mut nonce = [0u8; 32];
                getrandom::fill(&mut nonce)
                    .map_err(|_| "generate private Node instance identity")?;
                hex::encode(nonce)
            }
        };
        let mut nodes = BTreeMap::new();
        let pending_references = match &origin {
            Some(slot) => {
                if !slot.data.origin {
                    return Err("Node origin has candidate schema".into());
                }
                let marker = common_storage::read_optional_bytes(&dir.join("ready"), MARKER.len())?;
                if marker.is_some()
                    && common_storage::read_optional_bytes(&dir.join("origin.cbor"), MAX_RECORD)?
                        .is_none()
                {
                    return Err("Node ready marker lost its original origin record".into());
                }
                if slot.complete && marker.as_deref() != Some(MARKER) {
                    return Err("completed Node origin lost its original marker".into());
                }
                let retention_required = retention_required(&dir.join("origin.cbor"))?;
                if retention_required {
                    verify_material(&store, &slot.data)?;
                }
                let mut retained_bytes = 0usize;
                for reference in &slot.data.references {
                    match common_storage::read_cas_bounded(
                        &store,
                        reference,
                        formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    ) {
                        Ok(bytes) => {
                            retained_bytes = retained_bytes
                                .checked_add(bytes.len())
                                .ok_or("Node H dependency byte capacity")?;
                            if retained_bytes > formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES
                            {
                                return Err("Node H dependency byte capacity".into());
                            }
                            nodes.insert(reference.clone(), bytes);
                        }
                        Err(_) if !retention_required => {}
                        Err(_) => return Err("original Node H dependency is missing".into()),
                    }
                }
                if slot.complete {
                    Some(BTreeSet::new())
                } else {
                    Some(slot.data.references.iter().cloned().collect())
                }
            }
            None => {
                match fs::read_dir(&dir) {
                    Ok(mut entries) => {
                        if entries.next().is_some() {
                            return Err("Node private files lost their original origin".into());
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err("inspect original Node private directory".into()),
                }
                None
            }
        };
        let source = Arc::new(SeedReader {
            nodes,
            pending_references,
            fallback,
        });
        Ok((
            Self {
                instance_id,
                dir,
                store,
                origin,
                candidates: BTreeMap::new(),
            },
            source,
        ))
    }

    pub(super) fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub(super) fn bind_origin(
        &mut self,
        ledger_origin: &str,
        material: &str,
        h: u64,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize) -> Result<(), String>,
    ) -> Result<(), String> {
        let data = Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: true,
            ledger_origin_digest: ledger_origin.into(),
            material_identity: material.into(),
            instance_id: self.instance_id.clone(),
            height: h,
            value: None,
            actions_cbor: Vec::new(),
            references: nodes.keys().cloned().collect(),
        };
        if self.origin.as_ref().is_some_and(|slot| slot.data != data) {
            return Err("Node private origin differs from its exact qualified H".into());
        }
        if self.origin.is_none() {
            let prefix = format!("ledger-{}-", data.ledger_origin_digest);
            if self
                .store
                .list_pin_scope_shards(SCOPE)
                .map_err(|_| "inspect original Node H receipt")?
                .iter()
                .any(|shard| shard.starts_with(&prefix))
            {
                return Err(
                    "original Node H metadata is missing; receipt cannot initialize it".into(),
                );
            }
        }
        super::super::super::super::super::super::super::super::controlled_live::private_directory(
            &self.dir,
        )?;
        self.origin.get_or_insert(Slot {
            data: data.clone(),
            complete: false,
        });
        self.load_inventory(ledger_origin, material, h)?;
        reserve(self.usage_with(None, nodes)?)?;
        publish(
            &self.store,
            &self.dir.join("origin.cbor"),
            &data,
            nodes,
            self.origin.as_ref().unwrap().complete,
            true,
        )?;
        self.origin = Some(Slot {
            data,
            complete: true,
        });
        self.verify_inventory_pins()?;
        Ok(())
    }

    fn load_inventory(&mut self, origin: &str, material: &str, h: u64) -> Result<(), String> {
        let mut heights = BTreeSet::new();
        for (count, entry) in fs::read_dir(&self.dir)
            .map_err(|_| "read private Node inventory")?
            .enumerate()
        {
            if count > 3 * 64 + 8 {
                return Err("private Node directory capacity".into());
            }
            let entry = entry.map_err(|_| "inspect private Node inventory")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 private Node file")?;
            let target = files::stage_target(&name).unwrap_or(&name);
            if [
                "origin.cbor",
                "origin.intent.cbor",
                "origin.reservation.cbor",
                "ready",
            ]
            .contains(&target)
            {
                continue;
            }
            let rest = target
                .strip_prefix("candidate-")
                .ok_or("unknown private Node file; preserved")?;
            let digits = rest.get(..20).ok_or("invalid private Node filename")?;
            let suffix = rest.get(20..).ok_or("invalid private Node filename")?;
            if ![".cbor", ".intent.cbor", ".reservation.cbor"].contains(&suffix)
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("invalid private Node candidate filename".into());
            }
            let height: u64 = digits.parse().map_err(|_| "private Node height overflow")?;
            if height <= h || height - h > 64 {
                return Err("private Node height capacity".into());
            }
            heights.insert(height);
        }
        for height in heights {
            let slot = read_slot(&self.candidate_path(height))?
                .ok_or("private Node candidate identity is missing")?;
            if slot.data.origin
                || slot.data.height != height
                || slot.data.ledger_origin_digest != origin
                || slot.data.material_identity != material
                || slot.data.instance_id != self.instance_id
            {
                return Err("private Node candidate differs from original H identity".into());
            }
            if retention_required(&self.candidate_path(height))? {
                verify_material(&self.store, &slot.data)?;
            }
            self.candidates.insert(height, slot);
        }
        Ok(())
    }
    fn verify_inventory_pins(&self) -> Result<(), String> {
        let origin = self.origin.as_ref().ok_or("Node origin unavailable")?;
        let prefix = format!("ledger-{}-", origin.data.ledger_origin_digest);
        let known: BTreeSet<_> = std::iter::once(origin.data.shard())
            .chain(self.candidates.values().map(|slot| slot.data.shard()))
            .collect();
        for shard in self
            .store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "read Node retention inventory")?
        {
            if shard.starts_with(&prefix) && !known.contains(&shard) {
                return Err("Node retention receipt lost its original private metadata".into());
            }
        }
        Ok(())
    }
    pub(super) fn origin_nodes(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let slot = self.origin.as_ref().ok_or("Node origin unavailable")?;
        verify_original_slot(&self.store, &self.dir.join("origin.cbor"), slot)?;
        if common_storage::read_optional_bytes(&self.dir.join("ready"), MARKER.len())?.as_deref()
            != Some(MARKER)
        {
            return Err("original Node ready marker is missing or changed".into());
        }
        let origin = &slot.data;
        origin
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
            .collect()
    }
    pub(super) fn candidate(
        &self,
        height: u64,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        let Some(slot) = self.candidates.get(&height) else {
            return Ok(None);
        };
        verify_original_slot(&self.store, &self.candidate_path(height), slot)?;
        Ok(Some(NodeBftPrivateCandidateV3 {
            public_value: slot
                .data
                .value
                .clone()
                .ok_or("private Node candidate value missing")?,
            actions_cbor: slot.data.actions_cbor.clone(),
        }))
    }
    pub(super) fn retain(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize) -> Result<(), String>,
    ) -> Result<String, String> {
        let origin = &self
            .origin
            .as_ref()
            .ok_or("private Node origin unavailable")?
            .data;
        let height = candidate.public_value.execution_commitment.height;
        if height <= origin.height || height - origin.height > 64 {
            return Err("private Node height capacity".into());
        }
        let data = Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: candidate.actions_cbor.clone(),
            references: nodes.keys().cloned().collect(),
        };
        data.validate()?;
        if self
            .candidates
            .get(&height)
            .is_some_and(|slot| slot.data != data)
        {
            return Err(
                "Node height already retains a different original private candidate".into(),
            );
        }
        if let Some(slot) = self.candidates.get(&height) {
            verify_original_slot(&self.store, &self.candidate_path(height), slot)?;
        }
        let complete = self
            .candidates
            .get(&height)
            .is_some_and(|slot| slot.complete);
        reserve(self.usage_with(Some(&data), nodes)?)?;
        self.candidates.entry(height).or_insert(Slot {
            data: data.clone(),
            complete: false,
        });
        publish(
            &self.store,
            &self.candidate_path(height),
            &data,
            nodes,
            complete,
            false,
        )?;
        self.candidates.insert(
            height,
            Slot {
                data: data.clone(),
                complete: true,
            },
        );
        Ok(data.digest()?)
    }
    pub(super) fn usage(&self) -> Result<usize, String> {
        self.usage_with(None, &BTreeMap::new())
    }
    fn usage_with(
        &self,
        additional: Option<&Data>,
        nodes: &BTreeMap<String, Vec<u8>>,
    ) -> Result<usize, String> {
        let mut bytes = 0usize;
        let mut refs = BTreeSet::new();
        for data in self
            .origin
            .iter()
            .map(|slot| &slot.data)
            .chain(
                self.candidates
                    .values()
                    .filter(|slot| additional.is_none_or(|a| a.height != slot.data.height))
                    .map(|slot| &slot.data),
            )
            .chain(additional)
        {
            bytes = bytes
                .checked_add(
                    data.bytes()?
                        .len()
                        .checked_mul(2)
                        .ok_or("Node capacity overflow")?,
                )
                .filter(|sum| *sum <= 512 * 1024 * 1024)
                .ok_or("Node cumulative material capacity")?;
            refs.extend(data.references.iter().cloned());
        }
        if refs.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
            return Err("Node retained index count capacity".into());
        }
        for reference in refs {
            let size = if let Some(data) = nodes.get(&reference) {
                data.len()
            } else {
                common_storage::read_cas_bounded(
                    &self.store,
                    &reference,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )?
                .len()
            };
            bytes = bytes
                .checked_add(size)
                .filter(|sum| *sum <= 512 * 1024 * 1024)
                .ok_or("Node cumulative material capacity")?;
        }
        Ok(bytes)
    }
    fn candidate_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("candidate-{height:020}.cbor"))
    }
}

fn read_slot(path: &Path) -> Result<Option<Slot>, String> {
    let parent = path.parent().ok_or("private Node record parent missing")?;
    match fs::symlink_metadata(parent) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("inspect private Node record directory".into()),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err("invalid private Node record directory".into());
        }
        Ok(_) => {}
    }
    let intent = path.with_extension("intent.cbor");
    let reservation = path.with_extension("reservation.cbor");
    let record: Option<Data> = common_storage::read_optional_canonical(path, MAX_RECORD)
        .map_err(|_| "invalid original private Node record")?;
    let pending: Option<Data> = common_storage::read_optional_canonical(&intent, MAX_RECORD)
        .map_err(|_| "invalid original private Node intent")?;
    let reserved: Option<Data> = common_storage::read_optional_canonical(&reservation, MAX_RECORD)
        .map_err(|_| "invalid original private Node reservation")?;
    let staged_intent: Option<Data> = files::staged_value(&intent, MAX_RECORD)
        .map_err(|_| "invalid original private Node intent stage")?;
    let staged_reservation: Option<Data> = files::staged_value(&reservation, MAX_RECORD)
        .map_err(|_| "invalid original private Node reservation stage")?;
    if !files::stages(path)?.is_empty() {
        return Err("private Node record has unsupported direct staging".into());
    }
    let values = [
        record.as_ref(),
        pending.as_ref(),
        reserved.as_ref(),
        staged_intent.as_ref(),
        staged_reservation.as_ref(),
    ];
    let Some(first) = values.into_iter().flatten().next() else {
        return Ok(None);
    };
    first.validate()?;
    if values.into_iter().flatten().any(|data| data != first) {
        return Err("private Node original transaction bytes conflict".into());
    }
    match (record.is_some(), pending.is_some()) {
        (true, true) => files::same_pair(path, &intent)?,
        (true, false) => {
            files::single_link(path)?;
            if reserved.is_some() || staged_intent.is_some() || staged_reservation.is_some() {
                return Err("completed Node record has foreign pending material".into());
            }
        }
        (false, true) => files::single_link(&intent)?,
        _ => {}
    }
    if reserved.is_some() {
        files::single_link(&reservation)?;
    }
    Ok(Some(Slot {
        data: first.clone(),
        complete: record.is_some() && pending.is_none(),
    }))
}
fn verify_original_slot(store: &LocalCasStore, path: &Path, expected: &Slot) -> Result<(), String> {
    let actual = read_slot(path)?.ok_or("original private Node metadata is missing")?;
    if actual.data != expected.data || (expected.complete && !actual.complete) {
        return Err("original private Node metadata changed or regressed".into());
    }
    if retention_required(path)? {
        verify_material(store, &actual.data)?;
    }
    Ok(())
}
fn verify_material(store: &LocalCasStore, data: &Data) -> Result<(), String> {
    store
        .verify_pin_scope_shard(SCOPE, &data.shard(), &data.retained()?)
        .map_err(|_| "original Node retention receipt missing or changed")?;
    if common_storage::read_cas_bounded(store, &data.digest()?, MAX_RECORD)? != data.bytes()? {
        return Err("original private Node CAS record changed".into());
    }
    for reference in &data.references {
        common_storage::read_cas_bounded(
            store,
            reference,
            formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
        )?;
    }
    Ok(())
}
fn retention_required(path: &Path) -> Result<bool, String> {
    let intent = path.with_extension("intent.cbor");
    Ok(
        common_storage::read_optional_bytes(path, MAX_RECORD)?.is_some()
            || common_storage::read_optional_bytes(&intent, MAX_RECORD)?.is_some()
            || !files::stages(&intent)?.is_empty(),
    )
}
fn publish(
    store: &LocalCasStore,
    path: &Path,
    data: &Data,
    nodes: &BTreeMap<String, Vec<u8>>,
    complete: bool,
    origin: bool,
) -> Result<(), String> {
    if complete {
        return verify_material(store, data);
    }
    // An intent or record can exist only after original retention succeeded.
    // Its loss cannot authorize a new pin publication, even before finish.
    let already_retained = retention_required(path)?;
    if already_retained {
        verify_material(store, data)?;
    }
    let bytes = data.bytes()?;
    let intent = path.with_extension("intent.cbor");
    let reservation = path.with_extension("reservation.cbor");
    if common_storage::read_optional_bytes(&intent, MAX_RECORD)?.is_none()
        && common_storage::read_optional_bytes(path, MAX_RECORD)?.is_none()
    {
        files::publish(&reservation, &bytes)?;
    }
    let has_pins = store
        .list_pin_scope_shards(SCOPE)
        .map_err(|_| "inspect Node original pins")?
        .contains(&data.shard());
    if has_pins {
        verify_material(store, data)?;
    } else {
        if already_retained {
            return Err("original Node retention receipt is missing".into());
        }
        common_storage::put_cas_durable(store, &bytes, MAX_RECORD)?;
        for reference in &data.references {
            if let Some(bytes) = nodes.get(reference) {
                if blake3_hex(bytes) != *reference {
                    return Err("Node retained dependency identity mismatch".into());
                }
                common_storage::put_cas_durable(
                    store,
                    bytes,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )?;
            } else {
                common_storage::read_cas_bounded(
                    store,
                    reference,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )?;
            }
        }
        store
            .replace_pin_scope_shard(SCOPE, &data.shard(), &data.retained()?)
            .map_err(|_| "publish original Node pins")?;
        verify_material(store, data)?;
    }
    if common_storage::read_optional_bytes(path, MAX_RECORD)?.is_some() {
        files::same_pair(path, &intent)?;
    } else {
        files::promote(&reservation, &intent, &bytes)?;
        files::link_record(path, &intent, &bytes)?;
    }
    if origin {
        files::publish(
            &path
                .parent()
                .ok_or("Node origin parent missing")?
                .join("ready"),
            MARKER,
        )?;
    }
    files::finish(path, &intent, &bytes)
}
fn canonical_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn bounded_references<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Refs;
    impl<'de> serde::de::Visitor<'de> for Refs {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded Node references")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let max = super::super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES;
            if seq.size_hint().is_some_and(|n| n > max) {
                return Err(serde::de::Error::custom("Node references exceed bound"));
            }
            let mut values = Vec::new();
            while values.len() < max {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("Node references exceed bound"));
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::fs::PermissionsExt;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "oasis7-node-private-{}-{suffix}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn data() -> Data {
        Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: true,
            ledger_origin_digest: "a".repeat(64),
            material_identity: "b".repeat(64),
            instance_id: "c".repeat(64),
            height: 3,
            value: None,
            actions_cbor: Vec::new(),
            references: Vec::new(),
        }
    }

    #[test]
    fn formal_read_node_private_intent_and_link_cannot_republish_lost_pins() {
        for linked in [false, true] {
            let root = Scratch::new();
            let store = LocalCasStore::new(root.path().join("cas"));
            let path = root.path().join("origin.cbor");
            let intent = path.with_extension("intent.cbor");
            let original = data();
            let bytes = original.bytes().unwrap();
            files::publish(&intent, &bytes).unwrap();
            if linked {
                files::link_record(&path, &intent, &bytes).unwrap();
            }
            common_storage::put_cas_durable(&store, &bytes, MAX_RECORD).unwrap();
            store
                .replace_pin_scope_shard(SCOPE, &original.shard(), &original.retained().unwrap())
                .unwrap();
            store
                .remove_pin_scope_shard(SCOPE, &original.shard())
                .unwrap();
            assert!(publish(&store, &path, &original, &BTreeMap::new(), false, true).is_err());
            assert_eq!(fs::read(&intent).unwrap(), bytes);
            assert_eq!(path.exists(), linked);
            assert!(!path.parent().unwrap().join("ready").exists());
            assert!(store.list_pin_scope_shards(SCOPE).unwrap().is_empty());
        }
    }

    #[test]
    fn formal_read_node_private_live_metadata_loss_is_rejected() {
        let root = Scratch::new();
        let store = LocalCasStore::new(root.path().join("cas"));
        let path = root.path().join("origin.cbor");
        let original = data();
        publish(&store, &path, &original, &BTreeMap::new(), false, true).unwrap();
        let slot = read_slot(&path).unwrap().unwrap();
        assert!(slot.complete);
        verify_original_slot(&store, &path, &slot).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(verify_original_slot(&store, &path, &slot).is_err());
        assert!(!path.exists());
        verify_material(&store, &original).unwrap();
    }
}
