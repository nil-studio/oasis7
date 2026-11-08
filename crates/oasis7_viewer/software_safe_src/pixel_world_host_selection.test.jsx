import { fireEvent, waitFor } from '@solidjs/testing-library';
import { expect, it, vi } from 'vitest';
import { HEAVY_UI_TEST_TIMEOUT_MS, renderPixelWorldHost, sampleSnapshot, runtimeMock, useTestRustRenderState } from './pixel_world_host_test_support.jsx';

it('shares selection authorization across targets and invalidates it when the scope changes', async () => {
  const snapshot = sampleSnapshot();
  snapshot.model.agents = Object.fromEntries(Array.from({ length: 200 }, (_, i) => [`agent-${i}`, {
    id: `agent-${i}`, name: `Visible ${i}`, location_id: 'loc-0', resources: {},
    pos: { x_cm: 1000000 + i * 1000, y_cm: 1000000, z_cm: 0 },
  }]));
  snapshot.model.agent_player_bindings = Object.fromEntries(Object.keys(snapshot.model.agents).map(id => [id, "player-one"]));
  useTestRustRenderState();
  runtimeMock.sourceOverride = "wasm_bindgen_runtime";
  const { core } = await renderPixelWorldHost(snapshot, '?test_api=1&connect=0&locale=en');
  await waitFor(() => expect(runtimeMock.onEvent).toBeTypeOf('function'));
  runtimeMock.onEvent({type:'camera_state_changed',camera:{zoom:1,pan_x_px:0,pan_y_px:0}});
  await waitFor(() => expect(document.querySelectorAll('button[data-agent-id][data-renderer-target]')).toHaveLength(200));
  const scopeApi = await import('./pixel_world_scope.js');
  const scopeReads = vi.spyOn(scopeApi, 'pixelWorldScope');
  fireEvent.click(document.querySelector('button[data-agent-id="agent-1"][data-renderer-target]'));
  await waitFor(() => expect(document.querySelector('button[data-agent-id="agent-1"][data-renderer-target]')).toHaveAttribute('aria-pressed', 'true'));
  expect(document.querySelectorAll('button[data-agent-id][data-renderer-target]')).toHaveLength(200);
  // An authority fact is shared for a revision, rather than re-derived for each
  // marker's selected/aria attributes. Leave room for independent host guards.
  expect(scopeReads.mock.calls.length).toBeLessThan(100);
  core.state.auth.sessionEpoch = (core.state.auth.sessionEpoch || 0) + 1;
  core.requestRender();
  await waitFor(() => expect(document.querySelectorAll('[data-renderer-target]')).toHaveLength(0));
  expect(document.querySelector('[data-renderer-target][data-selected="true"]')).toBeNull();
}, HEAVY_UI_TEST_TIMEOUT_MS);
