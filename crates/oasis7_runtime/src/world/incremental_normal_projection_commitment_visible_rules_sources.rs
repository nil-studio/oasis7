//! Nominal original rows for the shared visibility relation kernels.
use super::*;
use oasis7_agent_api::world_action::EmergencyServiceRegistrationV1;
use oasis7_world_state::world_rules::*;
pub(super) struct VisibilitySource<'v, 'n, 'f, 'a, 'c> {
    pub view: &'v View<'f, 'a, 'c>,
    pub node: &'v ProjectionColumnRefV1<'n>,
}
impl VisibilitySource<'_, '_, '_, '_, '_> {
    fn raw(
        &self,
        node: &ProjectionColumnRefV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        super::super::super::shared_json::capture_shared_v1(
            node,
            &self.view.facts.common,
            &RefCell::new(None),
            usize::MAX,
        )
    }
    fn each(
        &self,
        name: &str,
        mut visit: impl FnMut(&str, ProjectionColumnRefV1<'_>) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        let memory = self.view.facts.common.borrow().memory().clone();
        self.view
            .member(self.node, name)?
            .object_members_data(
                &memory,
                &mut |s, b| self.view.facts.work(s, b as usize),
                &mut |key, node, _| visit(key, node).map_err(ProjectionColumnErrorV1::Work),
            )
            .map_err(col)
    }
}
impl RuleVisibilityWorkV1 for VisibilitySource<'_, '_, '_, '_, '_> {
    type Error = BodyMutationError;
    fn visibility_work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.view.facts.work(s, b)
    }
    fn visibility_owned(&self, n: usize) -> Result<(), Self::Error> {
        self.view.facts.before_owned(n)
    }
}
impl SupplyVisibilityFactsV1 for VisibilitySource<'_, '_, '_, '_, '_> {
    fn visibility_supply_authorizations(
        &self,
        mut visit: impl FnMut(&SupplyAuthorizationV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("supply_authorizations", |_, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_supply_authorization_data_v1(&raw, &self.view.facts.common, |value| {
                visit(value)
            })
        })
    }
    fn visibility_supply_authorization<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&SupplyAuthorizationV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let map = self.view.member(self.node, "supply_authorizations")?;
        match self.view.point(&map, id)? {
            None => visit(None),
            Some(node) => {
                let raw = self.raw(&node)?;
                fact_decode::with_supply_authorization_data_v1(
                    &raw,
                    &self.view.facts.common,
                    |value| visit(Some(value)),
                )
            }
        }
    }
}
impl OrganizationVisibilityFactsV1 for VisibilitySource<'_, '_, '_, '_, '_> {
    fn visibility_organization_registrations(
        &self,
        mut visit: impl FnMut(&OrganizationRegistrationV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("registrations", |_, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_organization_registration_data_v1(
                &raw,
                &self.view.facts.common,
                |value| visit(value),
            )
        })
    }
}
impl EmergencyVisibilityFactsV1 for VisibilitySource<'_, '_, '_, '_, '_> {
    fn visibility_emergency_needs(
        &self,
        mut visit: impl FnMut(&str, &EmergencyNeedLedgerV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("needs", |id, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_emergency_need_data_v1(&raw, &self.view.facts.common, |value| {
                visit(id, value)
            })
        })
    }
    fn visibility_emergency_services(
        &self,
        mut visit: impl FnMut(&EmergencyServiceRegistrationV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("services", |_, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_emergency_service_data_v1(&raw, &self.view.facts.common, |value| {
                visit(value)
            })
        })
    }
    fn visibility_emergency_authorizations(
        &self,
        mut visit: impl FnMut(&EmergencyAuthorizationV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("authorizations", |_, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_emergency_authorization_data_v1(
                &raw,
                &self.view.facts.common,
                |value| visit(value),
            )
        })
    }
    fn visibility_emergency_reviews(
        &self,
        mut visit: impl FnMut(&EmergencyReviewV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.each("reviews", |_, node| {
            let raw = self.raw(&node)?;
            fact_decode::with_emergency_review_data_v1(&raw, &self.view.facts.common, |value| {
                visit(value)
            })
        })
    }
    fn visibility_emergency_batch<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&EmergencyBatchV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let map = self.view.member(self.node, "batches")?;
        match self.view.point(&map, id)? {
            None => visit(None),
            Some(node) => {
                let raw = self.raw(&node)?;
                fact_decode::with_emergency_batch_data_v1(&raw, &self.view.facts.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn visibility_emergency_authorization<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&EmergencyAuthorizationV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let map = self.view.member(self.node, "authorizations")?;
        match self.view.point(&map, id)? {
            None => visit(None),
            Some(node) => {
                let raw = self.raw(&node)?;
                fact_decode::with_emergency_authorization_data_v1(
                    &raw,
                    &self.view.facts.common,
                    |value| visit(Some(value)),
                )
            }
        }
    }
}
