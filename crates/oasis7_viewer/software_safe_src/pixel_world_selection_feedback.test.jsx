import { render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { expect, it } from 'vitest';
import { PixelWorldCanvasRenderer } from './pixel_world_host.jsx';

it('keeps pending and unavailable feedback inside the responsive selection HUD, without claiming selection',()=>{
  const [status,setStatus]=createSignal('pending');
  const view=render(()=><PixelWorldCanvasRenderer locale={()=> 'en'} renderState={()=>null}
    selection={()=>null} selectionStatus={status} hoveredEntity={()=>null}
    visualOverlayEnabled={()=>false} onHover={()=>{}} rendererStatus={()=> 'ready'} />);
  try {
    const feedback=view.container.querySelector('[data-world-selection-status]');
    expect(feedback.closest('.pixel-world-canvas__selection')).not.toBeNull();
    expect(feedback.closest('.pixel-world-canvas')).not.toBeNull();
    expect(feedback.textContent).toContain('Checking');
    expect(view.container.querySelector('[data-selected=true]')).toBeNull();
    setStatus('unavailable');
    expect(feedback.textContent).toContain('could not be confirmed');
    setStatus(null);
    expect(view.container.querySelector('[data-world-selection-status]')).toBeNull();
  } finally {view.unmount();}
});
