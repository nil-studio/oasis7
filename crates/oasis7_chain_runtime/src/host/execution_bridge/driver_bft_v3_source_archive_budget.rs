//! Pure original pricing after fixed H and every earlier original have been
//! independently authenticated and actually replayed by the archive consumer.
use super::*;
pub(in super::super::super::super) fn source_admission_archive_reservation_bytes(
    input: &Input,
    plan: &super::super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
    retained: &BTreeSet<String>,
) -> Result<u64, String> {
    let data = input_data(input)?;
    let publication = PublicationReservationV1::from_plan(&publication_identity(&data)?, plan)?;
    let dependencies = nodes
        .iter()
        .filter(|(digest, _)| !retained.contains(*digest))
        .try_fold(0usize, |sum, (_, bytes)| {
            sum.checked_add(bytes.len())
                .ok_or("archived source dependency growth overflow")
        })?;
    let bytes = data
        .bytes()?
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(publication.accounted_bytes().ok()?))
        .and_then(|n| n.checked_add(private_pin_bound(&data).ok()?))
        .and_then(|n| n.checked_add(source_selection_bound(&data).ok()?))
        .and_then(|n| n.checked_add(dependencies))
        .filter(|n| *n <= 512 * 1024 * 1024)
        .ok_or("archived source original reservation capacity")?;
    u64::try_from(bytes).map_err(|_| "archived source original reservation conversion".into())
}
