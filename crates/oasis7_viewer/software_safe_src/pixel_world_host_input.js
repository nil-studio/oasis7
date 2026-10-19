import * as core from "./legacy_core.js";
import { buildViewerEntityLists } from "./viewer_entity_list_renderer.js";

// Targets search is a list presentation filter, never a world visibility grant.
export function buildPixelWorldRenderInput(locale = core.state.uiLocale) {
  const worldScaleSurface = core.buildWorldScaleSurface(locale);
  return {
    locale,
    snapshot: core.state.snapshot,
    lists: buildViewerEntityLists({entityCollections:core.entityCollections, isAgentVisibleToCurrentSession:core.isAgentVisibleToCurrentSession}),
    gameplay: core.buildGameplaySummary(locale),
    selected: core.state.selectedObject,
    selectedKind: core.state.selectedKind,
    selectedId: core.state.selectedId,
    presentation: {
      world_bounds_label: worldScaleSurface.physicalTruth.worldBoundsLabel,
      marker_truth_note: worldScaleSurface.presentationScale.markerTruthNote,
    },
  };
}
