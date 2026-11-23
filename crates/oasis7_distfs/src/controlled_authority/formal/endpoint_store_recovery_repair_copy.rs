//! The one original provider-bound unresolved entry is copied under the same
//! typed graph walker as committed history. No other pending slot is admitted.
use super::*;

pub(super) fn enqueue_original_pending(
    entry: &FormalRequestStateV1,
    profile_version: u32,
    work: &mut Vec<Visit>,
    mut memo: Option<&mut CopyReadMemo<'_>>,
    objects: &Path,
    source: &dyn FormalArchiveSourceV1,
    copy: bool,
) -> Result<(), FormalError> {
    let mut materials = Vec::new();
    match entry {
        FormalRequestStateV1::Frozen {
            input,
            material,
            certificate,
            ..
        } => {
            let frozen = certificate
                .as_ref()
                .ok_or_else(|| invalid("repair Frozen certificate missing"))?;
            if frozen.reservation != *input {
                return Err(invalid("repair Frozen original changed"));
            }
            verify_frozen_material(&input.body, material)?;
            materials.push((material, false));
        }
        FormalRequestStateV1::Prepared { candidate, .. }
        | FormalRequestStateV1::Decided { candidate, .. }
        | FormalRequestStateV1::Ready { candidate, .. } => {
            verify_record_material(&candidate.candidate.record, &candidate.material)?;
            let frozen = candidate
                .frozen
                .as_ref()
                .ok_or_else(|| invalid("repair Prepared Frozen certificate missing"))?;
            let material = candidate
                .frozen_material
                .as_ref()
                .ok_or_else(|| invalid("repair Prepared original material missing"))?;
            frozen.matches_record(&candidate.candidate.record)?;
            verify_frozen_material(&frozen.reservation.body, material)?;
            materials.push((&candidate.material, true));
            materials.push((material, false));
        }
        _ => {
            return Err(invalid("repair requires the original unresolved entry"));
        }
    }
    for (material, result_tree) in materials {
        if material
            .native_environment()
            .map_err(|error| invalid(error.to_string()))?
            .is_some()
        {
            crate::controlled_authority::formal::native_environment::verify_material_chunks(
                material,
                |id| {
                    let bytes = if copy {
                        source.read_object(id)?
                    } else {
                        crate::controlled_authority::formal::business_pack::read_object_durable(
                            &objects.join(id),
                            id,
                        )?
                    };
                    bytes.ok_or_else(|| invalid("repair native material chunk missing"))
                },
                |_, _| Ok(()),
            )?;
        }
        enqueue_material(
            material,
            profile_version,
            result_tree,
            work,
            memo.as_deref_mut(),
        )?;
    }
    Ok(())
}
