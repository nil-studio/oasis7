import { expect, it } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { renderPixelWorldHost, sampleSnapshot, useTestRustRenderState } from './pixel_world_host_test_support.jsx';

it('refreshes the renderer input after a same-scope Agent snapshot update and deletion', async () => {
  useTestRustRenderState();
  const { core } = await renderPixelWorldHost(sampleSnapshot());
  await waitFor(() => expect(document.querySelector('[data-agent-id="agent-0"]')).toBeTruthy());
  const snapshot = JSON.parse(JSON.stringify(core.state.snapshot));
  snapshot.model.agents['agent-0'].name = '更新后的独立中文目标';
  core.injectSnapshot(snapshot);
  await waitFor(() => expect(document.querySelector('[data-agent-id="agent-0"]')?.getAttribute('aria-label')).toContain('更新后的独立中文目标'));
  const removed = JSON.parse(JSON.stringify(core.state.snapshot));
  delete removed.model.agents['agent-0'];
  core.injectSnapshot(removed);
  await waitFor(() => expect(document.querySelector('[data-agent-id="agent-0"]')).toBeNull());
});
