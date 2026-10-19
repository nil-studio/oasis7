use oasis7_simulator::WorldEventKind;

use super::ViewerEventKind;

pub fn viewer_event_kind_matches(filter: &ViewerEventKind, kind: &WorldEventKind) -> bool {
    matches!(
        (filter, kind),
        (
            ViewerEventKind::LocationRegistered,
            WorldEventKind::LocationRegistered { .. }
        ) | (
            ViewerEventKind::AgentRegistered,
            WorldEventKind::AgentRegistered { .. }
        ) | (
            ViewerEventKind::AgentMoved,
            WorldEventKind::AgentMoved { .. }
        ) | (
            ViewerEventKind::AgentSpoke,
            WorldEventKind::AgentSpoke { .. }
        ) | (
            ViewerEventKind::TargetInspected,
            WorldEventKind::TargetInspected { .. }
        ) | (
            ViewerEventKind::SimpleInteractionPerformed,
            WorldEventKind::SimpleInteractionPerformed { .. }
        ) | (
            ViewerEventKind::ResourceTransferred,
            WorldEventKind::ResourceTransferred { .. }
        ) | (
            ViewerEventKind::RadiationHarvested,
            WorldEventKind::RadiationHarvested { .. }
        ) | (
            ViewerEventKind::ActionRejected,
            WorldEventKind::ActionRejected { .. }
        ) | (ViewerEventKind::Power, WorldEventKind::Power(_))
            | (
                ViewerEventKind::PromptUpdated,
                WorldEventKind::AgentPromptUpdated { .. }
            )
            | (
                ViewerEventKind::RuntimeEvent,
                WorldEventKind::RuntimeEvent { .. }
            )
    )
}
