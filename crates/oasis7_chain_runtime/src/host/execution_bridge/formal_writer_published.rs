//! Private publication reads. Parsed proofs remain untrusted history input.
use super::*;

#[path = "formal_writer_proof_memo.rs"]
mod proof_memo;

#[cfg(all(test, unix))]
#[path = "formal_writer_published_tests.rs"]
mod tests;

#[cfg(test)]
std::thread_local! {
    static DECODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn decode_count() -> usize {
    DECODE_COUNT.with(std::cell::Cell::get)
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    World::validate_offline_formal_writer_envelope_cbor_budget(
        bytes,
        oasis7_runtime::ObserverReadLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_single_allocation_bytes: 8 * 1024 * 1024,
            max_elements: 24 * 1024 * 1024 + 64 * 1024,
            ..oasis7_runtime::ObserverReadLimits::default()
        },
    )
    .map_err(|error| format!("formal writer envelope budget: {error:?}"))?;
    serde_cbor::from_slice(bytes).map_err(|error| error.to_string())
}

fn publication_bytes(writer_directory: &Path, position: u64) -> Result<Option<Vec<u8>>, String> {
    match std::fs::symlink_metadata(writer_directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => crate::controlled_live_config::require_private_owned(writer_directory, true)?,
    }
    let published = writer_directory.join("published");
    match std::fs::symlink_metadata(&published) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => crate::controlled_live_config::require_private_owned(&published, true)?,
    }
    let path = published.join(format!("{position:020}.cbor"));
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => crate::controlled_live_config::require_private_owned(&path, false)?,
    }
    crate::controlled_history_cli::read_bounded(&path, 64 * 1024 * 1024).map(Some)
}

fn decode_publication(bytes: &[u8], position: u64) -> Result<Published, String> {
    #[cfg(test)]
    DECODE_COUNT.with(|count| count.set(count.get() + 1));
    let publication: Published = decode(bytes)?;
    let record = &publication.candidate.candidate.record;
    let proof_record = &publication.proof.decision.record;
    let record_id = record.record_id().map_err(|error| error.to_string())?;
    if publication.schema_version != 1
        || record.position != position
        || record != proof_record
        || proof_record.position != position
        || proof_record
            .record_id()
            .map_err(|error| error.to_string())?
            != record_id
    {
        return Err("formal writer exact-position proof cache is inconsistent".into());
    }
    Ok(publication)
}

/// Read the current complete file before looking up its parsed proof. A hit
/// saves only repeated budget scanning/deserialization of these exact bytes;
/// the caller still verifies every signature, context and history transition.
pub(crate) fn read_published_proof_at(
    writer_directory: &Path,
    position: u64,
) -> Result<Option<FormalQualifiedCommitProofV1>, String> {
    let Some(bytes) = publication_bytes(writer_directory, position)? else {
        return Ok(None);
    };
    let key = proof_memo::key(&bytes);
    if let Some(proof) = proof_memo::get(&key) {
        // Copying the same publication bytes to another filename cannot make
        // its previously checked record identity valid at that position.
        if proof.decision.record.position != position {
            return Err("formal writer exact-position proof cache is inconsistent".into());
        }
        return Ok(Some(proof));
    }
    let publication = decode_publication(&bytes, position)?;
    proof_memo::insert(key, &publication.proof);
    Ok(Some(publication.proof))
}

/// Material consumers always perform the original complete decode. Seed only
/// its parsed proof after envelope and record/proof identity checks succeed.
pub(crate) fn read_published_candidate_at(
    writer_directory: &Path,
    position: u64,
) -> Result<Option<(FormalCandidateMaterialV1, FormalQualifiedCommitProofV1)>, String> {
    let Some(bytes) = publication_bytes(writer_directory, position)? else {
        return Ok(None);
    };
    let publication = decode_publication(&bytes, position)?;
    proof_memo::insert(proof_memo::key(&bytes), &publication.proof);
    Ok(Some((publication.candidate, publication.proof)))
}
