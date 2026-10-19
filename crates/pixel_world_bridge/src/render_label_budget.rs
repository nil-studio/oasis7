//! A single budget across every canvas text producer, after scene reconciliation.
//! Hidden labels retain their identity so shrinking scenes restore them without
//! touching markers, input regions, or the complete host-side Targets list.
use super::*;

const MAX_VISIBLE_LABELS: usize = 100;
const MAX_LABEL_SCALARS: usize = 128;

#[derive(Component)]
pub(crate) struct BudgetHidden(Visibility);

type LabelQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Text2d,
        &'static mut Visibility,
        Option<&'static BudgetHidden>,
        Option<&'static agent_labels::PixelWorldAgentLabel>,
        Option<&'static location_labels::PixelWorldLocationLabel>,
        Option<&'static module_visual_entities::PixelWorldModuleVisualLabel>,
        Option<&'static map_labels::PixelWorldMapLabel>,
        Option<&'static PixelWorldSelectedResourceReadout>,
        Option<&'static micro_depot_facilities::PixelWorldMicroDepotThroughputTextVisual>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct LabelBudgetQueries<'w, 's> {
    labels: LabelQuery<'w, 's>,
    throughput_backings: Query<
        'w,
        's,
        (
            Entity,
            &'static micro_depot_facilities::PixelWorldMicroDepotThroughputBackingVisual,
            &'static mut Visibility,
            Option<&'static BudgetHidden>,
        ),
        Without<Text2d>,
    >,
}

pub(crate) fn enforce_label_budget(
    mut commands: Commands,
    runtime: Res<BevyRuntimeState>,
    mut queries: LabelBudgetQueries,
) {
    let labels = &mut queries.labels;
    let state = runtime.render_state.as_ref();
    let selection = state.and_then(|state| state.selection.as_ref());
    let primary_hotspot = |kind: &str| {
        state.and_then(|state| {
            state
                .visual_hotspots
                .iter()
                .filter(|hotspot| hotspot.kind == kind)
                .map(|hotspot| hotspot.id.as_str())
                .min()
        })
    };
    let primary_goal = primary_hotspot("goal");
    let primary_blocker = primary_hotspot("blocker");
    let mut candidates = Vec::new();
    for (entity, text, visibility, hidden, agent, location, module, map, readout, _) in
        labels.iter_mut()
    {
        // Preserve any producer's intentional visibility. Only labels hidden
        // by this budget may be restored here.
        if *visibility == Visibility::Hidden && hidden.is_none() {
            continue;
        }
        let identity = agent
            .map(|label| ("agent", label.id.as_str()))
            .or_else(|| location.map(|label| ("location", label.id.as_str())))
            .or_else(|| module.map(|label| ("module_visual", label.id.as_str())));
        let selected = identity.is_some_and(|(kind, id)| {
            selection.is_some_and(|selection| selection.kind == kind && selection.id == id)
        });
        let hotspot = map.and_then(|label| {
            let id = label.id.strip_prefix("map:")?;
            state?
                .visual_hotspots
                .iter()
                .find(|hotspot| hotspot.id == id)
        });
        let priority = if selected || readout.is_some() {
            0
        } else if hotspot.is_some_and(|hotspot| Some(hotspot.id.as_str()) == primary_goal) {
            1
        } else if hotspot.is_some_and(|hotspot| Some(hotspot.id.as_str()) == primary_blocker) {
            2
        } else if hotspot.is_some() {
            3
        } else if identity.is_some() {
            4
        } else {
            5
        };
        let stable_id = identity
            .map(|(kind, id)| format!("{kind}:{id}"))
            .or_else(|| map.map(|label| label.id.clone()))
            .unwrap_or_else(|| text.0.chars().take(MAX_LABEL_SCALARS - 1).collect());
        candidates.push((priority, stable_id, entity));
    }
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.to_bits().cmp(&right.2.to_bits()))
    });
    let mut visible_throughput_ids = HashSet::new();
    for (index, (_, _, entity)) in candidates.into_iter().enumerate() {
        let Ok((_, mut text, mut visibility, hidden, _, _, _, _, _, throughput)) =
            labels.get_mut(entity)
        else {
            continue;
        };
        if text.0.chars().count() > MAX_LABEL_SCALARS {
            text.0 = text
                .0
                .chars()
                .take(MAX_LABEL_SCALARS - 1)
                .chain(['…'])
                .collect();
        }
        if index < MAX_VISIBLE_LABELS {
            if let Some(hidden) = hidden {
                *visibility = hidden.0;
                commands.entity(entity).remove::<BudgetHidden>();
            }
            if let Some(throughput) = throughput {
                visible_throughput_ids.insert(throughput.id.clone());
            }
        } else if hidden.is_none() {
            commands.entity(entity).insert(BudgetHidden(*visibility));
            *visibility = Visibility::Hidden;
        }
    }
    // The passive backing belongs to the label, not to the facility marker.
    // Keep it in the same visibility budget so capped text cannot leave empty
    // opaque strips behind. Stock runways, marker sprites and hit regions are
    // separate components and never participate in this filter.
    for (entity, backing, mut visibility, hidden) in queries.throughput_backings.iter_mut() {
        if visible_throughput_ids.contains(&backing.id) {
            if let Some(hidden) = hidden {
                *visibility = hidden.0;
                commands.entity(entity).remove::<BudgetHidden>();
            }
        } else if hidden.is_none() && *visibility != Visibility::Hidden {
            commands.entity(entity).insert(BudgetHidden(*visibility));
            *visibility = Visibility::Hidden;
        }
    }
}

#[cfg(test)]
#[path = "render_label_budget_tests.rs"]
mod tests;
