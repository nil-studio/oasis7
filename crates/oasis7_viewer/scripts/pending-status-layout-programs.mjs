// Browser-only DOM measurement. The four complete programs below invoke this
// source with literal fixture arguments; Node data is never serialized as code.
function inspectStatus(status, text, feedOpen) {
  const chip = document.querySelector('.pixel-world-canvas__selection');
  const s = document.createElement('div');
  s.className = 'pixel-world-selection-status';
  s.setAttribute('role', 'status');
  s.setAttribute('aria-live', 'polite');
  s.dataset.worldSelectionStatus = status;
  s.textContent = text;
  chip.append(s);
  const range = document.createRange();
  range.selectNodeContents(s);
  const r = range.getBoundingClientRect(), rect = s.getBoundingClientRect();
  window.__LAYOUT_INPUT__ = [];
  document.addEventListener('pointerdown', e => __LAYOUT_INPUT__.push({
    trusted: e.isTrusted, target: e.target.tagName, id: e.target.id,
  }), { once: true, capture: true });
  return {
    viewport: { width: innerWidth, height: innerHeight }, dpr: devicePixelRatio,
    status, feedOpen, textRect: r.toJSON(), rect: rect.toJSON(),
    chip: chip.getBoundingClientRect().toJSON(),
    pointer: [getComputedStyle(s).pointerEvents, getComputedStyle(chip).pointerEvents],
    text: chip.textContent,
    overlays: [...document.querySelectorAll('[data-viewer-overlay]')].map(e => ({
      kind: e.dataset.viewerOverlay, rect: e.getBoundingClientRect().toJSON(),
      z: getComputedStyle(e).zIndex,
    })),
    auth: window.__AW_TEST__.getState().authReady,
  };
}

// Preserve the original feed-closed/feed-open, pending/unavailable loop order.
// Only repository-owned function source and literal calls form these programs.
export const pendingStatusLayoutCases = Object.freeze([
  Object.freeze({
    feedOpen: false, status: 'pending',
    program: `(${inspectStatus})("pending", "Checking the selected object…", false)`,
  }),
  Object.freeze({
    feedOpen: false, status: 'unavailable',
    program: `(${inspectStatus})("unavailable", "This object could not be confirmed. Select again.", false)`,
  }),
  Object.freeze({
    feedOpen: true, status: 'pending',
    program: `(${inspectStatus})("pending", "Checking the selected object…", true)`,
  }),
  Object.freeze({
    feedOpen: true, status: 'unavailable',
    program: `(${inspectStatus})("unavailable", "This object could not be confirmed. Select again.", true)`,
  }),
]);
