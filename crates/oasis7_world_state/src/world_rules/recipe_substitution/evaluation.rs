use super::*;
pub fn candidate(
    state: &WorldState,
    rules: &SubstitutionStateV1,
    industrial: &IndustrialWorkStateV1,
    request: &SubstitutionPreviewRequestV1,
    reference: &IndustrialRefV1,
    context: SubstitutionContext<'_>,
) -> Result<SubstitutionCandidateV1, WorldError> {
    let SubstitutionContext { actor, world, now } = context;
    let source=super::super::readonly_start_industrial_overlay::IndustrialInputOverlayFactsV1{source:state,industrial,substitution:rules};
    match super::super::readonly_substitution::candidate(&source,request,reference,actor,world,now){Ok(value)=>value,Err(never)=>match never{}}
}
