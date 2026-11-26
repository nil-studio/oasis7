import { readExpandedCss } from "./test_support/read_expanded_css.js";
import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { pixelWorldMarkerClearance } from './pixel_world_marker_clearance.js';

describe('actionable marker overlay clearance', () => {
  it('gives unselected desktop entities the same 44px minimum hit area', async () => {
    const css = await readExpandedCss('viewer_terminal_shell.css');
    const globalMarkerRule = css.slice(css.indexOf('/* Markers stay terse;')).match(/\.pixel-world-entity\s*\{([^}]+)\}/)?.[1];
    expect(globalMarkerRule).toMatch(/min-width:\s*44px/);
    expect(globalMarkerRule).toMatch(/min-height:\s*44px/);
    expect(globalMarkerRule).toMatch(/box-sizing:\s*border-box/);
  });
  const rect = (left, top, right, bottom) => ({ left, top, right, bottom });
  it('uses the prioritized fallback only when no free placement exists', () => {
    const marker = rect(170,319,214,363);
    const bounds = rect(0,0,390,844);
    const fallback = {x:0,y:-105};
    expect(pixelWorldMarkerClearance(marker, [rect(0,0,390,260),rect(0,266,390,844)], bounds, fallback)).toEqual(fallback);
    expect(pixelWorldMarkerClearance(marker, [], bounds, fallback)).toEqual({x:0,y:0});
  });
  it.each([
    [390, 844, rect(253, 353, 297, 397), [rect(10, 266, 380, 363), rect(252, 371, 380, 469)]],
    [320, 844, rect(85, 344, 129, 388), [rect(10, 266, 310, 363)]],
    [1440, 1000, rect(448, 620, 492, 664), [rect(16, 652, 776, 880)]],
    [1440, 1000, rect(408, 654, 452, 698), [rect(16, 652, 776, 880)]],
  ])('clears rendered panels at %i by %i without shrinking targets', (width, height, marker, panels) => {
    const offset = pixelWorldMarkerClearance(marker, panels, rect(0, 0, width, height));
    const moved = rect(marker.left + offset.x, marker.top + offset.y, marker.right + offset.x, marker.bottom + offset.y);
    expect(moved.right - moved.left).toBe(44);
    expect(moved.bottom - moved.top).toBe(44);
    for (const panel of panels) expect(moved.right <= panel.left - 8 || moved.left >= panel.right + 8 || moved.bottom <= panel.top - 8 || moved.top >= panel.bottom + 8).toBe(true);
    expect(pixelWorldMarkerClearance(moved, panels, rect(0, 0, width, height))).toEqual({ x: 0, y: 0 });
  });
  it('retains the original projection when already clear', () => {
    expect(pixelWorldMarkerClearance(rect(100, 100, 144, 144), [rect(200, 200, 300, 300)], rect(0, 0, 400, 800))).toEqual({ x: 0, y: 0 });
  });
});
