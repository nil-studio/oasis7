//! Borrow only an actual Node view or the independent service's opaque view.
//! Neither variant can be constructed from wire fields or an arbitrary schedule.
use super::*;
use oasis7_distfs::protected_bft_v3::ObservedBftOriginalInputServingViewV1;
#[derive(Clone, Copy)]
pub enum NodeBftAdmissionInputReadViewV1<'a> {
    Node(&'a NodeBftPeerExecutionViewV3),
    Provider(&'a ObservedBftOriginalInputServingViewV1),
}
impl NodeBftAdmissionInputReadViewV1<'_> {
    pub fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        match self {
            Self::Node(view) => &view.schedule,
            Self::Provider(view) => view.schedule(),
        }
    }
    pub fn generation(&self) -> u64 {
        match self {
            Self::Node(view) => view.generation,
            Self::Provider(view) => view.generation(),
        }
    }
}
