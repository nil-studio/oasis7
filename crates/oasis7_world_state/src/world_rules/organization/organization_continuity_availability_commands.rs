use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_continuity_availability_command(
    state: &WorldState,
    continuity: &mut OrganizationContinuityStateV1,
    command: &Command,
    now: u64,
    world_binding: &str,
) -> Result<String, WorldError> {
    match command {
        Command::RecordAvailability {
            organization_id,
            from_world_tick,
            through_world_tick,
            world_writes_available,
            notice_available,
            remedy_available,
            certificate,
        } => {
            if world_binding.is_empty() {
                return Err(invalid("world_binding_required"));
            }
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            let accounted_through = accounted_through(continuity, &registration);
            let intervals = continuity
                .availability
                .entry(organization_id.clone())
                .or_default();
            let expected_from = accounted_through.saturating_add(1);
            if *from_world_tick != expected_from
                || through_world_tick < from_world_tick
                || *through_world_tick > now
                || through_world_tick
                    .saturating_sub(*from_world_tick)
                    .saturating_add(1)
                    > MAX_INTERVAL_TICKS
            {
                return Err(invalid("availability_interval_not_contiguous_or_bounded"));
            }
            if manifest_root_for_organization(state, &registration)? != registration.manifest_root {
                return Err(invalid("organization_manifest_drift"));
            }
            validate_availability_certificate(
                state,
                &registration,
                world_binding,
                *from_world_tick,
                *through_world_tick,
                *world_writes_available,
                *notice_available,
                *remedy_available,
                certificate,
            )?;
            let interval = OrganizationAvailabilityIntervalV1 {
                from_world_tick: *from_world_tick,
                through_world_tick: *through_world_tick,
                world_writes_available: *world_writes_available,
                notice_available: *notice_available,
                remedy_available: *remedy_available,
                evidence_status: OrganizationAvailabilityEvidenceStatusV1::Certified,
                world_binding: world_binding.to_owned(),
                registration_receipt: registration.registration_receipt,
                certificate_hash: crate::util::hash_json(certificate)?,
            };
            intervals.push(interval);
            let registration = continuity
                .registrations
                .get_mut(organization_id)
                .expect("registration validated above");
            registration.last_accounted_world_tick = *through_world_tick;
            registration.availability_paused_reason = if *through_world_tick < now {
                Some("availability_unknown".into())
            } else if !(*world_writes_available && *notice_available && *remedy_available) {
                Some("service_unavailable".into())
            } else {
                None
            };
            Ok(organization_id.clone())
        }
        Command::FinalizeAvailabilityUnknown {
            organization_id,
            through_world_tick,
        } => {
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            let expected_from = accounted_through(continuity, &registration).saturating_add(1);
            if *through_world_tick != now || *through_world_tick < expected_from {
                return Err(invalid(
                    "availability_unknown_finalization_not_current_or_contiguous",
                ));
            }
            let intervals = continuity
                .availability
                .entry(organization_id.clone())
                .or_default();
            if let Some(previous) = intervals.last_mut().filter(|previous| {
                previous.evidence_status == OrganizationAvailabilityEvidenceStatusV1::Unknown
                    && previous.through_world_tick.saturating_add(1) == expected_from
                    && previous.world_binding == registration.world_binding
                    && previous.registration_receipt == registration.registration_receipt
            }) {
                previous.through_world_tick = *through_world_tick;
            } else {
                intervals.push(OrganizationAvailabilityIntervalV1 {
                    from_world_tick: expected_from,
                    through_world_tick: *through_world_tick,
                    world_writes_available: false,
                    notice_available: false,
                    remedy_available: false,
                    evidence_status: OrganizationAvailabilityEvidenceStatusV1::Unknown,
                    world_binding: registration.world_binding.clone(),
                    registration_receipt: registration.registration_receipt.clone(),
                    certificate_hash: String::new(),
                });
            }
            let registration = continuity
                .registrations
                .get_mut(organization_id)
                .expect("registration validated above");
            registration.last_accounted_world_tick = *through_world_tick;
            registration.availability_paused_reason = Some("availability_unknown".into());
            Ok(organization_id.clone())
        }
        _ => unreachable!("unsupported command reached the specialized dispatcher"),
    }
}
