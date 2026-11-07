import { createMemo, For } from 'solid-js';
import { toCanvasPoint } from './pixel_world_hotspot_projection.js';
import { pixelWorldVisualState } from './pixel_world_visual_clarity.jsx';
import { pixelWorldReadableAgentLabel, pixelWorldReadableModuleLabel } from './pixel_world_identity.js';
import { isLocaleZh } from './legacy_core.js';
import { forwardRendererTargetPointer } from './pixel_world_renderer_target_input.js';

const RENDERER_TARGET_SIZE_PX = 44;
const MODULE_CO_ANCHOR_RING_OFFSETS = [
  [-48, -48],
  [0, -48],
  [48, -48],
  [-48, 0],
  [48, 0],
  [-48, 48],
  [0, 48],
  [48, 48],
];

function positionKey(position) {
  if (!position || typeof position !== 'object') return null;
  const x = Number(position.x_cm ?? position.xCm);
  const y = Number(position.y_cm ?? position.yCm);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  const z = Number(position.z_cm ?? position.zCm ?? 0);
  return `${x}|${y}|${Number.isFinite(z) ? z : 0}`;
}

function moduleCoAnchorOffset(index) {
  const ring = Math.floor(index / MODULE_CO_ANCHOR_RING_OFFSETS.length) + 1;
  const [x, y] = MODULE_CO_ANCHOR_RING_OFFSETS[index % MODULE_CO_ANCHOR_RING_OFFSETS.length];
  return { x: x * ring, y: y * ring };
}

export function moduleTargetOffsets(visualState) {
  const modules = visualState.moduleVisualEntities
    .filter(entity => entity?.pos)
    .slice()
    .sort((left, right) => left.id < right.id ? -1 : left.id > right.id ? 1 : 0);
  const parentPositions = new Set([
    ...visualState.agents.filter(entity => entity?.pos),
    ...visualState.locations.filter(entity => entity?.pos),
  ].map(entity => positionKey(entity.pos)).filter(key => key !== null));
  const modulesByPosition = new Map();
  for (const module of modules) {
    const key = positionKey(module.pos);
    if (key === null) continue;
    const group = modulesByPosition.get(key) || [];
    group.push(module);
    modulesByPosition.set(key, group);
  }
  const coAnchorSlots = new Map();
  for (const [key, group] of modulesByPosition) {
    group.forEach((module, index) => {
      coAnchorSlots.set(module, { key, index, count: group.length });
    });
  }
  const offsets = new Map();
  modules.forEach((module, index) => {
    const slot = coAnchorSlots.get(module);
    const hasParent = slot ? parentPositions.has(slot.key) : false;
    offsets.set(
      module.id,
      hasParent || slot?.count > 1
        ? moduleCoAnchorOffset(slot?.index ?? index)
        : { x: 0, y: 0 },
    );
  });
  return offsets;
}

// Mirrors the renderer's backing-canvas projection, including missing-position
// presentation. The backing point is converted to the CSS stage after camera
// pan/zoom and co-anchor offsets are applied in renderer coordinates.
export function rendererEntityTargetStyle(entity, worldBounds, size, camera, screenOffset, rendererSize = size) {
  const { width, height } = size;
  const rendererWidth = Number(rendererSize?.width) || width;
  const rendererHeight = Number(rendererSize?.height) || height;
  const idLength = new TextEncoder().encode(entity.id || '').length;
  const point = toCanvasPoint(entity.pos, worldBounds, width, height, camera)
    || toCanvasPoint({ x_cm: 36 + (idLength * 29) % Math.max(40, width - 72), y_cm: 44 + (idLength * 17) % Math.max(48, height - 88) }, { width_cm: width, depth_cm: height }, width, height, camera);
  const scale = Math.max(1, Math.floor(Math.min(rendererWidth / 640, rendererHeight / 360)));
  const snap = (value, pan, css, frame) => {
    const logical = Math.floor(frame / scale);
    const offset = Math.floor((frame - scale * logical) / 2);
    const unit = css / frame * scale;
    const t = pan / unit;
    const q = (value * frame / css - offset) / scale;
    return (Math.floor(q - t + Math.floor(t + 0.5) + 0.5) * scale + offset) * css / frame;
  };
  const x = snap(point.x, Number(camera?.pan_x_px ?? camera?.panX) || 0, width, rendererWidth) + (Number(screenOffset?.x) || 0);
  const y = snap(point.y, Number(camera?.pan_y_px ?? camera?.panY) || 0, height, rendererHeight) + (Number(screenOffset?.y) || 0);
  const halfTarget = RENDERER_TARGET_SIZE_PX / 2;
  return { left: `${x / width * 100}%`, top: `${y / height * 100}%`, width: `${RENDERER_TARGET_SIZE_PX}px`, height: `${RENDERER_TARGET_SIZE_PX}px`, transform: 'translate(-50%, -50%)', display: x + halfTarget <= 0 || y + halfTarget <= 0 || x - halfTarget >= width || y - halfTarget >= height ? 'none' : undefined };
}

export function PixelWorldRendererTargets(props) {
  const state = createMemo(() => pixelWorldVisualState(props.renderState()));
  const isZh = () => isLocaleZh(props.locale());
  const moduleOffsets = createMemo(() => moduleTargetOffsets(state()));
  // Solid's For retains nodes by key; snapshot objects are replaced routinely.
  // Keep kind/id keys stable while reading labels and positions from the latest map.
  const entities = createMemo(() => new Map([
    ...state().agents.map(entity => [JSON.stringify(['agent', entity.id]), entity]),
    ...(state().worldBounds ? state().locations.filter(entity => entity.pos) : [])
      .map(entity => [JSON.stringify(['location', entity.id]), entity]),
    ...state().moduleVisualEntities.filter(entity => entity.pos)
      .map(entity => [JSON.stringify(['module_visual', entity.id]), entity]),
  ]));
  const keys = createMemo(() => [...entities().keys()]);
  return <For each={keys()}>{key => {
    const [kind] = JSON.parse(key);
    const entity = createMemo(previous => entities().get(key) || previous);
    return <button type="button"
    class="pixel-world-entity pixel-world-renderer-target"
    data-agent-id={kind === 'agent' ? entity().id : undefined}
    data-location-id={kind === 'location' ? entity().id : undefined}
    data-pixel-world-agent-marker={kind === 'agent' ? 'true' : undefined}
    data-pixel-world-location-marker={kind === 'location' ? 'true' : undefined}
    data-pixel-world-module-marker={kind === 'module_visual' ? 'true' : undefined}
    data-module-id={kind === 'module_visual' ? entity().id : undefined}
    data-module-kind={kind === 'module_visual' ? entity().kind : undefined}
    data-renderer-target="true"
    data-selected={props.selection()?.kind === kind && props.selection()?.id === entity().id ? 'true' : 'false'}
    aria-pressed={props.selection()?.kind === kind && props.selection()?.id === entity().id}
    aria-label={`${isZh() ? '选择' : 'Select'} ${kind === 'agent' ? pixelWorldReadableAgentLabel(entity(), entity().id, isZh()) : kind === 'module_visual' ? pixelWorldReadableModuleLabel(entity(), entity().id, isZh()) : entity().label || entity().id}`}
    style={rendererEntityTargetStyle(entity(),state().worldBounds,props.stageSize(),props.cameraState?.(),kind === 'module_visual' ? moduleOffsets().get(entity().id) : undefined, props.rendererSize?.())}
    onClick={() => props.onSelect({kind,id:entity().id})}
    onPointerDown={forwardRendererTargetPointer}
    onMouseEnter={() => props.onHover({kind,id:entity().id})}
    onMouseLeave={() => props.onHover(null)} />; }}</For>;
}
