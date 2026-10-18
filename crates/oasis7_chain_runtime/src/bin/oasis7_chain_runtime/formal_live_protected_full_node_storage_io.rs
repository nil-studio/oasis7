//! Quiescent complete-directory accounting and fresh logical/typed receipts.
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::{
        endpoint::{FileFormalEndpoint, FormalEndpointAuthorizationV1},
        fencing::{RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    },
    storage_inventory::inventory_storage,
};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

pub(super) fn digest<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_cbor::to_vec(value)
        .map(|bytes| blake3_hex(&bytes))
        .map_err(|e| e.to_string())
}
pub(super) fn inventory(root: &Path) -> Result<Value, String> {
    let total = inventory_storage(root);
    if !total.complete {
        return Err(format!(
            "incomplete full-node inventory: {:?}",
            total.errors
        ));
    }
    let mut attribution = BTreeMap::new();
    for name in ["primary-data", "replica-data", "formal-writer"] {
        let path = root.join(name);
        let value = inventory_storage(&path);
        if !value.complete {
            return Err(format!("incomplete {name} inventory"));
        }
        let mut categories = BTreeMap::new();
        for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let value = inventory_storage(&entry.path());
            if !value.complete {
                return Err("incomplete category inventory".into());
            }
            categories.insert(entry.file_name().to_string_lossy().to_string(), value);
        }
        attribution.insert(
            name,
            serde_json::json!({"inventory":value,"categories":categories}),
        );
    }
    Ok(
        serde_json::json!({"complete":true,"total":total,"attribution":attribution,
        "scope":"entire fixture root, including P/R native executable/environment CAS, endpoint checkpoints/provider files, generation metadata, writer publications/attempts and private fixture metadata; shared installed binaries/build caches outside root excluded; categories are attribution and must not be summed"}),
    )
}
pub(super) fn allocated_change(before: &Value, after: &Value) -> Result<i64, String> {
    let before = before["total"]["allocated_bytes"]
        .as_u64()
        .ok_or("missing initial allocated bytes")?;
    let after = after["total"]["allocated_bytes"]
        .as_u64()
        .ok_or("missing final allocated bytes")?;
    i64::try_from(after as i128 - before as i128).map_err(|_| "allocated change overflow".into())
}
/// Hash every immutable writer publication, including its exact canonical name.
/// Sorting is bounded acceptance metadata; original bytes are streamed in 64KiB.
pub(super) fn published_receipt(
    root: &Path,
    original: u64,
    head: u64,
    budget: &mut Budget,
) -> Result<Value, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let directory = root.join("formal-writer").join("published");
    crate::controlled_live_config::require_private_owned(&directory, true)?;
    let mut names = BTreeMap::new();
    for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "publication name encoding")?;
        let position = name
            .strip_suffix(".cbor")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or("publication name is not a position")?;
        if name != format!("{position:020}.cbor")
            || position == 0
            || position > head
            || names.len() >= 65536
        {
            return Err("publication inventory filename/count/head budget".into());
        }
        names.insert(name, (position, entry.path()));
    }
    let mut receipt = Sha256::new();
    let mut bytes = 0_u64;
    let mut original_digest = None;
    let mut head_digest = None;
    for (index, (name, (position, path))) in names.iter().enumerate() {
        budget.check("writer_published_readback", index as u64)?;
        crate::controlled_live_config::require_private_owned(path, false)?;
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
            return Err("publication original-byte budget/type".into());
        }
        let mut hash = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            length = length
                .checked_add(count as u64)
                .ok_or("publication byte overflow")?;
            if length > metadata.len() {
                return Err("publication changed while hashing".into());
            }
            hash.update(&buffer[..count]);
        }
        if length != metadata.len() {
            return Err("publication was truncated while hashing".into());
        }
        let digest = hex::encode(hash.finalize());
        receipt.update((name.len() as u64).to_be_bytes());
        receipt.update(name.as_bytes());
        receipt.update(length.to_be_bytes());
        receipt.update(digest.as_bytes());
        bytes = bytes
            .checked_add(length)
            .ok_or("publication total byte overflow")?;
        if *position == original {
            original_digest = Some(digest.clone());
        }
        if *position == head {
            head_digest = Some(digest);
        }
    }
    if original_digest.is_none() || head_digest.is_none() {
        return Err("original or fixed head publication missing".into());
    }
    Ok(
        serde_json::json!({"complete":true,"files":names.len(),"bytes":bytes,"digest":hex::encode(receipt.finalize()),
        "original_position":original,"original_digest":original_digest,"fixed_head_position":head,"fixed_head_digest":head_digest,
        "file_count_budget":65536,"per_file_byte_budget":64*1024*1024,"read_buffer_bytes":65536}),
    )
}
pub(super) fn typed_history(
    world: &oasis7_runtime::World,
    budget: &mut Budget,
) -> Result<Value, String> {
    let mut hash = Sha256::new();
    let count = world.tick_consensus_record_count();
    for height in 1..=count {
        budget.check("typed_history", height)?;
        let record = world
            .read_tick_consensus_record(height)
            .map_err(|e| format!("typed Tick {height}: {e:?}"))?
            .ok_or_else(|| format!("typed Tick {height} missing"))?;
        let bytes = serde_cbor::to_vec(&record).map_err(|e| e.to_string())?;
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    Ok(
        serde_json::json!({"records":count,"digest":hex::encode(hash.finalize()),"snapshot":digest(&world.snapshot())?,"journal":digest(world.journal())?}),
    )
}
pub(super) fn fix_listen(config: &Path, address: std::net::SocketAddr) -> Result<(), String> {
    let mut value: Value = serde_json::from_slice(&fs::read(config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    value["listen"] = Value::String(address.to_string());
    fs::write(
        config,
        serde_json::to_vec(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub(super) fn open_endpoint(config: &Path) -> Result<FileFormalEndpoint, String> {
    #[derive(serde::Deserialize)]
    struct Config {
        initial_trust: InitialActivationTrustV1,
        initial_certificate: SignedInitialActivationV1,
        initial_qualified: Option<FormalQualifiedCommitProofV1>,
        role: EndpointRole,
        data_dir: PathBuf,
        oracle_authentication_key_file: PathBuf,
        oracle_address: std::net::SocketAddr,
        timeout_ms: u64,
    }
    let config: Config = serde_json::from_slice(&fs::read(config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let authority = verify_initial_activation(&config.initial_certificate, &config.initial_trust)
        .map_err(|e| e.to_string())?;
    let parent = config
        .initial_qualified
        .as_ref()
        .map(|proof| verify_formal_qualified(proof, &authority, None, None))
        .transpose()
        .map_err(|e| e.to_string())?;
    let authentication: [u8; 32] = hex::decode(
        fs::read_to_string(config.oracle_authentication_key_file)
            .map_err(|e| e.to_string())?
            .trim(),
    )
    .map_err(|e| e.to_string())?
    .try_into()
    .map_err(|_| "oracle authentication length")?;
    let provider = RemoteMonotonicEndpointProvider::new(
        config.oracle_address,
        Duration::from_millis(config.timeout_ms),
        SigningKey::from_bytes(&authentication),
        TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(|e| e.to_string())?;
    FileFormalEndpoint::open_protected(
        &config.data_dir,
        FormalEndpointAuthorizationV1 {
            original: authority.clone(),
            authority,
            recovery: None,
            parent,
        },
        config.role,
        Arc::new(provider),
        None,
    )
    .map_err(|e| e.to_string())
}
/// Complete ordinary open/guard recovery after killing an owned server, then
/// close this synchronous owner. No automatic server maintenance can run here.
pub(super) fn settle_endpoint(config: &Path) -> Result<(), String> {
    let mut endpoint = open_endpoint(config)?;
    endpoint.head().map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) fn logical_inventory(
    endpoint: &mut FileFormalEndpoint,
    budget: &mut Budget,
) -> Result<Value, String> {
    let mut cursor: Option<String> = None;
    let mut namespace: Option<String> = None;
    let mut hash = Sha256::new();
    let mut objects = 0_u64;
    let mut bytes = 0_u64;
    loop {
        budget.check("logical_inventory", objects)?;
        let page = endpoint
            .storage_logical_inventory_page(cursor.as_deref())
            .map_err(|e| e.to_string())?;
        if namespace
            .as_ref()
            .is_some_and(|value| value != &page.namespace_digest)
        {
            return Err("logical namespace changed within receipt".into());
        }
        namespace = Some(page.namespace_digest);
        for object in &page.objects {
            hash.update(object.digest.as_bytes());
            hash.update(object.length.to_be_bytes());
            objects = objects.checked_add(1).ok_or("object count overflow")?;
            bytes = bytes
                .checked_add(object.length)
                .ok_or("logical bytes overflow")?;
        }
        if page.complete {
            if page.next.is_some() {
                return Err("complete logical receipt has next cursor".into());
            }
            break;
        }
        if page.objects.is_empty() || page.next.is_none() || page.next == cursor {
            return Err("logical receipt did not progress".into());
        }
        cursor = page.next;
    }
    Ok(
        serde_json::json!({"retention_records_including_duplicate_digests":objects,"logical_bytes_including_duplicate_records":bytes,"digest":hex::encode(hash.finalize())}),
    )
}
